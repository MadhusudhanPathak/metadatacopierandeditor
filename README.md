# Metadata Copier and Editor

Compressed or converted your photos and videos, and lost the dates, places and camera details?
This app puts them back, offline, with a friendly preview and a real Undo.

- **Copy Metadata:** pairs each original (Source) wiSth its compressed or converted copy (Converted), copies *all* meaningful metadata across, then swaps the files: the Converted file takes the original's name and folder, and the original is kept next to it as `name_original`.
- **Edit Metadata:** fix the date (set it, or shift a whole batch when a camera clock was wrong), add a location (type, paste, or search and pick on a map), title, description, keywords, rating, author and copyright, for one file or many at once. Only the fields you touch are written, and each edited file keeps its untouched version next to it as `name_original`.

Powered by [ExifTool](https://exiftool.org/), so it works with practically every photo, video and RAW format.

## Install (Windows)

1. Download `MetadataCopierAndEditor-v<version>-windows-x64.exe` from the [Releases page](https://github.com/MadhusudhanPathak/metadatacopierandeditor/releases) (a `.sha256` file is published next to it).
2. Double-click it. There is no installer and it needs no admin rights.

Notes:

- The app is not code-signed, so Windows SmartScreen may say "Windows protected your PC". Click **More info → Run anyway**.
- It uses Microsoft Edge **WebView2**, which is built into Windows 11 and up-to-date Windows 10. If it's missing, the app tells you and links to Microsoft's free installer.
- On first launch the bundled metadata engine is unpacked to `%LOCALAPPDATA%\Metadata Copier and Editor\` (about 28 MB). Delete that folder to remove every trace of the app.

Android is planned next; see [DECISIONS.md](DECISIONS.md#android-status).

## Usage

### Copy metadata

1. **Pick** the Source folder (originals) and the Converted folder (copies), or individual files. A `converted` subfolder inside the Source folder is suggested automatically.
2. **Pair**: keep the original file names when converting (e.g. `IMG_001.HEIC` → `IMG_001.jpg`). Photos pair with photos and videos with videos, so iPhone Live Photos sort themselves out. Anything unclear goes to "Needs your help", where you pair files by tapping or dragging.
3. **Preview**: see every rename, a before/after of the key fields, and what will be copied. Untick a category to leave that part of the converted file exactly as it is. Nothing changes until you press **Apply**.
4. **Run** and **Done**: each file is finished or put back as it was. Then **Undo this batch** if you change your mind.

Example, iPhone photos and videos converted to JPEG and MP4 in a `converted` subfolder:

```text
Before                                   After Apply
Trip/IMG_0001.HEIC                       Trip/IMG_0001_original.HEIC   untouched original
Trip/IMG_0002.MOV                        Trip/IMG_0002_original.MOV    untouched original
Trip/converted/IMG_0001.jpg      ──►     Trip/IMG_0001.jpg             converted copy + dates, GPS, camera…
Trip/converted/IMG_0002.mp4      ──►     Trip/IMG_0002.mp4             converted copy + dates, GPS, camera…
```

The Source is treated as the ground truth: the Converted file's old descriptive metadata is replaced by the Source's. Technical details that describe the converted file itself (pixel size, codec, colour profile, thumbnails) are never touched, and orientation is only copied when it can't turn the picture twice.

Dates are handled automatically: photos usually store local time and videos store UTC, so the app works out the timezone (from the file, its GPS clock or GPS position, or this computer as a last resort, which the Preview flags) and converts correctly.

### Edit metadata

1. **Pick** one file, many files, or a folder.
2. **Edit**: with several files, fields that differ show "Mixed values" and stay untouched unless you change them. Examples:
   - A camera clock was an hour behind on a trip: **Date taken → Shift by → 1 hour, Later** moves every file's dates by exactly one hour.
   - Add a place: paste `48.8584, 2.2945` into Location, or **Pick on map** and search "Paris" (offline) or an address (**Search online**).
3. **Review** the before/after for each file, then **Save**. Each edited file keeps its untouched version as `name_original`.

## Safety

- Every operation is written to a journal before any file is touched. If the app or PC stops mid-way, the next launch offers to finish or put everything back.
- Your originals are never deleted or changed: they stay next to the result as visible `name_original` files. Nothing is hidden and no extra copies are kept.
- Undo puts every original back byte-for-byte, with its name and file dates. Converted files go back to their own folder and name, keeping the copied metadata. Undo refuses to run if you've changed those files since.

## Privacy

No accounts, analytics, telemetry or update checks. The only network use is the optional map picker: it loads OpenStreetMap tiles while it is open, and its **Search online** button (only when you press it) sends your search text to OpenStreetMap's Nominatim. Searching towns and cities works offline.

## Known limits

- Some formats can't hold metadata (for example MP3, WAV, AVI, MKV, BMP). They're listed in the Preview and skipped.
- Tags a destination format can't store are skipped and counted in the Preview (for example IPTC in PNG).
- Android (when released) can't change a file's "created" date, only "modified".

## Build from source

### Prerequisites

- Node.js 20 or later
- Rust (stable, MSVC toolchain on Windows)
- Visual Studio 2022 Build Tools with "Desktop development with C++" (MSVC compiler and Windows SDK)

### Setup and run

```sh
npm ci
npm run fetch-engine     # downloads the pinned ExifTool and checks its SHA-256
npm run tauri dev        # run in development
npm run build:exe        # portable exe → src-tauri/target/release/MetadataCopierAndEditor.exe
```

### Tests and checks

```sh
npm test                                     # Rust unit tests (no ExifTool needed)
npm run fetch-engine -- all && npm run fixtures
npm run test:e2e                             # end-to-end tests with the real ExifTool
npm run typecheck                            # frontend type check
```

The ExifTool version and SHA-256 are pinned in [engine/exiftool.lock.json](engine/exiftool.lock.json).

## Architecture

A [Tauri v2](https://tauri.app/) app: a Rust core and a small framework-free TypeScript frontend in a WebView. The official ExifTool package is embedded in the exe at build time (`src-tauri/build.rs`), unpacked on first launch, and driven as one persistent process.

| Part | Where | Role |
| --- | --- | --- |
| UI | `src/` | Screens for Copy, Edit, History and Settings; talks to the core only through typed commands in `src/api/`. All text lives in `src/strings.ts`. |
| Commands | `src-tauri/src/commands/` | The Tauri IPC surface; runs slow work off the UI thread. |
| Operations | `src-tauri/src/ops/` | Journal, executor (temp write → verify → swap), rollback, Undo and crash recovery. |
| Planning | `src-tauri/src/planning/` | Pairing and the Copy/Edit plans, including the exact ExifTool arguments. Never changes files. |
| Metadata | `src-tauri/src/metadata/` | Tag maps, curated fields, Copy categories, date and timezone rules. |
| Engine | `src-tauri/src/engine/` | The bundled ExifTool process and its wire protocol. |
| Platform | `src-tauri/src/platform/` | File operations that never overwrite, shell thumbnails, the WebView2 check. |

Requirements are in [PROJECT_SPEC.md](PROJECT_SPEC.md), design decisions and deviations from the spec in [DECISIONS.md](DECISIONS.md), and contributor notes in [AGENTS.md](AGENTS.md).

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for ExifTool, Perl, the fonts, Leaflet and OpenStreetMap.
