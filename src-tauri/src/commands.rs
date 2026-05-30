use crate::audio::capture::MicRecorder;
use crate::audio::encode::OpusOggWriter;
use crate::audio::meter::Meter;
use crate::audio::mixer;
use chrono::Local;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
use crate::audio::system::SystemAudioRecorder;

const OPUS_BITRATE_BPS: i32 = 32_000;
const METER_INTERVAL: Duration = Duration::from_millis(50);
const METER_EVENT: &str = "audio-level";

#[derive(Clone, serde::Serialize)]
struct AudioLevel {
    mic: f32,
    system: f32,
}

struct MeterWorker {
    stop_tx: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

pub struct AppState {
    pub mic: Mutex<MicRecorder>,
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub system: Mutex<SystemAudioRecorder>,
    mic_meter: Meter,
    system_meter: Meter,
    meter_worker: Mutex<Option<MeterWorker>>,
}

impl AppState {
    pub fn new() -> Self {
        let mic_meter = Meter::new();
        let system_meter = Meter::new();
        Self {
            mic: Mutex::new(MicRecorder::new(mic_meter.clone())),
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            system: Mutex::new(SystemAudioRecorder::new(system_meter.clone())),
            mic_meter,
            system_meter,
            meter_worker: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub fn start_recording(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    // Start mic first — it's the indispensable source. If it fails, bail
    // before touching the system stream (avoids misleading
    // screen-recording / monitor-source permission prompts for a mic that's
    // already broken).
    state.mic.lock().unwrap().start()?;

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        if let Err(e) = state.system.lock().unwrap().start() {
            // Wind back the mic so the user can retry with system-audio
            // disabled (M1.5) once we add a toggle. Hard-fail for now.
            let _ = state.mic.lock().unwrap().stop();
            return Err(format!(
                "system audio start failed (Screen Recording / PipeWire / WASAPI permission may be required): {e}"
            ));
        }
    }

    // Spawn the meter thread: polls both peaks every METER_INTERVAL and
    // emits a Tauri event the UI can subscribe to for VU display.
    let mic_meter = state.mic_meter.clone();
    let system_meter = state.system_meter.clone();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let handle = std::thread::Builder::new()
        .name("steno-meter".into())
        .spawn(move || loop {
            match stop_rx.recv_timeout(METER_INTERVAL) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let payload = AudioLevel {
                        mic: mic_meter.take_peak(),
                        system: system_meter.take_peak(),
                    };
                    let _ = app.emit(METER_EVENT, payload);
                }
            }
        })
        .map_err(|e| format!("spawn meter thread: {e}"))?;
    *state.meter_worker.lock().unwrap() = Some(MeterWorker { stop_tx, handle });

    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<String, String> {
    // Tear down the meter thread first so we don't emit a final dangling
    // event after recording stops.
    if let Some(worker) = state.meter_worker.lock().unwrap().take() {
        let _ = worker.stop_tx.send(());
        let _ = worker.handle.join();
    }

    // Stop mic first to bound capture time. The system stream is
    // best-effort: it may still be delivering buffered samples on the
    // audio thread; we accept whatever it has when we lock.
    let mic_samples = state.mic.lock().unwrap().stop()?;

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    let system_samples = state.system.lock().unwrap().stop().unwrap_or_else(|e| {
        eprintln!("[steno] system stop warning (saving mic only): {e}");
        Vec::new()
    });

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let system_samples: Vec<f32> = Vec::new();

    let has_system = !system_samples.is_empty();
    let mixed = if has_system {
        mixer::mix_mono(&mic_samples, &system_samples)
    } else {
        // Mic-only fallback: don't halve the amplitude.
        mic_samples
    };

    let path = audio_output_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
    }
    let mut writer = OpusOggWriter::new(&path, 1, OPUS_BITRATE_BPS)?;
    writer.encode_pcm(&mixed)?;
    writer.finalize()?;

    Ok(path.to_string_lossy().to_string())
}

fn audio_output_path() -> Result<PathBuf, String> {
    let documents = dirs::document_dir().ok_or("no Documents directory")?;
    let dir = documents.join("Steno").join(".steno").join("audio");
    let stamp = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
    Ok(dir.join(format!("{stamp}.opus")))
}
