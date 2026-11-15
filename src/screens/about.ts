// About: versions, privacy, platform limits and third-party licences (spec 4.1, 3.2).
import notices from "../../THIRD_PARTY_NOTICES.md?raw";
import { api } from "../api";
import { go } from "../router";
import { S } from "../strings";
import { clear, h, icon, loading } from "../ui";

export async function aboutScreen(page: HTMLElement) {
  page.append(
    h(
      "div",
      { class: "pagehead" },
      h("div", null, h("h1", null, S.about.title)),
      h("button", { class: "btn ghost", onclick: () => go("settings") }, icon("back"), S.back),
    ),
  );
  const box = h("div", { class: "stack" }, loading(""));
  page.append(box);
  const a = await api.about().catch(() => null);
  clear(box);
  const limits = a?.platform === "android" ? S.about.limitAndroid : S.about.limitWindows;
  box.append(
    h(
      "div",
      { class: "card stack" },
      h("h2", null, S.app),
      h(
        "dl",
        { class: "kv" },
        h("dt", null, S.about.version),
        h("dd", null, a?.appVersion ?? "—"),
        h("dt", null, S.about.engine),
        h("dd", null, S.about.engineName(a?.exiftoolVersion ?? "—")),
        h("dt", null, S.about.data),
        h("dd", { class: "mono" }, a?.dataDir ?? "—"),
      ),
      h("p", { class: "row", style: "gap:8px" }, icon("shield"), S.about.privacy),
      h("p", { class: "muted" }, S.about.licence),
    ),
    h("div", { class: "card stack" }, h("h3", null, S.about.limits), h("ul", { style: "margin:0;padding-left:20px" }, limits.map((l) => h("li", null, l)))),
    h("div", { class: "card stack" }, h("h3", null, S.about.licences), h("pre", { class: "notices" }, notices)),
  );
}
