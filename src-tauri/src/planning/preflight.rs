//! Pre-flight checks shared by the Copy and Edit plans (spec 5.5).

use super::{Issue, issue};
use crate::platform::fsops;
use std::path::Path;

/// Extra room needed beyond the file's own size: 10% plus 16 MiB.
const SPACE_MARGIN_FIXED: u64 = 16 * 1024 * 1024;

/// Bytes needed on a volume to write a new copy of a file of `size` bytes.
pub fn space_needed(size: u64) -> u64 {
    size + size / 10 + SPACE_MARGIN_FIXED
}

/// Read-only (T14) or locked by another program, for an existing file.
pub fn file_access_problem(p: &Path) -> Option<Issue> {
    let name = fsops::file_name(p);
    if fsops::is_readonly(p) {
        Some(issue("readOnly", format!("{name} is read-only. Untick \"Read-only\" in its Properties, then try again.")))
    } else if fsops::is_locked(p) {
        Some(issue("locked", format!("{name} is open in another program. Close it, then try again.")))
    } else {
        None
    }
}

/// The folder doesn't accept new files.
pub fn folder_problem(dir: &Path) -> Option<Issue> {
    (!fsops::dir_writable(dir)).then(|| issue("folderLocked", format!("The folder {} doesn't allow changes.", dir.display())))
}
