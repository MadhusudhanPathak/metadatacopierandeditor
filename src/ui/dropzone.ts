// OS file drops onto drop zones. Tauri's drag-drop event carries real file paths.
import { getCurrentWebview } from "@tauri-apps/api/webview";

interface Zone {
  el: HTMLElement;
  onPaths: (paths: string[]) => void;
}

const zones = new Set<Zone>();
let lastOver: HTMLElement | null = null;

function zoneAt(x: number, y: number): Zone | null {
  const dpr = window.devicePixelRatio || 1;
  const hit = document.elementFromPoint(x / dpr, y / dpr);
  for (const z of zones) if (z.el.isConnected && hit && z.el.contains(hit)) return z;
  // With a single visible zone, a drop anywhere in the window goes to it.
  const live = [...zones].filter((z) => z.el.isConnected);
  return live.length === 1 ? live[0] : null;
}

function clearHighlight() {
  lastOver?.classList.remove("over");
  lastOver = null;
}

export async function initDragDrop() {
  await getCurrentWebview().onDragDropEvent((ev) => {
    const p = ev.payload;
    if (p.type === "over" || p.type === "enter") {
      const z = zoneAt(p.position.x, p.position.y);
      if (lastOver && lastOver !== z?.el) lastOver.classList.remove("over");
      z?.el.classList.add("over");
      lastOver = z?.el ?? null;
    } else if (p.type === "drop") {
      clearHighlight();
      const z = zoneAt(p.position.x, p.position.y);
      if (z && p.paths.length) z.onPaths(p.paths);
    } else {
      clearHighlight();
    }
  });
}

/** Registers `el` as a drop target until it leaves the document. */
export function dropZone(el: HTMLElement, onPaths: (paths: string[]) => void) {
  const z = { el, onPaths };
  zones.add(z);
  const obs = new MutationObserver(() => {
    if (!el.isConnected) {
      zones.delete(z);
      obs.disconnect();
    }
  });
  obs.observe(document.body, { childList: true, subtree: true });
}
