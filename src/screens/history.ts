// History & Undo (spec 4.1, 5.5).
import { api, errText, type OpView } from "../api";
import { baseName } from "../lib/paths";
import { go } from "../router";
import { S } from "../strings";
import { add, clear, confirmBox, h, icon, loading, note, reveal, spinner, toast } from "../ui";

export async function historyScreen(page: HTMLElement) {
  const box = h("div");
  add(
    page,
    h(
      "div",
      { class: "pagehead" },
      h("div", null, h("h1", null, S.history.title), h("p", { class: "muted" }, S.history.lead)),
      h("button", { class: "btn ghost", onclick: () => go("home") }, icon("back"), S.back),
    ),
    box,
  );
  await refresh(box);
}

async function refresh(box: HTMLElement) {
  clear(box);
  box.append(loading(""));
  let ops: OpView[] = [];
  try {
    ops = await api.listHistory();
  } catch (e) {
    clear(box);
    box.append(note("bad", errText(e)));
    return;
  }
  clear(box);
  if (!ops.length) {
    box.append(h("div", { class: "empty-state" }, icon("history"), h("p", null, S.history.empty)));
    return;
  }
  box.append(h("div", { class: "list" }, ops.map((op) => opCard(op, box))));
}

function statusChip(op: OpView): HTMLElement {
  const s = op.summary;
  if (op.status === "undone") return h("span", { class: "chip moss" }, icon("undo"), S.history.status.undone);
  if (op.status === "rolledBack") return h("span", { class: "chip moss" }, S.history.status.rolledBack);
  if (s.failed) return h("span", { class: "chip warn" }, S.history.partial(s.ok + s.warnings, s.failed));
  return h("span", { class: "chip ok" }, icon("check"), S.history.status[op.status] ?? op.status);
}

function opCard(op: OpView, box: HTMLElement): HTMLElement {
  const when = new Date(op.createdMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  const done = op.status === "undone" || op.status === "rolledBack";
  const reason = op.undoAvailable ? S.done.undoTip : done ? "" : S.history.oldVersion;
  const undoBtn = h(
    "button",
    { class: "btn small", disabled: !op.undoAvailable, "data-tip": reason || null },
    icon("undo"),
    done ? S.history.undone : op.undoAvailable ? S.history.undo : S.history.cantUndo,
  );
  undoBtn.onclick = async (e: Event) => {
    e.preventDefault();
    if (!(await confirmBox(S.history.confirmUndo(op.title), S.history.undo))) return;
    undoBtn.disabled = true;
    undoBtn.prepend(spinner());
    try {
      await api.undoOp(op.id);
      toast(S.done.undone);
    } catch (err) {
      toast(errText(err), "bad", 10000);
    }
    await refresh(box);
  };
  return h(
    "details",
    { class: "pair" },
    h(
      "summary",
      null,
      h("span", { class: "thumb" }, icon(op.kind === "copy" ? "copy" : "edit")),
      h("div", { class: "names grow" }, h("div", { class: "name" }, op.title), h("div", { class: "sub" }, when)),
      statusChip(op),
      undoBtn,
      icon("chevron"),
    ),
    h(
      "div",
      { class: "body" },
      h(
        "div",
        { class: "list" },
        op.items.map((i) =>
          h(
            "div",
            { class: "item", style: "align-items:flex-start" },
            h(
              "div",
              { class: "names" },
              h("div", { class: "name" }, i.label),
              h("div", { class: "sub break", style: "white-space:normal" }, i.status === "done" ? i.finalPath : (i.message ?? "")),
              i.status === "done" && i.originalKeptAs ? h("div", { class: "sub" }, `${S.edit.backupAs} ${baseName(i.originalKeptAs)}`) : null,
            ),
            h("span", { class: `chip ${i.status === "done" ? "ok" : i.status === "failed" ? "bad" : ""}` }, S.run.status[i.status] ?? S.history.status[i.status] ?? i.status),
            i.status === "done"
              ? h("button", { class: "btn icon small ghost", "aria-label": S.done.openFolder, "data-tip": S.done.openFolder, onclick: () => reveal(i.finalPath) }, icon("folder"))
              : null,
          ),
        ),
      ),
    ),
  );
}
