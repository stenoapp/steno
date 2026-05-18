use crate::audio::capture::MicRecorder;
use chrono::Local;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub recorder: Mutex<MicRecorder>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recorder: Mutex::new(MicRecorder::new()),
        }
    }
}

#[tauri::command]
pub fn start_recording(state: State<AppState>) -> Result<(), String> {
    state.recorder.lock().unwrap().start()
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<String, String> {
    let path = audio_output_path()?;
    let written = state.recorder.lock().unwrap().stop(&path)?;
    Ok(written.to_string_lossy().to_string())
}

fn audio_output_path() -> Result<PathBuf, String> {
    let documents = dirs::document_dir().ok_or("no Documents directory")?;
    let dir = documents.join("Steno").join(".steno").join("audio");
    let stamp = Local::now().format("%Y-%m-%d-%H%M").to_string();
    Ok(dir.join(format!("{stamp}-mic.opus")))
}
