// Edit step 3 (Review) and the hand-off to Save and Done.
import { api, errText, type EditItemPlan, type EditPlan } from "../../api";
import { baseName, dirName } from "../../lib/paths";
import { go } from "../../router";
import { S } from "../../strings";
import { add, clear, diffTable, h, icon, loading, note, stepper, thumb, toast } from "../../ui";
import { doneView, runView } from "../run";
import { page, render, reset, st, STEP } from "./state";

function itemChip(it: EditItemPlan): HTMLElement {
  if (it.state === "blocked") return h("span", { class: "chip bad" }, S.run.status.failed);
  if (it.noChange) return h("span", { class: "chip" }, S.edit.unchanged);
  return h("span", { class: "chip ok" }, icon("check"), S.preview.readyChip);
}

function itemCard(it: EditItemPlan, open: boolean): HTMLElement {
  return h(
    "details",
    { class: "pair", open },
    h(
      "summary",
      null,
      thumb(it.path, st.data.find((d) => d.path === it.path)?.kind ?? "other"),
      h("div", { class: "names grow" }, h("div", { class: "name" }, it.name), h("div", { class: "sub" }, dirName(it.path))),
      it.warnings.length ? h("span", { class: "chip warn" }, icon("warn"), String(it.warnings.length)) : null,
      itemChip(it),
      icon("chevron"),
    ),
    h(
      "div",
      { class: "body" },
      it.problems.length ? h("div", { class: "notes" }, it.problems.map((x) => note("bad", x.message))) : null,
      it.warnings.length ? h("div", { class: "notes" }, it.warnings.map((x) => note("warn", x.message))) : null,
      it.diff.length ? diffTable(it.diff, true) : null,
      it.state === "ready" && !it.noChange ? h("div", { class: "small muted" }, `${S.edit.backupAs} ${baseName(it.backupPath)}`) : null,
    ),
  );
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
  box.append(
    h(
      "div",
      { class: "row", style: "margin-bottom:12px" },
      h("span", { class: "chip ok" }, S.preview.ready(plan.ready)),
      plan.unchanged ? h("span", { class: "chip" }, S.edit.unchangedCount(plan.unchanged)) : null,
      plan.blocked ? h("span", { class: "chip bad" }, S.preview.blocked(plan.blocked)) : null,
    ),
    h(
      "div",
      { class: "list" },
      plan.items.map((it) => itemCard(it, plan.items.length <= 4 || it.state !== "ready" || it.warnings.length > 0)),
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
