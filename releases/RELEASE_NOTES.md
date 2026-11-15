# Metadata Copier and Editor v0.1.0 — first release (Windows)

Bring back the dates, places and camera details that your photos and videos lost when you compressed or converted them, or edit them by hand. Everything runs on your PC.

## Download

- **`MetadataCopierAndEditor-v0.1.0-windows-x64.exe`** (15 MB): a single portable app for Windows 10/11 (64-bit). There's no installer and it doesn't need admin rights. Just run it.
- `….exe.sha256` lets you check the download (optional).

## What it does

- **Copy Metadata:** pairs your originals with their converted copies and copies all meaningful metadata across. Afterwards each folder holds `name_original` (your untouched original) and `name` (the converted file with the metadata). Photo ↔ video dates and timezones are converted for you.
- **Edit Metadata:** date taken (set it, or shift a whole batch), location (type, paste, or search and pick on a map), camera, title, description, keywords, rating, author and copyright, for one file or many. Only the fields you change are written, and each edited file keeps its untouched version as `name_original`.
- **Preview before anything changes**, then **Undo** from the Done screen or History. If the app closes mid-way, the next launch offers to finish or put everything back.

Works with practically every photo, video and RAW format, thanks to the bundled ExifTool 13.63.

## Good to know

- **Windows may warn "Windows protected your PC"** because the app isn't code-signed yet. Click **More info → Run anyway**.
- It needs **Microsoft Edge WebView2**, which is built into Windows 11 and up-to-date Windows 10. If it's missing, the app tells you and links to Microsoft's free installer.
- On first start it unpacks its metadata engine (about 28 MB) to `%LOCALAPPDATA%\Metadata Copier and Editor\`. Delete that folder to remove every trace.
- Keep the original file names when converting (for example `IMG_001.HEIC` → `IMG_001.jpg`) so files pair up automatically.
- Some formats can't hold metadata (MP3, WAV, AVI, MKV, BMP). The Preview lists them and skips them.
- Undo restores your originals exactly. Converted files go back to their own folder and name, keeping the copied metadata.
- **Privacy:** no accounts, analytics or update checks. Only the optional map goes online: it loads map tiles from OpenStreetMap, and its **Search online** button sends your search text to OpenStreetMap. Searching towns and cities works offline.

## Platform notes

- **Android** is planned. When it arrives, it won't be able to change a file's "created" date, only "modified" (an Android limitation).

Licence: GPL-3.0-or-later. Third-party licences are in the app's About screen and in `THIRD_PARTY_NOTICES.md`.
