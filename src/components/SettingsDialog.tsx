import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, on } from "../api";
import type { ModelStatus, Settings } from "../types";
import type { ToastMsg } from "./Toast";
import { useEscape } from "./useEscape";

interface Props {
  settings: Settings;
  onClose: () => void;
  onSaved: (s: Settings) => void;
  notify: (t: ToastMsg) => void;
}

export default function SettingsDialog({ settings, onClose, onSaved, notify }: Props) {
  const [s, setS] = useState<Settings>(settings);
  const [models, setModels] = useState<ModelStatus | null>(null);
  const [dl, setDl] = useState<{ done: number; total: number; name: string } | null>(null);
  useEscape(onClose);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((x) => ({ ...x, [k]: v }));

  useEffect(() => {
    api.aiStatus().then(setModels);
    const sub = on("models-progress", setDl);
    return () => void sub.then((u) => u());
  }, []);

  const pick = async (k: "libraryRoot" | "backupRoot", title: string) => {
    const dir = await open({ directory: true, title, defaultPath: s[k] || undefined });
    if (typeof dir === "string") set(k, dir);
  };

  const save = async () => {
    try {
      onSaved(await api.saveSettings(s));
    } catch (e) {
      notify({ text: String(e), bad: true });
    }
  };

  // For bug reports: release builds have no console, so the log file is the record.
  const showLog = async () => {
    try {
      await revealItemInDir(await api.logFile());
    } catch (e) {
      notify({ text: String(e), bad: true });
    }
  };

  const download = async () => {
    setDl({ done: 0, total: 1, name: "" });
    try {
      await api.downloadModels();
      setModels(await api.aiStatus());
      notify({ text: "AI helpers installed. New imports get face, blink and content checks." });
    } catch (e) {
      notify({ text: String(e), bad: true });
    } finally {
      setDl(null);
    }
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-label="Settings">
        <h2>Settings</h2>

        <div className="field">
          <label>Projects folder</label>
          <div className="row">
            <input className="input" value={s.libraryRoot} onChange={(e) => set("libraryRoot", e.target.value)} />
            <button className="btn" onClick={() => pick("libraryRoot", "Where should new projects go?")}>Choose…</button>
          </div>
          <span className="hint">
            New projects are created here.
            {s.libraryRoot.trim() !== settings.libraryRoot.trim() && " Projects in the current folder stay where they are and stay listed."}
          </span>
        </div>

        <div className="field">
          <label>Backup copy (optional)</label>
          <div className="row">
            <input className="input" value={s.backupRoot} placeholder="Off" onChange={(e) => set("backupRoot", e.target.value)} />
            <button className="btn" onClick={() => pick("backupRoot", "Where should backup copies go?")}>Choose…</button>
            {s.backupRoot && <button className="btn ghost" onClick={() => set("backupRoot", "")}>Turn off</button>}
          </div>
          <span className="hint">Every import is also copied and verified here, e.g. an external drive or NAS.</span>
        </div>

        <div className="row" style={{ gap: 12 }}>
          <div className="field" style={{ flex: 1 }}>
            <label>Your name</label>
            <input className="input" value={s.artist} placeholder="Photographer" onChange={(e) => set("artist", e.target.value)} />
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>Copyright</label>
            <input className="input" value={s.copyright} placeholder={`© ${new Date().getFullYear()} ${s.artist || "Your Name"}`} onChange={(e) => set("copyright", e.target.value)} />
          </div>
        </div>
        <span className="hint faint" style={{ fontSize: 12 }}>Written into every imported photo's metadata (XMP), and carried into Lightroom.</span>

        <div className="summary-box" style={{ marginTop: 22 }}>
          <label className="row" style={{ cursor: "pointer", alignItems: "flex-start" }}>
            <input type="checkbox" checked={s.aiEnabled} onChange={(e) => set("aiEnabled", e.target.checked)} style={{ marginTop: 3 }} />
            <div>
              <b>AI helper</b>
              <div className="muted" style={{ fontSize: 13 }}>Checks focus, blur, exposure and bursts. Everything runs on this computer, and nothing is uploaded.</div>
            </div>
          </label>
          {s.aiEnabled && models && !models.installed && (
            <div className="row" style={{ marginTop: 12 }}>
              <div style={{ flex: 1, fontSize: 13 }} className="muted">
                Add face, closed-eye, content and people detection (one-time download, about {models.downloadMb} MB).
              </div>
              <button className="btn" onClick={download} disabled={!!dl}>
                {dl ? <span className="spinner" /> : null} {dl ? "Downloading…" : "Download"}
              </button>
            </div>
          )}
          {dl && dl.total > 0 && (
            <div className="progress thin" style={{ marginTop: 10 }}>
              <div style={{ width: `${(dl.done / dl.total) * 100}%` }} />
            </div>
          )}
          {s.aiEnabled && models?.installed && (
            <div className="muted" style={{ fontSize: 13, marginTop: 8 }}>Face, blink, content and people detection are installed.</div>
          )}
        </div>

        <div className="actions">
          <button className="btn ghost" style={{ marginRight: "auto" }} onClick={showLog}>Show log file</button>
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <button className="btn primary" onClick={save}>Save</button>
        </div>
      </div>
    </div>
  );
}
