use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct RecordingMeta {
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
}

impl MicRecorder {
    pub fn new() -> Self {
        Self {
            stop_tx: None,
            thread: None,
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.thread.is_some() {
            return Err("recording already in progress".into());
        }

        self.samples.lock().unwrap().clear();
        let samples = Arc::clone(&self.samples);
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
            let sample_format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();

            let err_fn = |err| eprintln!("[steno] audio stream error: {err}");

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

    pub fn stop(&mut self, out_path: &Path) -> Result<PathBuf, String> {
        let stop_tx = self.stop_tx.take().ok_or("not recording")?;
        let thread = self.thread.take().ok_or("not recording")?;

        let _ = stop_tx.send(());
        let meta = thread.join().map_err(|_| "audio thread panicked")?;

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
        }

        let spec = hound::WavSpec {
            channels: meta.channels,
            sample_rate: meta.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(out_path, spec)
            .map_err(|e| format!("create wav writer: {e}"))?;

        let samples = self.samples.lock().unwrap();
        for &sample in samples.iter() {
            writer
                .write_sample(sample)
                .map_err(|e| format!("write sample: {e}"))?;
        }
        writer.finalize().map_err(|e| format!("finalize wav: {e}"))?;

        Ok(out_path.to_path_buf())
    }
}
