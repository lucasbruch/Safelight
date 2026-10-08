import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  Card, HandoffStatus, Item, ModelStatus, ProjectSummary, Progress, Report, ScanSummary, Settings,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  /** Returns the settings as stored (the backend keeps its own list of other projects). */
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  listCards: () => invoke<Card[]>("list_cards"),
  scanSource: (source: string, label?: string) => invoke<ScanSummary>("scan_source", { source, label }),
  startImport: (request: {
    source: string;
    projectRoot?: string;
    newName?: string;
    includeDuplicates: boolean;
    cardMount?: string;
  }) => invoke<string>("start_import", { request }),
  cancelImport: () => invoke<void>("cancel_import"),
  importStatus: () => invoke<Progress | null>("import_status"),
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  openProject: (root: string) => invoke<{ root: string; name: string; items: Item[] }>("open_project", { root }),
  setRating: (root: string, ids: number[], rating: number) => invoke<Item[]>("set_rating", { root, ids, rating }),
  setFlag: (root: string, ids: number[], flag: number) => invoke<Item[]>("set_flag", { root, ids, flag }),
  editTags: (root: string, ids: number[], add: string[], remove: string[]) => invoke<Item[]>("edit_tags", { root, ids, add, remove }),
  moveRejects: (root: string) => invoke<{ moved: number; failed: string[] }>("move_rejects", { root }),
  trashRejects: (root: string) => invoke<number>("trash_rejects", { root }),
  fullImage: (root: string, id: number) => invoke<string>("full_image", { root, id }),
  ejectCard: (mount: string) => invoke<void>("eject_card", { mount }),
  requestProxy: (root: string, id: number) => invoke<void>("request_proxy", { root, id }),
  rerunAi: (root: string) => invoke<number>("rerun_ai", { root }),
  aiStatus: () => invoke<ModelStatus>("ai_status"),
  downloadModels: () => invoke<void>("download_models"),
  handoffStatus: () => invoke<HandoffStatus>("handoff_status"),
  sendToLightroom: (root: string, ids: number[]) => invoke<string>("send_to_lightroom", { root, ids }),
  sendToResolve: (root: string, ids: number[]) => invoke<string>("send_to_resolve", { root, ids }),
  logFile: () => invoke<string>("log_file"),
};

export interface Events {
  "cards-changed": Card[];
  "scan-progress": { source: string; done: number; total: number };
  "import-progress": Progress;
  "import-finished": Report;
  "item-added": { projectRoot: string; item: Item };
  "item-updated": { projectRoot: string; item: Item };
  /** Background results (bursts, people, suggestions) for several items at once. */
  "items-updated": { projectRoot: string; items: Item[] };
  "import-error": string;
  "models-progress": { done: number; total: number; name: string };
}

export function on<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void): Promise<UnlistenFn> {
  return listen<Events[K]>(event, (e) => cb(e.payload));
}

/** Joins onto a project root using that root's own separator. */
export function joinPath(root: string, ...parts: string[]): string {
  const sep = root.includes("\\") ? "\\" : "/";
  const clean = parts.flatMap((p) => p.split("/"));
  return [root.replace(/[\\/]+$/, ""), ...clean].join(sep);
}

export function fileUrl(path: string, version?: number | string): string {
  const u = convertFileSrc(path);
  return version === undefined ? u : `${u}?v=${version}`;
}

export const thumbPath = (root: string, id: number) => joinPath(root, ".grabit", "cache", `${id}_t.jpg`);
export const previewPath = (root: string, id: number) => joinPath(root, ".grabit", "cache", `${id}_p.jpg`);
export const proxyPath = (root: string, id: number) => joinPath(root, ".grabit", "cache", `${id}_proxy.mp4`);
/** What the app's player can play for a clip, or null while its playback copy is being made. */
export function playbackUrl(root: string, it: Item): string | null {
  const v = it.video;
  if (!v) return null;
  if (v.playback === "proxy") return fileUrl(proxyPath(root, it.id), "p");
  if (v.playback === "original") return fileUrl(mediaPath(root, it));
  return null;
}
export const mediaPath = (root: string, it: Item) =>
  it.movedToRejected ? joinPath(root, "_Rejected", it.relPath) : joinPath(root, it.relPath);

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatDate(iso: string | null | undefined, withTime = false): string {
  if (!iso) return "";
  const d = new Date(iso.length === 10 ? `${iso}T12:00:00` : iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, withTime
    ? { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" }
    : { day: "numeric", month: "short", year: "numeric" });
}

export function formatDateRange(a: string | null, b: string | null): string {
  if (!a) return "";
  if (!b || a.slice(0, 10) === b.slice(0, 10)) return formatDate(a.slice(0, 10));
  return `${formatDate(a.slice(0, 10))} – ${formatDate(b.slice(0, 10))}`;
}

export function formatShutter(s?: number): string {
  if (!s) return "";
  return s >= 0.5 ? `${Math.round(s * 10) / 10}s` : `1/${Math.round(1 / s)}`;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}
