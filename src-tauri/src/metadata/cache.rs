//! Metadata cache keyed by path + size + modified time, so re-planning (e.g. toggling a
//! category in the Preview) doesn't re-read files.

use super::tags::TagMap;
use crate::engine::Engine;
use crate::platform::fsops::{self, Stamp};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct MetaCache {
    map: Mutex<HashMap<PathBuf, (Stamp, TagMap)>>,
}

/// `-api Geolocation` adds GeolocationTimeZone from GPS coordinates using ExifTool's bundled
/// offline city database (D8).
const READ_ARGS: &[&str] = &["-api", "Geolocation=1"];

impl MetaCache {
    fn fresh(&self, p: &Path) -> bool {
        let Ok(st) = fsops::stamp(p) else { return false };
        self.map.lock().unwrap().get(p).is_some_and(|(s, _)| *s == st)
    }

    /// Reads any files not already cached (in batches, one engine call each).
    pub fn prefetch(&self, engine: &Engine, paths: &[PathBuf]) {
        let mut todo: Vec<PathBuf> = paths.iter().filter(|p| p.is_file() && !self.fresh(p)).cloned().collect();
        todo.sort();
        todo.dedup();
        for chunk in todo.chunks(100) {
            let refs: Vec<&Path> = chunk.iter().map(PathBuf::as_path).collect();
            match engine.read(&refs, READ_ARGS) {
                Ok(maps) if maps.len() == chunk.len() => {
                    let mut m = self.map.lock().unwrap();
                    for (p, tm) in chunk.iter().zip(maps) {
                        if let Ok(st) = fsops::stamp(p) {
                            m.insert(p.clone(), (st, tm));
                        }
                    }
                }
                _ => {
                    // Fall back to one-by-one so a single bad file doesn't hide the others.
                    for p in chunk {
                        if let (Ok(tm), Ok(st)) = (engine.read_one(p, READ_ARGS), fsops::stamp(p)) {
                            self.map.lock().unwrap().insert(p.clone(), (st, tm));
                        }
                    }
                }
            }
        }
    }

    /// Cached tags for `p`, if the file hasn't changed since it was read.
    pub fn get(&self, p: &Path) -> Option<TagMap> {
        let st = fsops::stamp(p).ok()?;
        self.map.lock().unwrap().get(p).filter(|(s, _)| *s == st).map(|(_, m)| m.clone())
    }

    /// Forgets everything (after files were changed by an operation).
    pub fn clear(&self) {
        self.map.lock().unwrap().clear();
    }
}
