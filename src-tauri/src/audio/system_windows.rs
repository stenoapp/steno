//! Windows-only system audio capture via WASAPI loopback.
//!
//! cpal's wasapi backend already knows how to do loopback: when you call
//! `build_input_stream` on a device whose data_flow is eRender (i.e.,
//! an output device), it ORs `AUDCLNT_STREAMFLAGS_LOOPBACK` into the
//! stream flags. See cpal-0.15.3 host/wasapi/device.rs:570. So loopback
//! is "pick the default output device, ask for an *input* stream on it".
//!
//! Mirrors `system.rs` (macOS) and `system_linux.rs` (Linux) in shape:
//! same `SystemAudioRecorder` with `start()`/`stop()` returning mono
//! Float32 @ 48 kHz. Sample-rate is hard-pinned to 48 kHz (M1.5 will
//! add a `rubato` resampling fallback for devices that disagree).

#![cfg(target_os = "windows")]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const SAMPLE_RATE_HZ: u32 = 48_000;

#[allow(dead_code)]
pub const SYSTEM_AUDIO_SAMPLE_RATE: u32 = SAMPLE_RATE_HZ;

struct StreamMeta {
    channels: u16,
}

pub struct SystemAudioRecorder {
    stop_tx: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<StreamMeta>>,
    samples: Arc<Mutex<Vec<f32>>>,
}

impl SystemAudioRecorder {
    pub fn new() -> Self {
        Self {
            stop_tx: None,
            thread: None,
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.thread.is_some() {
            return Err("system audio recording already in progress".into());
        }
        self.samples.lock().unwrap().clear();

        let samples = Arc::clone(&self.samples);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<StreamMeta, String>>();

        let handle = std::thread::Builder::new()
            .name("steno-wasapi-loopback".into())
            .spawn(move || run_capture(samples, stop_rx, ready_tx))
            .map_err(|e| format!("spawn wasapi loopback thread: {e}"))?;

        match ready_rx.recv().map_err(|e| format!("ready channel: {e}"))? {
            Ok(_meta) => {
                self.stop_tx = Some(stop_tx);
                self.thread = Some(handle);
                Ok(())
            }
            Err(e) => {
                let _ = handle.join();
                Err(e)
            }
        }
    }

    pub fn stop(&mut self) -> Result<Vec<f32>, String> {
        let stop_tx = self.stop_tx.take().ok_or("not recording")?;
        let thread = self.thread.take().ok_or("not recording")?;

        let _ = stop_tx.send(());
        let meta = thread.join().map_err(|_| "wasapi loopback thread panicked")?;

        let interleaved = std::mem::take(&mut *self.samples.lock().unwrap());
        Ok(downmix_to_mono(&interleaved, meta.channels))
    }
}

fn run_capture(
    samples: Arc<Mutex<Vec<f32>>>,
    stop_rx: mpsc::Receiver<()>,
    ready_tx: mpsc::Sender<Result<StreamMeta, String>>,
) -> StreamMeta {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = ready_tx.send(Err("no default output device".into()));
            return StreamMeta { channels: 0 };
        }
    };

    // default_output_config because data_flow is eRender — cpal refuses
    // default_input_config on output devices even though we're about to
    // use it as a loopback input.
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("default output config: {e}")));
            return StreamMeta { channels: 0 };
        }
    };

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();

    if sample_rate != SAMPLE_RATE_HZ {
        let _ = ready_tx.send(Err(format!(
            "output device sample rate is {sample_rate} Hz; M1 requires {SAMPLE_RATE_HZ} Hz (rubato resampling lands in M1.5)"
        )));
        return StreamMeta { channels };
    }
    if !(1..=8).contains(&channels) {
        let _ = ready_tx.send(Err(format!(
            "output device reports {channels} channels; downmix supports 1-8"
        )));
        return StreamMeta { channels };
    }

    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err_fn = |err| eprintln!("[steno] wasapi loopback stream error: {err}");

    let build_result = match sample_format {
        cpal::SampleFormat::F32 => {
            let samples = Arc::clone(&samples);
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    samples.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let samples = Arc::clone(&samples);
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut buf = samples.lock().unwrap();
                    buf.reserve(data.len());
                    for &s in data {
                        buf.push(s as f32 / i16::MAX as f32);
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let samples = Arc::clone(&samples);
            device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mut buf = samples.lock().unwrap();
                    buf.reserve(data.len());
                    for &s in data {
                        buf.push((s as f32 - 32768.0) / 32768.0);
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            let _ = ready_tx.send(Err(format!("unsupported sample format: {other:?}")));
            return StreamMeta { channels };
        }
    };

    let stream = match build_result {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("build_input_stream on output device (loopback): {e}")));
            return StreamMeta { channels };
        }
    };

    if let Err(e) = stream.play() {
        let _ = ready_tx.send(Err(format!("stream play: {e}")));
        return StreamMeta { channels };
    }

    let _ = ready_tx.send(Ok(StreamMeta { channels }));

    let _ = stop_rx.recv();
    drop(stream);

    StreamMeta { channels }
}

fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let chs = channels as usize;
    let frames = interleaved.len() / chs;
    let mut mono = Vec::with_capacity(frames);
    for frame in interleaved.chunks_exact(chs) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / chs as f32);
    }
    mono
}
