import { useState } from "react";
import { api, formatBytes, plural } from "../api";
import type { Report } from "../types";
import type { ToastMsg } from "./Toast";
import { Icon } from "./Icon";
import { useEscape } from "./useEscape";

interface Props {
  report: Report;
  onClose: () => void;
  onOpen: () => void;
  notify: (t: ToastMsg) => void;
}

export default function ReportDialog({ report: r, onClose, onOpen, notify }: Props) {
  const [ejecting, setEjecting] = useState(false);
  const [ejected, setEjected] = useState(false);
  useEscape(onClose);
  const copiedAll = r.failed.length === 0 && !r.cancelled;
  const backupOk = !r.backupRoot || r.backupFailed.length === 0;
  // Only tell anyone to format the card once every file has every copy it should have.
  const ok = copiedAll && backupOk;
  const speed = r.seconds > 0 ? r.bytes / r.seconds : 0;

  const eject = async () => {
    if (!r.cardMount) return;
    setEjecting(true);
    try {
      await api.ejectCard(r.cardMount);
      setEjected(true);
    } catch (e) {
      notify({ text: String(e), bad: true });
    } finally {
      setEjecting(false);
    }
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-label="Import finished">
        <h2>{r.cancelled ? "Import stopped" : ok ? "Import complete" : "Import finished with problems"}</h2>
        <div className="muted">{r.projectName}</div>

        <div className={`notice ${copiedAll ? "ok" : r.cancelled ? "" : "bad"}`} style={{ fontSize: 14 }}>
          <Icon name={copiedAll ? "check" : "info"} />
          <div>
            <b>{plural(r.copied, "file")} copied and verified</b>
            {r.copied > 0 && <> · {formatBytes(r.bytes)} in {Math.max(1, Math.round(r.seconds))} s ({formatBytes(speed)}/s)</>}
            {r.skipped > 0 && <div>{plural(r.skipped, "file")} skipped: already imported before.</div>}
            {r.failed.length > 0 && <div>{plural(r.failed.length, "file")} could not be copied. {r.failed.length ? "They're still safe on the card." : ""}</div>}
          </div>
        </div>

        {r.failed.length > 0 && (
          <div className="summary-box" style={{ maxHeight: 160, overflowY: "auto", fontSize: 12.5 }}>
            {r.failed.map((f) => (
              <div key={f.file} style={{ marginBottom: 6 }}>
                <b>{f.file}</b> <span className="muted">{f.error}</span>
              </div>
            ))}
          </div>
        )}

        {r.backupRoot && (
          <div className={`notice ${r.backupFailed.length ? "bad" : "ok"}`}>
            <Icon name={r.backupFailed.length ? "info" : "check"} />
            <div>
              {r.backupFailed.length
                ? <>
                    <b>The backup is incomplete: {plural(r.backupFailed.length, "file")} couldn't be copied to {r.backupRoot}.</b>{" "}
                    Is the backup drive connected and does it have room? Importing the card again won't redo these,
                    so copy them to the backup yourself before you format the card.
                  </>
                : <>Backup copy verified in {r.backupRoot}</>}
            </div>
          </div>
        )}
        {r.backupFailed.length > 0 && (
          <div className="summary-box" style={{ maxHeight: 120, overflowY: "auto", fontSize: 12.5 }}>
            {r.backupFailed.map((f) => (
              <div key={f.file} style={{ marginBottom: 6 }}>
                <b>{f.file}</b> <span className="muted">{f.error}</span>
              </div>
            ))}
          </div>
        )}

        {r.cardMount && copiedAll && (
          <div className="summary-box row">
            <Icon name="sd" />
            <div style={{ flex: 1 }}>
              {ejected ? <b>You can remove the card now.</b>
                : ok ? <>Everything is safely copied. You can eject the card and format it in your camera.</>
                : <>Everything is in the project, but not yet in the backup. Keep the card until it is; don't format it.</>}
            </div>
            {!ejected && (
              <button className="btn" onClick={eject} disabled={ejecting}>
                {ejecting ? <span className="spinner" /> : <Icon name="eject" size={14} />} Eject card
              </button>
            )}
          </div>
        )}

        <div className="actions">
          <button className="btn ghost" onClick={onClose}>Close</button>
          <button className="btn primary" onClick={onOpen}>Open project</button>
        </div>
      </div>
    </div>
  );
}
