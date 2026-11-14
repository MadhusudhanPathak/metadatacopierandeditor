# Decisions

Decisions that refine or override [PROJECT_SPEC.md](PROJECT_SPEC.md). Where the two disagree, this file wins.
Decided with the product owner on 2026-11-14 unless marked *(agent)*.

## Product

| # | Topic | Decision | Spec section affected |
|---|---|---|---|
| D1 | Audience | Public app, not a personal tool. Prioritise common formats: iPhone HEIC/MOV, Android JPEG/MP4, common camera RAW (CR2/CR3/NEF/ARW/DNG). | 13 |
| D2 | Renamed outputs | No fuzzy matching (e.g. `IMG_001_compressed.jpg`). The Pair step explains the naming requirement plainly instead. | 4.2 |
| D3 | Pairing by kind | Pairing runs **within media kind first**: photo↔photo, video↔video (audio↔audio, other↔other). Ambiguity only counts within a kind, so iPhone Live Photos (`IMG_001.HEIC` + `IMG_001.MOV`) pair cleanly with `IMG_001.jpg` + `IMG_001.mp4`. | 4.2 |
| D4 | Scope | The app only touches metadata, never re-encodes or compresses. `_original` files are never deleted by the app (except by Undo restoring a prior state). | 1, 12 |
| D5 | Own backups | Pairing and folder scans **ignore** files the app created: `*_original.*` and `*_original_N.*`, plus `.mce-undo/`. The same `_original` suffix is used in Copy and Edit. | 4.2, 4.3 |
| D6 | Ground truth | **The Source is ground truth.** Before copying, the Converted file's writable descriptive metadata is cleared, then the Source's is written. Structural tags (dimensions, codec, colour profile, embedded previews, bitrate) are never cleared or copied. | 5.1 |
| D7 | Unticked category | An unticked category is **left untouched**: the Converted file keeps whatever it already had for those tags (it is neither cleared nor copied). | 5.1 |
| D8 | Timezone from GPS | Bundle an **offline timezone-boundary lookup** so GPS coordinates resolve to a real IANA zone. Order: explicit offset → GPS-derived zone → device timezone (flagged "assumed"). The size cost is accepted. | 5.2 |
| D9 | Undo Cache location | A hidden `.mce-undo/` folder **inside each processed folder** (always on the right volume). On Android it also gets a `.nomedia` file. | 5.5 |
| D10 | Date shift | Edit offers **Set to** *and* **Shift by** (± days/hours/minutes) for Date taken. Shift applies per file relative to its own current value. | 4.3, 7 |
| D11 | Edit backup | Edit keeps `<name>_original` by default (on): safety beyond the Undo Cache retention, and the user can review and delete it manually. | 4.3 |
| D12 | Licence | **GPL-3.0-or-later.** | 14 |
| D13 | Name | Final name **Metadata Copier and Editor**; internal codename/short id `MCE`. | — |
| D14 | Platform order | **Windows first**, then Android (tested on the owner's own arm64 phone). | 15 |

## Technical *(agent)*

| # | Topic | Decision |
|---|---|---|
| T1 | Framework | Tauri v2 (Rust + small TypeScript frontend), as recommended in the spec. A Python stack was considered and rejected: larger exe, slow one-file start-up, frequent antivirus false positives, and no shared UI with Android. |
| T2 | Windows toolchain | Rust (rustup, MSVC target) + Visual Studio 2022 Build Tools with only the MSVC compiler and Windows SDK components (no IDE). Build machine only; end users get one exe. |
| T3 | Delivery order | Thin vertical slice first (pick → pair → preview → copy → swap → undo on one real pair), then breadth (all categories, edit flow, map, polish). |
