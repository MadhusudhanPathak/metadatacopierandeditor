// Payloads of the Rust commands (serde camelCase). Keep in sync with src-tauri/src.

export type Kind = "photo" | "video" | "audio" | "other";
export type PairMode = "exact" | "anyExt" | "manual" | "auto";
export type FormatClass = "exifFull" | "exifXmp" | "xmpOnly" | "quickTime" | "pdf" | "unwritable";
export type Field =
  | "dateTaken"
  | "location"
  | "make"
  | "model"
  | "lens"
  | "title"
  | "description"
  | "keywords"
  | "rating"
  | "author"
  | "copyright";

export interface FileInfo {
  path: string;
  name: string;
  size: number;
  kind: Kind;
}

export interface Pair {
  source: string;
  converted: string;
  how: "exact" | "anyExt" | "manual";
}
export interface Leftover {
  path: string;
  status: "ambiguous" | "unmatched";
  candidates: string[];
}
export interface PairingResult {
  pairs: Pair[];
  sourcesLeft: Leftover[];
  convertedLeft: Leftover[];
}

export interface Issue {
  code: string;
  message: string;
}
export interface DiffRow {
  field: Field;
  label: string;
  before: string | null;
  after: string | null;
  note: string | null;
}
/** Copy checklist categories, in display order (src-tauri/src/metadata/categories.rs). */
export type CategoryId = "dates" | "location" | "camera" | "exposure" | "descriptive" | "author" | "makernotes" | "container" | "other";
export const CATEGORY_IDS: CategoryId[] = ["dates", "location", "camera", "exposure", "descriptive", "author", "makernotes", "container", "other"];

export interface CatCount {
  id: CategoryId;
  enabled: boolean;
  copied: number;
  notStorable: string[];
  kept: number;
}
export type TzSource = "explicit" | "gpsTime" | "gpsZone" | "assumed" | "override";
export interface TzView {
  source: TzSource;
  offset: string;
  zone: string | null;
}
export type PairState = "ready" | "conflict" | "blocked";
export interface PairPlan {
  id: string;
  source: string;
  converted: string;
  sourceKind: Kind;
  convertedKind: Kind;
  finalPath: string;
  sourceRenamed: string;
  state: PairState;
  conflictWith: string | null;
  problems: Issue[];
  warnings: Issue[];
  details: string[];
  diff: DiffRow[];
  categories: CatCount[];
  tz: TzView | null;
}
export interface CopyPlan {
  planId: string;
  pairs: PairPlan[];
  ready: number;
  conflicts: number;
  blocked: number;
  tzAssumed: number;
  categories: CatCount[];
}
export interface PairReq {
  id: string;
  source: string;
  converted: string;
  finalName?: string | null;
}

export interface DateValue {
  local: string;
  offset: string | null;
  offsetKnown: boolean;
}
export interface Location {
  lat: number;
  lon: number;
  alt: number | null;
}
export interface Fields {
  dateTaken: DateValue | null;
  location: Location | null;
  make: string | null;
  model: string | null;
  lens: string | null;
  title: string | null;
  description: string | null;
  keywords: string[];
  rating: number | null;
  author: string | null;
  copyright: string | null;
}
export interface FileFields {
  path: string;
  name: string;
  kind: Kind;
  class: FormatClass;
  size: number;
  fileType: string | null;
  dimensions: [number, number] | null;
  duration: number | null;
  fields: Fields;
  supported: Field[];
  error: string | null;
}

export type TextChange = { op: "set"; value: string } | { op: "clear" };
export type DateChange =
  | { op: "set"; local: string; offset: string }
  | { op: "shift"; shift: { days: number; hours: number; minutes: number } }
  | { op: "clear" };
export type LocChange = { op: "set"; lat: number; lon: number; alt: number | null } | { op: "clear" };
export type ListChange = { op: "set"; values: string[] } | { op: "clear" };
export type RatingChange = { op: "set"; value: number } | { op: "clear" };
export interface Changes {
  dateTaken?: DateChange;
  location?: LocChange;
  make?: TextChange;
  model?: TextChange;
  lens?: TextChange;
  title?: TextChange;
  description?: TextChange;
  keywords?: ListChange;
  rating?: RatingChange;
  author?: TextChange;
  copyright?: TextChange;
}
export interface EditItemPlan {
  path: string;
  name: string;
  state: PairState;
  noChange: boolean;
  problems: Issue[];
  warnings: Issue[];
  diff: DiffRow[];
  backupPath: string;
}
export interface EditPlan {
  planId: string;
  items: EditItemPlan[];
  ready: number;
  blocked: number;
  unchanged: number;
  setFileDates: boolean;
}

export type ItemStatus = "pending" | "done" | "failed" | "skipped" | "undone" | "rolledBack";
export type OpStatus = "running" | "completed" | "cancelled" | "interrupted" | "undone" | "rolledBack";
export interface ItemView {
  label: string;
  status: ItemStatus;
  message: string | null;
  warnings: string[];
  details: string[];
  finalPath: string;
  original: string;
  /** Where the user's original now lives (name_original). */
  originalKeptAs: string;
}
export type RecoveryAction = "finish" | "rollBack";
export interface Summary {
  ok: number;
  warnings: number;
  failed: number;
  skipped: number;
  undone: number;
}
export interface OpView {
  id: string;
  kind: "copy" | "edit";
  title: string;
  createdMs: number;
  finishedMs: number | null;
  status: OpStatus;
  summary: Summary;
  undoAvailable: boolean;
  items: ItemView[];
  error: string | null;
}
export interface Progress {
  opId: string;
  index: number;
  total: number;
  label: string;
  status: "working" | "done" | "warning" | "failed" | "skipped";
  message: string | null;
  done: number;
}

export interface Settings {
  pairingMode: PairMode;
  includeSubfolders: boolean;
  stopOnFailure: boolean;
  onboarded: boolean;
}
export interface Place {
  name: string;
  region: string;
  country: string;
  countryCode: string;
  lat: number;
  lon: number;
  population: number;
}

export interface EngineStatus {
  ready: boolean;
  error: string | null;
}

export interface About {
  appVersion: string;
  exiftoolVersion: string;
  platform: "windows" | "android" | "other";
  canSetCreated: boolean;
  dataDir: string;
}
