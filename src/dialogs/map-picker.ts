// GPS map picker (spec 8.3, D16). Leaflet is bundled and loaded on demand; tiles come from
// tile.openstreetmap.org. Place search is offline (bundled city database) unless the user
// presses "Search online" (OSM Nominatim).
import { api, errText, type Location } from "../api";
import { S } from "../strings";
import { clear, h, icon, modal, note, spinner, tip, toast } from "../ui";

interface Hit {
  title: string;
  sub: string;
  lat: number;
  lon: number;
  zoom: number;
}

const NOMINATIM = "https://nominatim.openstreetmap.org/search";
const TILES = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
/** Nominatim usage policy: at most one request per second, never as-you-type. */
const ONLINE_INTERVAL_MS = 1100;
let lastOnline = 0;

/** OSM Nominatim lookup, only on an explicit button press. */
async function searchOnline(q: string): Promise<Hit[]> {
  const wait = lastOnline + ONLINE_INTERVAL_MS - Date.now();
  if (wait > 0) {
    toast(S.loc.wait, "warn", 1500);
    await new Promise((r) => setTimeout(r, wait));
  }
  lastOnline = Date.now();
  const url = `${NOMINATIM}?format=jsonv2&limit=8&addressdetails=0&q=${encodeURIComponent(q)}`;
  const res = await fetch(url, { headers: { Accept: "application/json", "Accept-Language": navigator.language || "en" } });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const rows = (await res.json()) as { display_name: string; lat: string; lon: string; type?: string; class?: string }[];
  return rows.map((r) => {
    const [first, ...rest] = r.display_name.split(", ");
    const building = r.class === "building" || r.class === "amenity" || r.class === "tourism" || r.class === "highway";
    return { title: first, sub: rest.join(", "), lat: parseFloat(r.lat), lon: parseFloat(r.lon), zoom: building ? 17 : 13 };
  });
}

/** Opens the map dialog. Resolves with the chosen point, or null when cancelled. */
export async function pickOnMap(initial: Location | null): Promise<{ lat: number; lon: number } | null> {
  const L = (await import("leaflet")).default;
  await import("leaflet/dist/leaflet.css");
  return new Promise((resolve) => {
    let chosen: { lat: number; lon: number } | null = initial ? { lat: initial.lat, lon: initial.lon } : null;
    let resolved = false;
    const mapEl = h("div", { class: "mapbox", role: "application", "aria-label": S.loc.mapTitle });
    const offline = h("div", { style: "display:none;margin-top:10px" }, note("warn", S.loc.offline));
    const coords = h("span", { class: "mono muted" });
    const showCoords = () => (coords.textContent = chosen ? `${chosen.lat.toFixed(6)}, ${chosen.lon.toFixed(6)}` : S.loc.noPin);
    showCoords();
    const useBtn = h("button", { class: "btn primary" }, icon("check"), S.loc.use);

    const query = h("input", { type: "search", placeholder: S.loc.searchHint, "aria-label": S.loc.search, autocomplete: "off" });
    const results = h("div", { class: "list", style: "max-height:220px;overflow:auto", role: "listbox", "aria-label": S.loc.search });
    const showHits = (hits: Hit[], emptyText: string, credit = false) => {
      clear(results);
      if (!hits.length) {
        results.append(h("p", { class: "muted small", role: "status" }, emptyText));
        return;
      }
      for (const hit of hits) {
        results.append(
          h(
            "button",
            {
              type: "button",
              class: "item clickable",
              role: "option",
              style: "text-align:left;width:100%",
              onclick: () => {
                place(hit.lat, hit.lon);
                map.setView([hit.lat, hit.lon], hit.zoom);
              },
            },
            icon("pin"),
            h("span", { class: "names" }, h("span", { class: "name", style: "display:block" }, hit.title), h("span", { class: "sub", style: "display:block" }, hit.sub)),
          ),
        );
      }
      if (credit) results.append(h("p", { class: "muted small" }, S.loc.onlineCredit));
    };
    const busy = (on: boolean) => {
      searchBtn.disabled = on;
      onlineBtn.disabled = on;
      if (on) {
        clear(results);
        results.append(h("div", { class: "row muted small" }, spinner()));
      }
    };
    const doSearch = async () => {
      const q = query.value.trim();
      if (!q) return;
      busy(true);
      try {
        const places = await api.searchPlaces(q);
        showHits(
          places.map((p) => ({
            title: p.name,
            sub: [p.region, p.country].filter(Boolean).join(", ") + (p.population ? ` · ${S.loc.people(p.population)}` : ""),
            lat: p.lat,
            lon: p.lon,
            zoom: p.population > 500_000 ? 11 : 13,
          })),
          S.loc.noResults,
        );
      } catch (e) {
        showHits([], errText(e));
      }
      busy(false);
    };
    const doOnline = async () => {
      const q = query.value.trim();
      if (!q) return;
      busy(true);
      try {
        showHits(await searchOnline(q), S.loc.noResultsOnline, true);
      } catch {
        showHits([], S.loc.onlineFailed);
      }
      busy(false);
    };
    const searchBtn = h("button", { class: "btn", type: "button", onclick: doSearch }, S.loc.searchBtn);
    const onlineBtn = h("button", { class: "btn ghost", type: "button", "data-tip": S.loc.searchOnlineTip, onclick: doOnline }, icon("open"), S.loc.searchOnline);
    query.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        doSearch();
      }
    });

    const body = h(
      "div",
      { class: "stack" },
      h("h2", null, S.loc.mapTitle),
      h("p", { class: "muted" }, S.loc.mapLead),
      h("div", { class: "row" }, h("div", { class: "grow" }, query), searchBtn, tip(S.loc.searchTip), onlineBtn),
      results,
      mapEl,
      offline,
      h("div", { class: "actions", style: "margin-top:12px" }, h("span", { class: "left row" }, icon("pin"), coords), h("button", { class: "btn", onclick: () => close() }, S.cancel), useBtn),
    );
    const close = modal(body, {
      wide: true,
      label: S.loc.mapTitle,
      onClose: () => {
        map.remove();
        if (!resolved) resolve(null);
      },
    });
    useBtn.onclick = () => {
      if (!chosen) {
        toast(S.loc.noPin, "warn");
        return;
      }
      resolved = true;
      resolve(chosen);
      close();
    };

    const map = L.map(mapEl, { worldCopyJump: true, attributionControl: true }).setView(chosen ? [chosen.lat, chosen.lon] : [20, 0], chosen ? 14 : 2);
    map.attributionControl.setPrefix(false);
    const tiles = L.tileLayer(TILES, {
      maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
    }).addTo(map);
    let tileErrors = 0;
    tiles.on("tileerror", () => {
      if (++tileErrors >= 2) offline.style.display = "";
    });
    tiles.on("tileload", () => {
      tileErrors = 0;
      offline.style.display = "none";
    });
    if (!navigator.onLine) offline.style.display = "";

    const pinIcon = L.divIcon({ className: "", html: '<div class="pin"></div>', iconSize: [22, 22], iconAnchor: [11, 22] });
    let marker: ReturnType<typeof L.marker> | null = null;
    const place = (lat: number, lon: number) => {
      const wrapped = L.latLng(lat, lon).wrap();
      chosen = { lat: wrapped.lat, lon: wrapped.lng };
      if (!marker) {
        marker = L.marker(wrapped, { draggable: true, icon: pinIcon, keyboard: true, title: S.loc.mapTitle }).addTo(map);
        marker.on("dragend", () => {
          const p = marker!.getLatLng().wrap();
          chosen = { lat: p.lat, lon: p.lng };
          showCoords();
        });
      } else marker.setLatLng(wrapped);
      showCoords();
    };
    if (chosen) place(chosen.lat, chosen.lon);
    map.on("click", (e: { latlng: { lat: number; lng: number } }) => place(e.latlng.lat, e.latlng.lng));
    setTimeout(() => {
      map.invalidateSize();
      query.focus();
    }, 60);
  });
}
