import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, fileUrl, formatBytes, formatDateRange, plural } from "../api";
import type { Card, Progress, ProjectSummary } from "../types";
import type { ImportSource } from "../App";
import type { ToastMsg } from "../components/Toast";
import { Icon } from "../components/Icon";
import { Lamp, Readout } from "../components/Deck";

interface Props {
  cards: Card[];
  progress: Progress | null;
  onImport: (s: ImportSource) => void;
  onOpen: (root: string) => void;
  onSettings: () => void;
  notify: (t: ToastMsg) => void;
}

export default function Home({ cards, progress, onImport, onOpen, onSettings, notify }: Props) {
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);

  useEffect(() => {
    api.listProjects().then(setProjects).catch((e) => notify({ text: String(e), bad: true }));
  }, [notify]);

  const importFolder = async () => {
    const dir = await open({ directory: true, title: "Import photos from a folder" });
    if (typeof dir === "string") onImport({ source: dir, label: dir.split(/[\\/]/).pop() || dir });
  };

  const openFolder = async () => {
    const dir = await open({ directory: true, title: "Open a Safelight project folder" });
    if (typeof dir === "string") onOpen(dir);
  };

  return (
    <div className="home">
      <header className="topbar">
        <div className="brand">
          <Lamp color={progress ? "busy" : "pick"} blink={!!progress} title={progress ? "Importing" : "Ready"} />
          Safelight
        </div>
        <div className="spacer" />
        <button className="btn ghost" onClick={openFolder}><Icon name="folder" size={16} /> Open project folder…</button>
        <button className="btn ghost icon-btn" onClick={onSettings} title="Settings" aria-label="Settings">
          <Icon name="gear" />
        </button>
      </header>

      <div className="home-body">
        <div className="home-inner">
          <section>
            <h2 className="section-title">Ingest</h2>
            <div className="bay-list">
              {progress && (
                <div className="bay active">
                  <Lamp color="busy" blink />
                  <div className="bay-main">
                    <h3>Importing into {progress.projectName}</h3>
                    <div className="progress"><div style={{ width: `${pct(progress)}%` }} /></div>
                  </div>
                  <Readout label="Verified" lamp="pick" lit={progress.done - progress.failed > 0} slot={String(progress.total).length * 2 + 3}
                    title="Copied and checked against the card">{(progress.done - progress.failed).toLocaleString()} / {progress.total.toLocaleString()}</Readout>
                  <Readout label="Copied" slot={15}>{formatBytes(progress.bytesDone)} / {formatBytes(progress.bytesTotal)}</Readout>
                  <Readout label="Failed" lamp="reject" lit={progress.failed > 0} slot={3}
                    title={progress.failed ? "These files stay safe on the card; the report lists them" : "No copy has failed"}>{progress.failed.toLocaleString()}</Readout>
                  <button className="btn primary big" onClick={() => onOpen(progress.projectRoot)}>Start culling</button>
                </div>
              )}
              {cards.map((c) => (
                <div className="bay" key={c.mount}>
                  <Lamp color="pick" title="Card inserted" />
                  <div className="bay-main">
                    <h3>{c.label}</h3>
                    <div className="muted">{c.mount}</div>
                  </div>
                  <Readout label="Used">{formatBytes(c.totalBytes - c.freeBytes)}</Readout>
                  <Readout label="Size">{formatBytes(c.totalBytes)}</Readout>
                  <button
                    className="btn ghost"
                    onClick={() =>
                      api.ejectCard(c.mount).then(
                        () => notify({ text: `${c.label} can be removed now.` }),
                        (e) => notify({ text: String(e), bad: true }),
                      )
                    }
                  >
                    <Icon name="eject" size={14} /> Eject
                  </button>
                  <button
                    className={`btn big ${progress ? "" : "primary"}`}
                    disabled={!!progress}
                    title={progress ? "One import at a time. This card can be imported when the current one finishes." : undefined}
                    onClick={() => onImport({ source: c.mount, label: c.label, cardMount: c.mount })}
                  >
                    Import…
                  </button>
                </div>
              ))}
              {cards.length === 0 && (
                <div className="bay empty-bay">
                  <Lamp color="white" lit={false} />
                  <div className="bay-main">
                    <h3>Insert a camera card</h3>
                    <div className="muted">Safelight notices it and asks where the photos should go.</div>
                  </div>
                  <button className="btn" onClick={importFolder} disabled={!!progress}>
                    <Icon name="folder" size={16} /> Import from a folder…
                  </button>
                </div>
              )}
            </div>
          </section>

          <section>
            <h2 className="section-title">Projects{projects?.length ? <span className="section-count num">{projects.length}</span> : null}</h2>
            {projects === null ? null : projects.length === 0 ? (
              <div className="empty">No projects yet. Your first import shows up here.</div>
            ) : (
              <div className="projects">
                {projects.map((p) => (
                  <button className="project-card" key={p.root} onClick={() => onOpen(p.root)}>
                    <div
                      className="project-cover"
                      style={p.cover ? { backgroundImage: `url("${fileUrl(p.cover)}")` } : undefined}
                    >
                      {!p.cover && <Icon name="folder" size={26} />}
                    </div>
                    <div className="project-meta">
                      <h3 title={p.name}>{p.name}</h3>
                      <div className="project-date">{formatDateRange(p.firstDate, p.lastDate)}</div>
                      <div className="stats">
                        <span title={plural(p.photos, "photo") + (p.videos ? ` and ${plural(p.videos, "clip")}` : "")}>
                          <span className="engraved">Items</span><b className="num">{p.total.toLocaleString()}</b>
                        </span>
                        <span title={plural(p.picks, "pick")}>
                          <span className="engraved"><Lamp color="pick" lit={p.picks > 0} /> Picks</span><b className="num">{p.picks.toLocaleString()}</b>
                        </span>
                        <span title={plural(p.rejects, "reject")}>
                          <span className="engraved"><Lamp color="reject" lit={p.rejects > 0} /> Rejects</span><b className="num">{p.rejects.toLocaleString()}</b>
                        </span>
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

function pct(p: Progress) {
  return p.bytesTotal ? Math.min(100, (p.bytesDone / p.bytesTotal) * 100) : 0;
}
