// Home: the two main flows plus History.
import { go, type Screen } from "../router";
import { S } from "../strings";
import { add, h, icon, tip, type IconName } from "../ui";

function card(title: string, text: string, goText: string, ico: IconName, to: Screen): HTMLElement {
  return h(
    "button",
    { class: "bigcard", onclick: () => go(to) },
    h("span", { class: "ico" }, icon(ico)),
    h("h2", null, title),
    h("p", { class: "muted" }, text),
    h("span", { class: "go" }, goText, icon("right")),
  );
}

export function homeScreen(page: HTMLElement) {
  add(
    page,
    h(
      "div",
      { class: "hero" },
      h("p", { class: "muted display", style: "font-size:17px;margin-bottom:6px" }, S.home.hello),
      h("h1", { html: S.home.title }),
      h("p", { class: "muted", style: "margin-top:10px;max-width:620px" }, S.home.lead),
    ),
    h(
      "div",
      { class: "bigcards" },
      card(S.home.copyTitle, S.home.copyText, S.home.copyGo, "copy", "copy"),
      card(S.home.editTitle, S.home.editText, S.home.editGo, "edit", "edit"),
    ),
    h(
      "div",
      { class: "homefoot" },
      h("button", { class: "btn", onclick: () => go("history") }, icon("undo"), S.home.history),
      tip(S.home.historyTip),
      h("span", { class: "spacer grow" }),
      h("span", { class: "row small muted", style: "gap:6px" }, icon("shield"), S.home.privacy),
    ),
  );
}
