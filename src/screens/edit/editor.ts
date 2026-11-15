// Edit step 2: the field editor with the file list and file info.
import { fmtBytes, fmtDuration } from "../../lib/format";
import { S } from "../../strings";
import { add, h, icon, note, switchEl, thumb } from "../../ui";
import { dateField, keywordsField, locationField, ratingField, textField } from "./fields";
import { page, render, st, STEP } from "./state";

function fileInfo(): HTMLElement | null {
  if (st.data.length !== 1) return null;
  const d = st.data[0];
  return h(
    "div",
    { class: "card tight stack" },
    h("h3", null, S.edit.fileInfo),
    h(
      "dl",
      { class: "fileinfo" },
      h("dt", null, S.edit.type),
      h("dd", null, d.fileType ?? "—"),
      h("dt", null, S.edit.size),
      h("dd", null, fmtBytes(d.size)),
      d.dimensions ? [h("dt", null, S.edit.dims), h("dd", null, `${d.dimensions[0]} × ${d.dimensions[1]}`)] : null,
      d.duration ? [h("dt", null, S.edit.duration), h("dd", null, fmtDuration(d.duration))] : null,
    ),
    d.error ? note("bad", d.error) : null,
  );
}

function fileList(): HTMLElement {
  const single = st.data.length === 1;
  return h(
    "div",
    { class: "list", style: "max-height:46vh;overflow:auto" },
    st.data.map((d) =>
      h(
        "div",
        { class: "item" },
        thumb(d.path, d.kind, single),
        h("div", { class: "names" }, h("div", { class: "name" }, d.name), h("div", { class: "sub" }, d.error ?? `${d.fileType ?? ""} · ${fmtBytes(d.size)}`)),
        d.error ? h("span", { class: "chip bad" }, icon("warn")) : null,
      ),
    ),
  );
}

function review() {
  // Blur first so a text field still being typed in commits its value.
  (document.activeElement as HTMLElement | null)?.blur();
  st.step = STEP.review;
  st.plan = null;
  render();
}

export function renderEditor(head: HTMLElement) {
  const n = st.data.length;
  const touched = Object.keys(st.changes).length;
  add(
    page(),
    head,
    h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.edit.title(n)), h("p", { class: "muted" }, n === 1 ? S.edit.lead1 : S.edit.leadN))),
    h(
      "div",
      { class: "split" },
      h("div", { class: "col" }, fileInfo(), h("div", { class: "card tight" }, h("h3", { style: "margin-bottom:10px" }, `${S.edit.files} (${n})`), fileList())),
      h(
        "div",
        { class: "col" },
        h(
          "div",
          { class: "fields" },
          dateField(),
          locationField(),
          textField("make"),
          textField("model"),
          textField("lens"),
          ratingField(),
          textField("title"),
          textField("author"),
          textField("description", true),
          keywordsField(),
          textField("copyright"),
        ),
        h(
          "div",
          { class: "card tight col" },
          switchEl(S.edit.setFileDates, st.setFileDates, (v) => (st.setFileDates = v), S.edit.setFileDatesTip),
          note("info", S.edit.backupNote),
        ),
      ),
    ),
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
      touched ? null : h("span", { class: "muted small" }, S.edit.nothingTouched),
      h("button", { class: "btn primary", disabled: touched === 0, onclick: review }, S.edit.review, icon("right")),
    ),
  );
}
