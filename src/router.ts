// Screen routing. Screens import `go` from here; main.ts registers the screens, so there are
// no import cycles between the shell and the screens.

export type Screen = "home" | "copy" | "edit" | "history" | "settings" | "about";
export type ScreenRenderer = (page: HTMLElement) => void | Promise<void>;

let outlet: HTMLElement | null = null;
let screens: Record<Screen, ScreenRenderer> | null = null;
let leaveGuard: (() => boolean) | null = null;

/** Sets where screens render and which renderer handles each screen. */
export function initRouter(main: HTMLElement, renderers: Record<Screen, ScreenRenderer>) {
  outlet = main;
  screens = renderers;
}

/** While set (e.g. during a running batch), the guard decides whether navigation may happen. */
export function setLeaveGuard(fn: (() => boolean) | null) {
  leaveGuard = fn;
}

export function go(screen: Screen) {
  if (!outlet || !screens) throw new Error("Router used before initRouter()");
  if (leaveGuard && !leaveGuard()) return;
  leaveGuard = null;
  outlet.replaceChildren();
  const page = document.createElement("div");
  page.className = "page";
  outlet.append(page);
  outlet.scrollTop = 0;
  void screens[screen](page);
  outlet.focus({ preventScroll: true });
}
