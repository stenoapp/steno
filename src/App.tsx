import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [lastFile, setLastFile] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const start = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000));
    }, 250);
    return () => window.clearInterval(timer);
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
      <p className="subtitle">M1.1 · mic capture · v0.0.0</p>

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

      {lastFile && !recording && (
        <p className="info">
          Saved <code>{lastFile}</code>
        </p>
      )}

      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
