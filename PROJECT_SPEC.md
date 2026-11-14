# Project Spec: Metadata Copier & Editor (Windows + Android)

> **Working title:** "Metadata Copier and Editor" (codename `MCE`)
> **Audience:** the coding agent that will build this. Read the whole document before writing code.
> **Decision status:** every requirement below was confirmed by the product owner unless it sits under "Assumptions to confirm" (section 16).

---

## 1. What we are building

A small, fully offline, friendly app that does two jobs on photos, videos and any other file ExifTool supports:

1. **Copy metadata** from a *Source* file (the original, with the good metadata) onto a *Converted* file (compressed, re-encoded or converted, with its metadata lost or wrong). It then **swaps the files** so the Converted file takes the Source's name and folder, and the Source is kept as `<name>_original`.
2. **Edit metadata manually** (single or bulk) through a friendly form, with no source file needed.

The typical user converts or compresses a pile of photos/videos, loses dates, GPS and camera info, and wants them back without touching a command line.

### Glossary (use these terms consistently in code and UI)

| Term | Meaning |
|---|---|
| **Source** | File that has the good metadata (the original). |
| **Converted** | File that receives the metadata (compressed, re-encoded or converted copy). |
| **Pair** | One Source matched with one Converted. |
| **Swap** | Moving and renaming files after the metadata copy (section 5.4). |
| **Undo Cache** | Hidden, app-managed storage that keeps pre-change files so every operation is reversible. |

---

## 2. Hard requirements (non-negotiable)

1. Runs on **Windows (x64)** and **Android (arm64-v8a, min SDK 26 or higher, to be decided by the agent)**.
2. **Intuitive GUI** that exposes every feature.
3. **Zero setup.** All dependencies (ExifTool, Perl, fonts, map library) are bundled inside the app. **Fully offline**, with exactly one exception: the optional GPS map picker, which loads map tiles online (section 8.3). Everything else works with no connection.
4. Copy **all** metadata between files (dates, location, camera settings, descriptive tags, everything meaningful).
5. **Manual editing** of metadata, single and bulk.
6. **Swap behaviour:** the metadata-updated Converted file moves to the Source's folder and takes the Source's name; the Source is renamed with an `_original` suffix.
7. Fun fonts, friendly design, helpful messages on hover (desktop) or tap/long-press (mobile).
8. Theme colours: **Jet Black `#000000`**, **Muted Moss Green `#6D8F63`**, **Neon Emerald Green `#39FF88`**.
9. **As lightweight as possible.** Nothing that is not needed.

---

## 3. Recommended technical approach

The product owner delegated the framework choice, optimising for **small size and a fun UI**.

### 3.1 Framework: Tauri v2 (Rust core + web UI), one codebase for both platforms

Why: a very small base footprint (a few MB), native single-exe output on Windows, official Android support in v2, and a web-based UI is the easiest place to build a playful, animated, themed interface (CSS variables, custom fonts).

- **Frontend:** keep it tiny. Vanilla TypeScript with a very small component layer, or Svelte or Preact. **No heavy UI frameworks, no Electron, no large icon packs** (inline only the SVG icons used).
- **Backend (Rust):** engine adapter, pairing, planner, executor, journal, fs-time handling.
- **Android shim (Kotlin, Tauri plugin):** storage permission, native library path, media rescan.

**Fallback:** if Tauri-on-Android blocks something critical (for example running the bundled Perl binary or all-files access), the agent may switch to Flutter or Kotlin Multiplatform. In that case, document why in `DECISIONS.md`. The spec below is framework-agnostic from section 4 onward.

**Known risk, Windows:** Tauri uses the WebView2 runtime. It ships with Windows 11 and up-to-date Windows 10. **Do not bundle the fixed-version runtime** (it adds ~150 MB and breaks the lightweight goal). Instead, detect a missing runtime at launch and show a friendly one-screen explanation with a link to Microsoft's installer. Note this limitation in the README.

### 3.2 Metadata engine: bundled ExifTool + Perl (full coverage)

The owner chose full ExifTool coverage over a smaller native engine. This is the single biggest size and effort driver.

**Windows**
- Bundle the official standalone Windows ExifTool distribution (the exe plus its Perl runtime and libs).
- The app must still feel like a **single portable `.exe`**: embed the engine as a compressed archive inside the exe, and extract it on first launch to `%LOCALAPPDATA%\<AppName>\engine\<version>\`. Verify by hash and re-extract if missing or corrupted. No installer, no registry writes.

**Android**
- ExifTool is a Perl script, so ship a **statically linked Perl for `aarch64-linux-android`** (build it with `perl-cross` and the Android NDK) plus ExifTool's `lib/` Perl modules.
- Android 10+ forbids executing files from the app's writable data directory. Package the Perl binary as `libperl.so` in `jniLibs/arm64-v8a/` with `android:extractNativeLibs="true"` (or `useLegacyPackaging`), and run it from `nativeLibraryDir`. Keep ExifTool's `.pm` files in assets and extract them to `filesDir` on first launch.
- Link native binaries with 16 KB page alignment (`-Wl,-z,max-page-size=16384`) so they work on newer 16 KB-page devices.
- **arm64-v8a only.** Do not ship x86 or armv7 ABIs unless trivially cheap.

**Both platforms**
- Pin the latest stable ExifTool release at build time and record its **SHA-256** in the repo.
- Strip only what is safely unnecessary (for example ExifTool's `Lang/` translations, docs, sample configs). **Keep all format modules** so coverage stays "everything ExifTool supports".
- Include `THIRD_PARTY_NOTICES.md` (ExifTool, Perl, fonts, Leaflet, OSM attribution) and surface it in an About screen.
- Run ExifTool as a **persistent process** (`-stay_open True -@ <fifo/args>`) rather than spawning per file. Perl start-up is slow, especially on Android.
- Pass filenames safely: on Windows use `-charset filename=UTF8` and an args file (`-@`) with UTF-8 so Unicode and long paths work. For long paths, use extended-length paths (`\\?\`).

### 3.3 Size budget (targets, measure and report)

| Artifact | Target |
|---|---|
| Windows portable exe | about 25 MB or less |
| Android APK (arm64) | about 45 MB or less |
| Idle RAM | as low as practical; no background services |

No analytics, no telemetry, no accounts, no auto-update pinging. Add a Content-Security-Policy that blocks all network access except the map tile host (section 8.3).

---

## 4. Core workflows

### 4.1 App structure (screens)

1. **Home**: two big cards, **Copy Metadata** and **Edit Metadata**, plus a smaller **History / Undo** entry and a **Settings** gear.
2. **Copy flow**: Pick, then Pair, then Preview, then Run, then Done (a 5-step guided wizard with a visible step indicator).
3. **Edit flow**: Pick files, then Editor form, then Review, then Save, then Done.
4. **History / Undo**: list of past operations with an Undo button per operation.
5. **Settings**: defaults and cache management (section 10).
6. **About**: version, licences, bundled ExifTool version.

### 4.2 Copy flow in detail

**Step 1: Pick.** The user chooses how to supply files:
- Pick a **Source folder** and a **Converted folder**. If the Source folder contains a `converted` subfolder, offer it as the suggested Converted folder (mirrors the old script).
- Or pick **individual files** for each side.
- Optional toggle: *Include subfolders* (default off).
- Desktop: drag-and-drop zones. Android: system-style file and folder pickers.

**Step 2: Pair.** A **segmented control with four pairing modes** (the owner wants all four):

| Mode | Behaviour |
|---|---|
| **Exact name** | `IMG_001.jpg` pairs only with `IMG_001.jpg`. (Case-insensitive on Windows.) |
| **Name, any extension** | `clip.mov` pairs with `clip.mp4`. Matches on base name, ignoring the extension. |
| **Manual** | Two-column view; the user drags or taps to pair files by hand. |
| **Auto + fix leftovers** *(default)* | Runs Exact, then Name-any-extension. Unmatched and ambiguous files go into a "Needs your help" list for manual pairing. |

- **Ambiguity** (for example `clip.mov` could pair with both `clip.mp4` and `clip.gif`) is never auto-resolved. It lands in "Needs your help".
- Show the result as a list of pairs with thumbnails, names, a status chip (Matched / Ambiguous / Unmatched) and a quick "unpair" or "re-pair" action.

**Step 3: Preview** (required before any write; see section 5).

**Step 4: Run.** A progress screen with per-file status, a cancel button (stops cleanly between files, and the current file is finished or rolled back atomically), and a live counter.

**Step 5: Done.** Summary (success, warnings, failures), buttons for **Undo this batch**, **Open folder** (desktop) and **Start another**.

### 4.3 Edit flow in detail

- Pick one or many files, either individually or by folder.
- The editor shows **curated, friendly fields** (section 7).
- **Single file:** fields are pre-filled from the file's current metadata.
- **Bulk:** fields show a **"Mixed values" or "Leave unchanged"** placeholder. **Only fields the user actually touched are written.** Nothing else is modified.
- Each field has a small **clear** action, so the user can empty one field (this is *not* a "strip all metadata" feature; that is out of scope).
- A toggle: **"Also set file Created/Modified dates to match Date taken"** (default **on**).
- A toggle: **"Keep a backup as `<name>_original`"** (default **on**). If off, the pre-edit file is still retained in the Undo Cache.
- Same Preview, Run and Done pattern as Copy.

---

## 5. Copy semantics (the heart of the app)

### 5.1 What gets copied: "Everything by default, with an opt-out checklist"

Everything meaningful is copied by default. The Preview step shows a **checklist of categories**, all ticked, which the user can untick:

- Dates & times
- Location (GPS)
- Camera & lens
- Exposure & shooting settings
- Title, description, keywords, rating
- Author & copyright
- MakerNotes (manufacturer-specific data)
- Video / audio container metadata (QuickTime, etc.)
- Everything else (remaining writable tags)

Each category has a **tooltip** explaining what it contains in plain language, with one or two examples.

Implementation notes:
- Base command idea: `exiftool -tagsFromFile SRC -all:all [...] -o TEMP DEST`, tuned per format pair. The agent must test cross-format copies (for example HEIC to JPEG, MOV to MP4) and verify which tags actually land.
- Category toggles are implemented by building include/exclude arguments from a **category-to-tag-group map** maintained in one data file.
- **Structural/technical tags that describe the file's actual pixel or stream data** (dimensions, codec, colour profile, embedded thumbnails/previews, bitrate) must **never** be overwritten from the Source, because the Converted file legitimately differs. Do not expose these as options.
- **Orientation guard:** only copy `Orientation` when it will not double-rotate the image (for example, skip it if the Converted file's pixel dimensions already reflect the rotation). Flag skipped orientation in the Preview.
- Tags the destination format cannot store are skipped. The Preview shows a count, for example "12 tags can't be stored in PNG" (with an expandable list).

### 5.2 Date and timezone handling (auto)

The owner chose **automatic handling**. Photos usually store *local* time without a timezone. QuickTime/MP4 video usually stores **UTC**. Copying naively shifts times by hours.

Rules the agent must implement and unit-test:

1. Determine the Source's real **instant** and **UTC offset**. Use, in priority order: explicit offsets (`OffsetTimeOriginal`, QuickTime `Keys:CreationDate` which carries an offset on iPhone files, GPS timestamp), then the camera/device timezone fields if present.
2. **Photo to Video:** convert local time plus offset to UTC for the QuickTime tags (`CreateDate`, `ModifyDate`, `MediaCreateDate`, `TrackCreateDate`, etc.). Also write `Keys:CreationDate` with the offset where the container supports it.
3. **Video to Photo:** convert UTC to local using the Source's offset if known, otherwise a best-effort guess (GPS longitude is not accurate enough on its own). Write `DateTimeOriginal`, `CreateDate`, `ModifyDate`, and `OffsetTime*` where possible.
4. **Same-kind copies** (photo to photo, video to video) copy values as-is.
5. When the offset had to be **assumed** (no offset data anywhere), fall back to the device's current timezone and show a small, non-blocking note in the Preview ("Timezone assumed: ..."). The user can override the timezone per batch.
6. Do not rely on ExifTool's implicit `QuickTimeUTC` conversions. Compute explicit values in the app and write them, so behaviour is identical on Windows and Android regardless of the device timezone setting.

### 5.3 File-system dates (always)

After the swap, the final file's **Created** and **Modified** dates are set to match the Source's. This is **not optional** for Copy.

- **Windows:** set both Created and Modified (for example via `SetFileTime`).
- **Android:** the file creation (birth) time **cannot** be set. Set the modified time only, and tell the user this in the UI (tooltip on the Done screen and in Settings > About platform limits).
- Read the Source's file-system dates **before** the swap and store them in the journal so Undo can restore them.
- On Android, after the swap, trigger a **media rescan** (for example `MediaScannerConnection`) so the Gallery shows correct dates and the new file.

### 5.4 The Swap (requirement 6, precise definition)

Let **Source** = `Ds/Bs.Es` (directory, base name, extension) and **Converted** = `Dc/Bc.Ec`.

After the metadata is written into the Converted file:

1. **Source is renamed in place** to `Bs_original.Es` (same folder `Ds`).
   - If `Bs_original.Es` already exists, use `Bs_original_2.Es`, then `_3`, and so on. Never overwrite.
2. **Converted is moved to `Ds` and renamed to `Bs.Ec`.**
   - The **base name** comes from the Source. The **extension stays that of the Converted file**, because it reflects the real file type (a `clip.mov` Source plus a `clip.mp4` Converted gives `clip.mp4` plus `clip_original.mov`).
3. If a *different* file named `Bs.Ec` already exists in `Ds` (not the Source itself), stop for that pair, mark it as a **Conflict** in the Preview, and let the user choose *Skip* or *Pick another name*.
4. If Converted is on a different volume than `Ds`, use **copy, verify size and hash, then delete**. Never a bare cross-volume rename.

### 5.5 Safety: Preview and Undo (required)

**Preview (before any write):** show a per-pair table with:
- The pair, the planned final names and paths (what will be renamed and moved).
- A **before/after diff** of key fields (Date taken, GPS, Camera, Title, etc.).
- Counts per category (copied / skipped / not storable).
- Warnings (timezone assumed, conflict, low disk space, read-only file, orientation skipped).
- A prominent **Apply** button. Nothing is written until the user taps it.

**Pre-flight checks per pair:** both files exist and are readable and writable, they are not the same file, the file is not locked, there is enough free space for a full rewrite (writing MP4/MOV metadata can rewrite the whole file, so require roughly the Converted file's size plus a margin), and no name conflicts.

**Atomic execution per pair (journal-first):**
1. Write the operation plan to a **journal** (JSON in app data) *before* touching files.
2. Write metadata into a **temporary output file** (ExifTool `-o`), in the Converted file's directory. Verify: file opens, size is sensible, key tags re-read correctly.
3. Perform the swap steps (5.4), retaining the **pre-change Converted file** in the Undo Cache.
4. Apply file-system dates (5.3).
5. Mark the journal entry complete.

If anything fails, roll back that pair to its original state and continue with the next pair (or stop, depending on a Settings choice). On startup, if an incomplete journal is found, offer to **finish or roll back**.

**Undo:** per operation (a batch counts as one operation). Undo restores **exact prior state**: Source back to its original name, Converted back to its original location, name and content, and file-system dates restored from the journal. Undo is blocked, with a clear message, if files have since been changed or removed (checked by size and mtime stored in the journal).

**Undo Cache:** a hidden app-managed folder on the **same volume** as the files it protects (so retention is a cheap rename, not a copy). Auto-purge by age (default 7 days) and a size cap; Settings has a **Clear Undo Cache** button that shows the current size.

> Invariant: **no pre-change file is ever deleted until the operation is verified**, and every pre-change file is recoverable either as a visible `_original` or from the Undo Cache.

---

## 6. Android specifics

- ExifTool needs real file paths, so Storage Access Framework URIs are not enough. Because distribution is **sideload only** (GitHub APK), request **All Files Access** (`MANAGE_EXTERNAL_STORAGE`) with a friendly onboarding screen explaining why. If the user declines, show what is blocked and a button to reopen the permission screen. No other permissions except `INTERNET` (map tiles only, section 8.3).
- Skip files the OS will not let us modify (for example other apps' private `Android/data` folders) and report them clearly.
- Handle SD cards and USB storage if paths are accessible.
- Keep the app responsive: run ExifTool work off the UI thread, show progress, and keep working when the screen dims (use a foreground notification only if long batches require it; justify in `DECISIONS.md`).
- Use platform thumbnail APIs (for example `ContentResolver.loadThumbnail`), not ffmpeg or other bundled decoders.

## 6.1 Windows specifics

- Single portable `.exe`, no admin rights, no installer, no registry writes.
- Support Unicode filenames and long paths.
- Drag-and-drop of files and folders onto the window.
- Use shell thumbnail APIs for previews. Fall back to file-type icons.
- Code signing is optional; note SmartScreen warnings in the README.

---

## 7. Manual editor: curated fields

Curated fields only (no raw tag editor, no strip-all). The agent must consult ExifTool's tag documentation, write each value to the right groups per format, and **disable a field with an explanatory tooltip** when a file type cannot store it.

| Field (UI) | Notes and typical targets |
|---|---|
| **Date taken** (date, time, timezone offset) | Images: `DateTimeOriginal`, `CreateDate`, `ModifyDate`, `OffsetTime*`. Video: QuickTime date tags in UTC plus `Keys:CreationDate` with offset (follow section 5.2). XMP equivalents where applicable. |
| **Location** (lat, lon, altitude) | `GPS*` tags for images; QuickTime `Keys:GPSCoordinates` for video. Map picker (section 8.3), manual entry, and paste support for strings like `12.3456, 78.9012`. |
| **Camera** (make, model, lens) | `Make`, `Model`, `LensModel` (and equivalents for video where they exist). |
| **Title** | XMP-dc:Title and format equivalents. |
| **Description / caption** | `ImageDescription`, XMP-dc:Description, IPTC Caption-Abstract (keep in sync). |
| **Keywords** (chip input) | XMP-dc:Subject, IPTC:Keywords (keep in sync). |
| **Rating** (0-5 stars) | XMP:Rating (and Windows rating tags where relevant). |
| **Author / artist** | `Artist`, XMP-dc:Creator, IPTC By-line (keep in sync). |
| **Copyright** | `Copyright`, XMP-dc:Rights, IPTC CopyrightNotice (keep in sync). |

Also show a small read-only **File info** panel (type, size, dimensions, duration). It helps people tell files apart and costs almost nothing.

---

## 8. UI / UX design

### 8.1 Theme tokens

Dark only. Base the entire palette on the three owner colours; derive a few neutrals and keep semantic colours minimal.

```css
:root {
  --jet:        #000000;  /* app background */
  --moss:       #6D8F63;  /* secondary actions, borders, icons, muted text accents */
  --neon:       #39FF88;  /* primary actions, focus, progress, success glow */

  /* derived neutrals (tints of the base colours) */
  --surface-1:  #0B0F0C;  /* cards */
  --surface-2:  #121A14;  /* raised / inputs */
  --border:     rgba(109,143,99,0.35);
  --text:       #E8F2E9;
  --text-muted: #9DB596;

  /* semantic (use sparingly) */
  --warn:       #FFB547;
  --danger:     #FF6B6B;
}
```

Rules:
- **Primary button:** neon fill with **black text**. **Secondary:** moss outline. Neon is for emphasis, never for large body-text areas.
- Neon glow (soft outer shadow) on focus and active states. Check text contrast (WCAG AA minimum).
- Cards: large rounded corners (about 16 to 20 px), generous spacing.

### 8.2 Typography (bundled, offline, subset to Latin to save size)

- **Display / headings / buttons:** a rounded, playful face such as **Fredoka** (SIL OFL).
- **Body / forms:** **Nunito** (SIL OFL).
- Ship only the weights used, as `woff2`. Include font licences in `THIRD_PARTY_NOTICES.md`. No web-font CDN calls, ever.

### 8.3 Map picker for GPS (the only online feature)

- Use a **small map library (Leaflet)** bundled locally, with OpenStreetMap tiles loaded online at runtime. Include proper **OSM attribution** and respect the tile usage policy (single-picker usage, identifiable app, no bulk prefetching).
- The map is opened on demand from the Location field. **Manual lat/lon entry and "paste coordinates" must always work offline.** If offline, show a friendly inline message ("No connection. You can still type or paste coordinates!").
- The CSP and any network allow-list permit only the tile host. Nothing else touches the network.
- Tap/click to drop a pin, drag to adjust, with a "use this location" confirm button.

### 8.4 Personality: friendly and clean (no mascot)

- Fun fonts, **soft animations** (150-250 ms eases, gentle emerald pulses on success, small slide/fade transitions). **Respect reduced-motion** settings.
- **Light humour in tooltips and empty states**, never at the cost of clarity. Examples:
  - Empty drop zone: "Drop your files here. We'll be gentle."
  - Preview heading: "Here's what's about to happen. Nothing is changed yet."
  - Success: "All done! Your memories are back where they belong."
  - Undo hint: "Changed your mind? Undo puts everything back exactly as it was."
- Every non-obvious control has an **info tooltip**: hover on desktop, tap or long-press on mobile (a small "i" icon). Tooltips explain *what it does* and *why you might want it*.
- Plain language everywhere. Avoid jargon (say "Camera & lens", not "EXIF IFD0"). Errors say what happened, why, and what to do next.

### 8.5 Layout and accessibility

- **Desktop:** two-pane layouts where useful (file list and detail/preview). **Android:** single column, large touch targets (at least 48 dp), bottom navigation or a clear back path.
- Keyboard navigable on Windows with a visible focus ring. Screen-reader labels on all controls.
- Unicode-safe display of filenames and metadata values (any script, emoji).

---

## 9. Suggested module breakdown

| Module | Responsibility |
|---|---|
| `engine/` | Starts and manages the persistent ExifTool process; read and write APIs; argument builders; output parsing. Platform-specific launch logic (Windows exe vs Android `libperl.so`). |
| `categories/` | Data-driven map from UI categories to ExifTool tag groups; per-format capability notes. |
| `pairing/` | The four pairing modes, ambiguity detection, manual pairing state. |
| `datetime/` | Local/UTC/offset resolution and conversion rules (5.2) with exhaustive unit tests. |
| `planner/` | Builds a Preview plan (names, diffs, warnings, conflicts) without touching files. |
| `executor/` | Atomic per-pair execution, temp files, swap, fs-dates, verification, cancellation. |
| `journal/` | Operation journal, crash recovery, Undo logic, Undo Cache management. |
| `platform/` | File-system dates, long paths, thumbnails, Android storage permission, media rescan. |
| `ui/` | Screens, components, theme tokens, tooltip system, microcopy in one strings file. |

Keep all user-facing text in a single strings file (English only for v1, but structured for later localisation).

---

## 10. Settings (keep it short)

- Default pairing mode (default: Auto + fix leftovers).
- Include subfolders by default (default: off).
- On failure in a batch: *continue with the rest* (default) or *stop*.
- Undo Cache: retention days (default 7), size cap, **Clear Undo Cache** (shows size).
- About: versions, licences, bundled ExifTool version, known platform limits.

---

## 11. Error handling

Every failure is shown per file with a human-readable reason and a suggested action. Cover at least:
- Unsupported or unwritable format (offer "skip").
- Read-only, locked or permission-denied files.
- Insufficient disk space (checked pre-flight, and handled if it still happens).
- Name conflicts (5.4).
- Corrupt files or ExifTool warnings (show the warning text under an "Details" expander).
- Source/Converted mismatch (for example the pair has wildly different durations or dimensions): warn, and let the user continue or skip. This is a hint only, never a block.
- Crash or kill mid-operation: recover from the journal on next launch.

---

## 12. Non-goals (do not build)

Cloud sync, accounts, analytics, telemetry, auto-update checks, image or video editing, AI features, a photo gallery or library manager, a raw tag editor, "strip all metadata" mode, light theme, ffmpeg or other heavy codecs, and extra languages (v1 is English only). Anything not listed in sections 4-10 should be treated as out of scope unless it is needed to make a listed feature work.

---

## 13. Testing requirements

**Fixture matrix** (small, committed or generated test files):
- JPEG to JPEG (recompressed), HEIC to JPEG, PNG, WebP, a RAW (for example DNG/CR2/NEF) to JPEG, PDF, an audio file, MP4 to MP4, MOV to MP4, an iPhone-style MOV with `Keys:CreationDate`.
- Filenames with spaces, Unicode, emoji and very long paths.
- Read-only files, same-name-different-extension pairs, an already-existing `_original` file, cross-volume swap, low-disk simulation, cancel mid-batch, forced crash mid-write.

**Must-pass behaviours:**
- Date/timezone conversion tests (photo to video, video to photo, offset known vs assumed), independent of the machine's timezone.
- Swap produces the exact names in 5.4, including the extension rule and collision suffixes.
- Undo returns **byte-identical** files, names, locations and file-system dates (hash comparison).
- A bulk edit writes **only touched fields** (diff all other tags before and after).
- Orientation is never double-applied.
- The app makes **no network requests** except the map tiles (verify with a packet capture or proxy).

**Android:** test on multiple versions (at least Android 10, 12, 14 and 15 or newer), including a 16 KB page-size environment, and with SD-card storage if available.

---

## 14. Delivery

- **Windows:** one portable `.exe`.
- **Android:** a signed sideload `.apk` (arm64), with a README explaining "install from unknown sources" and the All Files Access permission.
- Repo with `README.md` (what it is, install, limits), `THIRD_PARTY_NOTICES.md`, `DECISIONS.md` (framework choice, Android launch approach, deviations), reproducible build scripts, and the pinned ExifTool version and SHA-256.
- Release notes list known platform differences (Android cannot set file creation time; Windows needs the WebView2 runtime).

---

## 15. Acceptance checklist

- [ ] Works on Windows x64 (portable exe) and Android arm64 (APK).
- [ ] First launch works with **no internet and nothing preinstalled** (except WebView2 where required, with a friendly message if missing).
- [ ] Copy flow works with all **four pairing modes**, including extension-insensitive matching.
- [ ] Preview shows renames, diffs, category checklist and warnings, and nothing changes until Apply.
- [ ] All metadata categories copy by default, and unticking a category excludes it.
- [ ] Photo/video date conversion is correct and timezone-independent.
- [ ] Swap: Converted takes the Source's folder and base name; Source becomes `_original` (with collision handling).
- [ ] File-system Created/Modified dates match the Source (Created on Windows; Modified on Android, with the limitation explained).
- [ ] Single and bulk manual editing work; bulk writes only touched fields.
- [ ] Map picker works online; manual coordinate entry works offline.
- [ ] Undo restores exact prior state; crash recovery works; Undo Cache is manageable.
- [ ] Theme uses only the specified colours (plus minimal semantic warn/danger), bundled fun fonts, hover/tap tooltips everywhere that matters.
- [ ] Size targets reported and met (or deviations justified in `DECISIONS.md`).
- [ ] No telemetry, no analytics, no unexpected network calls.

---

## 16. Assumptions to confirm (decisions I made while interpreting requirements)

1. **Extension on swap:** the final file keeps the *Converted* file's extension with the *Source's* base name (because the extension must match the real file type). If you want the Source's extension forced, that would corrupt type information, so I advise against it.
2. **Structural tags are never copied** (dimensions, codec, colour profile, embedded thumbnails), and **Orientation uses a guard**. This slightly narrows "everything" to "everything that is safe", and it prevents broken or rotated output.
3. **Undo keeps the pre-change Converted file** in a hidden Undo Cache (default 7 days) so Undo is exact. The cost is temporary disk space.
4. **Android requires All Files Access** (sideload only), because ExifTool needs real file paths.
5. **The map is the only online feature**, and `INTERNET` is used for map tiles only.
6. **Tauri v2** is my recommended stack. The agent may deviate with justification.
7. **Manual edit mode keeps an `_original` backup by default**, mirroring the Copy workflow's philosophy (toggle available).
