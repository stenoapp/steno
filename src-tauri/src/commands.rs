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

/// String tag in error messages that lets the frontend offer a one-click
/// "Open System Settings" button via the open_permission_panel command.
/// Keep in sync with frontend's permission-error detector.
const PERM_HINT_MIC: &str = "[STENO_PERM:microphone]";
const PERM_HINT_SCREEN: &str = "[STENO_PERM:screen-capture]";

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
    if let Err(e) = state.mic.lock().unwrap().start() {
        return Err(humanize_mic_error(e));
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        if let Err(e) = state.system.lock().unwrap().start() {
            // Wind back the mic so the user can retry with system-audio
            // disabled (M1.5) once we add a toggle. Hard-fail for now.
            let _ = state.mic.lock().unwrap().stop();
            return Err(humanize_system_error(e));
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
    // Stream the mixed PCM into the encoder. feed() can be called multiple
    // times — for now we hand it the whole mix in one shot; the worker-
    // thread streaming architecture that uses feed() during capture lives
    // in the M7 backlog (see plan/pending.md).
    let mut writer = OpusOggWriter::new(&path, 1, OPUS_BITRATE_BPS)?;
    writer.feed(&mixed)?;
    writer.finalize()?;

    Ok(path.to_string_lossy().to_string())
}

fn audio_output_path() -> Result<PathBuf, String> {
    let documents = dirs::document_dir().ok_or("no Documents directory")?;
    let dir = documents.join("Steno").join(".steno").join("audio");
    let stamp = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
    Ok(dir.join(format!("{stamp}.opus")))
}

// ──────────────────────────────────────────────────────────────────────
// Permission UX (M1.5)
//
// The system-audio path can fail on each OS for reasons that look like
// "permission denied" once you know what to look for:
//   - macOS: ScreenCaptureKit needs Screen Recording perm
//   - macOS: cpal needs Microphone perm
//   - Linux: PipeWire daemon not running, or no monitor source available
//   - Windows: WASAPI device busy, or mic privacy setting off
// We rewrite raw errors to actionable text and tag them with a marker
// the frontend uses to surface an "Open Settings" button.

fn humanize_mic_error(raw: String) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("no default input device") {
        return format!("No input device found. Plug in a mic or check OS audio settings. {PERM_HINT_MIC} (raw: {raw})");
    }
    if lower.contains("permission") || lower.contains("not authorized") {
        return format!("Microphone permission denied. {PERM_HINT_MIC} (raw: {raw})");
    }
    // Unknown error — pass through but still tag it so the UI can offer
    // the help button. Microphone perms are the single most common cause
    // of mystery cpal failures.
    format!("Microphone start failed. {PERM_HINT_MIC} (raw: {raw})")
}

fn humanize_system_error(raw: String) -> String {
    let lower = raw.to_lowercase();
    #[cfg(target_os = "macos")]
    {
        if lower.contains("scshareablecontent") || lower.contains("screen recording") {
            return format!("Screen Recording permission needed for system audio. {PERM_HINT_SCREEN} (raw: {raw})");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if lower.contains("pipewire") || lower.contains("connect") {
            return format!("PipeWire daemon doesn't appear to be running. Try: systemctl --user start pipewire pipewire-pulse wireplumber. (raw: {raw})");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if lower.contains("device") && (lower.contains("busy") || lower.contains("in use")) {
            return format!("Default audio output is busy. Close other apps that may be using it exclusively. (raw: {raw})");
        }
    }
    format!("System audio start failed. {PERM_HINT_SCREEN} (raw: {raw})")
}

/// Opens the OS-native system settings panel for a given permission kind.
/// `kind` is currently one of "microphone" or "screen-capture". The frontend
/// invokes this from the error toast's "Open Settings" button.
#[tauri::command]
pub fn open_permission_panel(kind: String) -> Result<(), String> {
    match kind.as_str() {
        "microphone" => open_mic_settings(),
        "screen-capture" => open_screen_capture_settings(),
        other => Err(format!("unknown permission kind: {other}")),
    }
}

#[cfg(target_os = "macos")]
fn open_mic_settings() -> Result<(), String> {
    spawn_open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
}
#[cfg(target_os = "macos")]
fn open_screen_capture_settings() -> Result<(), String> {
    spawn_open("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
}

#[cfg(target_os = "windows")]
fn open_mic_settings() -> Result<(), String> {
    spawn_open("ms-settings:privacy-microphone")
}
#[cfg(target_os = "windows")]
fn open_screen_capture_settings() -> Result<(), String> {
    // Windows has no Screen Recording panel; loopback capture isn't
    // permission-gated. Open the general privacy page.
    spawn_open("ms-settings:privacy")
}

#[cfg(target_os = "linux")]
fn open_mic_settings() -> Result<(), String> {
    // Best-effort: most desktops use gnome-control-center sound; the URL
    // is desktop-specific so we don't try to guess. Tell the user what to
    // look for instead.
    Err("On Linux, microphone access is configured per app via your desktop's privacy/sound settings. Look for an Input or Microphone panel.".into())
}
#[cfg(target_os = "linux")]
fn open_screen_capture_settings() -> Result<(), String> {
    Err("On Linux, system audio capture is handled by PipeWire — there is no OS-level permission panel. Confirm the daemon is running: systemctl --user status pipewire.".into())
}

#[cfg(target_os = "macos")]
fn spawn_open(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open {url}: {e}"))
}
#[cfg(target_os = "windows")]
fn spawn_open(url: &str) -> Result<(), String> {
    // `start` is a cmd builtin; an empty title is required so the URL
    // isn't interpreted as the window title.
    std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("start {url}: {e}"))
}
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn spawn_open(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("xdg-open {url}: {e}"))
}
