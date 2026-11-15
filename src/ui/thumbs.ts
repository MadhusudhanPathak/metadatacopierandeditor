// Lazy, cached file thumbnails from the platform (falls back to a kind icon).
import { api, type Kind } from "../api";
import { clear, h } from "./dom";
import { kindIcon } from "./icons";

const cache = new Map<string, Promise<string | null>>();

const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const el = e.target as HTMLElement;
      observer.unobserve(el);
      const path = el.dataset.path;
      if (!path) continue;
      let url = cache.get(path);
      if (!url) {
        url = api.thumbnail(path, 128).catch(() => null);
        cache.set(path, url);
      }
      url.then((src) => {
        if (src) {
          clear(el);
          el.append(h("img", { src, alt: "" }));
        }
      });
    }
  },
  { rootMargin: "200px" },
);

export function thumb(path: string, kind: Kind, big = false): HTMLElement {
  const el = h("div", { class: `thumb ${big ? "big" : ""}`, "data-path": path }, kindIcon(kind));
  observer.observe(el);
  return el;
}
