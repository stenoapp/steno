use crate::audio::capture::MicRecorder;
use crate::audio::encode::OpusOggWriter;
use crate::audio::mixer;
use chrono::Local;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::audio::system::SystemAudioRecorder;

const OPUS_BITRATE_BPS: i32 = 32_000;

pub struct AppState {
    pub mic: Mutex<MicRecorder>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub system: Mutex<SystemAudioRecorder>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mic: Mutex::new(MicRecorder::new()),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            system: Mutex::new(SystemAudioRecorder::new()),
        }
    }
}

#[tauri::command]
pub fn start_recording(state: State<AppState>) -> Result<(), String> {
    // Start mic first — it's the indispensable source. If it fails, bail
    // before touching the system stream (avoids misleading
    // screen-recording / monitor-source permission prompts for a mic that's
    // already broken).
    state.mic.lock().unwrap().start()?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if let Err(e) = state.system.lock().unwrap().start() {
            // Wind back the mic so the user can retry with system-audio
            // disabled (M1.5) once we add a toggle. For M1.3/M1.4 hard-fail.
            let _ = state.mic.lock().unwrap().stop();
            return Err(format!(
                "system audio start failed (Screen Recording / PipeWire permission may be required): {e}"
            ));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<String, String> {
    // Stop mic first to bound capture time. The system stream is
    // best-effort: it may still be delivering buffered samples on the
    // audio thread; we accept whatever it has when we lock.
    let mic_samples = state.mic.lock().unwrap().stop()?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let system_samples = state.system.lock().unwrap().stop().unwrap_or_else(|e| {
        eprintln!("[steno] system stop warning (saving mic only): {e}");
        Vec::new()
    });

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
