// Typed wrappers around the Rust commands (src-tauri/src/commands) and backend events.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { S } from "../strings";
import type {
  About,
  Changes,
  CopyPlan,
  EditPlan,
  EngineStatus,
  FileFields,
  FileInfo,
  OpView,
  PairingResult,
  PairMode,
  PairReq,
  Place,
  Progress,
  RecoveryAction,
  Settings,
} from "./types";

export type * from "./types";
export { CATEGORY_IDS } from "./types";

export const api = {
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  about: () => invoke<About>("about"),
  expandPaths: (paths: string[], recursive: boolean, exclude?: string | null) =>
    invoke<FileInfo[]>("expand_paths", { paths, recursive, exclude: exclude ?? null }),
  suggestConverted: (sourceDir: string) => invoke<string | null>("suggest_converted", { sourceDir }),
  pairFiles: (sources: string[], converted: string[], mode: PairMode) => invoke<PairingResult>("pair_files", { sources, converted, mode }),
  thumbnail: (path: string, size: number) => invoke<string | null>("thumbnail", { path, size }),
  planCopy: (pairs: PairReq[], categories: string[], tzOverride: string | null) =>
    invoke<CopyPlan>("plan_copy", { req: { pairs, categories, tzOverride } }),
  runCopy: (planId: string, skip: string[]) => invoke<string>("run_copy", { planId, skip }),
  cancelRun: () => invoke<void>("cancel_run"),
  readEditFields: (paths: string[]) => invoke<FileFields[]>("read_edit_fields", { paths }),
  planEdit: (paths: string[], changes: Changes, setFileDates: boolean) => invoke<EditPlan>("plan_edit", { req: { paths, changes, setFileDates } }),
  runEdit: (planId: string) => invoke<string>("run_edit", { planId }),
  listHistory: () => invoke<OpView[]>("list_history"),
  undoOp: (id: string) => invoke<OpView>("undo_op", { id }),
  pendingRecovery: () => invoke<OpView[]>("pending_recovery"),
  recoverOp: (id: string, action: RecoveryAction) => invoke<OpView>("recover_op", { id, action }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  searchPlaces: (query: string) => invoke<Place[]>("search_places", { query }),
};

/** Per-item progress of the running operation. */
export function onProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("op-progress", (e) => cb(e.payload));
}

/** Fired once when an operation started by `runCopy`/`runEdit` has finished. */
export function onFinished(cb: (op: OpView) => void): Promise<UnlistenFn> {
  return listen<OpView>("op-finished", (e) => cb(e.payload));
}

/** Readable message from a rejected command (commands reject with plain strings). */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return S.errors.generic;
}
