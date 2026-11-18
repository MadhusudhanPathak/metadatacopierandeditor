//! Picking files: folder expansion, the Converted-folder suggestion, pairing, thumbnails and
//! offline place search.

use super::{R, app_state, blocking};
use crate::geo::{Nearby, Place};
use crate::metadata::tags::{Kind, kind_of};
use crate::planning::pairing::{self, PairMode, PairingResult};
use crate::platform::thumbs;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// Thumbnail edge length limits (px).
const THUMB_MIN: u32 = 32;
const THUMB_MAX: u32 = 256;
/// Places returned per search.
const PLACE_RESULTS: usize = 40;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    path: String,
    name: String,
    size: u64,
    kind: Kind,
}

impl FileInfo {
    fn of(p: &Path) -> Self {
        FileInfo {
            path: p.to_string_lossy().into_owned(),
            name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            kind: kind_of(p),
        }
    }
}

/// Expands dropped or picked paths: folders are scanned (optionally recursively, skipping
/// `exclude`), files are kept. App files are skipped either way. Sorted, without duplicates.
#[tauri::command]
pub async fn expand_paths(paths: Vec<String>, recursive: bool, exclude: Option<String>) -> R<Vec<FileInfo>> {
    blocking(move || {
        let excl = exclude.map(PathBuf::from);
        let mut out = vec![];
        for p in paths.into_iter().map(PathBuf::from) {
            if p.is_dir() {
                out.extend(pairing::scan(&p, recursive, excl.as_deref())?.iter().map(|f| FileInfo::of(f)));
            } else if p.is_file() && !pairing::is_app_file(&p) {
                out.push(FileInfo::of(&p));
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        Ok(out)
    })
    .await
}

/// If the Source folder has a `converted` subfolder, suggest it (spec 4.2 step 1).
#[tauri::command]
pub fn suggest_converted(source_dir: String) -> Option<String> {
    std::fs::read_dir(&source_dir)
        .ok()?
        .flatten()
        .find(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_string_lossy().eq_ignore_ascii_case("converted"))
        .map(|e| e.path().to_string_lossy().into_owned())
}

/// Pairs Source and Converted files with the chosen mode (spec 4.2).
#[tauri::command]
pub fn pair_files(sources: Vec<String>, converted: Vec<String>, mode: PairMode) -> PairingResult {
    let s: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).collect();
    let c: Vec<PathBuf> = converted.into_iter().map(PathBuf::from).collect();
    pairing::pair(&s, &c, mode)
}

/// Platform thumbnail as a PNG data URL, or None (the UI then shows a kind icon).
#[tauri::command]
pub async fn thumbnail(path: String, size: u32) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || thumbs::thumbnail(Path::new(&path), size.clamp(THUMB_MIN, THUMB_MAX))).await.ok().flatten()
}

/// Offline place search for the map picker (D16), from ExifTool's bundled city database.
#[tauri::command]
pub async fn search_places(app: AppHandle, query: String) -> R<Vec<Place>> {
    let s = app_state(&app);
    blocking(move || Ok(s.places()?.search(&query, PLACE_RESULTS))).await
}

/// The nearest town or city to a point, offline (T21), to name a dropped pin or a file's GPS.
#[tauri::command]
pub async fn nearest_place(app: AppHandle, lat: f64, lon: f64) -> R<Option<Nearby>> {
    let s = app_state(&app);
    blocking(move || Ok(s.places()?.nearest(lat, lon))).await
}
