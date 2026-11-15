//! Pairing (spec 4.2 step 2, D2, D3, D5): four modes, kind-first matching, ambiguity detection,
//! and skipping of the app's own `_original` and temp files.

use crate::metadata::tags::{Kind, kind_of};
use crate::platform::fsops::path_key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum PairMode {
    /// Identical file names.
    Exact,
    /// Same base name and media kind, any extension.
    AnyExt,
    /// Nothing is paired automatically.
    Manual,
    /// Exact first, then AnyExt for the rest.
    Auto,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum MatchHow {
    Exact,
    AnyExt,
    Manual,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub source: String,
    pub converted: String,
    pub how: MatchHow,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LeftStatus {
    Ambiguous,
    Unmatched,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub status: LeftStatus,
    /// For ambiguous files: the files on the other side it could pair with.
    pub candidates: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PairingResult {
    pub pairs: Vec<Pair>,
    pub sources_left: Vec<Leftover>,
    pub converted_left: Vec<Leftover>,
}

/// Files the app itself creates and must never treat as user files (D5):
/// `name_original.ext`, `name_original_N.ext`, ExifTool's `x.jpg_original`, `.mce-*` temp
/// files, and `.mce-undo/` folders left by early test builds.
pub fn is_app_file(path: &Path) -> bool {
    if path.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".mce-undo")) {
        return true;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if name.contains(".mce-tmp") || name.starts_with(".mce-") {
        return true;
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if ext.ends_with("_original") {
        return true;
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    is_original_stem(&stem)
}

fn is_original_stem(stem: &str) -> bool {
    if stem.ends_with("_original") {
        return true;
    }
    if let Some(idx) = stem.rfind("_original_") {
        let tail = &stem[idx + "_original_".len()..];
        return !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Lists files in `dir` (optionally recursive), skipping app files, hidden system files
/// and the `exclude` folder (e.g. the Converted folder nested inside the Source folder).
pub fn scan(dir: &Path, recursive: bool, exclude: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    let excl = exclude.map(path_key);
    while let Some(d) = stack.pop() {
        let rd = std::fs::read_dir(&d).map_err(|e| format!("Couldn't open the folder {}: {e}", d.display()))?;
        for entry in rd.flatten() {
            let p = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            if is_app_file(&p) || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") || name.eq_ignore_ascii_case(".nomedia") {
                continue;
            }
            if ft.is_dir() {
                if recursive && excl.as_deref() != Some(path_key(&p).as_str()) && !name.starts_with('.') {
                    stack.push(p);
                }
            } else if ft.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Name comparison is case-insensitive on Windows (spec 4.2).
fn norm(s: &str) -> String {
    if cfg!(windows) { s.to_lowercase() } else { s.to_string() }
}

fn exact_key(p: &Path) -> String {
    norm(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

fn anyext_key(p: &Path) -> (Kind, String) {
    (kind_of(p), norm(&p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
}

struct Matched {
    pairs: Vec<Pair>,
    ambiguous_src: Vec<Leftover>,
    ambiguous_conv: Vec<Leftover>,
    rest_src: Vec<PathBuf>,
    rest_conv: Vec<PathBuf>,
}

fn match_by<K: Ord + Clone>(src: &[PathBuf], conv: &[PathBuf], key: impl Fn(&Path) -> K, how: MatchHow) -> Matched {
    let mut s_map: BTreeMap<K, Vec<&PathBuf>> = BTreeMap::new();
    let mut c_map: BTreeMap<K, Vec<&PathBuf>> = BTreeMap::new();
    for p in src {
        s_map.entry(key(p)).or_default().push(p);
    }
    for p in conv {
        c_map.entry(key(p)).or_default().push(p);
    }
    let mut m = Matched { pairs: vec![], ambiguous_src: vec![], ambiguous_conv: vec![], rest_src: vec![], rest_conv: vec![] };
    let s_str = |v: &Vec<&PathBuf>| v.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
    for (k, ss) in &s_map {
        match c_map.get(k) {
            Some(cs) if ss.len() == 1 && cs.len() == 1 => {
                m.pairs.push(Pair { source: ss[0].to_string_lossy().into_owned(), converted: cs[0].to_string_lossy().into_owned(), how })
            }
            Some(cs) => {
                for s in ss {
                    m.ambiguous_src.push(Leftover { path: s.to_string_lossy().into_owned(), status: LeftStatus::Ambiguous, candidates: s_str(cs) });
                }
                for c in cs {
                    m.ambiguous_conv.push(Leftover { path: c.to_string_lossy().into_owned(), status: LeftStatus::Ambiguous, candidates: s_str(ss) });
                }
            }
            None => m.rest_src.extend(ss.iter().map(|p| (*p).clone())),
        }
    }
    for (k, cs) in &c_map {
        if !s_map.contains_key(k) {
            m.rest_conv.extend(cs.iter().map(|p| (*p).clone()));
        }
    }
    m
}

fn unmatched(v: &[PathBuf]) -> Vec<Leftover> {
    v.iter().map(|p| Leftover { path: p.to_string_lossy().into_owned(), status: LeftStatus::Unmatched, candidates: vec![] }).collect()
}

/// Pairs Source files with Converted files using the chosen mode.
pub fn pair(sources: &[PathBuf], converted: &[PathBuf], mode: PairMode) -> PairingResult {
    let sources: Vec<PathBuf> = sources.iter().filter(|p| !is_app_file(p)).cloned().collect();
    let converted: Vec<PathBuf> = converted.iter().filter(|p| !is_app_file(p)).cloned().collect();
    let mut r = PairingResult::default();
    match mode {
        PairMode::Manual => {
            r.sources_left = unmatched(&sources);
            r.converted_left = unmatched(&converted);
        }
        PairMode::Exact => {
            let m = match_by(&sources, &converted, exact_key, MatchHow::Exact);
            r.pairs = m.pairs;
            r.sources_left = [m.ambiguous_src, unmatched(&m.rest_src)].concat();
            r.converted_left = [m.ambiguous_conv, unmatched(&m.rest_conv)].concat();
        }
        PairMode::AnyExt => {
            let m = match_by(&sources, &converted, anyext_key, MatchHow::AnyExt);
            r.pairs = m.pairs;
            r.sources_left = [m.ambiguous_src, unmatched(&m.rest_src)].concat();
            r.converted_left = [m.ambiguous_conv, unmatched(&m.rest_conv)].concat();
        }
        PairMode::Auto => {
            let a = match_by(&sources, &converted, exact_key, MatchHow::Exact);
            let b = match_by(&a.rest_src, &a.rest_conv, anyext_key, MatchHow::AnyExt);
            r.pairs = [a.pairs, b.pairs].concat();
            r.sources_left = [a.ambiguous_src, b.ambiguous_src, unmatched(&b.rest_src)].concat();
            r.converted_left = [a.ambiguous_conv, b.ambiguous_conv, unmatched(&b.rest_conv)].concat();
        }
    }
    r.pairs.sort_by(|x, y| x.source.cmp(&y.source));
    r.sources_left.sort_by(|x, y| x.path.cmp(&y.path));
    r.converted_left.sort_by(|x, y| x.path.cmp(&y.path));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: &[&str]) -> Vec<PathBuf> {
        v.iter().map(PathBuf::from).collect()
    }
    fn names(r: &PairingResult) -> Vec<(String, String)> {
        r.pairs
            .iter()
            .map(|x| {
                (
                    Path::new(&x.source).file_name().unwrap().to_string_lossy().into_owned(),
                    Path::new(&x.converted).file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn exact_mode() {
        let r = pair(&p(&["s/IMG_001.jpg", "s/IMG_002.jpg"]), &p(&["c/IMG_001.jpg", "c/IMG_002.png"]), PairMode::Exact);
        assert_eq!(names(&r), vec![("IMG_001.jpg".into(), "IMG_001.jpg".into())]);
        assert_eq!(r.sources_left.len(), 1);
        assert_eq!(r.converted_left.len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn exact_mode_is_case_insensitive_on_windows() {
        let r = pair(&p(&["s/img_001.JPG"]), &p(&["c/IMG_001.jpg"]), PairMode::Exact);
        assert_eq!(r.pairs.len(), 1);
    }

    #[test]
    fn any_extension_mode() {
        let r = pair(&p(&["s/clip.mov"]), &p(&["c/clip.mp4"]), PairMode::AnyExt);
        assert_eq!(names(&r), vec![("clip.mov".into(), "clip.mp4".into())]);
    }

    #[test]
    fn live_photos_pair_by_kind() {
        // D3: IMG_001.HEIC + IMG_001.MOV must pair with IMG_001.jpg + IMG_001.mp4 without ambiguity.
        let r = pair(&p(&["s/IMG_001.HEIC", "s/IMG_001.MOV"]), &p(&["c/IMG_001.jpg", "c/IMG_001.mp4"]), PairMode::Auto);
        let mut n = names(&r);
        n.sort();
        assert_eq!(n, vec![("IMG_001.HEIC".into(), "IMG_001.jpg".into()), ("IMG_001.MOV".into(), "IMG_001.mp4".into())]);
        assert!(r.sources_left.is_empty() && r.converted_left.is_empty());
    }

    #[test]
    fn ambiguity_within_a_kind_needs_help() {
        // clip.mov could become clip.mp4 or clip.m4v: both videos, never auto-resolved.
        let r = pair(&p(&["s/clip.mov"]), &p(&["c/clip.mp4", "c/clip.m4v"]), PairMode::Auto);
        assert!(r.pairs.is_empty());
        assert_eq!(r.sources_left[0].status, LeftStatus::Ambiguous);
        assert_eq!(r.sources_left[0].candidates.len(), 2);
        assert_eq!(r.converted_left.len(), 2);
    }

    #[test]
    fn auto_prefers_exact_then_any_extension() {
        let r = pair(&p(&["s/a.jpg", "s/b.heic", "s/c.jpg"]), &p(&["c/a.jpg", "c/b.jpg", "c/zzz.jpg"]), PairMode::Auto);
        let n = names(&r);
        assert!(n.contains(&("a.jpg".into(), "a.jpg".into())));
        assert!(n.contains(&("b.heic".into(), "b.jpg".into())));
        assert_eq!(r.pairs.iter().find(|x| x.source.ends_with("a.jpg")).unwrap().how, MatchHow::Exact);
        assert_eq!(r.pairs.iter().find(|x| x.source.ends_with("b.heic")).unwrap().how, MatchHow::AnyExt);
        assert_eq!(r.sources_left.len(), 1);
        assert_eq!(r.converted_left.len(), 1);
    }

    #[test]
    fn manual_mode_pairs_nothing() {
        let r = pair(&p(&["s/a.jpg"]), &p(&["c/a.jpg"]), PairMode::Manual);
        assert!(r.pairs.is_empty());
        assert_eq!(r.sources_left.len(), 1);
    }

    #[test]
    fn own_files_are_ignored() {
        for f in ["IMG_original.jpg", "IMG_original_2.jpg", "IMG_ORIGINAL_13.HEIC", "x.jpg_original", ".mce-undo/op/1-a.jpg", "a.mce-tmp-1.jpg"] {
            assert!(is_app_file(Path::new(f)), "{f}");
        }
        for f in ["IMG_001.jpg", "my_originals.jpg", "x_original_b.jpg", "original.jpg"] {
            assert!(!is_app_file(Path::new(f)), "{f}");
        }
        let r = pair(&p(&["s/IMG_original.jpg", "s/IMG.jpg"]), &p(&["c/IMG.jpg"]), PairMode::Auto);
        assert_eq!(r.pairs.len(), 1);
        assert!(r.sources_left.is_empty());
    }
}
