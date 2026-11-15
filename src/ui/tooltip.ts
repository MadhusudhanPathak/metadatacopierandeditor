// Tooltips for any element with `data-tip` (hover, keyboard focus, tap on "i", long-press).
import { S } from "../strings";
import { h } from "./dom";

const tipEl = h("div", { id: "tooltip", role: "tooltip" });
let tipFor: HTMLElement | null = null;
let hoverTimer = 0;

function showTip(target: HTMLElement) {
  const html = target.dataset.tip;
  if (!html) return;
  tipFor = target;
  tipEl.innerHTML = html;
  tipEl.classList.add("show");
  if (target.classList.contains("tipbtn")) target.setAttribute("aria-expanded", "true");
  const r = target.getBoundingClientRect();
  const t = tipEl.getBoundingClientRect();
  let x = r.left + r.width / 2 - t.width / 2;
  x = Math.max(10, Math.min(x, window.innerWidth - t.width - 10));
  let y = r.bottom + 8;
  if (y + t.height > window.innerHeight - 10) y = r.top - t.height - 8;
  tipEl.style.left = `${x}px`;
  tipEl.style.top = `${Math.max(10, y)}px`;
}

function hideTip() {
  window.clearTimeout(hoverTimer);
  if (tipFor?.classList.contains("tipbtn")) tipFor.setAttribute("aria-expanded", "false");
  tipFor = null;
  tipEl.classList.remove("show");
}

export function initTooltips() {
  document.body.append(tipEl);
  const find = (e: Event) => (e.target as HTMLElement | null)?.closest?.("[data-tip]") as HTMLElement | null;
  document.addEventListener("mouseover", (e) => {
    const t = find(e);
    if (!t || t === tipFor) return;
    window.clearTimeout(hoverTimer);
    hoverTimer = window.setTimeout(() => showTip(t), t.classList.contains("tipbtn") ? 80 : 450);
  });
  document.addEventListener("mouseout", (e) => {
    const t = find(e);
    if (t && !t.contains(e.relatedTarget as Node)) hideTip();
  });
  document.addEventListener("focusin", (e) => {
    const t = find(e);
    if (t && (e.target as HTMLElement).matches(":focus-visible")) showTip(t);
  });
  document.addEventListener("focusout", hideTip);
  // A tap on an "i" button toggles its tooltip; a long-press shows any tooltip (touch screens).
  document.addEventListener("click", (e) => {
    const t = find(e);
    if (t?.classList.contains("tipbtn")) {
      e.preventDefault();
      if (tipFor === t) hideTip();
      else showTip(t);
    } else if (tipFor) hideTip();
  });
  let press = 0;
  document.addEventListener(
    "touchstart",
    (e) => {
      const t = find(e);
      if (!t) return;
      press = window.setTimeout(() => showTip(t), 500);
    },
    { passive: true },
  );
  document.addEventListener("touchend", () => window.clearTimeout(press), { passive: true });
  document.addEventListener("scroll", hideTip, true);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") hideTip();
  });
}

/** Small "i" button that explains something (spec 8.4). */
export function tip(html: string, label = S.moreInfo): HTMLButtonElement {
  return h("button", { type: "button", class: "tipbtn", "data-tip": html, "aria-label": `${label}: ${html.replace(/<[^>]+>/g, "")}`, "aria-expanded": "false" }, "i");
}
