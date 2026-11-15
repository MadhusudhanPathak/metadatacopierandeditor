// Copy step 1: pick the Source and Converted files or folders.
import { open } from "@tauri-apps/plugin-dialog";
import { api, errText, type FileInfo } from "../../api";
import { baseName } from "../../lib/paths";
import { go } from "../../router";
import { S } from "../../strings";
import { add, dropZone, h, icon, switchEl, tip, toast } from "../../ui";
import { doPair } from "./pair";
import { page, render, st, type Side } from "./state";

/** Expands a side's picked paths into files. A Converted folder inside the Source folder is excluded from the Source. */
async function loadSide(side: Side, isSource: boolean) {
  let exclude: string | null = null;
  if (isSource && st.conv.folder && st.src.folder && st.conv.folder.toLowerCase().startsWith(st.src.folder.toLowerCase())) exclude = st.conv.folder;
  side.files = side.picked.length ? await api.expandPaths(side.picked, st.subfolders, exclude) : [];
}

async function setSide(isSource: boolean, paths: string[], folder: string | null) {
  const side = isSource ? st.src : st.conv;
  side.picked = paths;
  side.folder = folder;
  try {
    await loadSide(side, isSource);
    if (!isSource && st.src.folder) await loadSide(st.src, true);
    if (folder && side.files.length === 0) toast(S.pick.emptyFolder, "warn");
    if (isSource && folder) {
      const sug = await api.suggestConverted(folder);
      st.suggestion = sug && sug !== st.conv.folder ? sug : null;
    }
  } catch (e) {
    toast(errText(e), "bad");
  }
  render();
}

function sideZone(isSource: boolean): HTMLElement {
  const side = isSource ? st.src : st.conv;
  const title = isSource ? S.pick.source : S.pick.converted;
  const sub = isSource ? S.pick.sourceSub : S.pick.convertedSub;
  const chooseFolder = async () => {
    const d = await open({ directory: true, multiple: false, title });
    if (typeof d === "string") await setSide(isSource, [d], d);
  };
  const chooseFiles = async () => {
    const f = await open({ directory: false, multiple: true, title });
    if (Array.isArray(f) && f.length) await setSide(isSource, f, null);
    else if (typeof f === "string") await setSide(isSource, [f], null);
  };
  const zone = h(
    "section",
    { class: `drop ${side.files.length ? "filled" : ""}`, "aria-label": title },
    h(
      "div",
      { class: "what" },
      icon(isSource ? "shield" : "copy"),
      h("div", { class: "grow" }, h("h2", null, title), h("p", { class: "muted small" }, sub)),
      tip(isSource ? S.pick.sourceTip : S.pick.convertedTip),
    ),
    side.picked.length
      ? h(
          "div",
          { class: "chosen" },
          h(
            "div",
            { class: "row" },
            icon(side.folder ? "folder" : "file"),
            h("b", { class: "grow break" }, side.folder ? baseName(side.folder) : S.pick.files(side.files.length)),
            h("button", { class: "btn small ghost", onclick: () => setSide(isSource, [], null) }, S.pick.clear),
          ),
          side.folder ? h("div", { class: "mono muted", style: "margin-top:4px" }, side.folder) : null,
          h("div", { class: "small muted", style: "margin-top:4px" }, S.pick.files(side.files.length)),
        )
      : h("div", { class: "empty" }, h("div", null, h("p", null, S.pick.drop), h("p", { class: "small" }, S.pick.dropOr))),
    h(
      "div",
      { class: "row" },
      h("button", { class: "btn", onclick: chooseFolder }, icon("folder"), S.pick.chooseFolder),
      h("button", { class: "btn", onclick: chooseFiles }, icon("file"), S.pick.chooseFiles),
    ),
  );
  dropZone(zone, async (paths) => {
    const single = paths.length === 1 ? paths[0] : null;
    const files = await api.expandPaths(paths, false).catch(() => [] as FileInfo[]);
    const isFolder = single !== null && !files.some((f) => f.path === single);
    await setSide(isSource, paths, isFolder ? single : null);
  });
  return zone;
}

export function renderPick(head: HTMLElement) {
  const ready = st.src.files.length > 0 && st.conv.files.length > 0;
  const suggestion = st.suggestion;
  add(
    page(),
    head,
    h("div", { class: "pagehead" }, h("div", null, h("h1", null, S.pick.title), h("p", { class: "muted" }, S.pick.lead))),
    suggestion
      ? h(
          "div",
          { class: "banner", style: "margin-bottom:16px" },
          icon("folder"),
          h("span", { class: "grow" }, S.pick.suggest(baseName(suggestion))),
          h("button", { class: "btn small primary", onclick: () => setSide(false, [suggestion], suggestion) }, S.pick.useIt),
        )
      : null,
    h("div", { class: "grid2" }, sideZone(true), sideZone(false)),
    h(
      "div",
      { style: "margin-top:16px" },
      switchEl(
        S.pick.subfolders,
        st.subfolders,
        async (v) => {
          st.subfolders = v;
          try {
            await loadSide(st.src, true);
            await loadSide(st.conv, false);
          } catch (e) {
            toast(errText(e), "bad");
          }
          render();
        },
        S.pick.subfoldersTip,
      ),
    ),
    h(
      "div",
      { class: "actions" },
      h("button", { class: "btn ghost left", onclick: () => go("home") }, icon("back"), S.back),
      h("button", { class: "btn primary", disabled: !ready, onclick: () => doPair() }, S.next, icon("right")),
    ),
  );
}
