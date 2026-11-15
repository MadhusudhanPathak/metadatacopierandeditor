// Toasts, modal dialogs and confirmation prompts.
import { S } from "../strings";
import { h } from "./dom";

const toasts = h("div", { id: "toasts", role: "status", "aria-live": "polite" });

export function initToasts() {
  document.body.append(toasts);
}

export function toast(text: string, kind: "ok" | "warn" | "bad" = "ok", ms = 4500) {
  const t = h("div", { class: `toast ${kind === "ok" ? "" : kind}` }, text);
  toasts.append(t);
  window.setTimeout(() => t.remove(), ms);
}

/** Shows `content` in a modal. Escape or a click outside closes it. Returns the close function. */
export function modal(content: Node, opts: { wide?: boolean; label?: string; onClose?: () => void } = {}): () => void {
  const prev = document.activeElement as HTMLElement | null;
  const box = h("div", { class: `modal ${opts.wide ? "wide" : ""}`, role: "dialog", "aria-modal": "true", "aria-label": opts.label ?? "" }, content);
  const ov = h("div", { class: "overlay" }, box);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    ov.remove();
    document.removeEventListener("keydown", esc);
    prev?.focus();
    opts.onClose?.();
  };
  const esc = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  document.addEventListener("keydown", esc);
  ov.addEventListener("mousedown", (e) => {
    if (e.target === ov) close();
  });
  document.body.append(ov);
  (box.querySelector("button.primary, button, input, select, textarea") as HTMLElement | null)?.focus();
  return close;
}

/** Yes/no prompt. Resolves false when dismissed. */
export function confirmBox(text: string, ok: string, danger = false): Promise<boolean> {
  return new Promise((resolve) => {
    let answered = false;
    const done = (v: boolean) => {
      answered = true;
      close();
      resolve(v);
    };
    const body = h(
      "div",
      { class: "stack" },
      h("p", null, text),
      h(
        "div",
        { class: "actions" },
        h("button", { class: "btn", onclick: () => done(false) }, S.cancel),
        h("button", { class: `btn ${danger ? "danger" : "primary"}`, onclick: () => done(true) }, ok),
      ),
    );
    const close = modal(body, {
      onClose: () => {
        if (!answered) resolve(false);
      },
    });
  });
}
