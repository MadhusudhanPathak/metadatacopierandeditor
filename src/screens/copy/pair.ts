// Copy step 2: automatic pairing plus manual pairing of leftovers (tap or drag).
import { api, errText, type Leftover, type PairMode } from "../../api";
import { baseName, dirName } from "../../lib/paths";
import { S } from "../../strings";
import { add, h, icon, thumb, tip, toast } from "../../ui";
import { kindOf, nextPairId, page, render, st, STEP, type IdPair, type SideKey } from "./state";

const MODES: PairMode[] = ["auto", "exact", "anyExt", "manual"];
/** Pointer travel (px) before a press becomes a drag. */
const DRAG_THRESHOLD = 8;

/** Pairs the picked files with the current mode and shows the Pair step. */
export async function doPair() {
  try {
    const r = await api.pairFiles(
      st.src.files.map((f) => f.path),
      st.conv.files.map((f) => f.path),
      st.mode,
    );
    st.pairs = r.pairs.map((p) => ({ ...p, id: nextPairId() }));
    st.srcLeft = r.sourcesLeft;
    st.convLeft = r.convertedLeft;
    st.picking = null;
    st.step = STEP.pair;
  } catch (e) {
    toast(errText(e), "bad");
  }
  render();
}

function makePair(source: string, converted: string) {
  st.pairs.push({ source, converted, how: "manual", id: nextPairId() });
  st.srcLeft = st.srcLeft.filter((l) => l.path !== source);
  st.convLeft = st.convLeft.filter((l) => l.path !== converted);
  st.picking = null;
  render();
}

function unpair(p: IdPair) {
  st.pairs = st.pairs.filter((x) => x.id !== p.id);
  st.srcLeft.push({ path: p.source, status: "unmatched", candidates: [] });
  st.convLeft.push({ path: p.converted, status: "unmatched", candidates: [] });
  render();
}

function pairRow(p: IdPair): HTMLElement {
  const how = p.how === "exact" ? S.pair.matchedExact : p.how === "anyExt" ? S.pair.matchedAny : S.pair.matchedManual;
  return h(
    "div",
    { class: "item" },
    thumb(p.source, kindOf(p.source, st.src)),
    h("div", { class: "names" }, h("div", { class: "name" }, baseName(p.source)), h("div", { class: "sub" }, dirName(p.source))),
    h("span", { class: "arrow" }, icon("right")),
    thumb(p.converted, kindOf(p.converted, st.conv)),
    h("div", { class: "names" }, h("div", { class: "name" }, baseName(p.converted)), h("div", { class: "sub" }, dirName(p.converted))),
    h("span", { class: `chip ${p.how === "manual" ? "moss" : "ok"}` }, icon("check"), how),
    h("button", { class: "btn icon small ghost", "aria-label": `${S.pair.unpair} ${baseName(p.source)}`, "data-tip": S.pair.unpair, onclick: () => unpair(p) }, icon("unlink")),
  );
}

/**
 * Pointer-based drag between the two "needs help" columns. HTML5 drag-and-drop is unavailable
 * because Tauri's OS file drop is enabled (T13).
 */
function enableDrag(el: HTMLElement, side: SideKey, path: string) {
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const startX = e.clientX;
    const startY = e.clientY;
    let ghost: HTMLElement | null = null;
    let over: HTMLElement | null = null;
    const move = (m: PointerEvent) => {
      if (!ghost && Math.hypot(m.clientX - startX, m.clientY - startY) < DRAG_THRESHOLD) return;
      if (!ghost) {
        ghost = el.cloneNode(true) as HTMLElement;
        Object.assign(ghost.style, { position: "fixed", pointerEvents: "none", opacity: "0.85", width: `${el.offsetWidth}px`, zIndex: "1000" });
        document.body.append(ghost);
      }
      ghost.style.left = `${m.clientX - 20}px`;
      ghost.style.top = `${m.clientY - 20}px`;
      const target = (document.elementFromPoint(m.clientX, m.clientY) as HTMLElement | null)?.closest(`[data-side="${side === "s" ? "c" : "s"}"]`) as HTMLElement | null;
      if (over !== target) {
        over?.classList.remove("dragover");
        target?.classList.add("dragover");
        over = target;
      }
    };
    const up = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      ghost?.remove();
      over?.classList.remove("dragover");
      const partner = over?.dataset.path;
      if (ghost && partner) {
        if (side === "s") makePair(path, partner);
        else makePair(partner, path);
      }
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  });
}

function leftItem(l: Leftover, side: SideKey): HTMLElement {
  const sel = st.picking?.path === l.path;
  const sideObj = side === "s" ? st.src : st.conv;
  const choose = () => {
    if (st.picking && st.picking.side !== side) {
      if (side === "c") makePair(st.picking.path, l.path);
      else makePair(l.path, st.picking.path);
      return;
    }
    st.picking = sel ? null : { side, path: l.path };
    render();
  };
  const chip =
    l.status === "ambiguous"
      ? h("span", { class: "chip warn", "data-tip": `${S.pair.ambiguous}:<br>${l.candidates.map((c) => baseName(c)).join("<br>")}` }, S.pair.ambiguous)
      : h("span", { class: "chip" }, S.pair.unmatched);
  const el = h(
    "div",
    {
      class: `item clickable ${sel ? "sel" : ""}`,
      role: "button",
      tabindex: "0",
      "data-side": side,
      "data-path": l.path,
      "aria-pressed": sel ? "true" : "false",
      onclick: choose,
      onkeydown: (e: KeyboardEvent) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          choose();
        }
      },
    },
    thumb(l.path, kindOf(l.path, sideObj)),
    h("div", { class: "names" }, h("div", { class: "name" }, baseName(l.path)), h("div", { class: "sub" }, dirName(l.path))),
    chip,
  );
  enableDrag(el, side, l.path);
  return el;
}

export function renderPair(head: HTMLElement) {
  const seg = h(
    "div",
    { class: "segmented", role: "group", "aria-label": S.pair.modeLabel },
    MODES.map((m) =>
      h(
        "button",
        {
          "aria-pressed": st.mode === m ? "true" : "false",
          "data-tip": S.pair.modeTips[m],
          onclick: () => {
            st.mode = m;
            void doPair();
          },
        },
        S.pair.modes[m],
      ),
    ),
  );
  const needs = st.srcLeft.length + st.convLeft.length > 0;
  add(
    page(),
    head,
    h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.pair.title), h("p", { class: "muted" }, S.pair.lead)), seg),
    h("div", { class: "banner", style: "margin-bottom:14px" }, icon("info"), h("span", { class: "grow small", html: S.pair.namingHelp })),
    h("div", { class: "section-title" }, h("h3", null, S.pair.pairs(st.pairs.length))),
    st.pairs.length ? h("div", { class: "list" }, st.pairs.map(pairRow)) : h("p", { class: "muted" }, S.pair.none),
    needs
      ? h(
          "div",
          null,
          h("div", { class: "section-title" }, h("h3", null, S.pair.needsHelp), tip(S.pair.needsHelpTip), st.picking ? h("span", { class: "chip ok" }, S.pair.pickPartner) : null),
          h(
            "div",
            { class: "columns" },
            h("div", null, h("div", { class: "col-title" }, S.pair.leftSources), h("div", { class: "list" }, st.srcLeft.map((l) => leftItem(l, "s")))),
            h("div", null, h("div", { class: "col-title" }, S.pair.leftConverted), h("div", { class: "list" }, st.convLeft.map((l) => leftItem(l, "c")))),
          ),
        )
      : st.pairs.length
        ? h("p", { class: "muted", style: "margin-top:12px" }, S.pair.allPaired)
        : null,
    h(
      "div",
      { class: "actions sticky-actions" },
      h(
        "button",
        {
          class: "btn ghost left",
          onclick: () => {
            st.step = STEP.pick;
            render();
          },
        },
        icon("back"),
        S.back,
      ),
      h(
        "button",
        {
          class: "btn primary",
          disabled: st.pairs.length === 0,
          onclick: () => {
            st.step = STEP.preview;
            st.plan = null;
            render();
          },
        },
        S.pair.toPreview,
        icon("right"),
      ),
    ),
  );
}
