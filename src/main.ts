// App shell: top bar, engine status chip, routing and startup checks.
import "./styles.css";
import { api } from "./api";
import { checkRecovery } from "./dialogs/recovery";
import { go, initRouter } from "./router";
import { aboutScreen } from "./screens/about";
import { copyScreen } from "./screens/copy";
import { editScreen } from "./screens/edit";
import { historyScreen } from "./screens/history";
import { homeScreen } from "./screens/home";
import { settingsScreen } from "./screens/settings";
import { S } from "./strings";
import { clear, h, icon, initDragDrop, initLinks, initToasts, initTooltips, spinner } from "./ui";

const ENGINE_POLL_MS = 500;

const main = h("main", { id: "main", tabindex: "-1" });
const engineChip = h("span", { class: "row small muted", style: "gap:8px" });

function topbar(): HTMLElement {
  return h(
    "header",
    { class: "topbar" },
    h("button", { class: "brand", onclick: () => go("home"), "aria-label": `${S.app} — ${S.nav.home}` }, icon("logo", "logo"), h("span", null, S.app)),
    h("span", { class: "spacer" }),
    engineChip,
    h("button", { class: "btn ghost small", onclick: () => go("history"), "data-tip": S.home.historyTip }, icon("history"), S.nav.history),
    h("button", { class: "btn icon ghost", onclick: () => go("settings"), "aria-label": S.nav.settings, "data-tip": S.nav.settings }, icon("gear")),
  );
}

/** Shows a "warming up" chip until the bundled engine is ready, or its error if it fails. */
async function watchEngine() {
  for (;;) {
    const s = await api.engineStatus().catch(() => ({ ready: false, error: null }));
    clear(engineChip);
    if (s.ready) return;
    if (s.error) {
      engineChip.append(h("span", { class: "chip bad", "data-tip": s.error }, icon("warn"), S.engine.failed));
      return;
    }
    engineChip.append(spinner(), h("span", { "data-tip": S.engine.startingTip }, S.engine.starting));
    await new Promise((r) => setTimeout(r, ENGINE_POLL_MS));
  }
}

async function start() {
  document.title = S.app;
  document.getElementById("app")?.append(topbar(), main);
  initTooltips();
  initToasts();
  initLinks();
  await initDragDrop().catch(() => {});
  initRouter(main, {
    home: homeScreen,
    copy: copyScreen,
    edit: editScreen,
    history: historyScreen,
    settings: settingsScreen,
    about: aboutScreen,
  });
  go("home");
  void watchEngine();
  void checkRecovery();
}

void start();
