//! File-system helpers (spec 5.3, 5.4, 5.5, 6.1): file times, never-overwriting moves, path
//! comparison, free space, lock/read-only checks and hashing.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// File-system dates, in milliseconds since the Unix epoch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileTimes {
    pub created_ms: Option<i64>,
    pub modified_ms: i64,
}

/// Size + modified time, used to detect files changed since an operation (spec 5.5 Undo).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub size: u64,
    pub modified_ms: i64,
}

fn to_ms(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

fn from_ms(ms: i64) -> SystemTime {
    if ms >= 0 { UNIX_EPOCH + Duration::from_millis(ms as u64) } else { UNIX_EPOCH - Duration::from_millis((-ms) as u64) }
}

/// Created (if the platform reports it) and Modified dates.
pub fn read_times(p: &Path) -> std::io::Result<FileTimes> {
    let m = std::fs::metadata(p)?;
    Ok(FileTimes { created_ms: m.created().ok().map(to_ms), modified_ms: to_ms(m.modified()?) })
}

/// Size and Modified date.
pub fn stamp(p: &Path) -> std::io::Result<Stamp> {
    let m = std::fs::metadata(p)?;
    Ok(Stamp { size: m.len(), modified_ms: to_ms(m.modified()?) })
}

/// Sets Created (Windows only; Android cannot set birth time, spec 5.3) and Modified.
pub fn set_times(p: &Path, t: &FileTimes) -> std::io::Result<()> {
    let f = std::fs::OpenOptions::new().write(true).open(p)?;
    #[allow(unused_mut)]
    let mut ft = std::fs::FileTimes::new().set_modified(from_ms(t.modified_ms)).set_accessed(SystemTime::now());
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTimesExt;
        if let Some(c) = t.created_ms {
            ft = ft.set_created(from_ms(c));
        }
    }
    f.set_times(ft)
}

/// True when the platform lets us set the Created date.
pub const CAN_SET_CREATED: bool = cfg!(windows);

/// Hex SHA-256 of a file, streamed in 1 MiB chunks.
pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut f = File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// NUL-terminated UTF-16 extended-length path for Win32 calls.
#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    long_path(p).as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Win32 error from the `windows` crate as an `io::Error` (keeps the raw OS error code).
#[cfg(windows)]
fn win32_io_error(e: windows::core::Error) -> std::io::Error {
    std::io::Error::from_raw_os_error(e.code().0 & 0xFFFF)
}

/// Moves a file without ever overwriting the destination. Same volume: atomic rename.
/// Different volume: copy, verify size and SHA-256, then delete the original (spec 5.4 rule 4).
pub fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        return Err(format!("{} already exists, so nothing was overwritten.", to.display()));
    }
    match rename_no_replace(from, to) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => copy_verify_delete(from, to),
        Err(e) => Err(describe_io(&e, from)),
    }
}

#[cfg(windows)]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::PCWSTR;
    let (a, b) = (wide(from), wide(to));
    // No MOVEFILE_REPLACE_EXISTING and no MOVEFILE_COPY_ALLOWED: fails if the target exists
    // or lives on another volume (the caller then copies and verifies).
    unsafe { MoveFileExW(PCWSTR(a.as_ptr()), PCWSTR(b.as_ptr()), MOVE_FILE_FLAGS(0)) }.map_err(win32_io_error)
}

#[cfg(not(windows))]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "target exists"));
    }
    std::fs::rename(from, to)
}

/// Atomically replaces `to` with `from` (same folder). Used only when the Converted file
/// already has the final name, so the updated copy takes its place in one step.
#[cfg(windows)]
pub fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    use windows::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
    use windows::core::PCWSTR;
    let (a, b) = (wide(from), wide(to));
    unsafe { MoveFileExW(PCWSTR(a.as_ptr()), PCWSTR(b.as_ptr()), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) }
        .map_err(|e| describe_io(&win32_io_error(e), to))
}

#[cfg(not(windows))]
pub fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|e| describe_io(&e, to))
}

/// ERROR_NOT_SAME_DEVICE on Windows, EXDEV elsewhere.
fn is_cross_device(e: &std::io::Error) -> bool {
    const CROSS_DEVICE: i32 = if cfg!(windows) { 17 } else { 18 };
    e.raw_os_error() == Some(CROSS_DEVICE)
}

fn copy_verify_delete(from: &Path, to: &Path) -> Result<(), String> {
    let times = read_times(from).ok();
    std::fs::copy(from, to).map_err(|e| describe_io(&e, to))?;
    let ok = (|| -> std::io::Result<bool> { Ok(std::fs::metadata(from)?.len() == std::fs::metadata(to)?.len() && sha256_file(from)? == sha256_file(to)?) })()
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(to);
        return Err("The copy to the other drive didn't match the original, so the original was kept.".into());
    }
    if let Some(t) = times {
        let _ = set_times(to, &t);
    }
    std::fs::remove_file(from).map_err(|e| describe_io(&e, from))
}

/// Extended-length path on Windows so paths over 260 characters work (spec 3.2).
#[cfg(windows)]
pub fn long_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s.starts_with(r"\\?\") || !p.is_absolute() {
        return p.to_path_buf();
    }
    let s = s.replace('/', "\\");
    if let Some(unc) = s.strip_prefix(r"\\") { PathBuf::from(format!(r"\\?\UNC\{unc}")) } else { PathBuf::from(format!(r"\\?\{s}")) }
}

#[cfg(not(windows))]
pub fn long_path(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// `<dir>/<base>_original.<ext>`, then `_original_2`, `_3`, ... — the first name for which
/// `taken` is false (spec 5.4). `taken` lets callers also reserve names planned in the same batch.
pub fn unique_original_name(dir: &Path, base: &str, ext: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let dot = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    let first = dir.join(format!("{base}_original{dot}"));
    if !taken(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let p = dir.join(format!("{base}_original_{n}{dot}"));
        if !taken(&p) {
            return p;
        }
        n += 1;
    }
}

/// Free bytes on the volume holding `dir`.
#[cfg(windows)]
pub fn free_space(dir: &Path) -> Option<u64> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;
    let w = wide(dir);
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

#[cfg(not(windows))]
pub fn free_space(_dir: &Path) -> Option<u64> {
    None
}

/// True if another program holds the file open without sharing (Windows sharing or lock violation).
#[cfg(windows)]
pub fn is_locked(p: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(p) {
        Ok(_) => false,
        Err(e) => matches!(e.raw_os_error(), Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)),
    }
}

#[cfg(not(windows))]
pub fn is_locked(_p: &Path) -> bool {
    false
}

/// True if the file has the read-only attribute.
pub fn is_readonly(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.permissions().readonly()).unwrap_or(false)
}

/// True if both paths resolve to the same existing file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// True if a directory accepts new files (creates and removes a probe file).
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".mce-probe-{}", std::process::id()));
    match File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// Win32 error codes used to describe I/O failures.
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const ERROR_HANDLE_DISK_FULL: i32 = 39;
const ERROR_FILE_EXISTS: i32 = 80;
const ERROR_DISK_FULL: i32 = 112;
const ERROR_ALREADY_EXISTS: i32 = 183;

/// Plain-language description of an I/O error (spec 11).
pub fn describe_io(e: &std::io::Error, p: &Path) -> String {
    use std::io::ErrorKind;
    let name = file_name(p);
    match (e.kind(), e.raw_os_error()) {
        (ErrorKind::NotFound, _) => format!("{name} couldn't be found. It may have been moved or deleted."),
        (ErrorKind::PermissionDenied, _) | (_, Some(ERROR_ACCESS_DENIED)) => {
            format!("Windows didn't allow changes to {name}. It may be read-only or protected.")
        }
        (_, Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)) => format!("{name} is open in another program. Close it and try again."),
        (_, Some(ERROR_DISK_FULL | ERROR_HANDLE_DISK_FULL)) | (ErrorKind::StorageFull, _) => {
            format!("There isn't enough free space to finish {name}.")
        }
        (ErrorKind::AlreadyExists, _) | (_, Some(ERROR_FILE_EXISTS | ERROR_ALREADY_EXISTS)) => {
            format!("A file named {name} already exists, so nothing was overwritten.")
        }
        _ => format!("{name}: {e}"),
    }
}

/// The file name for messages, or the whole path when it has none.
pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// Comparison key for paths: `/` separators, no trailing slash, case-insensitive on Windows.
pub fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) { s.to_lowercase() } else { s.to_string() }
}

/// True if two path strings name the same location (without touching the disk).
pub fn same_path(a: &str, b: &str) -> bool {
    path_key(Path::new(a)) == path_key(Path::new(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_names_never_collide() {
        let dir = Path::new("D");
        let taken = [PathBuf::from("D/IMG_original.jpg"), PathBuf::from("D/IMG_original_2.jpg")];
        let p = unique_original_name(dir, "IMG", "jpg", |p| taken.contains(&p.to_path_buf()));
        assert_eq!(p, PathBuf::from("D/IMG_original_3.jpg"));
        let p = unique_original_name(dir, "clip", "mov", |_| false);
        assert_eq!(p, PathBuf::from("D/clip_original.mov"));
    }

    #[test]
    fn move_refuses_to_overwrite_and_preserves_content() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.bin");
        let b = d.path().join("b.bin");
        std::fs::write(&a, b"one").unwrap();
        std::fs::write(&b, b"two").unwrap();
        assert!(move_file(&a, &b).is_err());
        assert_eq!(std::fs::read(&b).unwrap(), b"two");
        let c = d.path().join("c.bin");
        move_file(&a, &c).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&c).unwrap(), b"one");
    }

    #[test]
    fn times_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.bin");
        std::fs::write(&a, b"x").unwrap();
        let want = FileTimes { created_ms: Some(1_500_000_000_000), modified_ms: 1_600_000_000_000 };
        set_times(&a, &want).unwrap();
        let got = read_times(&a).unwrap();
        assert_eq!(got.modified_ms, want.modified_ms);
        if CAN_SET_CREATED {
            assert_eq!(got.created_ms, want.created_ms);
        }
    }
}
