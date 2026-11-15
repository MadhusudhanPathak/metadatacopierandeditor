// Edit step 1: pick files or folders.
import { open } from "@tauri-apps/plugin-dialog";
import { api, errText } from "../../api";
import { fmtBytes } from "../../lib/format";
import { dirName } from "../../lib/paths";
import { go } from "../../router";
import { S } from "../../strings";
import { add, clear, dropZone, h, icon, loading, stepper, switchEl, thumb, toast } from "../../ui";
import { page, render, st, STEP } from "./state";

async function addPaths(paths: string[]) {
  st.picked = [...new Set([...st.picked, ...paths])];
  try {
    st.files = await api.expandPaths(st.picked, st.subfolders);
  } catch (e) {
    toast(errText(e), "bad");
  }
  render();
}

function fileList(): HTMLElement {
  return h(
    "div",
    { class: "list", style: "max-height:300px;overflow:auto" },
    st.files.map((f) =>
      h(
        "div",
        { class: "item" },
        thumb(f.path, f.kind),
        h("div", { class: "names" }, h("div", { class: "name" }, f.name), h("div", { class: "sub" }, `${dirName(f.path)} · ${fmtBytes(f.size)}`)),
        h(
          "button",
          {
            class: "btn icon small ghost",
            "aria-label": S.edit.remove(f.name),
            onclick: () => {
              st.files = st.files.filter((x) => x.path !== f.path);
              st.picked = st.picked.filter((x) => x !== f.path);
              render();
            },
          },
          icon("x"),
        ),
      ),
    ),
  );
}

export function renderPick(head: HTMLElement) {
  const zone = h(
    "section",
    { class: `drop ${st.files.length ? "filled" : ""}`, "aria-label": S.edit.pickTitle },
    h("div", { class: "what" }, icon("edit"), h("h2", { class: "grow" }, S.edit.files)),
    st.files.length ? fileList() : h("div", { class: "empty" }, h("p", null, S.edit.drop)),
    h(
      "div",
      { class: "row" },
      h(
        "button",
        {
          class: "btn",
          onclick: async () => {
            const f = await open({ multiple: true, title: S.edit.pickTitle });
            if (Array.isArray(f)) await addPaths(f);
            else if (typeof f === "string") await addPaths([f]);
          },
        },
        icon("file"),
        S.pick.chooseFiles,
      ),
      h(
        "button",
        {
          class: "btn",
          onclick: async () => {
            const d = await open({ directory: true, title: S.edit.pickTitle });
            if (typeof d === "string") await addPaths([d]);
          },
        },
        icon("folder"),
        S.pick.chooseFolder,
      ),
      st.files.length
        ? h(
            "button",
            {
              class: "btn ghost",
              onclick: () => {
                st.files = [];
                st.picked = [];
                render();
              },
            },
            S.pick.clear,
          )
        : null,
    ),
  );
  dropZone(zone, addPaths);
  add(
    page(),
    head,
    h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.edit.pickTitle), h("p", { class: "muted" }, S.edit.pickLead))),
    zone,
    h(
      "div",
      { style: "margin-top:16px" },
      switchEl(
        S.pick.subfolders,
        st.subfolders,
        async (v) => {
          st.subfolders = v;
          if (st.picked.length) st.files = await api.expandPaths(st.picked, v).catch(() => st.files);
          render();
        },
        S.pick.subfoldersTip,
      ),
    ),
    h(
      "div",
      { class: "actions" },
      h("button", { class: "btn ghost left", onclick: () => go("home") }, icon("back"), S.back),
      h("button", { class: "btn primary", disabled: st.files.length === 0, onclick: () => loadFields() }, S.next, icon("right")),
    ),
  );
}

/** Reads the curated fields of every picked file, then shows the editor. */
async function loadFields() {
  st.step = STEP.edit;
  clear(page());
  add(page(), stepper(S.steps.edit, STEP.edit), loading(S.preview.loading));
  try {
    st.data = await api.readEditFields(st.files.map((f) => f.path));
    st.changes = {};
  } catch (e) {
    toast(errText(e), "bad", 8000);
    st.step = STEP.pick;
  }
  render();
}
