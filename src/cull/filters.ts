import type { Item } from "../types";

export type FlagFilter = "all" | "picks" | "unflagged" | "rejects";
export type AiFilter = "keep" | "reject" | "blurry" | "eyes" | "best" | "tilted" | "exposure";

export interface Filters {
  flag: FlagFilter;
  minRating: number;
  kind: "all" | "photo" | "video";
  cameras: string[];
  dates: string[];
  lenses: string[];
  flash: "any" | "yes" | "no";
  ai: AiFilter[];
  tags: string[];
  people: number[];
  collapseBursts: boolean;
}

export const emptyFilters: Filters = {
  flag: "all",
  minRating: 0,
  kind: "all",
  cameras: [],
  dates: [],
  lenses: [],
  flash: "any",
  ai: [],
  tags: [],
  people: [],
  collapseBursts: false,
};

export const AI_FILTERS: { key: AiFilter; label: string }[] = [
  { key: "keep", label: "Suggested keepers" },
  { key: "reject", label: "Suggested rejects" },
  { key: "best", label: "Best of burst" },
  { key: "blurry", label: "Soft or blurry" },
  { key: "eyes", label: "Eyes closed" },
  { key: "exposure", label: "Exposure problems" },
  { key: "tilted", label: "Tilted" },
];

/** Number of non-default "More filters" settings, for the button badge. */
export function activeExtraCount(f: Filters): number {
  return (
    (f.minRating > 0 ? 1 : 0) + (f.kind !== "all" ? 1 : 0) + f.cameras.length + f.dates.length +
    f.lenses.length + (f.flash !== "any" ? 1 : 0) + f.ai.length + f.tags.length + f.people.length
  );
}

const dateOf = (it: Item) => it.capturedAt?.slice(0, 10) ?? "Unknown";

function aiMatch(it: Item, key: AiFilter): boolean {
  const a = it.ai;
  if (!a) return false;
  switch (key) {
    case "keep": return a.suggestion === 1;
    case "reject": return a.suggestion === -1;
    case "best": return !!a.burstBest;
    case "blurry": return (a.blurLevel ?? 0) >= 1;
    case "eyes": return (a.eyesClosed ?? 0) > 0;
    case "exposure": return (a.overExposed ?? 0) > 0.04 || (a.underExposed ?? 0) > 0.5;
    case "tilted": return a.horizonTilt != null;
  }
}

export function applyFilters(items: Item[], f: Filters): Item[] {
  const out = items.filter((it) => {
    if (f.flag === "picks" && it.flag !== 1) return false;
    if (f.flag === "unflagged" && it.flag !== 0) return false;
    if (f.flag === "rejects" && it.flag !== -1) return false;
    if (f.flag !== "rejects" && it.movedToRejected) return false;
    if (f.minRating && it.rating < f.minRating) return false;
    if (f.kind !== "all" && it.kind !== f.kind) return false;
    if (f.cameras.length && !f.cameras.includes(it.camera)) return false;
    if (f.dates.length && !f.dates.includes(dateOf(it))) return false;
    if (f.lenses.length && !f.lenses.includes(it.meta.lens ?? "")) return false;
    if (f.flash === "yes" && it.meta.flash !== true) return false;
    if (f.flash === "no" && it.meta.flash !== false) return false;
    if (f.ai.length && !f.ai.some((k) => aiMatch(it, k))) return false;
    if (f.tags.length && !f.tags.some((t) => it.ai?.tags.includes(t))) return false;
    if (f.people.length && !f.people.some((p) => it.ai?.people.includes(p))) return false;
    return true;
  });
  if (!f.collapseBursts) return out;
  // One tile per burst: the best shot (or the first until the AI has picked one).
  const shown = new Set<number>();
  const bestOf = new Map<number, number>();
  for (const it of out) {
    const b = it.ai?.burstId;
    if (b != null && (it.ai?.burstBest || !bestOf.has(b))) bestOf.set(b, it.id);
  }
  return out.filter((it) => {
    const b = it.ai?.burstId;
    if (b == null) return true;
    if (bestOf.get(b) !== it.id || shown.has(b)) return false;
    shown.add(b);
    return true;
  });
}

export interface FilterOptions {
  cameras: string[];
  dates: string[];
  lenses: string[];
  tags: string[];
  people: number[];
  hasVideo: boolean;
  hasFlash: boolean;
}

export function filterOptions(items: Item[]): FilterOptions {
  const cams = new Set<string>(), dates = new Set<string>(), lenses = new Set<string>();
  const tags = new Map<string, number>(), people = new Set<number>();
  let hasVideo = false, hasFlash = false;
  for (const it of items) {
    cams.add(it.camera);
    dates.add(dateOf(it));
    if (it.meta.lens) lenses.add(it.meta.lens);
    if (it.kind === "video") hasVideo = true;
    if (it.meta.flash) hasFlash = true;
    it.ai?.tags.forEach((t) => tags.set(t, (tags.get(t) ?? 0) + 1));
    it.ai?.people.forEach((p) => people.add(p));
  }
  return {
    cameras: [...cams].sort(),
    dates: [...dates].sort(),
    lenses: [...lenses].sort(),
    tags: [...tags.entries()].sort((a, b) => b[1] - a[1]).map(([t]) => t),
    people: [...people].sort((a, b) => a - b),
    hasVideo,
    hasFlash,
  };
}
