// Small reusable view pieces.
import type { DiffRow } from "../api";
import { S } from "../strings";
import { h } from "./dom";
import { icon } from "./icons";
import { tip } from "./tooltip";

export function stepper(steps: string[], now: number): HTMLElement {
  return h(
    "ol",
    { class: "stepper", "aria-label": S.progress },
    steps.map((s, i) =>
      h(
        "li",
        { class: i < now ? "done" : i === now ? "now" : "", "aria-current": i === now ? "step" : null },
        h("span", { class: "dot" }, i < now ? icon("check") : String(i + 1)),
        s,
      ),
    ),
  );
}

export function note(kind: "warn" | "bad" | "info", text: string): HTMLElement {
  return h("div", { class: `note ${kind}` }, icon(kind === "info" ? "info" : "warn"), h("span", { class: "break" }, text));
}

export function switchEl(label: string, checked: boolean, onChange: (v: boolean) => void, tipHtml?: string): HTMLElement {
  const input = h("input", { type: "checkbox", role: "switch", checked, onchange: (e: Event) => onChange((e.target as HTMLInputElement).checked) });
  return h("div", { class: "row", style: "gap:8px" }, h("label", { class: "switch" }, input, h("span", null, label)), tipHtml ? tip(tipHtml) : null);
}

export function spinner(): HTMLElement {
  return h("span", { class: "spinner", "aria-hidden": "true" });
}

export function loading(text: string): HTMLElement {
  return h("div", { class: "row muted", style: "padding:30px 0", role: "status" }, spinner(), text);
}

/** Collapsible block with raw engine messages ("Details"). */
export function detailsBlock(lines: string[], style?: string): HTMLElement {
  return h("details", { class: "more", style }, h("summary", null, S.preview.details), h("div", { class: "mono" }, lines.join("\n")));
}

/**
 * Before/after table for curated fields. By default only values that differ are highlighted;
 * `highlightAll` marks every row as changed (Edit review: every row is a field the user touched).
 */
export function diffTable(rows: DiffRow[], highlightAll = false): HTMLElement {
  return h(
    "table",
    { class: "diff" },
    h("thead", null, h("tr", null, h("th", null, S.preview.field), h("th", null, S.preview.before), h("th", null, S.preview.after))),
    h(
      "tbody",
      null,
      rows.map((r) =>
        h(
          "tr",
          null,
          h("td", null, r.label),
          h("td", null, r.before ?? "—"),
          h(
            "td",
            { class: r.after === null ? "gone" : highlightAll || r.after !== r.before ? "changed" : "" },
            r.after ?? (r.before ? S.preview.removed : "—"),
            r.note ? h("div", { class: "small", style: "color:var(--warn)" }, r.note) : null,
          ),
        ),
      ),
    ),
  );
}
