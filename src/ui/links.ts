// Opening things outside the app: external links in the browser, files in the file manager.
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { errText } from "../api";
import { toast } from "./overlay";

/** External links open in the user's browser, never inside the app window. */
export function initLinks() {
  document.addEventListener("click", (e) => {
    const a = (e.target as HTMLElement).closest("a[href]") as HTMLAnchorElement | null;
    if (!a) return;
    const href = a.getAttribute("href") ?? "";
    if (/^https?:/i.test(href)) {
      e.preventDefault();
      openUrl(href).catch(() => toast(href));
    }
  });
}

export async function reveal(path: string) {
  try {
    await revealItemInDir(path);
  } catch (e) {
    toast(errText(e), "bad");
  }
}
