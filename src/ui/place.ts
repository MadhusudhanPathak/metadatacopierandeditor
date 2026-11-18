// Human place names for coordinates: the name the user picked in the map, or an offline
// "nearest town" lookup (T21). Never touches the network.
import { api, type Place } from "../api";
import { S } from "../strings";

const picked = new Map<string, string>();
const looked = new Map<string, Promise<string | null>>();

const key = (lat: number, lon: number) => `${lat.toFixed(5)},${lon.toFixed(5)}`;

/** "Paris, Île-de-France, France", without repeated parts ("São Paulo, São Paulo, Brazil" → "São Paulo, Brazil"). */
export function placeAddress(p: Pick<Place, "name" | "region" | "country">, withName = true): string {
  const parts = withName ? [p.name, p.region, p.country] : [p.region, p.country];
  const out: string[] = [];
  for (const s of parts) if (s && !out.some((o) => o.toLowerCase() === s.toLowerCase())) out.push(s);
  return out.join(", ");
}

/** Remembers the label of a place the user chose, so it's shown instead of "near …". */
export function rememberPlace(lat: number, lon: number, label: string) {
  picked.set(key(lat, lon), label);
}

/** The best label for a point: the picked place's name, else the nearest town ("3.9 km from Paris, France"). */
export function placeLabel(lat: number, lon: number): Promise<string | null> {
  const k = key(lat, lon);
  const p = picked.get(k);
  if (p) return Promise.resolve(p);
  let r = looked.get(k);
  if (!r) {
    r = api
      .nearestPlace(lat, lon)
      .then((n) => (n ? S.loc.near(n.km, placeAddress(n.place)) : null))
      .catch(() => null);
    looked.set(k, r);
  }
  return r;
}

/** Fills `el` with the place label for a point once it's known (empty when there's none). */
export function fillPlace(el: HTMLElement, lat: number, lon: number): HTMLElement {
  placeLabel(lat, lon).then((t) => {
    el.textContent = t ?? "";
    el.hidden = !t;
  });
  el.hidden = true;
  return el;
}

/** Parses "lat, lon" out of a display string like "48.858400, 2.294500 · 35 m". */
export function coordsOf(s: string | null): [number, number] | null {
  const m = s?.match(/^\s*(-?\d+(?:\.\d+)?),\s*(-?\d+(?:\.\d+)?)/);
  if (!m) return null;
  const lat = parseFloat(m[1]);
  const lon = parseFloat(m[2]);
  return Math.abs(lat) <= 90 && Math.abs(lon) <= 180 ? [lat, lon] : null;
}
