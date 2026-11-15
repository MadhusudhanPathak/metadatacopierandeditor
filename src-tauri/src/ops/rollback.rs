//! Reversing items on disk (spec 5.5): used after a failed step, for crash recovery and for
//! Undo. The user's original (renamed `_original`) is always put back unchanged and never
//! deleted.

use super::journal::Item;
use crate::platform::fsops::{self, Stamp, same_path};
use std::path::{Path, PathBuf};

/// Reverses an item from whatever step it reached (failure, crash recovery or Undo).
/// Checks what is actually on disk, not just the logged step, so a crash between a move and
/// its log entry is still reversible. The user's original (`source`, renamed `_original`) is
/// always put back unchanged. The Converted file comes back to its own place and name; once
/// step 5 has removed its pre-change copy (D15), it comes back *with* the copied metadata.
pub fn rollback_item(it: &Item) -> Result<(), String> {
    let p = |s: &str| PathBuf::from(s);
    let tmp = p(&it.temp);
    let fin = p(&it.final_path);
    let orig = p(&it.original);
    // Our final file is there if step 3 was logged, or if a crash hit between the move and
    // its log entry (the temp is gone and the final file has exactly the temp's size/mtime).
    let final_is_ours = fin.exists() && (it.step >= 3 || (!tmp.exists() && it.temp_stamp.is_some() && fsops::stamp(&fin).ok() == it.temp_stamp));
    if final_is_ours {
        let edit = same_path(&it.source, &it.original);
        if same_path(&it.final_path, &it.original) && !edit {
            // Copy where the Converted file already had the final name (same folder, same
            // base name): it was replaced in place, so it simply stays where it is.
            let _ = fsops::set_times(&fin, &it.original_times);
        } else if edit || orig.exists() {
            // Edit (the `_original` is put back below) or a Converted file that still exists:
            // the updated file is ours and redundant.
            std::fs::remove_file(fsops::long_path(&fin)).map_err(|e| fsops::describe_io(&e, &fin))?;
        } else {
            // The pre-change Converted copy was already removed: move the updated file back
            // to the Converted file's place and name.
            fsops::move_file(&fin, &orig)?;
            let _ = fsops::set_times(&orig, &it.original_times);
        }
    }
    if tmp.exists() {
        let _ = std::fs::remove_file(fsops::long_path(&tmp));
    }
    let (src, ren) = (p(&it.source), p(&it.source_renamed));
    if ren.exists() && !src.exists() {
        fsops::move_file(&ren, &src)?;
    }
    if it.step >= 2 {
        let _ = fsops::set_times(&src, &it.source_times);
    }
    Ok(())
}

/// Checks that Undo can put everything back (spec 5.5: blocked if files changed/removed).
pub fn undo_blocker(it: &Item) -> Option<String> {
    let name = |s: &str| Path::new(s).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let changed = |path: &str, want: Option<Stamp>| -> bool {
        match (fsops::stamp(Path::new(path)), want) {
            (Ok(now), Some(w)) => now.size != w.size || now.modified_ms != w.modified_ms,
            (Err(_), _) => true,
            (Ok(_), None) => false,
        }
    };
    if !Path::new(&it.final_path).exists() {
        return Some(format!("{} is no longer there, so it can't be put back.", name(&it.final_path)));
    }
    if changed(&it.final_path, it.final_stamp) {
        return Some(format!("{} was changed after this operation, so Undo could lose those changes.", name(&it.final_path)));
    }
    if !Path::new(&it.source_renamed).exists() {
        return Some(format!("{} is no longer there.", name(&it.source_renamed)));
    }
    if changed(&it.source_renamed, it.source_renamed_stamp) {
        return Some(format!("{} was changed after this operation.", name(&it.source_renamed)));
    }
    let edit = same_path(&it.source, &it.original);
    if !edit && !same_path(&it.original, &it.final_path) && Path::new(&it.original).exists() {
        return Some(format!("A new file named {} is in the way.", name(&it.original)));
    }
    if !same_path(&it.source, &it.final_path) && Path::new(&it.source).exists() {
        return Some(format!("A new file named {} is in the way.", name(&it.source)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::journal::tests::item;
    use crate::platform::fsops::FileTimes;

    /// Simulates the on-disk state after `step` for a Copy item, then rolls back.
    #[test]
    fn copy_rollback_from_every_step() {
        for step in 1..=5u8 {
            let d = tempfile::tempdir().unwrap();
            let dir = d.path();
            std::fs::create_dir_all(dir.join("conv")).unwrap();
            let mut it = item(dir);
            it.source_times = FileTimes { created_ms: None, modified_ms: 1_500_000_000_000 };
            std::fs::write(&it.temp, b"converted+meta").unwrap();
            std::fs::write(&it.source, b"source").unwrap();
            std::fs::write(&it.original, b"converted").unwrap();
            if step >= 2 {
                std::fs::rename(&it.source, &it.source_renamed).unwrap();
            }
            if step >= 3 {
                std::fs::rename(&it.temp, &it.final_path).unwrap();
            }
            if step >= 5 {
                std::fs::remove_file(&it.original).unwrap();
            }
            it.step = step;
            rollback_item(&it).unwrap();
            assert_eq!(std::fs::read(&it.source).unwrap(), b"source", "step {step}");
            let want: &[u8] = if step >= 5 { b"converted+meta" } else { b"converted" };
            assert_eq!(std::fs::read(&it.original).unwrap(), want, "step {step}");
            assert!(!Path::new(&it.temp).exists());
            assert!(!Path::new(&it.source_renamed).exists());
        }
    }

    #[test]
    fn edit_rollback_from_every_step() {
        for step in 1..=4u8 {
            let d = tempfile::tempdir().unwrap();
            let dir = d.path();
            let f = dir.join("a.jpg").to_string_lossy().into_owned();
            let mut it = item(dir);
            it.source = f.clone();
            it.original = f.clone();
            it.final_path = f.clone();
            it.source_renamed = dir.join("a_original.jpg").to_string_lossy().into_owned();
            it.temp = dir.join(".a.mce-tmp.jpg").to_string_lossy().into_owned();
            std::fs::write(&f, b"before").unwrap();
            std::fs::write(&it.temp, b"after").unwrap();
            if step >= 2 {
                std::fs::rename(&f, &it.source_renamed).unwrap();
            }
            if step >= 3 {
                std::fs::rename(&it.temp, &f).unwrap();
            }
            it.step = step;
            rollback_item(&it).unwrap();
            assert_eq!(std::fs::read(&f).unwrap(), b"before", "step {step}");
            assert!(!Path::new(&it.source_renamed).exists());
            assert!(!Path::new(&it.temp).exists());
        }
    }
}
