//! Checking a freshly written temp file before it replaces anything (spec 5.5 step 2, T9), and
//! turning engine errors into plain language.

use crate::engine::Engine;
use crate::metadata::datetime::parse_exif_dt;
use std::path::Path;

/// Loose equality for read-back values: exact text, numbers within 1e-6, or the same local
/// date-time (the engine may append an offset).
fn matches_expected(got: &str, want: &str) -> bool {
    let (g, w) = (got.trim(), want.trim());
    if g == w {
        return true;
    }
    if let (Ok(a), Ok(b)) = (g.parse::<f64>(), w.parse::<f64>()) {
        return (a - b).abs() < 1e-6;
    }
    if let (Some(a), Some(b)) = (parse_exif_dt(g), parse_exif_dt(w)) {
        return a.local == b.local;
    }
    false
}

/// The temp file must be non-empty, open without an engine error, and read back every
/// expected tag value.
pub fn verify(engine: &Engine, temp: &Path, expect: &[(String, String)]) -> Result<(), String> {
    let size = std::fs::metadata(temp).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        return Err("The new file came out empty, so nothing was changed.".into());
    }
    let m = engine.read_one(temp, &[])?;
    if let Some(e) = m.error() {
        return Err(format!("The new file didn't check out ({e}), so nothing was changed."));
    }
    for (pat, want) in expect {
        match m.str(&[pat]) {
            Some(got) if matches_expected(&got, want) => {}
            got => {
                let tag = pat.rsplit(':').next().unwrap_or(pat);
                return Err(format!("{tag} didn't save as expected (wanted \"{want}\", found \"{}\"), so nothing was changed.", got.unwrap_or_default()));
            }
        }
    }
    Ok(())
}

/// Plain-language message for ExifTool write errors (spec 11).
pub fn human_engine_error(errors: &[String]) -> String {
    let e = errors.join("; ");
    let lower = e.to_lowercase();
    if lower.contains("not a valid") || lower.contains("corrupt") || lower.contains("format error") {
        format!("This file looks damaged or isn't really the type its name says, so it couldn't be updated. ({e})")
    } else if lower.contains("not yet supported") || lower.contains("can't currently write") || lower.contains("writing of") {
        format!("Metadata can't be written into this kind of file. ({e})")
    } else if lower.contains("permission") || lower.contains("error creating") {
        format!("Windows didn't allow a new file to be created here. ({e})")
    } else {
        format!("The metadata couldn't be written: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_values_compare_loosely() {
        assert!(matches_expected("2023:05:01 12:22:10", "2023:05:01 12:22:10"));
        assert!(matches_expected("2023:05:01 12:22:10+02:00", "2023:05:01 12:22:10"));
        assert!(matches_expected("12.500000", "12.5"));
        assert!(!matches_expected("Canon", "Nikon"));
    }
}
