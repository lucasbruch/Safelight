import { useEffect, useMemo, useState } from "react";
import { api, plural } from "../api";
import type { HandoffStatus, Item } from "../types";
import { Icon } from "../components/Icon";
import { useEscape } from "../components/useEscape";

type Scope = "picks" | "selected" | "keepers";

interface Props {
  root: string;
  items: Item[];
  selection: Set<number>;
  /** Resolves once every pick and rating made so far is saved. */
  settled: () => Promise<void>;
  onClose: () => void;
}

export default function SendDialog({ root, items, selection, settled, onClose }: Props) {
  const [status, setStatus] = useState<HandoffStatus | null>(null);
  const picks = useMemo(() => items.filter((i) => i.flag === 1 && !i.movedToRejected), [items]);
  const [scope, setScope] = useState<Scope>(picks.length ? "picks" : selection.size ? "selected" : "keepers");
  const [busy, setBusy] = useState<"lr" | "resolve" | null>(null);
  const [result, setResult] = useState<{ text: string; bad?: boolean } | null>(null);

  useEscape(onClose, !busy);
  useEffect(() => void api.handoffStatus().then(setStatus), []);

  const chosen = useMemo(() => {
    if (scope === "picks") return picks;
    if (scope === "selected") return items.filter((i) => selection.has(i.id) && !i.movedToRejected);
    return items.filter((i) => i.flag !== -1 && !i.movedToRejected);
  }, [scope, picks, items, selection]);
  const photos = chosen.filter((i) => i.kind === "photo").length;
  const videos = chosen.length - photos;

  const send = async (to: "lr" | "resolve") => {
    setBusy(to);
    setResult(null);
    try {
      const ids = chosen.map((i) => i.id);
      // The editors read stars, picks and tags from disk: let your last key presses land first.
      await settled();
      const msg = to === "lr" ? await api.sendToLightroom(root, ids) : await api.sendToResolve(root, ids);
      setResult({ text: msg });
    } catch (e) {
      setResult({ text: String(e), bad: true });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label="Send">
        <h2>Send to an editor</h2>
        <div className="field">
          <label>What to send</label>
          <div className="segmented" style={{ alignSelf: "flex-start" }}>
            <button className={scope === "picks" ? "on" : ""} onClick={() => setScope("picks")}>Picks · {picks.length}</button>
            <button className={scope === "selected" ? "on" : ""} onClick={() => setScope("selected")} disabled={!selection.size}>Selected · {selection.size}</button>
            <button className={scope === "keepers" ? "on" : ""} onClick={() => setScope("keepers")}>Everything not rejected</button>
          </div>
          <span className="hint">
            {chosen.length === 0 ? "Nothing chosen yet. Mark some picks with P first." : `${plural(photos, "photo")}${videos ? ` and ${plural(videos, "clip")}` : ""}. Files stay where they are, nothing is copied.`}
          </span>
        </div>

        <div className="send-option">
          <div className="logo">Lr</div>
          <div style={{ flex: 1 }}>
            <b>Lightroom Classic</b>
            <div className="muted" style={{ fontSize: 13 }}>Adds exactly these photos to a Safelight collection. Stars, picks, keywords and copyright come along.</div>
          </div>
          <button className="btn primary" disabled={!status?.lightroom || !photos || !!busy} onClick={() => send("lr")}>
            {busy === "lr" ? <span className="spinner" /> : <Icon name="send" size={15} />} Send
          </button>
        </div>
        {status && !status.lightroom && <div className="hint faint" style={{ fontSize: 12, marginTop: 4 }}>Lightroom Classic isn't installed on this computer.</div>}

        <div className="send-option">
          <div className="logo">Dr</div>
          <div style={{ flex: 1 }}>
            <b>DaVinci Resolve Studio</b>
            <div className="muted" style={{ fontSize: 13 }}>Creates a project named after this one, with bins per day and camera. Opens on the Photo page.</div>
          </div>
          <button className="btn primary" disabled={!status?.resolve || !chosen.length || !!busy} onClick={() => send("resolve")}>
            {busy === "resolve" ? <span className="spinner" /> : <Icon name="send" size={15} />} Send
          </button>
        </div>
        {busy === "resolve" && <div className="hint muted" style={{ fontSize: 12.5, marginTop: 6 }}>Starting Resolve can take a minute…</div>}
        {status && !status.resolve && <div className="hint faint" style={{ fontSize: 12, marginTop: 4 }}>DaVinci Resolve isn't installed on this computer.</div>}

        {result && <div className={`notice ${result.bad ? "bad" : "ok"}`}><Icon name={result.bad ? "info" : "check"} /><div>{result.text}</div></div>}

        <div className="actions">
          <button className="btn" onClick={onClose} disabled={!!busy}>Done</button>
        </div>
      </div>
    </div>
  );
}
