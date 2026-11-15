//! First-launch extraction of the embedded ExifTool engine (T4) to
//! `<LocalAppData>\Metadata Copier and Editor\engine\<version>\`. A manifest records the pack
//! hash plus per-file SHA-256; files are verified on every launch and the engine is re-extracted
//! if anything is missing or changed. The pack format is described in `build.rs`.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const EXIFTOOL_VERSION: &str = env!("MCE_EXIFTOOL_VERSION");
const ENGINE_SHA256: &str = env!("MCE_ENGINE_SHA256");
static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/engine.bin"));

const MANIFEST: &str = ".mce-engine.manifest";
const PACK_MAGIC: &[u8] = b"MCEENG1\n";

/// Ensures the engine is extracted and intact. Returns the engine directory.
pub fn ensure_engine(base: &Path) -> Result<PathBuf, String> {
    if PACKED.is_empty() {
        return Err("This build has no embedded metadata engine for this platform.".into());
    }
    let dir = base.join("engine").join(EXIFTOOL_VERSION);
    if verify(&dir) {
        return Ok(dir);
    }
    extract(&dir)?;
    if !verify(&dir) {
        return Err("The metadata engine could not be unpacked correctly. Check free disk space and try again.".into());
    }
    Ok(dir)
}

/// True if the manifest matches this build's pack and every listed file has its recorded hash.
fn verify(dir: &Path) -> bool {
    let Ok(manifest) = std::fs::read_to_string(dir.join(MANIFEST)) else { return false };
    let mut lines = manifest.lines();
    if lines.next() != Some(ENGINE_SHA256) {
        return false;
    }
    lines.all(|line| line.split_once(' ').is_some_and(|(hash, rel)| std::fs::read(dir.join(rel)).is_ok_and(|data| hex(&Sha256::digest(&data)) == hash)))
}

/// Unpacks into `<version>.partial`, then renames it into place so a half-written engine is
/// never used.
fn extract(dir: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("Could not unpack the metadata engine: {e}");
    if dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
    let tmp = dir.with_file_name(format!("{EXIFTOOL_VERSION}.partial"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(err)?;

    let mut raw = Vec::new();
    flate2::read::DeflateDecoder::new(PACKED).read_to_end(&mut raw).map_err(err)?;
    let mut cur = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        if cur + n > raw.len() {
            return Err("The embedded metadata engine is damaged. Please download the app again.".into());
        }
        let s = &raw[cur..cur + n];
        cur += n;
        Ok(s)
    };
    if take(PACK_MAGIC.len())? != PACK_MAGIC {
        return Err("The embedded metadata engine has an unknown format.".into());
    }
    let count = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
    let mut manifest = String::from(ENGINE_SHA256);
    manifest.push('\n');
    for _ in 0..count {
        let plen = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
        let rel = String::from_utf8(take(plen)?.to_vec()).map_err(|_| "Bad engine file name".to_string())?;
        if rel.contains("..") || rel.starts_with('/') {
            return Err("Unsafe path in engine archive".into());
        }
        let size = u64::from_le_bytes(take(8)?.try_into().unwrap()) as usize;
        let data = take(size)?;
        let path = tmp.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::write(&path, data).map_err(err)?;
        manifest.push_str(&hex(&Sha256::digest(data)));
        manifest.push(' ');
        manifest.push_str(&rel);
        manifest.push('\n');
    }
    std::fs::write(tmp.join(MANIFEST), manifest).map_err(err)?;
    std::fs::rename(&tmp, dir).map_err(err)?;
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
