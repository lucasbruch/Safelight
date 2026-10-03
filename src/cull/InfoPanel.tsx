import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { formatBytes, formatDate, formatShutter, mediaPath } from "../api";
import type { Item } from "../types";
import { Stars } from "./bits";
import { Icon } from "../components/Icon";
import { Lamp } from "../components/Deck";

interface Props {
  root: string;
  item: Item | null;
  count: number;
  /** The photos a tag edit applies to: the selection, or just the current photo. */
  tagTargets: Item[];
  tagSuggestions: string[];
  onEditTags: (add: string[], remove: string[]) => void;
}

export default function InfoPanel({ root, item, count, tagTargets, tagSuggestions, onEditTags }: Props) {
  if (!item) return <aside className="sidebar muted">Nothing selected.</aside>;
  const m = item.meta;
  const ai = item.ai;
  const exposure = [formatShutter(m.shutter), m.aperture && `f/${m.aperture}`, m.iso && `ISO\u00a0${m.iso}`, m.focal && `${Math.round(m.focal)}\u00a0mm`]
    .filter(Boolean).join("  ·  ");

  return (
    <aside className="sidebar info">
      {count > 1 && <div className="pill" style={{ marginBottom: 10 }}>{count} selected</div>}
      <h3>{item.fileName}</h3>
      {(item.flag !== 0 || item.rating > 0) && (
        <div className="row" style={{ gap: 10, marginTop: 6 }}>
          {item.flag === 1 && <span className="state-label pick"><Lamp color="pick" /> Pick</span>}
          {item.flag === -1 && <span className="state-label reject"><Lamp color="reject" /> Reject</span>}
          <Stars n={item.rating} size={13} />
        </div>
      )}

      <TagEditor items={tagTargets.length ? tagTargets : [item]} suggestions={tagSuggestions} onEdit={onEditTags} />

      {ai && item.aiState === 1 && (
        <div className="block">
          <h4>AI helper</h4>
          <div style={{ fontWeight: 600, color: ai.suggestion === 1 ? "var(--pick)" : ai.suggestion === -1 ? "var(--reject)" : "var(--muted)" }}>
            {ai.suggestion === 1 ? "Looks like a keeper" : ai.suggestion === -1 ? "Probably a reject" : "No strong opinion"}
          </div>
          {ai.reasons.length > 0 && (
            <div className="reason-list" style={{ marginTop: 8 }}>
              {ai.reasons.map((r) => <span className="tag" key={r}>{r}</span>)}
            </div>
          )}
          <dl>
            <dt>Sharpness</dt>
            <dd>
              <span className="num">{Math.round(ai.sharpness ?? 0)}</span> <span className="muted">/ 100</span>
              <SegmentMeter value={ai.sharpness ?? 0} tone={ai.blurLevel === 0 ? "pick" : ai.blurLevel === 1 ? "star" : "reject"} />
            </dd>
            {ai.faces != null && <><dt>Faces</dt><dd>{ai.faces}{(ai.eyesClosed ?? 0) > 0 ? ` · ${ai.eyesClosed} blinking` : ""}</dd></>}
            {ai.aesthetic != null && (
              <><dt>Look</dt><dd><span className="num">{ai.aesthetic.toFixed(1)}</span> <span className="muted">/ 10</span><SegmentMeter value={ai.aesthetic * 10} tone="white" /></dd></>
            )}
            <dt>Highlights</dt><dd><span className="num">{pct(ai.overExposed)}</span> <span className="muted">clipped</span><SegmentMeter value={(ai.overExposed ?? 0) * 100} tone={(ai.overExposed ?? 0) > 0.25 ? "reject" : "white"} /></dd>
            <dt>Shadows</dt><dd><span className="num">{pct(ai.underExposed)}</span> <span className="muted">black</span><SegmentMeter value={(ai.underExposed ?? 0) * 100} tone={(ai.underExposed ?? 0) > 0.5 ? "reject" : "white"} /></dd>
            {ai.burstSize && <><dt>Burst</dt><dd>{ai.burstBest ? `Best of ${ai.burstSize}` : `1 of ${ai.burstSize}`}</dd></>}
            {ai.people.length > 0 && <><dt>People</dt><dd>{ai.people.map((p) => `Person ${p}`).join(", ")}</dd></>}
          </dl>
          {ai.tags.length > 0 && (
            <div className="reason-list" style={{ marginTop: 10 }}>
              {ai.tags.map((t) => <span className="tag" key={t}>{t}</span>)}
            </div>
          )}
        </div>
      )}
      {item.kind === "photo" && item.aiState === 0 && (
        <div className="block muted" style={{ fontSize: 13 }}><span className="spinner" style={{ width: 12, height: 12, verticalAlign: -2 }} /> AI is still checking this photo…</div>
      )}

      <div className="block">
        <h4>{item.kind === "video" ? "Video" : "Photo"}</h4>
        <dl>
          <dt>Taken</dt><dd>{formatDate(item.capturedAt, true)}</dd>
          <dt>Camera</dt><dd>{item.camera}</dd>
          {m.lens && <><dt>Lens</dt><dd>{m.lens}</dd></>}
          {exposure && <><dt>Exposure</dt><dd>{exposure}</dd></>}
          {item.kind === "video" && m.duration ? <><dt>Length</dt><dd>{Math.floor(m.duration / 60)}:{String(Math.round(m.duration % 60)).padStart(2, "0")}</dd></> : null}
          {item.video?.log && <><dt>Log</dt><dd>{item.video.log} <span className="muted">· shown with a normal look here; the file itself is untouched</span></dd></>}
          {m.flash != null && <><dt>Flash</dt><dd>{m.flash ? "Fired" : "Didn't fire"}</dd></>}
          {m.width && m.height && <><dt>Size</dt><dd>{m.width} × {m.height} · {((m.width * m.height) / 1e6).toFixed(1)} MP</dd></>}
          <dt>File</dt><dd>{formatBytes(item.size)}</dd>
          {m.gpsLat != null && m.gpsLon != null && <><dt>Location</dt><dd>{m.gpsLat.toFixed(4)}, {m.gpsLon.toFixed(4)}</dd></>}
          <dt>Folder</dt><dd className="muted">{item.movedToRejected ? "_Rejected/" : ""}{item.relPath.split("/").slice(0, -1).join(" / ")}</dd>
        </dl>
        <button className="btn small" style={{ marginTop: 12 }} onClick={() => revealItemInDir(mediaPath(root, item))}>
          <Icon name="folder" size={14} /> Show in folder
        </button>
      </div>
    </aside>
  );
}

/** Your own tags. With several photos selected it lists every tag on any of them;
 *  a dashed tag is only on some, and adding or removing applies to all. */
function TagEditor({ items, suggestions, onEdit }: { items: Item[]; suggestions: string[]; onEdit: (add: string[], remove: string[]) => void }) {
  const [text, setText] = useState("");
  const counts = new Map<string, number>();
  for (const it of items) for (const t of it.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
  const tags = [...counts.keys()];
  const has = (s: string) => tags.some((t) => t.toLowerCase() === s.toLowerCase());
  const add = () => {
    if (!text.trim()) return;
    onEdit([text], []);
    setText("");
  };
  return (
    <div className="block">
      <h4>Tags{items.length > 1 ? ` · ${items.length} selected` : ""}</h4>
      {tags.length > 0 && (
        <div className="reason-list" style={{ marginBottom: 8 }}>
          {tags.map((t) => {
            const n = counts.get(t)!;
            return (
              <span className={`tag mine ${n < items.length ? "partial" : ""}`} key={t} title={n < items.length ? `On ${n} of ${items.length} selected` : undefined}>
                {t}
                <button aria-label={`Remove tag ${t}`} title="Remove" onClick={() => onEdit([], [t])}><Icon name="x" size={10} /></button>
              </span>
            );
          })}
        </div>
      )}
      <input
        id="tag-input"
        className="input tag-input"
        list="tag-suggestions"
        placeholder={items.length > 1 ? `Add a tag to ${items.length} photos` : "Add a tag"}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); add(); }
          else if (e.key === "Escape") { setText(""); e.currentTarget.blur(); }
        }}
      />
      <datalist id="tag-suggestions">
        {suggestions.filter((s) => !has(s)).slice(0, 200).map((s) => <option key={s} value={s} />)}
      </datalist>
    </div>
  );
}

/** A 20-segment level meter, like the ones on a mixing desk. */
function SegmentMeter({ value, tone }: { value: number; tone: "pick" | "star" | "reject" | "white" }) {
  const on = Math.round(Math.max(0, Math.min(100, value)) / 5);
  return (
    <div className={`seg-meter ${tone}`} role="meter" aria-valuenow={Math.round(value)} aria-valuemin={0} aria-valuemax={100}>
      {Array.from({ length: 20 }, (_, i) => <i key={i} className={i < on ? "on" : ""} />)}
    </div>
  );
}

function pct(v?: number) {
  if (v == null) return "–";
  return v < 0.001 ? "0%" : `${(v * 100).toFixed(v < 0.1 ? 1 : 0)}%`;
}
