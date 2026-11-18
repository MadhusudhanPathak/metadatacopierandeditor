// Edit step 3 (Review) and the hand-off to Save and Done (D17): a "what will be written"
// summary for bulk edits, then one compact before → after block per file. Files with nothing
// to change collapse into a single line.
import { api, errText, type DiffRow, type EditItemPlan, type EditPlan, type Field } from "../../api";
import { baseName, dirName } from "../../lib/paths";
import { go } from "../../router";
import { S } from "../../strings";
import { add, clear, coordsOf, fillPlace, h, icon, loading, note, stepper, thumb, toast } from "../../ui";
import { doneView, runView } from "../run";
import { page, render, reset, st, STEP } from "./state";

/** Field order, matching the editor. */
const ORDER: Field[] = ["dateTaken", "location", "make", "model", "lens", "rating", "title", "author", "description", "keywords", "copyright"];

/** A value cell; location values get their place name underneath. */
function value(field: Field, v: string | null, cls: string): HTMLElement {
  const el = h("div", { class: `rv-val ${cls}` }, v ?? "—");
  const c = field === "location" ? coordsOf(v) : null;
  if (c) el.append(fillPlace(h("div", { class: "placeline" }), c[0], c[1]));
  return el;
}

function rows(diff: DiffRow[]): HTMLElement {
  const sorted = [...diff].sort((a, b) => ORDER.indexOf(a.field) - ORDER.indexOf(b.field));
  return h(
    "div",
    { class: "rv-rows" },
    sorted.map((r) => [
      h("div", { class: "rv-label" }, r.label),
      value(r.field, r.before, "before"),
      h("div", { class: "rv-arrow", "aria-hidden": "true" }, icon("right")),
      h(
        "div",
        null,
        r.after === null ? h("div", { class: "rv-val gone" }, r.before ? S.preview.removed : "—") : value(r.field, r.after, "after"),
        r.note ? h("div", { class: "small", style: "color:var(--warn)" }, r.note) : null,
      ),
    ]),
  );
}

function fileBlock(it: EditItemPlan): HTMLElement {
  const blocked = it.state === "blocked";
  return h(
    "div",
    { class: `rv-file ${blocked ? "blocked" : ""}` },
    h(
      "div",
      { class: "rv-filehead" },
      thumb(it.path, st.data.find((d) => d.path === it.path)?.kind ?? "other"),
      h("div", { class: "names grow" }, h("div", { class: "name" }, it.name), h("div", { class: "sub" }, dirName(it.path))),
      blocked ? h("span", { class: "chip bad" }, S.preview.blockedChip) : null,
      !blocked && it.state === "ready" ? h("span", { class: "rv-backup", "data-tip": S.edit.backupNote }, `${S.edit.backupAs} ${baseName(it.backupPath)}`) : null,
    ),
    it.problems.length || it.warnings.length ? h("div", { class: "notes" }, it.problems.map((x) => note("bad", x.message)), it.warnings.map((x) => note("warn", x.message))) : null,
    !blocked && it.diff.length ? rows(it.diff) : null,
  );
}

/** Bulk only: each touched field once, with the value written (or how it changes) and on how many files. */
function summary(plan: EditPlan): HTMLElement | null {
  const ready = plan.items.filter((i) => i.state === "ready" && !i.noChange);
  if (plan.items.length < 2 || !ready.length) return null;
  const fields = ORDER.filter((f) => st.changes[f] !== undefined);
  const lines = fields.map((f) => {
    const hitRows = ready.map((i) => i.diff.find((r) => r.field === f)).filter((r): r is DiffRow => !!r);
    const afters = new Set(hitRows.map((r) => r.after));
    const ch = st.changes[f];
    const label = hitRows[0]?.label ?? S.fields[f][0];
    let val: HTMLElement;
    if (!hitRows.length) val = h("div", { class: "rv-val gone" }, "—");
    else if (ch?.op === "shift") val = h("div", { class: "rv-val after" }, S.edit.shiftBy(ch.shift));
    else if (afters.size === 1) {
      const a = hitRows[0].after;
      val = a === null ? h("div", { class: "rv-val gone" }, S.edit.willClear) : value(f, a, "after");
    } else val = h("div", { class: "rv-val muted" }, S.edit.varies);
    return [h("div", { class: "rv-label" }, label), val, h("div", { class: "rv-count" }, S.edit.onFiles(hitRows.length, plan.items.length))];
  });
  return h("div", { class: "card tight rv-summary" }, h("h3", null, S.edit.willWrite), h("div", { class: "rv-sumrows" }, lines));
}

export function renderReview(head: HTMLElement) {
  const box = h("div");
  add(page(), head, h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.edit.reviewTitle), h("p", { class: "muted" }, S.edit.reviewLead))), box);
  if (!st.plan) {
    box.append(loading(S.preview.loading));
    api
      .planEdit(
        st.data.map((d) => d.path),
        st.changes,
        st.setFileDates,
      )
      .then((p) => {
        st.plan = p;
        if (st.step === STEP.review) render();
      })
      .catch((e) => {
        clear(box);
        box.append(note("bad", errText(e)));
      });
    return;
  }
  const plan = st.plan;
  const rank = (i: EditItemPlan) => (i.state === "blocked" ? 0 : i.warnings.length ? 1 : 2);
  const shown = plan.items.filter((i) => i.state === "blocked" || !i.noChange).sort((a, b) => rank(a) - rank(b));
  const unchanged = plan.items.filter((i) => i.state !== "blocked" && i.noChange);
  const MAX_NAMES = 6;
  add(
    box,
    h(
      "div",
      { class: "row rv-chips" },
      h("span", { class: "chip ok" }, icon("check"), S.preview.ready(plan.ready)),
      plan.unchanged ? h("span", { class: "chip" }, S.edit.unchangedCount(plan.unchanged)) : null,
      plan.blocked ? h("span", { class: "chip bad" }, S.preview.blocked(plan.blocked)) : null,
    ),
    summary(plan),
    h(
      "div",
      { class: "rv-list" },
      plan.items.length > 1 && shown.length ? h("h3", null, S.edit.perFile) : null,
      shown.map(fileBlock),
      unchanged.length
        ? h(
            "div",
            { class: "rv-unchanged small muted" },
            S.edit.unchangedList(unchanged.length),
            " ",
            h("span", { class: "break" }, unchanged.slice(0, MAX_NAMES).map((i) => i.name).join(", ") + (unchanged.length > MAX_NAMES ? `, ${S.edit.andMore(unchanged.length - MAX_NAMES)}` : "")),
          )
        : null,
    ),
    h(
      "div",
      { class: "actions sticky-actions" },
      h(
        "button",
        {
          class: "btn ghost left",
          onclick: () => {
            st.step = STEP.edit;
            render();
          },
        },
        icon("back"),
        S.back,
      ),
      h("button", { class: "btn primary", disabled: plan.ready === 0, onclick: () => save(plan) }, icon("check"), S.edit.save(plan.ready)),
    ),
  );
}

async function save(plan: EditPlan) {
  st.step = STEP.save;
  clear(page());
  await runView({
    container: page(),
    header: stepper(S.steps.edit, STEP.save),
    start: () => api.runEdit(plan.planId),
    onError: (e) => {
      toast(e, "bad", 8000);
      st.step = STEP.review;
      render();
    },
    onDone: (op) => {
      st.step = STEP.done;
      doneView({
        container: page(),
        header: stepper(S.steps.edit, STEP.done),
        op,
        canSetCreated: st.canSetCreated,
        onAnother: () => {
          reset();
          render();
        },
        onHome: () => go("home"),
      });
    },
  });
}
