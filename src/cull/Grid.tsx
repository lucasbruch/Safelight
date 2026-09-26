import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Item } from "../types";
import { aiHints, FlagBadge, formatDuration, Stars, Thumb } from "./bits";
import { Icon } from "../components/Icon";
import { formatShutter } from "../api";

interface Props {
  root: string;
  items: Item[];
  currentId: number | null;
  selection: Set<number>;
  thumbSize: number;
  onClick: (id: number, e: React.MouseEvent) => void;
  onOpen: (id: number) => void;
  onColumns: (cols: number) => void;
}

const GAP = 8;
const CAPTION = 26;

export default function Grid({ root, items, currentId, selection, thumbSize, onClick, onOpen, onColumns }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [height, setHeight] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);

  useLayoutEffect(() => {
    const el = ref.current!;
    const ro = new ResizeObserver(() => {
      setWidth(el.clientWidth);
      setHeight(el.clientHeight);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const cols = Math.max(1, Math.floor((width - GAP) / (thumbSize + GAP)));
  const cellW = width > 0 ? (width - GAP * (cols + 1)) / cols : thumbSize;
  const cellH = Math.round(cellW * 0.78) + CAPTION;
  const rows = Math.ceil(items.length / cols);
  const totalH = rows * (cellH + GAP) + GAP;

  useEffect(() => onColumns(cols), [cols, onColumns]);

  // Keep the current photo in view when moving with the keyboard.
  useEffect(() => {
    const el = ref.current;
    if (!el || currentId == null) return;
    const idx = items.findIndex((i) => i.id === currentId);
    if (idx < 0) return;
    const top = GAP + Math.floor(idx / cols) * (cellH + GAP);
    if (top < el.scrollTop) el.scrollTop = top - GAP;
    else if (top + cellH > el.scrollTop + el.clientHeight) el.scrollTop = top + cellH - el.clientHeight + GAP;
  }, [currentId, cols, cellH, items]);

  const first = Math.max(0, Math.floor(scrollTop / (cellH + GAP)) - 2);
  const last = Math.min(rows, Math.ceil((scrollTop + height) / (cellH + GAP)) + 2);
  const visible = items.slice(first * cols, last * cols);

  return (
    <div className="grid-scroll" ref={ref} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
      <div style={{ height: totalH, position: "relative" }}>
        {visible.map((it, i) => {
          const idx = first * cols + i;
          const r = Math.floor(idx / cols), c = idx % cols;
          const hints = aiHints(it.ai);
          const cls = ["cell", it.id === currentId && "current", selection.has(it.id) && "selected", it.flag === -1 && "rejected"]
            .filter(Boolean).join(" ");
          return (
            <div
              key={it.id}
              className={cls}
              style={{ left: GAP + c * (cellW + GAP), top: GAP + r * (cellH + GAP), width: cellW, height: cellH }}
              onClick={(e) => onClick(it.id, e)}
              onDoubleClick={() => onOpen(it.id)}
              title={it.ai?.reasons.join(" · ") || undefined}
            >
              <div className="img-wrap"><Thumb root={root} item={it} /></div>
              <FlagBadge flag={it.flag} />
              {hints.length > 0 && (
                <div className="ai-badges">
                  {hints.map((h) => <span key={h.text} className={`ai-badge ${h.tone}`}>{h.text}</span>)}
                </div>
              )}
              {it.kind === "video" && (
                <span className="video-tag"><Icon name="play" size={10} /> {formatDuration(it.meta.duration)}{it.video?.log ? ` · ${it.video.log}` : ""}</span>
              )}
              {(it.ai?.burstSize ?? 0) > 1 && !it.ai?.burstBest && (
                <span className="burst-count" title={`One of a burst of ${it.ai!.burstSize}`}><Icon name="burst" size={11} /> Burst</span>
              )}
              <div className="caption">
                <span className="name">{it.fileName}</span>
                {it.kind === "photo" && (it.meta.shutter || it.meta.aperture) ? (
                  <span className="cap-exp num">{[formatShutter(it.meta.shutter), it.meta.aperture && `f/${it.meta.aperture}`].filter(Boolean).join(" ")}</span>
                ) : null}
                {it.meta.lens && <span className="cap-lens">{it.meta.lens}</span>}
                <Stars n={it.rating} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
