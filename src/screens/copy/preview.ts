// Copy step 3 (Preview) and the hand-off to Run and Done.
import { api, CATEGORY_IDS, errText, type CopyPlan, type PairPlan } from "../../api";
import { utcOffsets } from "../../lib/format";
import { baseName, dirName, extOf } from "../../lib/paths";
import { go } from "../../router";
import { S } from "../../strings";
import { add, clear, detailsBlock, diffTable, h, icon, loading, note, stepper, thumb, tip, toast } from "../../ui";
import { doneView, runView } from "../run";
import { page, render, reset, st, STEP } from "./state";

/** Debounce for re-planning after a category is toggled. */
const REPLAN_DELAY_MS = 250;
/** Max tag names listed in the "can't be stored" tooltip. */
const NOT_STORABLE_TIP_MAX = 30;

let planTimer = 0;

function planBox(): HTMLElement {
  return page().querySelector("#plan") as HTMLElement;
}

async function replan(container: HTMLElement) {
  clear(container);
  container.append(loading(S.preview.loading));
  try {
    st.plan = await api.planCopy(
      st.pairs.map((p) => ({ id: p.id, source: p.source, converted: p.converted, finalName: st.finalNames[p.id] ?? null })),
      [...st.categories],
      st.tz,
    );
    if (st.step === STEP.preview) render();
  } catch (e) {
    clear(container);
    container.append(note("bad", errText(e)), h("button", { class: "btn", style: "margin-top:12px", onclick: () => replan(container) }, S.tryAgain));
  }
}

function catChecklist(plan: CopyPlan): HTMLElement {
  return h(
    "div",
    { class: "cats" },
    CATEGORY_IDS.map((id) => {
      const [label, tipText] = S.preview.cat[id];
      const c = plan.categories.find((x) => x.id === id);
      const on = st.categories.has(id);
      const counts = h("span", { class: "count" });
      if (c && on) {
        counts.append(S.preview.copied(c.copied));
        if (c.notStorable.length) {
          const names = c.notStorable.slice(0, NOT_STORABLE_TIP_MAX).join(", ") + (c.notStorable.length > NOT_STORABLE_TIP_MAX ? "…" : "");
          counts.append(" · ", h("span", { style: "color:var(--warn)", "data-tip": `${S.preview.notStorableTip}<br><br>${names}` }, S.preview.notStorable(c.notStorable.length, "")));
        }
      } else if (c && !on && c.kept) counts.append(S.preview.kept(c.kept));
      const input = h("input", {
        type: "checkbox",
        checked: on,
        onchange: (e: Event) => {
          if ((e.target as HTMLInputElement).checked) st.categories.add(id);
          else st.categories.delete(id);
          window.clearTimeout(planTimer);
          planTimer = window.setTimeout(() => replan(planBox()), REPLAN_DELAY_MS);
        },
      });
      return h("div", { class: `cat ${on ? "" : "off"}` }, h("label", { class: "check" }, input, h("span", null, h("b", null, label), h("br"), counts)), tip(tipText));
    }),
  );
}

function stateChip(p: PairPlan, skipped: boolean): HTMLElement {
  if (p.state === "ready") return skipped ? h("span", { class: "chip" }, S.preview.skipped) : h("span", { class: "chip ok" }, icon("check"), S.preview.readyChip);
  if (p.state === "conflict") return h("span", { class: "chip warn" }, S.preview.conflictChip);
  return h("span", { class: "chip bad" }, S.preview.blockedChip);
}

function moves(p: PairPlan): HTMLElement {
  const sameFolder = dirName(p.converted) === dirName(p.finalPath);
  return h(
    "div",
    { class: "moves" },
    h("span", { class: "lbl" }, S.preview.willRename),
    h("span", { class: "break" }, h("span", { class: "muted" }, baseName(p.source)), " → ", h("b", null, baseName(p.sourceRenamed))),
    h("span", { class: "lbl" }, S.preview.willMove),
    h(
      "span",
      { class: "break" },
      h("span", { class: "muted" }, baseName(p.converted)),
      " → ",
      h("b", { style: "color:var(--neon)" }, baseName(p.finalPath)),
      h("span", { class: "muted small" }, sameFolder ? ` (${S.preview.stays})` : ` (in ${dirName(p.finalPath)})`),
    ),
  );
}

/** "Pick another name" for a pair whose final name is taken. */
function conflictBox(p: PairPlan, conflictWith: string): HTMLElement {
  const input = h("input", { type: "text", placeholder: S.preview.renamePlaceholder, value: st.finalNames[p.id] ?? "", "aria-label": S.preview.rename });
  return h(
    "div",
    { class: "banner warn" },
    icon("warn"),
    h("div", { class: "grow" }, h("div", null, S.preview.conflict(baseName(conflictWith))), h("div", { class: "small muted" }, S.preview.conflictTip)),
    h(
      "div",
      { class: "row", style: "width:100%" },
      input,
      h("span", { class: "muted" }, `.${extOf(p.converted).toLowerCase()}`),
      h(
        "button",
        {
          class: "btn small primary",
          onclick: () => {
            const v = input.value.trim();
            if (!v) return;
            st.finalNames[p.id] = v;
            void replan(planBox());
          },
        },
        S.preview.renameApply,
      ),
    ),
  );
}

function pairCard(p: PairPlan, open: boolean): HTMLElement {
  const skipped = st.skip.has(p.id);
  const lost = p.categories.filter((c) => c.enabled && c.notStorable.length);
  const toggleSkip = h(
    "button",
    {
      class: "btn small",
      onclick: (e: Event) => {
        e.preventDefault();
        if (skipped) st.skip.delete(p.id);
        else st.skip.add(p.id);
        render();
      },
    },
    skipped ? S.preview.unskip : S.preview.skip,
  );

  return h(
    "details",
    { class: "pair", open },
    h(
      "summary",
      null,
      thumb(p.converted, p.convertedKind),
      h("div", { class: "names grow" }, h("div", { class: "name" }, baseName(p.finalPath)), h("div", { class: "sub" }, `${baseName(p.source)} + ${baseName(p.converted)}`)),
      p.warnings.length ? h("span", { class: "chip warn" }, icon("warn"), String(p.warnings.length)) : null,
      stateChip(p, skipped),
      icon("chevron"),
    ),
    h(
      "div",
      { class: "body" },
      moves(p),
      p.state === "conflict" && p.conflictWith ? conflictBox(p, p.conflictWith) : null,
      p.problems.length ? h("div", { class: "notes" }, p.problems.map((x) => note("bad", x.message))) : null,
      p.warnings.length ? h("div", { class: "notes" }, p.warnings.map((x) => note("warn", x.message))) : null,
      p.tz ? note("info", S.preview.tzNote(p.tz.offset, S.preview.tzSrc[p.tz.source] + (p.tz.zone ? `, ${p.tz.zone}` : ""))) : null,
      p.diff.length ? h("div", null, h("div", { class: "small muted", style: "margin-bottom:6px" }, S.preview.diff), diffTable(p.diff)) : null,
      lost.length
        ? h(
            "details",
            { class: "more" },
            h("summary", null, S.preview.notStorable(lost.reduce((a, c) => a + c.notStorable.length, 0), extOf(p.converted))),
            h("div", { class: "small muted" }, lost.flatMap((c) => c.notStorable).join(", ")),
          )
        : null,
      p.details.length ? detailsBlock(p.details) : null,
      p.state === "ready" || p.state === "conflict" ? h("div", { class: "row" }, toggleSkip) : null,
    ),
  );
}

export function renderPreview(head: HTMLElement) {
  const box = h("div", { id: "plan" });
  add(page(), head, h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.preview.title), h("p", { class: "muted" }, S.preview.lead))), box);
  if (!st.plan) {
    void replan(box);
    return;
  }
  const plan = st.plan;
  const runnable = plan.pairs.filter((p) => p.state === "ready" && !st.skip.has(p.id));
  const anyCross = plan.pairs.some((p) => p.tz);
  const tzSelect = h(
    "select",
    {
      "aria-label": S.preview.tz,
      onchange: (e: Event) => {
        st.tz = (e.target as HTMLSelectElement).value || null;
        void replan(box);
      },
    },
    h("option", { value: "" }, S.preview.tzAuto),
    utcOffsets().map((o) => h("option", { value: o, selected: st.tz === o }, `UTC${o}`)),
  );

  const cards = plan.pairs.map((p) => pairCard(p, p.state !== "ready" || p.warnings.length > 0 || plan.pairs.length <= 3));
  box.append(
    h(
      "div",
      { class: "card stack" },
      h("div", { class: "row" }, h("h3", null, S.preview.categories), tip(S.preview.categoriesTip)),
      catChecklist(plan),
      anyCross
        ? h(
            "div",
            { class: "row", style: "margin-top:14px" },
            h("span", { class: "fieldhead" }, S.preview.tz, tip(S.preview.tzTip)),
            h("div", { style: "width:220px" }, tzSelect),
            plan.tzAssumed ? h("span", { class: "chip warn" }, S.preview.tzAssumedCount(plan.tzAssumed)) : null,
          )
        : null,
    ),
    h(
      "div",
      { class: "section-title" },
      h("h3", { class: "grow" }, S.preview.pairsTitle),
      h("span", { class: "chip ok" }, S.preview.ready(plan.ready)),
      plan.conflicts ? h("span", { class: "chip warn" }, S.preview.conflicts(plan.conflicts)) : null,
      plan.blocked ? h("span", { class: "chip bad" }, S.preview.blocked(plan.blocked)) : null,
      h(
        "button",
        {
          class: "btn small ghost",
          onclick: () => {
            const all = [...box.querySelectorAll("details.pair")] as HTMLDetailsElement[];
            const openAll = all.some((d) => !d.open);
            all.forEach((d) => (d.open = openAll));
          },
        },
        S.preview.expandAll,
      ),
    ),
    h("div", { class: "list" }, cards),
    h(
      "div",
      { class: "actions sticky-actions" },
      h(
        "button",
        {
          class: "btn ghost left",
          onclick: () => {
            st.step = STEP.pair;
            render();
          },
        },
        icon("back"),
        S.back,
      ),
      runnable.length ? null : h("span", { class: "muted" }, S.preview.nothing),
      h("button", { class: "btn primary", disabled: runnable.length === 0, onclick: () => apply(plan) }, icon("check"), S.preview.apply(runnable.length)),
    ),
  );
}

async function apply(plan: CopyPlan) {
  const skip = [...st.skip];
  st.step = STEP.run;
  clear(page());
  await runView({
    container: page(),
    header: stepper(S.steps.copy, STEP.run),
    start: () => api.runCopy(plan.planId, skip),
    onError: (e) => {
      toast(e, "bad", 8000);
      st.step = STEP.preview;
      render();
    },
    onDone: (op) => {
      st.step = STEP.done;
      doneView({
        container: page(),
        header: stepper(S.steps.copy, STEP.done),
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
