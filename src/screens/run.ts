// Run (live progress) and Done (summary) views, shared by the Copy and Edit flows.
import { api, errText, onFinished, onProgress, type OpView, type Progress } from "../api";
import { baseName } from "../lib/paths";
import { setLeaveGuard } from "../router";
import { S } from "../strings";
import { add, clear, confirmBox, detailsBlock, h, icon, note, reveal, spinner, toast } from "../ui";

export interface RunOptions {
  container: HTMLElement;
  header: HTMLElement;
  start: () => Promise<string>;
  onDone: (op: OpView) => void;
  onError: (e: string) => void;
}

/** Starts an operation and shows live progress. Resolves when the backend finishes it. */
export async function runView(o: RunOptions) {
  const list = h("div", { class: "list", "aria-live": "polite" });
  const bar = h("div");
  const counter = h("div", { class: "counter" }, "0");
  const of = h("span", { class: "muted" });
  const cancelBtn = h("button", { class: "btn" }, S.run.cancel);
  const rows = new Map<number, HTMLElement>();

  clear(o.container);
  add(
    o.container,
    o.header,
    h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.run.title), h("p", { class: "muted" }, S.run.lead))),
    h("div", { class: "card stack" }, h("div", { class: "row" }, counter, of, h("span", { class: "grow" }), cancelBtn), h("div", { class: "progress", role: "progressbar", "aria-label": S.progress }, bar)),
    h("div", { class: "section-title" }, h("h3", null, S.history.files)),
    list,
  );
  cancelBtn.onclick = async () => {
    cancelBtn.disabled = true;
    cancelBtn.textContent = S.run.cancelling;
    await api.cancelRun().catch((e) => toast(errText(e), "bad"));
  };
  setLeaveGuard(() => {
    toast(S.errors.busy, "warn");
    return false;
  });

  let opId = "";
  const update = (p: Progress) => {
    if (opId && p.opId !== opId) return;
    const pct = p.total ? Math.round((p.done / p.total) * 100) : 0;
    bar.style.width = `${pct}%`;
    counter.textContent = String(p.done);
    of.textContent = `/ ${p.total}`;
    let row = rows.get(p.index);
    if (!row) {
      row = h("div", { class: "item" });
      rows.set(p.index, row);
      list.prepend(row);
    }
    clear(row);
    const ico =
      p.status === "working"
        ? spinner()
        : h("span", { class: `chip ${p.status === "done" ? "ok" : p.status === "warning" ? "warn" : p.status === "skipped" ? "" : "bad"}` }, S.run.status[p.status]);
    row.append(h("div", { class: "names" }, h("div", { class: "name" }, p.label), p.message ? h("div", { class: "sub" }, p.message) : null), ico);
  };

  const unP = await onProgress(update);
  let resolveFinished: (op: OpView) => void = () => {};
  const finished = new Promise<OpView>((resolve) => (resolveFinished = resolve));
  const unF = await onFinished((op) => {
    if (!opId || op.id === opId) resolveFinished(op);
  });
  try {
    opId = await o.start();
  } catch (e) {
    unP();
    unF();
    setLeaveGuard(null);
    o.onError(errText(e));
    return;
  }
  const op = await finished;
  unP();
  unF();
  setLeaveGuard(null);
  o.onDone(op);
}

export interface DoneOptions {
  container: HTMLElement;
  header: HTMLElement;
  op: OpView;
  canSetCreated: boolean;
  onAnother: () => void;
  onHome: () => void;
}

/** Summary of a finished operation, with Undo and "open folder". */
export function doneView(o: DoneOptions) {
  const { op } = o;
  const s = op.summary;
  const changed = s.ok + s.warnings;
  const good = s.failed === 0 && changed > 0 && op.status !== "cancelled";
  const title =
    op.status === "cancelled" ? S.done.cancelled : changed === 0 ? S.done.nothingDone : good && s.warnings === 0 ? S.done.allGood : S.done.someIssues;
  const firstFinal = op.items.find((i) => i.status === "done")?.finalPath;

  const undoBtn = h("button", { class: "btn", disabled: !op.undoAvailable, "data-tip": S.done.undoTip }, icon("undo"), S.done.undo);
  undoBtn.onclick = async () => {
    if (!(await confirmBox(S.history.confirmUndo(op.title), S.done.undo))) return;
    undoBtn.disabled = true;
    try {
      await api.undoOp(op.id);
      toast(S.done.undone);
      o.onAnother();
    } catch (e) {
      toast(errText(e), "bad", 9000);
      undoBtn.disabled = false;
    }
  };

  const failed = op.items.filter((i) => i.status === "failed");
  const noted = op.items.filter((i) => i.status === "done" && (i.warnings.length || i.details.length));
  clear(o.container);
  add(
    o.container,
    o.header,
    h(
      "div",
      { class: "row", style: "gap:22px;margin:10px 0 22px" },
      h("div", { class: `donebadge ${good && s.warnings === 0 ? "pulse" : "warn"}` }, icon(good ? "check" : "warn")),
      h("div", { class: "grow" }, h("h1", null, title), op.error ? h("p", { class: "muted", style: "margin-top:6px" }, op.error) : null),
    ),
    h(
      "div",
      { class: "stats" },
      h("div", { class: "stat" }, h("b", { style: "color:var(--neon)" }, String(s.ok)), S.done.ok),
      s.warnings ? h("div", { class: "stat" }, h("b", { style: "color:var(--warn)" }, String(s.warnings)), S.done.warnings) : null,
      s.failed ? h("div", { class: "stat" }, h("b", { style: "color:var(--danger)" }, String(s.failed)), S.done.failed) : null,
      s.skipped ? h("div", { class: "stat" }, h("b", null, String(s.skipped)), S.done.skipped) : null,
    ),
    !o.canSetCreated && changed ? h("div", { style: "margin-top:14px" }, note("info", S.done.createdLimit)) : null,
    failed.length
      ? h(
          "div",
          null,
          h("div", { class: "section-title" }, h("h3", null, S.done.failed)),
          h(
            "div",
            { class: "list" },
            failed.map((i) =>
              h(
                "div",
                { class: "item", style: "align-items:flex-start" },
                h(
                  "div",
                  { class: "names" },
                  h("div", { class: "name" }, i.label || baseName(i.original)),
                  h("div", { class: "break", style: "margin-top:4px" }, i.message ?? ""),
                  i.details.length ? detailsBlock(i.details, "margin-top:6px") : null,
                ),
                h("span", { class: "chip bad" }, S.run.status.failed),
              ),
            ),
          ),
        )
      : null,
    noted.length
      ? h(
          "div",
          null,
          h("div", { class: "section-title" }, h("h3", null, S.done.warnings)),
          h(
            "div",
            { class: "list" },
            noted.map((i) =>
              h(
                "div",
                { class: "item", style: "align-items:flex-start" },
                h(
                  "div",
                  { class: "names" },
                  h("div", { class: "name" }, i.label),
                  h("div", { class: "notes", style: "margin-top:4px" }, i.warnings.map((w) => note("warn", w))),
                  i.details.length ? detailsBlock(i.details, "margin-top:6px") : null,
                ),
              ),
            ),
          ),
        )
      : null,
    h(
      "div",
      { class: "actions" },
      h("button", { class: "btn ghost left", onclick: o.onHome }, icon("back"), S.done.home),
      undoBtn,
      firstFinal ? h("button", { class: "btn", onclick: () => reveal(firstFinal) }, icon("folder"), S.done.openFolder) : null,
      h("button", { class: "btn primary", onclick: o.onAnother }, S.done.another),
    ),
  );
}
