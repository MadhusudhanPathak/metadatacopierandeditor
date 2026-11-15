// Settings (spec 10). Every change is saved immediately.
import { api, errText, type PairMode, type Settings } from "../api";
import { go } from "../router";
import { S } from "../strings";
import { clear, h, icon, loading, note, switchEl, tip, toast } from "../ui";

const PAIR_MODES: PairMode[] = ["auto", "exact", "anyExt", "manual"];

export async function settingsScreen(page: HTMLElement) {
  page.append(
    h(
      "div",
      { class: "pagehead" },
      h("div", null, h("h1", null, S.settings.title), h("p", { class: "muted" }, S.settings.lead)),
      h("button", { class: "btn ghost", onclick: () => go("home") }, icon("back"), S.back),
    ),
  );
  const box = h("div", { class: "stack" }, loading(""));
  page.append(box);
  let s: Settings;
  try {
    s = await api.getSettings();
  } catch (e) {
    clear(box);
    box.append(note("bad", errText(e)));
    return;
  }
  const save = async () => {
    try {
      s = await api.setSettings(s);
    } catch (e) {
      toast(errText(e), "bad");
    }
  };

  const modeSel = h(
    "select",
    {
      "aria-label": S.settings.pairing,
      onchange: (e: Event) => {
        s.pairingMode = (e.target as HTMLSelectElement).value as PairMode;
        void save();
      },
    },
    PAIR_MODES.map((m) => h("option", { value: m, selected: s.pairingMode === m }, S.pair.modes[m])),
  );
  const failSel = h(
    "select",
    {
      "aria-label": S.settings.onFailure,
      onchange: (e: Event) => {
        s.stopOnFailure = (e.target as HTMLSelectElement).value === "stop";
        void save();
      },
    },
    h("option", { value: "continue", selected: !s.stopOnFailure }, S.settings.continue),
    h("option", { value: "stop", selected: s.stopOnFailure }, S.settings.stop),
  );
  clear(box);
  box.append(
    h(
      "div",
      { class: "card stack" },
      h("label", { class: "field" }, h("span", { class: "fieldhead" }, S.settings.pairing, tip(S.pair.modeTips.auto)), h("div", { style: "max-width:320px" }, modeSel)),
      switchEl(
        S.settings.subfolders,
        s.includeSubfolders,
        (v) => {
          s.includeSubfolders = v;
          void save();
        },
        S.pick.subfoldersTip,
      ),
      h("label", { class: "field" }, h("span", { class: "fieldhead" }, S.settings.onFailure, tip(S.settings.onFailureTip)), h("div", { style: "max-width:320px" }, failSel)),
    ),
    h("div", null, h("button", { class: "btn", onclick: () => go("about") }, icon("info"), S.settings.about)),
  );
}
