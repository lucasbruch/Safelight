import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, fileUrl, playbackUrl, previewPath } from "../api";
import type { Item } from "../types";
import { aiHints, FlagBadge, Stars } from "./bits";

export type Center = { x: number; y: number };

const fullCache = new Map<string, string>();

function useFullImage(root: string, item: Item, enabled: boolean): string | null {
  const key = `${root}|${item.id}`;
  // Remember which photo the loaded URL belongs to: after moving to the next
  // photo, the previous one's full-size image must never be shown for it.
  const [loaded, setLoaded] = useState<{ key: string; src: string } | null>(null);
  useEffect(() => {
    if (!enabled || item.kind !== "photo" || fullCache.has(key)) return;
    let alive = true;
    api.fullImage(root, item.id).then((p) => {
      const u = fileUrl(p);
      fullCache.set(key, u);
      if (alive) setLoaded({ key, src: u });
    }).catch(() => {});
    return () => { alive = false; };
  }, [key, enabled, root, item.id, item.kind]);
  if (!enabled) return null;
  return fullCache.get(key) ?? (loaded?.key === key ? loaded.src : null);
}

/** Pixel size of the photo at 100% (upright). */
function naturalSize(item: Item): { w: number; h: number } | null {
  const { width, height, orientation } = item.meta;
  if (!width || !height) return null;
  return (orientation ?? 1) >= 5 ? { w: height, h: width } : { w: width, h: height };
}

interface ZoomProps {
  root: string;
  item: Item;
  zoom: boolean;
  center: Center;
  onCenter: (c: Center) => void;
  onToggleZoom: (c: Center) => void;
}

/** A photo that fits the pane, or shows at 100% around `center` and pans by dragging. */
export function ZoomImage({ root, item, zoom, center, onCenter, onToggleZoom }: ZoomProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const full = useFullImage(root, item, zoom);
  const [loadedSize, setLoadedSize] = useState<{ w: number; h: number } | null>(null);
  const drag = useRef<{ x: number; y: number; c: Center; moved: boolean } | null>(null);

  useLayoutEffect(() => {
    const el = ref.current!;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => setLoadedSize(null), [item.id]);

  const preview = fileUrl(previewPath(root, item.id), item.previewState);
  const nat = loadedSize ?? naturalSize(item) ?? { w: box.w * 2, h: box.h * 2 };

  // Where a click landed, as a fraction of the (fitted) image.
  const pointToCenter = (e: React.MouseEvent): Center => {
    const r = ref.current!.getBoundingClientRect();
    const s = Math.min(box.w / nat.w, box.h / nat.h);
    const [dw, dh] = [nat.w * s, nat.h * s];
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left - (box.w - dw) / 2) / dw)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top - (box.h - dh) / 2) / dh)),
    };
  };

  const onDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    drag.current = { x: e.clientX, y: e.clientY, c: center, moved: false };
  };
  const onMove = (e: React.MouseEvent) => {
    const d = drag.current;
    if (!d || !zoom) return;
    const dx = e.clientX - d.x, dy = e.clientY - d.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
    onCenter({ x: clamp01(d.c.x - dx / nat.w), y: clamp01(d.c.y - dy / nat.h) });
  };
  const onUp = (e: React.MouseEvent) => {
    const d = drag.current;
    drag.current = null;
    if (d && !d.moved) onToggleZoom(zoom ? center : pointToCenter(e));
  };

  let style: React.CSSProperties = {};
  if (zoom) {
    // Keep the image covering the pane where possible.
    const left = clampPos(box.w / 2 - center.x * nat.w, box.w, nat.w);
    const top = clampPos(box.h / 2 - center.y * nat.h, box.h, nat.h);
    style = { position: "absolute", left, top, width: nat.w, height: nat.h, maxWidth: "none", maxHeight: "none" };
  }

  return (
    <div
      ref={ref}
      className={`loupe ${zoom ? "zoomed" : ""}`}
      style={{ cursor: zoom ? (drag.current ? "grabbing" : "grab") : "zoom-in" }}
      onMouseDown={onDown}
      onMouseMove={onMove}
      onMouseUp={onUp}
      onMouseLeave={() => (drag.current = null)}
    >
      {item.kind === "video" ? (
        <VideoPlayer root={root} item={item} poster={preview} />
      ) : (
        <>
          <img src={preview} style={style} alt="" draggable={false} />
          {zoom && full && (
            <img
              src={full}
              style={style}
              alt=""
              draggable={false}
              onLoad={(e) => setLoadedSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
            />
          )}
          {zoom && !full && <div className="loading pill"><span className="spinner" /> Loading full resolution…</div>}
        </>
      )}
    </div>
  );
}

function VideoPlayer({ root, item, poster }: { root: string; item: Item; poster: string }) {
  const url = playbackUrl(root, item);
  const pending = item.video?.playback === "pending" || (item.previewState === 0 && !item.video);
  // Looking at a clip whose playback copy isn't ready yet: make it next in line.
  useEffect(() => {
    if (item.video?.playback === "pending") api.requestProxy(root, item.id).catch(() => {});
  }, [root, item.id, item.video?.playback]);
  const stop = (e: React.MouseEvent) => e.stopPropagation();
  if (url) {
    return (
      <video key={url} src={url} poster={item.previewState === 1 ? poster : undefined} controls autoPlay
        onMouseDown={stop} onMouseUp={stop} />
    );
  }
  return (
    <>
      {item.previewState === 1 && <img src={poster} alt="" draggable={false} />}
      <div className="loading pill">
        {pending ? <><span className="spinner" /> Preparing playback…</> :
          "This clip can't be played in Safelight (is ffmpeg missing?). It's safely imported, so open it in Resolve."}
      </div>
    </>
  );
}

function clamp01(v: number) {
  return Math.min(1, Math.max(0, v));
}
function clampPos(pos: number, box: number, size: number) {
  if (size <= box) return (box - size) / 2;
  return Math.min(0, Math.max(box - size, pos));
}

interface CompareProps {
  root: string;
  items: Item[];
  currentId: number | null;
  onSelect: (id: number) => void;
  zoom: boolean;
  setZoom: (z: boolean) => void;
}

/** 2–4 photos side by side; zoom and panning are linked across all of them. */
export function Compare({ root, items, currentId, onSelect, zoom, setZoom }: CompareProps) {
  const [center, setCenter] = useState<Center>({ x: 0.5, y: 0.5 });
  const cols = items.length === 3 ? 3 : 2;
  return (
    <div className="compare" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)`, gridTemplateRows: items.length > 2 && cols === 2 ? "1fr 1fr" : "1fr" }}>
      {items.map((it) => (
        <div
          key={it.id}
          className={`compare-pane ${it.id === currentId ? "current" : ""}`}
          onMouseDownCapture={() => onSelect(it.id)}
        >
          <ZoomImage
            root={root}
            item={it}
            zoom={zoom}
            center={center}
            onCenter={setCenter}
            onToggleZoom={(c) => {
              setCenter(c);
              setZoom(!zoom);
            }}
          />
          <div className="label">
            <FlagBadge flag={it.flag} small />
            <span>{it.fileName}</span>
            <Stars n={it.rating} />
            {aiHints(it.ai).map((h) => <span key={h.text} className={`ai-badge ${h.tone}`}>{h.text}</span>)}
          </div>
        </div>
      ))}
    </div>
  );
}
