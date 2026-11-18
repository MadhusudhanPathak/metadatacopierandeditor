# AGENTS.md — context for coding agents

Read this first. It is the single source of project context: the original `PROJECT_SPEC.md` and `DECISIONS.md` were retired by the owner on 2026-11-18, and the decisions still cited in code (`D6`, `T9`, …) are summarised under [Decision IDs](#decision-ids). Code comments citing "spec N.N" refer to that retired spec; the behaviour they describe is what the code and this file say. Keep this file current: when you change architecture, commands, state or gotchas, update the relevant section here in the same change.

*Last updated: 2026-11-18.*

## What this is

**Metadata Copier and Editor** (codename MCE): a small, offline Tauri v2 app for Windows (done) and Android (not started) that

1. **copies all metadata** from a *Source* (original) onto a *Converted* (compressed/re-encoded) file, then **swaps** them: Converted takes the Source's folder and base name (keeping its own extension), and the Source is renamed `name_original.ext`;
2. **edits curated metadata fields** (single or bulk) with Preview/Undo.

After any operation a folder holds only **`name_original` (untouched original) + the updated file**, both visible (D15). There is no hidden Undo Cache and no backup option.

The engine is the bundled ExifTool (Perl), driven as one persistent process. Licence: GPL-3.0-or-later. Repo: `https://github.com/MadhusudhanPathak/metadatacopierandeditor`.

## Rules from the product owner (do not break)

- **Never `git commit` or `git push`.** The owner commits and pushes. Leave changes in the working tree.
- Product behaviour is the owner's call: ask about anything user-visible that this file doesn't settle. Technical choices are delegated: make them and add a one-line entry to [Decision IDs](#decision-ids) (next free numbers: **D18**, **T22**).
- Windows first; Android next (D14). App is for the general public, not one person.
- Lightweight: no heavy frameworks, no icon packs, no analytics or telemetry. Network: OSM map tiles, plus Nominatim only when the user presses "Search online" (D16). Nothing else.
- No hidden files or folders, no extra copies: just `name_original` + the updated file (D15).
- Theme colours only: `#000000`, `#6D8F63`, `#39FF88` (+ minimal warn/danger). Fonts Fredoka/Nunito, bundled.

## Current state

| Area | State |
| --- | --- |
| Windows app (all spec screens & flows) | Implemented. Release exe ~15 MB (`src-tauri/target/release/MetadataCopierAndEditor.exe`). |
| Rust unit tests | 57 pass (`npm test`). |
| End-to-end tests with real ExifTool | 14 pass (`npm run test:e2e`). |
| Lint/format | `cargo clippy --all-targets` clean, `cargo fmt --check` clean, `npm run typecheck` clean. |
| UI click-through | Done by the owner on their desktop (2026-11-18): works, including the redesigned map picker / location field and Edit Review (D17) in v0.2.0. |
| Network check (tiles + opt-in Nominatim only) | Not done (needs a proxy or packet capture). |
| WebView2-missing dialog | Written (`platform/webview.rs`), untested (this PC has WebView2). |
| Android | **Not started; next up.** See "Next steps" item 1. |
| Git | Owner commits and pushes (latest: `574df85 Readme`). One early scaffold commit (`0801e6f`) was made by an agent before the no-commit rule. |
| Working tree | Clean for push: no `node_modules`, `target`, `dist`, `engine/cache` or fixtures are tracked. Run `npm ci && npm run fetch-engine` before building; `npm run fetch-engine -- all && npm run fixtures` before e2e tests. |
| Release v0.2.0 (current) | `releases/MetadataCopierAndEditor-v0.2.0-windows-x64.exe` + `.sha256` (gitignored; the owner uploads them to GitHub Releases) and `releases/RELEASE_NOTES.md` (tracked, v0.2.0 notes). v0.1.0 binaries were removed locally; they live on GitHub Releases. Version is in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`; bump all five. |
| App icon | Owner artwork since 2026-11-18. Master: `src-tauri/icons/app-icon.png` (1024 px, transparent), made from the owner's 920 px `Icon.png` (the owner's originals `Icon.png` and `Icon.ico` stay in the repo root, tracked): white background made transparent, two stray marks below the tile removed, centred on a square. `src-tauri/icons/` keeps only the master + the 5 files `tauri.conf.json` lists. Regenerate with `npm run tauri icon src-tauri/icons/app-icon.png` (for Android: after `tauri android init`, so it also writes the launcher icons), then delete the extra platform files it creates. |

## Components

Each component has one job and only depends on the components listed after it (T19). The UI never touches files; the planning layer never changes files; only `ops` (through `platform::fsops`) moves or removes user files.

### Rust core (`src-tauri/src/`)

| Component | Files | Responsibility | Boundaries / interaction |
| --- | --- | --- | --- |
| Entry | `main.rs`, `lib.rs` | Starts Tauri, registers commands, startup (mark interrupted ops, start the engine), engine shutdown on exit. | Only wiring; no logic. |
| `commands/` | `system` (engine status, About), `files` (expand paths, Converted suggestion, pairing, thumbnails, place search, nearest place), `copy`, `edit`, `history` (History, Undo, recovery, `OpView`), `settings`, `runner` (background Apply with `op-progress`/`op-finished` events; foreground Undo/recovery) | The IPC surface the UI calls. Converts requests to domain calls, runs slow work on blocking threads, returns plain-language `String` errors. | Thin: calls `planning`, `ops`, `state`. Mirrored by `src/api/`. |
| `state.rs` | `AppState`, `BusyGuard` | Engine lifecycle (background unpack + start, waiters), metadata cache, journal, the latest Copy/Edit plan, place DB, cancel flag, the single-operation lock (RAII). | Owned by Tauri; commands get it via `app_state()`. |
| `ops/` | `journal` (Op/Item model, JSON + fsync'd log, `mark_interrupted`), `items` (plans → journal items, temp names), `executor` (run, Undo, recover, step machine), `rollback` (`rollback_item`, `undo_blocker`), `verify` (temp-file checks, engine error wording) | Applying plans safely: journal first, temp write → verify → swap → dates → drop old Converted copy; reverse from any step. | Uses `engine` to write, `platform::fsops` to move. Never re-plans. |
| `planning/` | `pairing` (4 modes, D3 kind-first, ignores app files), `copy` (requests, Preview assembly, pre-flight), `copy_pair` (per-pair clean-slate args, counts, diff, expectations — pure), `guards` (orientation T8, mismatch hints), `edit` (field reading, Review, `plan_file` — pure), `preflight` (read-only/locked/folder/space checks), `mod.rs` (`Issue`, `DiffRow`, `PairState`, `diff_rows`) | Building the Preview/Review and the exact ExifTool arguments. | Reads files and metadata only; **must not change files**. |
| `metadata/` | `tags` (`TagMap`, `Kind`, `FormatClass`), `categories` + `categories.json` (Copy checklist, spec 5.1), `fields` (curated fields: extract, write targets per format), `datetime` (timezone resolution, photo↔video dates, `Shift`), `cache` (path+size+mtime cache, reads with `-api Geolocation=1`) | Interpreting metadata. Device timezone is injected so tests are machine-independent. | Pure except `cache` (calls `engine`). |
| `engine/` | `mod.rs` (`Engine`, `Launch::for_dir`), `protocol.rs` (argument encoding, `-executeN` framing, stderr parsing), `extract.rs` (first-launch unpack + per-file SHA-256 verify) | Owning the one ExifTool process; restart once on death. | Knows nothing about plans or journals. |
| `platform/` | `fsops` (file times, never-overwriting moves, `\\?\` paths, `path_key`/`same_path`, free space, locks, describe I/O errors), `thumbs` (Windows shell thumbnails → PNG data URL), `webview` (WebView2 check) | OS services; all `cfg(windows)` code lives here. | Leaf layer. |
| Other | `geo.rs` (offline place search and nearest town over `Geolocation.dat`, T17, T21), `settings.rs` (settings JSON, atomic save) | | |
| Build | `build.rs` | Verifies the pinned ExifTool zip hash, strips `Lang/`, `.pod`, `TAP/`, `Test*/`, deflate-packs it into `OUT_DIR/engine.bin`. | Fails without `npm run fetch-engine`. |
| Tests | `tests/e2e.rs` | End-to-end with the real ExifTool; fixtures in `tests/fixtures` (gitignored). | Skips with a note if fixtures are missing. |

### Frontend (`src/`, vanilla TypeScript, no framework)

| Component | Files | Responsibility |
| --- | --- | --- |
| Shell | `main.ts`, `router.ts` | Top bar, engine status chip, screen registration and `go()`, leave guard while a batch runs, startup recovery check. Screens import `router.ts`, never `main.ts`. |
| API | `api/index.ts`, `api/types.ts` | Typed `invoke` wrappers, event listeners, `errText`. Types mirror Rust serde (camelCase); keep in sync. |
| Strings | `strings.ts` | **All** user-facing text (spec 9). Never hard-code UI strings elsewhere. |
| UI toolkit | `ui/` (`dom` h/add/clear, `icons`, `tooltip`, `overlay` toasts/modals, `thumbs`, `place` (place labels for coordinates: picked name or offline "km from …"), `dropzone`, `links`, `widgets` stepper/note/switch/diff table), `ui/index.ts` re-exports | Reusable view pieces; no screen state. |
| Helpers | `lib/format.ts`, `lib/paths.ts` | Pure formatting (bytes, durations, UTC offsets) and path helpers. |
| Screens | `screens/home`, `history`, `settings`, `about`, `run` (Run + Done, shared), `copy/` (`state`, `pick`, `pair`, `preview`, `index`), `edit/` (`state`, `pick`, `fields`, `editor`, `review`, `index`) | One folder per multi-step flow: `state.ts` holds the flow's state and `render()`, step files import only `state.ts`. |
| Dialogs | `dialogs/map-picker.ts` (Leaflet, lazy; results list beside the map, offline as-you-type + explicit online search, D17), `dialogs/recovery.ts` | Modal flows used by screens or startup. |
| Styles | `styles.css` | Theme tokens and all styling. |

### Other files

```text
README.md                  user-facing; THIRD_PARTY_NOTICES.md is shown in the About screen
engine/exiftool.lock.json  pinned ExifTool version + official SHA-256 (Windows zip + source tar.gz)
engine/cache/              downloads (gitignored)
scripts/                   fetch-engine.mjs (download + verify ExifTool), make-fixtures.mjs (test media)
releases/                  release binaries + .sha256 (gitignored) and RELEASE_NOTES.md (tracked)
src-tauri/tauri.conf.json  CSP: img-src only https://tile.openstreetmap.org; connect-src adds https://nominatim.openstreetmap.org
src-tauri/capabilities/    minimal permissions (dialog open, reveal, allow-listed open-url)
src-tauri/rustfmt.toml     160 columns, "Max" heuristics
```

## Commands

```sh
npm ci
npm run fetch-engine            # pinned Windows ExifTool, hash-checked (build.rs fails without it)
npm run fetch-engine -- all     # also the source tar.gz (needed for fixtures / Android)
npm run fixtures                # extract test media from ExifTool's own t/images
npm test                        # Rust unit tests (no ExifTool needed)
npm run test:e2e                # end-to-end, real ExifTool, single-threaded
npm run typecheck               # frontend typecheck
npm run tauri dev               # dev app
npm run build:exe               # release exe (LTO; ~3–6 min)
cd src-tauri && cargo fmt && cargo clippy --all-targets   # keep both clean
```

## Environment notes (this Windows machine)

- Rust is at `%USERPROFILE%\.cargo\bin`; in a fresh Bash tool call use `export PATH="$HOME/.cargo/bin:$PATH"`. VS Code must be restarted to see it.
- The **PowerShell tool is broken** here (its launcher fails with `Invoke-Expression not recognized`; `powershell.exe` can't autoload modules). Use the Bash tool. `wmic`, `winget`, `python` (3.11) and `node` work.
- In the Bash tool, heredocs that contain backslashes or certain quote mixes get mangled. For scripted multi-line edits, write the script to a file first and run it.
- Running the downloaded `perl.exe`/ExifTool directly from a shell was blocked once by the agent safety classifier. Running it via the test suite was fine after the owner asked for tests to be run.
- First debug build ~1 min; release build with LTO ~3–6 min. One early build took 4 h (laptop sleep), so don't panic.

## Architecture invariants (keep these true)

- **Nothing changes before Apply.** `planning` must not touch files. Execution re-checks size/mtime against the plan (`executor::recheck`).
- **Journal first.** `executor::run` writes the full op JSON before touching files; every step is appended to `journal/<id>.log` with fsync.
- **Never delete or change the user's original.** Copy: the Source becomes visible `name_original` (or `_original_N`). Edit: the edited file itself becomes `name_original`. The only file removed is the *pre-change Converted copy*, and only as the very last step after the updated file is in place (D15). No hidden folders, no hidden files.
- **Never overwrite.** `fsops::move_file` uses `MoveFileExW` without REPLACE_EXISTING; cross-volume = copy + size + SHA-256 verify + delete.
- **The Converted file is never modified in place.** ExifTool writes to a temp (`.<stem>.mce-tmp-<op>-<i>.<ext>`, `ops::items`) via `-o`, which is verified (opens, same FileType, key tags read back) before the swap.
- **Step machine** (both flows): 1 temp written & verified → 2 `source` renamed to `_original` → 3 temp moved to final (atomic replace if the Converted file already has the final name) → 4 file dates → 5 Copy only: pre-change Converted removed. Item fields: Copy `source`=Source, `original`=Converted; Edit `source`=`original`=`final_path`=the file. `ops::rollback::rollback_item` reverses from any step and checks the disk, not just the logged step.
- **One operation at a time.** Apply, Undo and recovery all take `AppState::begin_operation()`; the guard releases the lock when dropped.
- **Undo (D15):** originals come back byte-identical with names and dates; the Converted file returns to its own folder/name *with* the copied metadata (its pre-change copy no longer exists).
- **Source is ground truth (D6):** clear the Converted file's metadata (`-all=`, keeping ICC/JFIF/Adobe), restore its structural tags + unticked categories from `@`, copy Source with `-all:all`. Structural tags (dimensions, codec, thumbnails…) are never copied or cleared. Orientation goes through the guard.
- **Dates:** QuickTime dates are UTC; photos are local. Never rely on ExifTool's QuickTimeUTC; compute explicit values (`metadata/datetime.rs`). Assumed offsets are flagged; the user override replaces only assumed offsets.
- **Pairing ignores app files:** `*_original.*`, `*_original_N.*`, `*.ext_original`, `.mce-tmp`, and legacy `.mce-undo/` folders from early test builds.
- **No panics on input (T20):** release builds use `panic = "abort"`, so a panic closes the app. Treat UI values and file metadata as untrusted (checked arithmetic, `str::get` instead of slicing).

## Gotchas learned the hard way

- **ExifTool argument encoding:** arguments are sent one per line with `-ec`; only the value of `-TAG=value` is C-escaped (`engine/protocol.rs` `encode_arg`). Do **not** use `#[CSTR]` argfile lines: they leave a stray backslash before `$` and `@` (this broke `-tagsFromFile @`).
- **Composite tags are writable** (e.g. Composite GPSLatitude writes EXIF GPS + Ref). Copy commands must include `--Composite:all` or unticked categories leak through.
- Writing into a file with no EXIF block makes ExifTool add 4 required IFD0 tags (T16). The bulk-edit test allows exactly those.
- `--GROUP:all` after `-all=` (before any `-tagsFromFile`) excludes a *group* from deletion; individual tags can't be excluded from `-all=`, so they're put back with `-tagsFromFile @ -G1:Tag`.
- Tauri's OS file drop (`dragDropEnabled`) disables HTML5 drag-and-drop in WebView2, so manual pairing uses pointer events (`screens/copy/pair.ts` `enableDrag`).
- DOM `append()` rejects `null`; use `add(el, ...)` from `ui/dom.ts` for conditional children.
- ExifTool reports family groups as `-G0:1` keys like `EXIF:IFD0:Make`; Composite keys can be 2-part (`Composite:GPSLatitude`). `TagMap::find` patterns accept `G0:G1:Name`, `G1:Name`, `Name`, with `*` and `Track*` wildcards.
- **Never mark the temp file hidden.** Windows keeps the hidden attribute through a rename, so the updated file ended up hidden (T18). The e2e tests assert visibility.
- Timezone from GPS uses ExifTool's own offline geolocation (`GeolocationTimeZone`) + `chrono-tz`, not a separate boundary map (T6).
- Tauri command functions must be registered by their full module path in `lib.rs` (`commands::copy::plan_copy`); re-exports don't carry the generated `__cmd__` items.
- **Changing `src-tauri/icons/` alone does not rebuild the exe icon**: the Windows resource is cached in `OUT_DIR`. `touch src-tauri/tauri.conf.json` before building, then check the exe (2026-11-18: the first build after the icon swap still had the old icon).

## Decision IDs

Short meanings of the IDs cited in code comments. **D** = product (owner's call), **T** = technical (delegated). Missing numbers were superseded or never cited.

| ID | Decision |
| --- | --- |
| D1 | Public app; prioritise iPhone HEIC/MOV, Android JPEG/MP4, common RAW (CR2/CR3/NEF/ARW/DNG). |
| D2 | No fuzzy name matching (`IMG_001_compressed.jpg`); the Pair step explains the naming rule instead. |
| D3 | Pair within media kind first (photo↔photo, video↔video), so Live Photos pair cleanly. |
| D4 | The app only touches metadata; it never re-encodes. |
| D5 | Scans and pairing ignore the app's own files (`*_original*`, temps, legacy `.mce-undo/`). |
| D6 | The Source is ground truth: clear the Converted file's descriptive metadata, then copy the Source's. Structural tags are never cleared or copied. |
| D7 | An unticked Copy category is left exactly as it is on the Converted file. |
| D8 | Timezone order: explicit offset → GPS-derived zone → device zone (flagged "assumed"). |
| D10 | Date taken: "Set to" and "Shift by" (± days/hours/minutes), per file relative to its own value. |
| D11 | Edit keeps the untouched file as `name_original` (now always, see D15). |
| D12 | Licence GPL-3.0-or-later. D13: name "Metadata Copier and Editor", codename MCE. D14: Windows first, then Android. |
| D15 | No hidden Undo Cache or backup option: a folder holds only `name_original` + the updated file, both visible. Undo works from those. |
| D16 | Network: OSM tiles while the map is open; Nominatim only when the user presses "Search online". |
| D17 | Location UI (2026-11-18): one compact coordinates box + altitude + map button, with the place name shown under it. Map picker: side-by-side results list and map, offline results as you type, each result shows its name, type and full address; online search stays an explicit button. Edit Review: a "what will be written" summary, then one compact before → after block per file; unchanged files collapse into one line. |
| T1–T3 | Tauri v2 (Rust + vanilla TS); MSVC toolchain; thin vertical slice first. |
| T4 | ExifTool is embedded in the exe and unpacked on first launch with per-file SHA-256 checks. |
| T5 | One persistent ExifTool process, `-executeN` framing, stderr parsed per command. |
| T6 | GPS → timezone via ExifTool's own geolocation + `chrono-tz` (no separate boundary map). |
| T7 | Clean-slate copy as one ExifTool command (`-all=`, restore structural + unticked from `@`, copy Source). |
| T8 | Orientation guard: copy Orientation only when it can't rotate the picture twice. |
| T9 | Temp file is verified (opens, same FileType, key tags read back) before any swap. |
| T10 | Journal: full op JSON first, then an fsync'd step log per op. |
| T13 | Manual pairing uses pointer events because Tauri's OS file drop disables HTML5 drag-and-drop. |
| T14 | Read-only and locked files are reported in pre-flight, never forced. |
| T16 | Writing into a file with no EXIF adds ExifTool's 4 required IFD0 tags; accepted. |
| T17 | Offline place search reads ExifTool's `Geolocation.dat` directly (`geo.rs`). |
| T18 | Never mark the temp file hidden (the attribute survives the rename). |
| T19 | Component layering: UI → commands → planning/ops → metadata → engine/platform. |
| T20 | No panics on input (release uses `panic = "abort"`). |
| T21 | "Near …" place names are an offline nearest-city lookup over the same `Geolocation.dat` (`geo::Db::nearest`, `nearest_place` command); no network, no reverse geocoding online. |

## Next steps (suggested order)

1. **Android plan** (next up): perl-cross + NDK static Perl for `aarch64-linux-android` with `-Wl,-z,max-page-size=16384`, shipped as `jniLibs/arm64-v8a/libperl.so` (`extractNativeLibs=true`); ExifTool `lib/` as assets extracted to `filesDir`; Kotlin Tauri plugin for All Files Access onboarding, `nativeLibraryDir`, media rescan (add a per-item callback to `ops::executor::Ctx`, called after each finished or undone item), `loadThumbnail`. `engine::Launch::for_dir` already has the non-Windows branch. Needs a Linux host (WSL or CI). Owner tests on their own arm64 phone.
2. Network verification (spec 13 as widened by D16): proxy/packet capture; only `tile.openstreetmap.org` while the map is open and `nominatim.openstreetmap.org` after pressing "Search online"; nothing otherwise.
3. More e2e coverage from spec 13: cross-volume swap, cancel mid-batch, forced crash + `recover`, DNG/CR2 → JPEG, M4A, orientation-guarded real files, low-disk.
4. Release notes: `releases/RELEASE_NOTES.md` holds the latest release's notes ("What's new" + the standing sections). For the Android release add its platform differences (can't set created time) next to the Windows ones (WebView2; unsigned exe → SmartScreen).
