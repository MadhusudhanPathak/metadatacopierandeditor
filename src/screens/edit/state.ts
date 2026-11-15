// State of the Edit flow, shared by its steps.
import type { Changes, EditPlan, FileFields, FileInfo } from "../../api";

export const STEP = { pick: 0, edit: 1, review: 2, save: 3, done: 4 } as const;

export const st = {
  step: 0,
  files: [] as FileInfo[],
  picked: [] as string[],
  subfolders: false,
  data: [] as FileFields[],
  /** Only fields present here were touched; nothing else is written. */
  changes: {} as Changes,
  setFileDates: true,
  dateMode: "set" as "set" | "shift",
  plan: null as EditPlan | null,
  canSetCreated: true,
};

/** Back to a fresh Pick step. The subfolder switch is kept. */
export function reset() {
  st.step = STEP.pick;
  st.files = [];
  st.picked = [];
  st.data = [];
  st.changes = {};
  st.setFileDates = true;
  st.dateMode = "set";
  st.plan = null;
}

let root: HTMLElement | null = null;
let renderer: () => void = () => {};

/** Called once per visit by the screen entry point. */
export function mount(page: HTMLElement, render: () => void) {
  root = page;
  renderer = render;
}

export function page(): HTMLElement {
  if (!root) throw new Error("Edit screen is not mounted");
  return root;
}

/** Redraws the current step. */
export function render() {
  renderer();
}
