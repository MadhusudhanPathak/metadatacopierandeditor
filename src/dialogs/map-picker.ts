// GPS map picker (D16, D17). Leaflet is bundled and loaded on demand; tiles come from
// tile.openstreetmap.org. Towns and cities are searched offline as you type (bundled database);
// streets, addresses and landmarks only when the user presses "Search online" (OSM Nominatim).
// A dropped pin is named by an offline nearest-town lookup (T21), never by the network.
import { api, type Location } from "../api";
import { S } from "../strings";
import { clear, h, icon, modal, placeAddress, placeLabel, spinner, tip, toast } from "../ui";

export interface Picked {
  lat: number;
  lon: number;
  /** Name and address of the chosen search result; null for a dropped pin. */
  label: string | null;
}

interface Hit {
  name: string;
  /** Kind of place, e.g. "City", "Museum", "House". */
  type: string;
  /** Full address without the name. */
  address: string;
  /** Extra detail on the right, e.g. population. */
  meta: string;
  lat: number;
  lon: number;
  /** [south, north, west, east] when the place is an area. */
  bbox: [number, number, number, number] | null;
  zoom: number;
}

const NOMINATIM = "https://nominatim.openstreetmap.org/search";
const TILES = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
/** Nominatim usage policy: at most one request per second, never as-you-type. */
const ONLINE_INTERVAL_MS = 1100;
const ONLINE_LIMIT = 20;
let lastOnline = 0;

const pretty = (s: string | undefined) => (s ? (s.charAt(0).toUpperCase() + s.slice(1)).replace(/_/g, " ") : "");

/** OSM Nominatim lookup, only on an explicit button press. */
async function searchOnline(q: string): Promise<Hit[]> {
  const wait = lastOnline + ONLINE_INTERVAL_MS - Date.now();
  if (wait > 0) {
    toast(S.loc.wait, "warn", 1500);
    await new Promise((r) => setTimeout(r, wait));
  }
  lastOnline = Date.now();
  const url = `${NOMINATIM}?format=jsonv2&limit=${ONLINE_LIMIT}&addressdetails=0&q=${encodeURIComponent(q)}`;
  const res = await fetch(url, { headers: { Accept: "application/json", "Accept-Language": navigator.language || "en" } });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const rows = (await res.json()) as {
    name?: string;
    display_name: string;
    lat: string;
    lon: string;
    type?: string;
    addresstype?: string;
    boundingbox?: string[];
  }[];
  return rows
    .map((r): Hit => {
      const parts = r.display_name.split(", ");
      const name = r.name?.trim() || parts[0];
      const address = (parts[0] === name ? parts.slice(1) : parts).join(", ");
      const b = r.boundingbox?.map(parseFloat);
      const bbox = b && b.length === 4 && b.every(Number.isFinite) ? ([b[0], b[1], b[2], b[3]] as [number, number, number, number]) : null;
      // `type` is the most specific ("museum", "city"); generic ones fall back to `addresstype`.
      const kind = !r.type || ["yes", "administrative", "place", "unclassified"].includes(r.type) ? r.addresstype : r.type;
      return { name, type: pretty(kind), address, meta: "", lat: parseFloat(r.lat), lon: parseFloat(r.lon), bbox, zoom: 17 };
    })
    .filter((x) => Number.isFinite(x.lat) && Number.isFinite(x.lon));
}

/** Opens the map dialog. Resolves with the chosen point, or null when cancelled. */
export async function pickOnMap(initial: Location | null): Promise<Picked | null> {
  const L = (await import("leaflet")).default;
  await import("leaflet/dist/leaflet.css");
  return new Promise((resolve) => {
    let chosen: Picked | null = initial ? { lat: initial.lat, lon: initial.lon, label: null } : null;
    let resolved = false;

    // ---- footer: what will be used
    const selName = h("div", { class: "name" });
    const selSub = h("div", { class: "sub" });
    const selCoords = h("div", { class: "sub mono" });
    let labelSeq = 0;
    const showChosen = () => {
      const seq = ++labelSeq;
      if (!chosen) {
        selName.textContent = S.loc.noPin;
        selSub.textContent = "";
        selCoords.textContent = "";
        useBtn.disabled = true;
        return;
      }
      useBtn.disabled = false;
      selCoords.textContent = `${chosen.lat.toFixed(6)}, ${chosen.lon.toFixed(6)}`;
      if (chosen.label) {
        const [n, ...rest] = chosen.label.split(", ");
        selName.textContent = n;
        selSub.textContent = rest.join(", ");
        return;
      }
      selName.textContent = S.loc.pin;
      selSub.textContent = "";
      const { lat, lon } = chosen;
      placeLabel(lat, lon).then((t) => {
        if (seq === labelSeq) selSub.textContent = t ?? "";
      });
    };
    const useBtn = h("button", { class: "btn primary" }, icon("check"), S.loc.use);

    // ---- search
    const query = h("input", { type: "search", placeholder: S.loc.searchHint, "aria-label": S.loc.search, autocomplete: "off", spellcheck: false });
    const status = h("div", { class: "mp-status", role: "status" });
    const results = h("div", { class: "mp-results", role: "listbox", "aria-label": S.loc.search });
    const onlineBtn = h("button", { class: "btn small ghost mp-online", type: "button", "data-tip": S.loc.searchOnlineTip, disabled: true }, icon("open"), h("span", { class: "break" }, S.loc.searchOnline));
    let hits: Hit[] = [];
    let active = -1;
    let seq = 0;

    const choose = (i: number) => {
      const hit = hits[i];
      if (!hit) return;
      setActive(i);
      chosen = { lat: hit.lat, lon: hit.lon, label: [hit.name, hit.address].filter(Boolean).join(", ") };
      putPin();
      if (hit.bbox && (hit.bbox[1] - hit.bbox[0] > 0.004 || hit.bbox[3] - hit.bbox[2] > 0.004)) {
        map.fitBounds([[hit.bbox[0], hit.bbox[2]], [hit.bbox[1], hit.bbox[3]]], { maxZoom: hit.zoom });
      } else map.setView([hit.lat, hit.lon], hit.zoom);
      showChosen();
    };
    const setActive = (i: number) => {
      active = i;
      results.querySelectorAll<HTMLElement>(".mp-hit").forEach((el, j) => {
        el.classList.toggle("sel", j === i);
        el.setAttribute("aria-selected", j === i ? "true" : "false");
        if (j === i) el.scrollIntoView({ block: "nearest" });
      });
    };
    const showHits = (list: Hit[], statusText: string, emptyText: string) => {
      hits = list;
      active = -1;
      clear(results);
      status.textContent = list.length ? statusText : "";
      if (!list.length) {
        results.append(h("p", { class: "mp-empty" }, emptyText));
        return;
      }
      list.forEach((hit, i) =>
        results.append(
          h(
            "button",
            { type: "button", class: "mp-hit", role: "option", "aria-selected": "false", tabindex: "-1", onclick: () => choose(i) },
            h(
              "span",
              { class: "mp-line" },
              h("span", { class: "mp-name" }, hit.name),
              hit.type ? h("span", { class: "mp-type" }, hit.type) : null,
              hit.meta ? h("span", { class: "mp-meta" }, hit.meta) : null,
            ),
            hit.address ? h("span", { class: "mp-addr" }, hit.address) : null,
          ),
        ),
      );
    };
    const busy = () => {
      clear(results);
      results.append(h("div", { class: "mp-empty" }, spinner()));
    };

    const searchOffline = async () => {
      const q = query.value.trim();
      const my = ++seq;
      onlineBtn.disabled = !q;
      onlineBtn.querySelector("span")!.textContent = q ? S.loc.searchOnlineFor(q) : S.loc.searchOnline;
      if (!q) {
        hits = [];
        clear(results);
        status.textContent = "";
        results.append(h("p", { class: "mp-empty" }, S.loc.typeToSearch));
        return;
      }
      try {
        const places = await api.searchPlaces(q);
        if (my !== seq) return;
        showHits(
          places.map((p) => ({
            name: p.name,
            type: p.population >= 100_000 ? S.loc.city : S.loc.town,
            address: placeAddress(p, false),
            meta: p.population ? S.loc.people(p.population) : "",
            lat: p.lat,
            lon: p.lon,
            bbox: null,
            zoom: p.population > 1_000_000 ? 11 : p.population > 100_000 ? 12 : 14,
          })),
          S.loc.offlineCount(places.length),
          S.loc.noResults,
        );
      } catch (e) {
        if (my === seq) showHits([], "", String(e));
      }
    };
    let typing: number | undefined;
    query.addEventListener("input", () => {
      clearTimeout(typing);
      typing = window.setTimeout(searchOffline, 120);
    });
    const doOnline = async () => {
      const q = query.value.trim();
      if (!q) return;
      const my = ++seq;
      onlineBtn.disabled = true;
      busy();
      try {
        const list = await searchOnline(q);
        if (my !== seq) return;
        showHits(list, S.loc.onlineCount(list.length), S.loc.noResultsOnline);
        if (list.length) status.append(h("span", { class: "mp-credit" }, ` · ${S.loc.onlineCredit}`));
      } catch {
        if (my === seq) showHits([], "", S.loc.onlineFailed);
      }
      onlineBtn.disabled = false;
    };
    onlineBtn.onclick = doOnline;
    query.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (!hits.length) return;
        const n = hits.length;
        setActive(e.key === "ArrowDown" ? (active + 1) % n : (active - 1 + n) % n);
      } else if (e.key === "Enter") {
        e.preventDefault();
        // Enter only picks a result; going online always needs the explicit button (D16).
        if (hits.length) choose(active >= 0 ? active : 0);
      }
    });

    // ---- layout
    const mapEl = h("div", { class: "mapbox", role: "application", "aria-label": S.loc.mapTitle });
    const offline = h("div", { class: "mp-offline", hidden: true }, icon("warn"), h("span", null, S.loc.offline));
    const body = h(
      "div",
      { class: "mp" },
      h("div", { class: "mp-head" }, h("h2", null, S.loc.mapTitle), h("p", { class: "muted small" }, S.loc.mapLead)),
      h(
        "div",
        { class: "mp-body" },
        h("div", { class: "mp-side" }, h("div", { class: "row mp-search" }, h("div", { class: "grow" }, query), tip(S.loc.searchTip)), status, results, onlineBtn),
        h("div", { class: "mp-map" }, mapEl, offline),
      ),
      h(
        "div",
        { class: "mp-foot" },
        h("span", { class: "mp-pinicon" }, icon("pin")),
        h("div", { class: "names grow" }, selName, selSub, selCoords),
        h("button", { class: "btn", onclick: () => close() }, S.cancel),
        useBtn,
      ),
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

    // ---- map
    const map = L.map(mapEl, { worldCopyJump: true, attributionControl: true }).setView(chosen ? [chosen.lat, chosen.lon] : [20, 0], chosen ? 14 : 2);
    map.attributionControl.setPrefix(false);
    const tiles = L.tileLayer(TILES, {
      maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
    }).addTo(map);
    let tileErrors = 0;
    tiles.on("tileerror", () => {
      if (++tileErrors >= 2) offline.hidden = false;
    });
    tiles.on("tileload", () => {
      tileErrors = 0;
      offline.hidden = true;
    });
    if (!navigator.onLine) offline.hidden = false;

    const pinIcon = L.divIcon({ className: "", html: '<div class="pin"></div>', iconSize: [22, 22], iconAnchor: [11, 22] });
    let marker: ReturnType<typeof L.marker> | null = null;
    const putPin = () => {
      if (!chosen) return;
      const at = L.latLng(chosen.lat, chosen.lon);
      if (!marker) {
        marker = L.marker(at, { draggable: true, icon: pinIcon, keyboard: true, title: S.loc.pin }).addTo(map);
        marker.on("dragend", () => {
          const p = marker!.getLatLng().wrap();
          dropAt(p.lat, p.lng);
        });
      } else marker.setLatLng(at);
    };
    const dropAt = (lat: number, lon: number) => {
      const w = L.latLng(lat, lon).wrap();
      chosen = { lat: w.lat, lon: w.lng, label: null };
      setActive(-1);
      putPin();
      showChosen();
    };
    map.on("click", (e: { latlng: { lat: number; lng: number } }) => dropAt(e.latlng.lat, e.latlng.lng));

    putPin();
    showChosen();
    searchOffline();
    setTimeout(() => {
      map.invalidateSize();
      query.focus();
    }, 60);
  });
}
