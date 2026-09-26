import { useEffect, useMemo, useRef, useState } from "react";
import { api, formatBytes, formatDateRange, joinPath, on, plural } from "../api";
import type { ProjectSummary, ScanSummary, Settings } from "../types";
import type { ImportSource } from "../App";
import { Icon } from "./Icon";
import { useEscape } from "./useEscape";
import { Readout } from "./Deck";

interface Props {
  source: ImportSource;
  settings: Settings;
  onClose: () => void;
  onStarted: (projectRoot: string) => void;
  /** Called instead of showing the dialog when an auto-opened card has nothing new. */
  onNothingNew?: (label: string) => void;
}

export default function ImportDialog({ source, settings, onClose, onStarted, onNothingNew }: Props) {
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [scanned, setScanned] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"new" | "existing">("new");
  const [name, setName] = useState("");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [existing, setExisting] = useState("");
  const [again, setAgain] = useState(false);
  const [starting, setStarting] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  useEscape(onClose);

  useEffect(() => {
    const sub = on("scan-progress", (p) => p.source === source.source && setScanned(p));
    api.scanSource(source.source, source.label).then((s) => {
      if (onNothingNew && s.newPhotos + s.newVideos === 0) onNothingNew(source.label);
      else setScan(s);
    }, (e) => setError(String(e)));
    api.listProjects().then((ps) => {
      setProjects(ps);
      if (ps[0]) setExisting(ps[0].root);
    });
    return () => void sub.then((u) => u());
  }, [source]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (scan) nameRef.current?.focus();
  }, [scan]);

  const count = scan ? (again ? scan.photos + scan.videos : scan.newPhotos + scan.newVideos) : 0;
  const bytes = scan ? (again ? scan.bytes : scan.newBytes) : 0;
  // Mirrors `ingest::resolve_root`, so the path shown is the folder that gets created.
  const folderPreview = useMemo(() => {
    const n = name.trim();
    if (!n || !scan) return null;
    const date = (again ? scan.firstDate : scan.firstNewDate) ?? localToday();
    const hasDate = /^\d{4}-\d{2}-\d{2}/.test(n) && !isNaN(Date.parse(n.slice(0, 10)));
    return joinPath(settings.libraryRoot, sanitizeComponent(hasDate ? n : `${date}_${n}`));
  }, [name, scan, again, settings.libraryRoot]);

  const canImport = scan && count > 0 && !starting && (mode === "new" ? name.trim().length > 0 : !!existing);

  const start = async () => {
    if (!canImport) return;
    setStarting(true);
    try {
      const root = await api.startImport({
        source: source.source,
        cardMount: source.cardMount,
        includeDuplicates: again,
        ...(mode === "new" ? { newName: name.trim() } : { projectRoot: existing }),
      });
      onStarted(root);
    } catch (e) {
      setError(String(e));
      setStarting(false);
    }
  };

  return (
    <div className="overlay" style={!scan && !error && onNothingNew ? { visibility: "hidden" } : undefined} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-label="Import">
        <div className="row" style={{ alignItems: "flex-start" }}>
          <div style={{ flex: 1 }}>
            <h2>Import from {source.label}</h2>
            <div className="muted" style={{ fontSize: 13 }}>{source.source}</div>
          </div>
          <button className="btn ghost icon-btn" onClick={onClose} aria-label="Close"><Icon name="x" /></button>
        </div>

        {error && <div className="notice bad">{error}</div>}

        {!scan && !error && (
          <div className="summary-box">
            <div className="row"><span className="spinner" /> Looking at your files…</div>
            {scanned && scanned.total > 0 && (
              <div className="progress thin" style={{ marginTop: 12 }}>
                <div style={{ width: `${(scanned.done / scanned.total) * 100}%` }} />
              </div>
            )}
          </div>
        )}

        {scan && (
          <>
            <div className="summary-box">
              {scan.photos + scan.videos === 0 ? (
                <div className="muted">No photos or videos found here.</div>
              ) : (
                <div className="readout-grid">
                  <Readout label="Photos">{scan.photos.toLocaleString()}</Readout>
                  {scan.videos > 0 && <Readout label="Clips">{scan.videos.toLocaleString()}</Readout>}
                  <Readout label="Size">{formatBytes(scan.bytes)}</Readout>
                  {scan.firstDate && <Readout label="Shot">{formatDateRange(scan.firstDate, scan.lastDate)}</Readout>}
                  <Readout label={scan.cameras.length === 1 ? "Camera" : "Cameras"}>{scan.cameras.join(", ") || "Unknown"}</Readout>
                </div>
              )}
            </div>

            {scan.alreadyImported > 0 && (
              <div className={`notice ${count === 0 ? "ok" : ""}`}>
                <Icon name="info" />
                <div>
                  {plural(scan.alreadyImported, "file")} {scan.alreadyImported === 1 ? "was" : "were"} imported before
                  {scan.alreadyIn.length > 0 && <> (into <b>{scan.alreadyIn.join(", ")}</b>)</>}.{" "}
                  {again ? "They'll be copied again." : count > 0 ? `Only the ${count} new ones will be copied.` : "There's nothing new on this card."}
                  <label className="row" style={{ marginTop: 8, gap: 8, cursor: "pointer" }}>
                    <input type="checkbox" checked={again} onChange={(e) => setAgain(e.target.checked)} />
                    Import them again anyway
                  </label>
                </div>
              </div>
            )}

            {count > 0 && (
              <>
                <div className="field" style={{ marginTop: 22 }}>
                  <div className="segmented" style={{ alignSelf: "flex-start" }}>
                    <button className={mode === "new" ? "on" : ""} onClick={() => setMode("new")}>New project</button>
                    <button
                      className={mode === "existing" ? "on" : ""}
                      onClick={() => setMode("existing")}
                      disabled={projects.length === 0}
                    >
                      Add to existing
                    </button>
                  </div>
                </div>
                {mode === "new" ? (
                  <div className="field">
                    <label htmlFor="pname">Project name</label>
                    <input
                      id="pname"
                      ref={nameRef}
                      className="input big"
                      placeholder="e.g. Wedding Smith"
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && start()}
                    />
                    {folderPreview && <span className="hint">Saved to {folderPreview}</span>}
                  </div>
                ) : (
                  <div className="field">
                    <label htmlFor="pexisting">Project</label>
                    <select id="pexisting" className="input" value={existing} onChange={(e) => setExisting(e.target.value)}>
                      {projects.map((p) => <option key={p.root} value={p.root}>{p.name}</option>)}
                    </select>
                  </div>
                )}
                <div className="muted" style={{ fontSize: 12.5, marginTop: 12 }}>
                  Sorted into <b>date / camera</b> folders. Every file is checked after copying and your card is never changed.
                  {settings.backupRoot && <> A backup copy goes to <b>{settings.backupRoot}</b>.</>}
                </div>
              </>
            )}
          </>
        )}

        <div className="actions">
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <button className="btn primary big" disabled={!canImport} onClick={start}>
            {starting ? <span className="spinner" /> : null}
            {count > 0 ? `Import ${plural(count, "file")} · ${formatBytes(bytes)}` : "Import"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Same rules as `meta::sanitize_component` in the backend. */
function sanitizeComponent(raw: string): string {
  const out = raw
    .replace(/[<>:"/\\|?*\s\p{Cc}]/gu, "-")
    .replace(/-+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "");
  return out || "Untitled";
}

/** Today in local time (what the backend uses), not UTC. */
function localToday(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
