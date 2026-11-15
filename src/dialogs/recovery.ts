// Crash recovery prompt (spec 5.5): offer to finish or roll back an interrupted operation.
import { api, errText, type OpView, type RecoveryAction } from "../api";
import { S } from "../strings";
import { h, modal, toast } from "../ui";

/** Asks about each operation the last session left unfinished, one at a time. */
export async function checkRecovery() {
  let ops: OpView[] = [];
  try {
    ops = await api.pendingRecovery();
  } catch {
    return;
  }
  for (const op of ops) await ask(op);
}

function ask(op: OpView): Promise<void> {
  return new Promise((resolve) => {
    const act = async (action: RecoveryAction) => {
      close();
      try {
        await api.recoverOp(op.id, action);
        toast(action === "finish" ? S.recovery.finished : S.done.undone);
      } catch (e) {
        toast(errText(e), "bad", 10000);
      }
    };
    const when = new Date(op.createdMs).toLocaleString();
    const close = modal(
      h(
        "div",
        { class: "stack" },
        h("h2", null, S.recovery.title),
        h("p", { class: "muted" }, S.recovery.lead),
        h("div", { class: "card tight" }, h("b", null, op.title), h("div", { class: "small muted" }, when)),
        h(
          "div",
          { class: "actions" },
          h("button", { class: "btn ghost left", onclick: () => close() }, S.recovery.later),
          h("button", { class: "btn", "data-tip": S.recovery.rollBackTip, onclick: () => act("rollBack") }, S.recovery.rollBack),
          h("button", { class: "btn primary", "data-tip": S.recovery.finishTip, onclick: () => act("finish") }, S.recovery.finish),
        ),
      ),
      { label: S.recovery.title, onClose: () => resolve() },
    );
  });
}
