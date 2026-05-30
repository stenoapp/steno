use crate::audio::meter::Meter;
use crate::audio::resample::resample_mono_to_48k;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct RecordingMeta {
    // The device's native sample rate. If it isn't 48 kHz, samples get
    // resampled in `stop()` via crate::audio::resample.
    pub sample_rate: u32,
    pub channels: u16,
}

// The cpal Stream is held on its own thread to sidestep Send/Sync constraints
// that vary by platform. The main thread only holds the stop-signal sender
// and the joiner; samples flow into a shared Arc<Mutex<Vec<f32>>>.
pub struct MicRecorder {
    stop_tx: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<RecordingMeta>>,
    samples: Arc<Mutex<Vec<f32>>>,
    meter: Meter,
}

impl MicRecorder {
    pub fn new(meter: Meter) -> Self {
        Self {
            stop_tx: None,
            thread: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            meter,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.thread.is_some() {
            return Err("recording already in progress".into());
        }

        self.samples.lock().unwrap().clear();
        let samples = Arc::clone(&self.samples);
        let meter = self.meter.clone();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<RecordingMeta, String>>();

        let handle = std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    let _ = ready_tx.send(Err("no default input device".into()));
                    return RecordingMeta { sample_rate: 0, channels: 0 };
                }
            };

            let supported = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("default input config: {e}")));
                    return RecordingMeta { sample_rate: 0, channels: 0 };
                }
            };

            let sample_rate = supported.sample_rate().0;
            let channels = supported.channels();

            if !(1..=8).contains(&channels) {
                let _ = ready_tx.send(Err(format!(
                    "device reports {channels} channels; refusing to capture (downmix supports 1-8)"
                )));
                return RecordingMeta { sample_rate, channels };
            }
            // Non-48 kHz devices are accepted; their samples get resampled
            // in stop() via resample::resample_mono_to_48k. This is the
            // M1.5 path that replaces the M1.1-era hard error.

            let sample_format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();
            let err_fn = |err| eprintln!("[steno] audio stream error: {err}");

            let build_result = match sample_format {
                cpal::SampleFormat::F32 => {
                    let samples = Arc::clone(&samples);
                    let meter = meter.clone();
                    device.build_input_stream(
                        &config,
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            meter.observe(data);
                            samples.lock().unwrap().extend_from_slice(data);
                        },
                        err_fn,
                        None,
                    )
                }
                cpal::SampleFormat::I16 => {
                    let samples = Arc::clone(&samples);
                    let meter = meter.clone();
                    device.build_input_stream(
                        &config,
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            let mut buf = samples.lock().unwrap();
                            buf.reserve(data.len());
                            for &s in data {
                                buf.push(s as f32 / i16::MAX as f32);
                            }
                            // Slice the just-pushed range to observe peak.
                            let new_start = buf.len() - data.len();
                            meter.observe(&buf[new_start..]);
                        },
                        err_fn,
                        None,
                    )
                }
                cpal::SampleFormat::U16 => {
                    let samples = Arc::clone(&samples);
                    let meter = meter.clone();
                    device.build_input_stream(
                        &config,
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            let mut buf = samples.lock().unwrap();
                            buf.reserve(data.len());
                            for &s in data {
                                buf.push((s as f32 - 32768.0) / 32768.0);
                            }
                            let new_start = buf.len() - data.len();
                            meter.observe(&buf[new_start..]);
                        },
                        err_fn,
                        None,
                    )
                }
                other => {
                    let _ = ready_tx.send(Err(format!("unsupported sample format: {other:?}")));
                    return RecordingMeta { sample_rate, channels };
                }
            };

            let stream = match build_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("build input stream: {e}")));
                    return RecordingMeta { sample_rate, channels };
                }
            };

            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("stream play: {e}")));
                return RecordingMeta { sample_rate, channels };
            }

            let _ = ready_tx.send(Ok(RecordingMeta { sample_rate, channels }));

            // Block until stop signalled; dropping the stream stops capture.
            let _ = stop_rx.recv();
            drop(stream);

            RecordingMeta { sample_rate, channels }
        });

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

    /// Stop the cpal stream and return the captured samples downmixed to
    /// mono and resampled to 48 kHz. Caller owns the samples — encoding
    /// happens in `commands.rs`.
    pub fn stop(&mut self) -> Result<Vec<f32>, String> {
        let stop_tx = self.stop_tx.take().ok_or("not recording")?;
        let thread = self.thread.take().ok_or("not recording")?;

        let _ = stop_tx.send(());
        let meta = thread.join().map_err(|_| "audio thread panicked")?;

        let interleaved = std::mem::take(&mut *self.samples.lock().unwrap());
        let mono = downmix_to_mono(&interleaved, meta.channels);
        Ok(resample_mono_to_48k(&mono, meta.sample_rate))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_mono_passthrough() {
        let input = vec![0.1, 0.2, 0.3];
        let mono = downmix_to_mono(&input, 1);
        assert_eq!(mono, input);
    }

    #[test]
    fn downmix_stereo_averages_lr() {
        // Interleaved [L0, R0, L1, R1]
        let input = vec![1.0, 0.0, 0.5, 0.5];
        let mono = downmix_to_mono(&input, 2);
        assert_eq!(mono, vec![0.5, 0.5]);
    }
}
