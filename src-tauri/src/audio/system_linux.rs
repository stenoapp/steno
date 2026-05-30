//! Linux-only system audio capture via PipeWire.
//!
//! Connects to PipeWire as an `Audio/Capture` stream with
//! `PW_KEY_STREAM_CAPTURE_SINK = "true"`, which routes the default sink's
//! monitor source to us — the standard pattern for system audio loopback
//! on PipeWire systems.
//!
//! Mirrors `system.rs` (macOS) in shape: same `SystemAudioRecorder` with
//! `start()`/`stop()` returning mono Float32 @ 48 kHz. PipeWire's main loop
//! is single-threaded, so we spawn a dedicated thread and use
//! `pw::channel` for cross-thread stop signalling.
//!
//! Rate handling note: we request F32LE but leave rate/channels open so
//! PipeWire can fit us into the native graph rate (usually 48 kHz). If a
//! Linux box runs the graph at a non-48k rate we'll capture at that rate;
//! resampling lands with `rubato` in M1.5.

#![cfg(target_os = "linux")]

use crate::audio::meter::Meter;
use crate::audio::resample::resample_mono_to_48k;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;
use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const SAMPLE_RATE_HZ: u32 = 48_000;

#[allow(dead_code)]
pub const SYSTEM_AUDIO_SAMPLE_RATE: u32 = SAMPLE_RATE_HZ;

struct StreamUserData {
    format: spa::param::audio::AudioInfoRaw,
    samples: Arc<Mutex<Vec<f32>>>,
    meter: Meter,
    // PipeWire negotiates the actual rate in param_changed; stop() reads
    // it here for the resample step. 0 means "not yet negotiated".
    sample_rate: Arc<AtomicU32>,
}

pub struct SystemAudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stop_tx: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    meter: Meter,
    sample_rate: Arc<AtomicU32>,
}

impl SystemAudioRecorder {
    pub fn new(meter: Meter) -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stop_tx: None,
            thread: None,
            meter,
            sample_rate: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.thread.is_some() {
            return Err("system audio recording already in progress".into());
        }
        self.samples.lock().unwrap().clear();

        let (pw_sender, pw_receiver) = pw::channel::channel::<()>();
        // Setup channel: thread reports Ok once the stream is connected,
        // or Err if any setup step failed. Sized 1 so the thread doesn't
        // block if start() times out and stops listening.
        let (setup_tx, setup_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        let samples = Arc::clone(&self.samples);
        let meter = self.meter.clone();
        let sample_rate = Arc::clone(&self.sample_rate);
        self.sample_rate.store(0, Ordering::Relaxed);
        let thread = std::thread::Builder::new()
            .name("steno-pw-capture".into())
            .spawn(move || {
                run_capture(samples, meter, sample_rate, pw_receiver, setup_tx);
            })
            .map_err(|e| format!("spawn pw thread: {e}"))?;

        // 2s is generous — pw setup is usually milliseconds. If the daemon
        // isn't running or perms are missing, we want a clear error, not
        // a silent hang in the recording UI.
        match setup_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {
                self.stop_tx = Some(pw_sender);
                self.thread = Some(thread);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                // Best-effort: send stop in case the thread is mid-setup.
                let _ = pw_sender.send(());
                let _ = thread.join();
                Err("pipewire setup timed out after 2s".into())
            }
        }
    }

    /// Stop the PipeWire stream and return the captured mono Float32 samples,
    /// resampled to 48 kHz if PipeWire negotiated a different rate.
    /// Returns Ok with whatever was captured even on join errors because the
    /// user's audio data is precious.
    pub fn stop(&mut self) -> Result<Vec<f32>, String> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let samples = std::mem::take(&mut *self.samples.lock().unwrap());
        let rate = self.sample_rate.load(Ordering::Relaxed);
        if rate == 0 {
            // param_changed never fired (no audio? broken negotiation?). Skip
            // the resample step and return whatever we have.
            return Ok(samples);
        }
        Ok(resample_mono_to_48k(&samples, rate))
    }
}

fn run_capture(
    samples: Arc<Mutex<Vec<f32>>>,
    meter: Meter,
    sample_rate: Arc<AtomicU32>,
    stop_rx: pw::channel::Receiver<()>,
    setup_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    // Helper closure that ships an error report up to start() and bails.
    let report_setup_err = |e: String, tx: &std::sync::mpsc::SyncSender<Result<(), String>>| {
        let _ = tx.send(Err(e));
    };

    pw::init();

    let mainloop = match pw::main_loop::MainLoop::new(None) {
        Ok(m) => m,
        Err(e) => return report_setup_err(format!("MainLoop::new: {e}"), &setup_tx),
    };
    let context = match pw::context::Context::new(&mainloop) {
        Ok(c) => c,
        Err(e) => return report_setup_err(format!("Context::new: {e}"), &setup_tx),
    };
    let core = match context.connect(None) {
        Ok(c) => c,
        Err(e) => return report_setup_err(format!("Context::connect (pipewire daemon running?): {e}"), &setup_tx),
    };

    // Stop signal: when start() (or stop()) sends (), quit the main loop.
    // Must keep the AttachedReceiver alive — dropping it would detach.
    let _stop_receiver = stop_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };
    // The load-bearing line: routes our capture stream to the default
    // sink's monitor port — i.e., everything the system is playing back.
    props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");

    let user_data = StreamUserData {
        format: spa::param::audio::AudioInfoRaw::new(),
        samples: Arc::clone(&samples),
        meter,
        sample_rate: Arc::clone(&sample_rate),
    };

    let stream = match pw::stream::Stream::new(&core, "steno-system-capture", props) {
        Ok(s) => s,
        Err(e) => return report_setup_err(format!("Stream::new: {e}"), &setup_tx),
    };

    let _listener = match stream
        .add_local_listener_with_user_data(user_data)
        .param_changed(|_, ud, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else { return };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            // Stash the negotiated rate/channels so the process callback
            // knows how many channels each frame has, and so stop() can
            // resample to 48 kHz if PipeWire picked a different rate.
            let _ = ud.format.parse(param);
            ud.sample_rate.store(ud.format.rate(), Ordering::Relaxed);
        })
        .process(|stream, ud| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let n_channels = ud.format.channels().max(1) as usize;
            // chunk().size() before data() — data() returns &mut [u8] which
            // would conflict with the immutable chunk() borrow otherwise.
            let chunk_bytes = data.chunk().size() as usize;
            let Some(bytes) = data.data() else { return };
            if chunk_bytes == 0 || bytes.len() < chunk_bytes {
                return;
            }
            const BYTES_PER_SAMPLE: usize = mem::size_of::<f32>();
            let total_samples = chunk_bytes / BYTES_PER_SAMPLE;
            let frames = total_samples / n_channels;
            if frames == 0 {
                return;
            }
            // SAFETY: PipeWire delivers F32LE in chunks aligned by SPA
            // buffer allocation; len bounded above.
            let floats: &[f32] = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), total_samples)
            };
            let mut out = ud.samples.lock().unwrap();
            let before = out.len();
            out.reserve(frames);
            if n_channels == 1 {
                out.extend_from_slice(&floats[..frames]);
            } else {
                for frame_idx in 0..frames {
                    let mut sum = 0.0_f32;
                    for ch in 0..n_channels {
                        sum += floats[frame_idx * n_channels + ch];
                    }
                    out.push(sum / n_channels as f32);
                }
            }
            let new_samples = &out[before..];
            ud.meter.observe(new_samples);
        })
        .register()
    {
        Ok(l) => l,
        Err(e) => return report_setup_err(format!("Stream listener register: {e}"), &setup_tx),
    };

    // EnumFormat param: F32LE; rate/channels left open so PipeWire fits us
    // into the native graph rate (usually 48 kHz).
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = match pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    ) {
        Ok(s) => s.0.into_inner(),
        Err(e) => return report_setup_err(format!("POD serialize: {e}"), &setup_tx),
    };
    let Some(pod) = Pod::from_bytes(&values) else {
        return report_setup_err("POD from_bytes returned None".into(), &setup_tx);
    };
    let mut params = [pod];

    if let Err(e) = stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    ) {
        return report_setup_err(format!("Stream::connect: {e}"), &setup_tx);
    }

    // Setup succeeded — tell start() it can return Ok.
    let _ = setup_tx.send(Ok(()));

    mainloop.run();
}
