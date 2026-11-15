//! User settings (spec 10), stored as JSON in app data.

use crate::planning::pairing::PairMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub pairing_mode: PairMode,
    pub include_subfolders: bool,
    /// On failure in a batch: false = continue with the rest (default), true = stop.
    pub stop_on_failure: bool,
    /// Seen the "this changes your files" welcome note.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { pairing_mode: PairMode::Auto, include_subfolders: false, stop_on_failure: false, onboarded: false }
    }
}

fn path(base: &Path) -> PathBuf {
    base.join("settings.json")
}

/// Saved settings, or the defaults if there are none or they can't be read.
pub fn load(base: &Path) -> Settings {
    std::fs::read(path(base)).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

/// Writes a temp file and renames it over the old one, so a crash never leaves half a file.
pub fn save(base: &Path, s: &Settings) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    let tmp = path(base).with_extension("json.tmp");
    std::fs::write(&tmp, data).map_err(|e| format!("Couldn't save settings: {e}"))?;
    std::fs::rename(&tmp, path(base)).map_err(|e| format!("Couldn't save settings: {e}"))
}
