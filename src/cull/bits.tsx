import { fileUrl, thumbPath } from "../api";
import type { Ai, Item } from "../types";
import { Icon } from "../components/Icon";
import { Lamp } from "../components/Deck";

export function Stars({ n, size = 11 }: { n: number; size?: number }) {
  if (n <= 0) return null;
  return (
    <span className="stars" aria-label={`${n} star${n > 1 ? "s" : ""}`}>
      {[1, 2, 3, 4, 5].map((i) => <Icon key={i} name="star" size={size} className={i <= n ? undefined : "off"} />)}
    </span>
  );
}

/** A lamp plate on the frame: lit green for a pick, red for a reject. */
export function FlagBadge({ flag, small }: { flag: number; small?: boolean }) {
  if (flag === 0) return null;
  const pick = flag === 1;
  return (
    <div className={`flag-badge ${pick ? "pick" : "reject"} ${small ? "small" : ""}`} title={pick ? "Pick" : "Reject"}>
      <Lamp color={pick ? "pick" : "reject"} />
      {!small && <span>{pick ? "Pick" : "Rej"}</span>}
    </div>
  );
}

/** The one or two most useful AI hints for a thumbnail. */
export function aiHints(ai: Ai | null): { text: string; tone: "good" | "bad" | "warn" }[] {
  if (!ai) return [];
  const out: { text: string; tone: "good" | "bad" | "warn" }[] = [];
  if (ai.burstBest && ai.burstSize) out.push({ text: `Best of ${ai.burstSize}`, tone: "good" });
  if ((ai.eyesClosed ?? 0) > 0) out.push({ text: "Eyes closed", tone: "bad" });
  if (ai.blurLevel === 2) out.push({ text: ai.motionBlur ? "Motion blur" : "Blurry", tone: "bad" });
  else if (ai.blurLevel === 1) out.push({ text: "Soft", tone: "warn" });
  if ((ai.overExposed ?? 0) > 0.25) out.push({ text: "Blown out", tone: "bad" });
  else if ((ai.underExposed ?? 0) > 0.5) out.push({ text: "Too dark", tone: "bad" });
  if (out.length === 0 && ai.suggestion === 1) out.push({ text: "Keeper", tone: "good" });
  return out.slice(0, 2);
}

export function Thumb({ root, item }: { root: string; item: Item }) {
  if (item.previewState === 0) return <span className="thumb-pending" aria-label="Preview on its way" />;
  if (item.previewState === 2) return <span className="faint" style={{ fontSize: 12 }}>No preview</span>;
  return <img src={fileUrl(thumbPath(root, item.id), item.previewState)} alt="" loading="lazy" decoding="async" />;
}

export function formatDuration(s?: number): string {
  if (!s) return "Video";
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.round(s % 60)).padStart(2, "0")}`;
}
