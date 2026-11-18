// Edit step 2: one input per curated field (spec 7). In bulk mode, differing values show as
// "Mixed" and only fields the user touches end up in `st.changes`.
import type { Field, FileFields, Location } from "../../api";
import { deviceOffset, utcOffsets } from "../../lib/format";
import { pickOnMap } from "../../dialogs/map-picker";
import { S } from "../../strings";
import { fillPlace, h, icon, rememberPlace, tip, toast } from "../../ui";
import { page, render, st } from "./state";

export type TextFieldName = "make" | "model" | "lens" | "title" | "description" | "author" | "copyright";

interface Common<T> {
  mixed: boolean;
  value: T | null;
}

/** The value shared by all readable files, or `mixed` when they differ. */
function common<T>(get: (f: FileFields) => T | null, eq: (a: T | null, b: T | null) => boolean = (a, b) => JSON.stringify(a) === JSON.stringify(b)): Common<T> {
  const vals = st.data.filter((d) => !d.error).map(get);
  if (!vals.length) return { mixed: false, value: null };
  const first = vals[0];
  return vals.every((v) => eq(v, first)) ? { mixed: false, value: first } : { mixed: true, value: null };
}

function supportCount(field: Field): number {
  return st.data.filter((d) => d.supported.includes(field)).length;
}

function placeholderFor(field: Field, c: Common<unknown>): string {
  if (st.changes[field]?.op === "clear") return S.edit.willClear;
  return c.mixed ? S.edit.mixed : S.edit.empty;
}

/** Common frame: label, help, "will change" badge, revert and clear buttons, support hint. */
function fieldBox(field: Field, body: HTMLElement, opts: { full?: boolean; onClear?: () => void } = {}): HTMLElement {
  const [label, tipText] = S.fields[field];
  const n = supportCount(field);
  const total = st.data.length;
  const change = st.changes[field];
  const touched = change !== undefined;
  const cleared = change?.op === "clear";
  const disabled = n === 0;
  const box = h(
    "div",
    { class: `efield ${opts.full ? "full" : ""} ${touched ? "touched" : ""} ${disabled ? "disabled" : ""}`, "data-tip": disabled ? S.edit.unsupported : null },
    h(
      "div",
      { class: "fieldhead" },
      h("span", null, label),
      tip(tipText),
      touched ? h("span", { class: "badge" }, cleared ? S.edit.willClear : S.edit.changed) : null,
      h(
        "span",
        { class: "tools" },
        touched
          ? h(
              "button",
              {
                class: "btn icon small ghost",
                "aria-label": `${S.edit.revert}: ${label}`,
                "data-tip": S.edit.revert,
                onclick: () => {
                  delete st.changes[field];
                  render();
                },
              },
              icon("undo"),
            )
          : null,
        !disabled
          ? h(
              "button",
              {
                class: "btn icon small ghost",
                "aria-label": `${S.edit.clear}: ${label}`,
                "data-tip": S.edit.clear,
                onclick: () => {
                  st.changes[field] = { op: "clear" };
                  opts.onClear?.();
                  render();
                },
              },
              icon("trash"),
            )
          : null,
      ),
    ),
    body,
    n > 0 && n < total ? h("div", { class: "small", style: "color:var(--warn)" }, S.edit.unsupportedSome(total - n)) : null,
  );
  if (disabled) box.querySelectorAll("input,select,textarea,button:not(.tipbtn)").forEach((e) => ((e as HTMLInputElement).disabled = true));
  return box;
}

export function textField(field: TextFieldName, multiline = false): HTMLElement {
  const c = common<string>((d) => d.fields[field]);
  const ch = st.changes[field];
  const value = ch?.op === "set" ? ch.value : ch?.op === "clear" ? "" : (c.value ?? "");
  const attrs = {
    placeholder: placeholderFor(field, c),
    value,
    "aria-label": S.fields[field][0],
    oninput: (e: Event) => {
      const v = (e.target as HTMLInputElement).value;
      st.changes[field] = v.trim() ? { op: "set", value: v } : { op: "clear" };
      (e.target as HTMLElement).closest(".efield")?.classList.add("touched");
    },
    onchange: () => render(),
  };
  const input = multiline ? h("textarea", attrs) : h("input", { type: "text", ...attrs });
  if (multiline) (input as HTMLTextAreaElement).value = value;
  return fieldBox(field, input, { full: multiline });
}

function setDateBody(seg: HTMLElement): HTMLElement {
  const c = common((d) => d.fields.dateTaken, (a, b) => a?.local === b?.local && a?.offset === b?.offset);
  const ch = st.changes.dateTaken;
  const offsets = utcOffsets();
  const local = ch?.op === "set" ? ch.local : (c.value?.local ?? "");
  const offset = ch?.op === "set" ? ch.offset : (c.value?.offset ?? deviceOffset());
  const dt = h("input", { type: "datetime-local", step: "1", value: local.slice(0, 19), "aria-label": S.fields.dateTaken[0] });
  const tz = h("select", { "aria-label": S.date.tz }, offsets.map((o) => h("option", { value: o, selected: o === offset }, `UTC${o}`)));
  if (!offsets.includes(offset)) tz.prepend(h("option", { value: offset, selected: true }, `UTC${offset}`));
  const update = () => {
    if (dt.value) st.changes.dateTaken = { op: "set", local: dt.value, offset: tz.value };
    render();
  };
  dt.onchange = update;
  tz.onchange = update;
  return h(
    "div",
    { class: "col" },
    seg,
    h("div", { class: "row" }, h("div", { class: "grow" }, dt), h("div", { style: "width:150px" }, tz)),
    c.mixed && !ch ? h("div", { class: "small muted" }, S.edit.mixed) : ch?.op === "clear" ? h("div", { class: "small muted" }, S.edit.willClear) : null,
    c.value && !c.value.offsetKnown && !ch && c.value.offset ? h("div", { class: "small muted" }, S.date.assumed(c.value.offset)) : null,
  );
}

function shiftDateBody(seg: HTMLElement): HTMLElement {
  const ch = st.changes.dateTaken;
  const cur = ch?.op === "shift" ? ch.shift : { days: 0, hours: 0, minutes: 0 };
  const neg = cur.days < 0 || cur.hours < 0 || cur.minutes < 0;
  const num = (label: string, v: number) =>
    h("label", { class: "field", style: "width:100px" }, h("span", { class: "small muted" }, label), h("input", { type: "number", min: "0", value: String(Math.abs(v)), "aria-label": label }));
  const d = num(S.date.days, cur.days);
  const hh = num(S.date.hours, cur.hours);
  const mm = num(S.date.minutes, cur.minutes);
  const dir = h("select", { "aria-label": S.date.direction }, h("option", { value: "+", selected: !neg }, S.date.later), h("option", { value: "-", selected: neg }, S.date.earlier));
  const update = () => {
    const sign = dir.value === "-" ? -1 : 1;
    const val = (el: HTMLElement) => Math.abs(parseInt((el.querySelector("input") as HTMLInputElement).value || "0", 10) || 0) * sign;
    const shift = { days: val(d), hours: val(hh), minutes: val(mm) };
    if (shift.days || shift.hours || shift.minutes) st.changes.dateTaken = { op: "shift", shift };
    else delete st.changes.dateTaken;
    render();
  };
  [d, hh, mm].forEach((el) => ((el.querySelector("input") as HTMLInputElement).onchange = update));
  dir.onchange = update;
  return h("div", { class: "col" }, h("div", { class: "row" }, seg, tip(S.date.shiftTip)), h("div", { class: "row", style: "align-items:flex-end" }, d, hh, mm, h("div", { style: "width:130px" }, dir)));
}

/** Date taken: "Set to" an exact time + timezone, or "Shift by" a relative amount (D10). */
export function dateField(): HTMLElement {
  const ch = st.changes.dateTaken;
  const segBtn = (mode: "set" | "shift", label: string) =>
    h(
      "button",
      {
        "aria-pressed": st.dateMode === mode ? "true" : "false",
        onclick: () => {
          st.dateMode = mode;
          if (ch && ch.op !== "clear") delete st.changes.dateTaken;
          render();
        },
      },
      label,
    );
  const seg = h("div", { class: "segmented", role: "group", "aria-label": S.fields.dateTaken[0] }, segBtn("set", S.date.set), segBtn("shift", S.date.shift));
  const body = st.dateMode === "set" ? setDateBody(seg) : shiftDateBody(seg);
  return fieldBox("dateTaken", body, { full: true, onClear: () => (st.dateMode = "set") });
}

/** Parses "48.85, 2.29", "48.85 N 2.29 E" and similar. */
function parseCoords(s: string): [number, number] | null {
  const m = s.match(/(-?\d+(?:\.\d+)?)\s*°?\s*([NS])?[\s,;]+(-?\d+(?:\.\d+)?)\s*°?\s*([EW])?/i);
  if (!m) return null;
  let lat = parseFloat(m[1]);
  let lon = parseFloat(m[3]);
  if (m[2]?.toUpperCase() === "S") lat = -Math.abs(lat);
  if (m[4]?.toUpperCase() === "W") lon = -Math.abs(lon);
  if (Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
  return [lat, lon];
}

/** Location: one coordinates box (type or paste "lat, lon"), optional altitude, and the map picker (D17). */
export function locationField(): HTMLElement {
  const c = common<Location>((d) => d.fields.location);
  const ch = st.changes.location;
  const cur: Location | null = ch?.op === "set" ? { lat: ch.lat, lon: ch.lon, alt: ch.alt } : ch?.op === "clear" ? null : c.value;
  const fmt = (l: Location) => `${+l.lat.toFixed(6)}, ${+l.lon.toFixed(6)}`;
  const coords = h("input", {
    type: "text",
    class: "mono-input",
    placeholder: ch?.op === "clear" || c.mixed ? placeholderFor("location", c) : S.loc.coordsHint,
    value: cur ? fmt(cur) : "",
    "aria-label": S.loc.coords,
    spellcheck: false,
  });
  const alt = h("input", { type: "number", step: "any", placeholder: S.loc.altHint, value: cur?.alt != null ? String(Math.round(cur.alt)) : "", "aria-label": S.loc.alt });
  const commit = () => {
    const v = coords.value.trim();
    if (!v) {
      if (cur) st.changes.location = { op: "clear" };
      render();
      return;
    }
    const r = parseCoords(v);
    if (!r) {
      toast(S.loc.badPaste, "warn");
      return;
    }
    const al = parseFloat(alt.value);
    st.changes.location = { op: "set", lat: r[0], lon: r[1], alt: Number.isFinite(al) ? al : null };
    render();
  };
  coords.onchange = commit;
  alt.onchange = () => {
    if (!cur && !coords.value.trim()) return;
    commit();
  };
  const mapBtn = h(
    "button",
    {
      class: "btn",
      type: "button",
      onclick: async () => {
        const r = await pickOnMap(cur);
        if (r) {
          if (r.label) rememberPlace(r.lat, r.lon, r.label);
          st.changes.location = { op: "set", lat: r.lat, lon: r.lon, alt: cur?.alt ?? null };
          render();
        }
      },
    },
    icon("pin"),
    S.loc.map,
  );
  const place = cur ? fillPlace(h("div", { class: "placeline" }), cur.lat, cur.lon) : null;
  return fieldBox(
    "location",
    h(
      "div",
      { class: "col", style: "gap:6px" },
      h(
        "div",
        { class: "locrow" },
        h("div", { class: "grow" }, coords),
        h("label", { class: "altbox", "data-tip": S.loc.alt }, alt, h("span", { class: "unit" }, "m")),
        mapBtn,
      ),
      place,
    ),
    { full: true },
  );
}

export function keywordsField(): HTMLElement {
  const c = common<string[]>((d) => d.fields.keywords);
  const ch = st.changes.keywords;
  const list: string[] = ch?.op === "set" ? [...ch.values] : ch?.op === "clear" ? [] : [...(c.value ?? [])];
  const input = h("input", { type: "text", placeholder: list.length ? "" : placeholderFor("keywords", c), "aria-label": S.fields.keywords[0] });
  const set = (vals: string[]) => {
    st.changes.keywords = vals.length ? { op: "set", values: vals } : { op: "clear" };
    render();
    (page().querySelector(".kw input") as HTMLInputElement | null)?.focus();
  };
  input.onkeydown = (e: KeyboardEvent) => {
    const v = input.value.trim().replace(/,$/, "");
    if ((e.key === "Enter" || e.key === ",") && v) {
      e.preventDefault();
      if (!list.includes(v)) set([...list, v]);
      else input.value = "";
    } else if (e.key === "Backspace" && !input.value && list.length) {
      set(list.slice(0, -1));
    }
  };
  input.onblur = () => {
    const v = input.value.trim();
    if (v && !list.includes(v)) set([...list, v]);
  };
  const box = h(
    "div",
    { class: "kw" },
    list.map((k) => h("span", { class: "tag" }, k, h("button", { type: "button", "aria-label": S.edit.remove(k), onclick: () => set(list.filter((x) => x !== k)) }, icon("x")))),
    input,
  );
  return fieldBox("keywords", box, { full: true });
}

export function ratingField(): HTMLElement {
  const c = common<number>((d) => d.fields.rating);
  const ch = st.changes.rating;
  const val = ch?.op === "set" ? ch.value : ch?.op === "clear" ? 0 : (c.value ?? 0);
  const stars = h(
    "div",
    { class: "stars", role: "radiogroup", "aria-label": S.fields.rating[0] },
    [1, 2, 3, 4, 5].map((n) =>
      h(
        "button",
        {
          type: "button",
          class: n <= val ? "on" : "",
          role: "radio",
          "aria-checked": n === val ? "true" : "false",
          "aria-label": S.edit.stars(n),
          onclick: () => {
            st.changes.rating = { op: "set", value: n };
            render();
          },
        },
        icon("star"),
      ),
    ),
    c.mixed && !ch ? h("span", { class: "small muted", style: "align-self:center;margin-left:6px" }, S.edit.mixed) : null,
  );
  return fieldBox("rating", stars);
}
