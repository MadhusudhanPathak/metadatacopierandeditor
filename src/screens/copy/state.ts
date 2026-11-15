// State of the Copy flow, shared by its steps.
import { CATEGORY_IDS, type CategoryId, type CopyPlan, type FileInfo, type Kind, type Leftover, type Pair, type PairMode } from "../../api";

export interface Side {
  folder: string | null;
  /** Raw picked or dropped paths (files or folders). */
  picked: string[];
  files: FileInfo[];
}

export type IdPair = Pair & { id: string };
export type SideKey = "s" | "c";

export const STEP = { pick: 0, pair: 1, preview: 2, run: 3, done: 4 } as const;

export const st = {
  step: 0,
  src: emptySide(),
  conv: emptySide(),
  subfolders: false,
  suggestion: null as string | null,
  mode: "auto" as PairMode,
  pairs: [] as IdPair[],
  srcLeft: [] as Leftover[],
  convLeft: [] as Leftover[],
  picking: null as { side: SideKey; path: string } | null,
  categories: new Set<CategoryId>(CATEGORY_IDS),
  tz: null as string | null,
  finalNames: {} as Record<string, string>,
  skip: new Set<string>(),
  plan: null as CopyPlan | null,
  canSetCreated: true,
  seq: 0,
};

function emptySide(): Side {
  return { folder: null, picked: [], files: [] };
}

/** Back to a fresh Pick step. Pairing mode and the subfolder switch are kept. */
export function reset() {
  st.step = STEP.pick;
  st.src = emptySide();
  st.conv = emptySide();
  st.suggestion = null;
  st.pairs = [];
  st.srcLeft = [];
  st.convLeft = [];
  st.picking = null;
  st.categories = new Set(CATEGORY_IDS);
  st.tz = null;
  st.finalNames = {};
  st.skip = new Set();
  st.plan = null;
}

export function nextPairId(): string {
  return `p${++st.seq}`;
}

export function kindOf(path: string, side: Side): Kind {
  return side.files.find((f) => f.path === path)?.kind ?? "other";
}

let root: HTMLElement | null = null;
let renderer: () => void = () => {};

/** Called once per visit by the screen entry point. */
export function mount(page: HTMLElement, render: () => void) {
  root = page;
  renderer = render;
}

export function page(): HTMLElement {
  if (!root) throw new Error("Copy screen is not mounted");
  return root;
}

/** Redraws the current step. */
export function render() {
  renderer();
}
