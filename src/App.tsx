import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface AudioLevel {
  mic: number;
  system: number;
}

function App() {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [lastFile, setLastFile] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [levels, setLevels] = useState<AudioLevel>({ mic: 0, system: 0 });
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!recording) return;
    const start = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 250);
    return () => window.clearInterval(timer);
  }, [recording]);

  useEffect(() => {
    if (!recording) {
      setLevels({ mic: 0, system: 0 });
      return;
    }
    let active = true;
    listen<AudioLevel>("audio-level", (event) => {
      if (active) setLevels(event.payload);
    }).then((unlisten) => {
      if (active) {
        unlistenRef.current = unlisten;
      } else {
        unlisten();
      }
    });
    return () => {
      active = false;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [recording]);

  async function handleStart() {
    setError(null);
    setLastFile(null);
    try {
      await invoke("start_recording");
      setElapsed(0);
      setRecording(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleStop() {
    try {
      const file = await invoke<string>("stop_recording");
      setRecording(false);
      setLastFile(file);
    } catch (e) {
      setError(String(e));
      setRecording(false);
    }
  }

  const mm = String(Math.floor(elapsed / 60)).padStart(2, "0");
  const ss = String(elapsed % 60).padStart(2, "0");

  return (
    <main className="container">
      <h1>Steno</h1>
      <p className="subtitle">M1.5 · audio capture · v0.0.0</p>

      <div className="controls">
        {recording ? (
          <button onClick={handleStop} className="rec stop" aria-label="Stop recording">
            <span className="dot" aria-hidden="true" /> Stop
          </button>
        ) : (
          <button onClick={handleStart} className="rec start" aria-label="Start recording">
            <span className="dot" aria-hidden="true" /> Start
          </button>
        )}

        <p className="timer" aria-live="polite">
          {recording ? `${mm}:${ss}` : "00:00"}
        </p>
      </div>

      {recording && (
        <div className="meters" aria-hidden="true">
          <LevelBar label="mic" level={levels.mic} />
          <LevelBar label="sys" level={levels.system} />
        </div>
      )}

      {lastFile && !recording && (
        <p className="info">
          Saved <code>{lastFile}</code>
        </p>
      )}

      {error && <ErrorBox raw={error} onClear={() => setError(null)} />}
    </main>
  );
}

// Backend tags permission errors with these markers so the UI can offer a
// one-click "Open Settings" button. Keep in sync with commands.rs.
const PERM_MARKERS = {
  microphone: "[STENO_PERM:microphone]",
  "screen-capture": "[STENO_PERM:screen-capture]",
} as const;

type PermKind = keyof typeof PERM_MARKERS;

function ErrorBox({ raw, onClear }: { raw: string; onClear: () => void }) {
  const permKind = (Object.keys(PERM_MARKERS) as PermKind[]).find((k) =>
    raw.includes(PERM_MARKERS[k]),
  );
  // Strip the marker tag from the visible text.
  const visible = permKind ? raw.replace(PERM_MARKERS[permKind], "").trim() : raw;

  async function handleOpen() {
    if (!permKind) return;
    try {
      await invoke("open_permission_panel", { kind: permKind });
      onClear();
    } catch (e) {
      // Linux returns a message instead of opening — surface it.
      // Re-tag-free so we don't loop the button.
      const msg = String(e).replace(PERM_MARKERS[permKind], "").trim();
      // eslint-disable-next-line no-alert
      alert(msg);
    }
  }

  return (
    <p className="error">
      {visible}
      {permKind && (
        <>
          {" "}
          <button type="button" className="link-btn" onClick={handleOpen}>
            Open Settings
          </button>
        </>
      )}
    </p>
  );
}

function LevelBar({ label, level }: { label: string; level: number }) {
  // Map linear 0..1 to a perceptual 0..100% via sqrt — closer to how the
  // ear hears loudness without going full log/dBFS.
  const pct = Math.min(100, Math.max(0, Math.sqrt(Math.max(0, level)) * 100));
  return (
    <div className="meter">
      <span className="meter-label">{label}</span>
      <div className="meter-bar" role="meter" aria-valuemin={0} aria-valuemax={1} aria-valuenow={level}>
        <div className="meter-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export default App;
