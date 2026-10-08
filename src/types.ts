export type Kind = "photo" | "video";

export interface Settings {
  libraryRoot: string;
  backupRoot: string;
  artist: string;
  copyright: string;
  aiEnabled: boolean;
  resolveHintSeen: boolean;
  otherProjects: string[];
}

export interface Card {
  mount: string;
  label: string;
  totalBytes: number;
  freeBytes: number;
}

export interface ScanSummary {
  source: string;
  label: string;
  photos: number;
  videos: number;
  bytes: number;
  newPhotos: number;
  newVideos: number;
  newBytes: number;
  alreadyImported: number;
  /** Of those, rejects that were sent to the Trash from Safelight. */
  trashed: number;
  /** First capture date among files not imported yet; names a new project. */
  firstNewDate: string | null;
  alreadyIn: string[];
  cameras: string[];
  firstDate: string | null;
  lastDate: string | null;
}

export interface Meta {
  make?: string;
  model?: string;
  capturedAt?: string;
  lens?: string;
  iso?: number;
  shutter?: number;
  aperture?: number;
  focal?: number;
  flash?: boolean;
  width?: number;
  height?: number;
  orientation?: number;
  gpsLat?: number;
  gpsLon?: number;
  serial?: string;
  duration?: number;
}

export interface Ai {
  sharpness?: number;
  blurLevel?: number;
  motionBlur?: boolean;
  faces?: number;
  eyesClosed?: number;
  overExposed?: number;
  underExposed?: number;
  horizonTilt?: number;
  aesthetic?: number;
  burstId?: number;
  burstSize?: number;
  burstBest?: boolean;
  suggestion?: number;
  reasons: string[];
  tags: string[];
  people: number[];
}

export interface VideoInfo {
  /** e.g. "Canon Log 3": shown through a viewing LUT in Safelight, the original stays untouched. */
  log: string | null;
  playback: "original" | "pending" | "proxy" | "failed";
}

export interface Item {
  id: number;
  relPath: string;
  kind: Kind;
  fileName: string;
  size: number;
  capturedAt: string | null;
  camera: string;
  meta: Meta;
  rating: number;
  flag: number;
  movedToRejected: boolean;
  previewState: number;
  aiState: number;
  ai: Ai | null;
  video: VideoInfo | null;
  /** Tags you added yourself; the AI's are in `ai.tags`. */
  tags: string[];
}

export interface ProjectSummary {
  root: string;
  name: string;
  total: number;
  photos: number;
  videos: number;
  picks: number;
  rejects: number;
  firstDate: string | null;
  lastDate: string | null;
  cover: string | null;
}

export interface Progress {
  projectRoot: string;
  projectName: string;
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
  skipped: number;
  failed: number;
  finished: boolean;
}

export interface FileError {
  file: string;
  error: string;
}

export interface Report {
  source: string;
  cardMount: string | null;
  projectRoot: string;
  projectName: string;
  seconds: number;
  copied: number;
  photos: number;
  videos: number;
  skipped: number;
  bytes: number;
  failed: FileError[];
  backupRoot: string | null;
  backupFailed: FileError[];
  cancelled: boolean;
}

export interface HandoffStatus {
  lightroom: boolean;
  resolve: boolean;
}

export interface ModelStatus {
  installed: boolean;
  downloadMb: number;
}
