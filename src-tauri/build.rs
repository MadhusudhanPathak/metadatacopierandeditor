//! Build script: runs tauri-build, then packs the pinned ExifTool engine into OUT_DIR so the
//! app can embed it (spec 3.2: single portable exe, extracted on first launch).
//!
//! Pack format (`engine.bin`) is one deflate stream containing:
//!   b"MCEENG1\n", u32 file count, then per file: u32 path len, path (UTF-8, '/'), u64 size, bytes.
//! Its SHA-256 is exported as MCE_ENGINE_SHA256 and the ExifTool version as MCE_EXIFTOOL_VERSION.

use std::io::{Read, Write};
use std::path::PathBuf;

fn main() {
    tauri_build::build();
    pack_engine();
}

fn pack_engine() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().unwrap().to_path_buf();
    let lock_path = root.join("engine").join("exiftool.lock.json");
    println!("cargo:rerun-if-changed={}", lock_path.display());

    let lock: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&lock_path).expect("engine/exiftool.lock.json missing"))
        .expect("engine/exiftool.lock.json is not valid JSON");
    let version = lock["version"].as_str().unwrap().to_string();
    println!("cargo:rustc-env=MCE_EXIFTOOL_VERSION={version}");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let out = out_dir.join("engine.bin");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "windows" {
        // Android/other targets ship the engine differently (spec 3.2, Android); embed nothing.
        std::fs::write(&out, b"").unwrap();
        println!("cargo:rustc-env=MCE_ENGINE_SHA256=none");
        return;
    }

    let win = &lock["windows"];
    let zip_path = root.join("engine").join("cache").join(win["file"].as_str().unwrap());
    println!("cargo:rerun-if-changed={}", zip_path.display());
    let zip_bytes = std::fs::read(&zip_path).unwrap_or_else(|_| {
        panic!("\n\nExifTool engine not found at {}.\nRun `npm run fetch-engine` first (it downloads and verifies the pinned release).\n\n", zip_path.display())
    });
    let expected = win["sha256"].as_str().unwrap();
    let got = hex(&sha256(&zip_bytes));
    assert_eq!(got, expected, "ExifTool archive hash mismatch; delete engine/cache and fetch again");

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("bad ExifTool zip");
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        if f.is_dir() {
            continue;
        }
        let name = f.name().replace('\\', "/");
        // Keep only exiftool_files/ (Perl + ExifTool). The "(-k).exe" launcher is not needed:
        // the app runs perl.exe exiftool.pl directly.
        let Some(idx) = name.find("exiftool_files/") else { continue };
        let rel = name[idx + "exiftool_files/".len()..].to_string();
        if rel.is_empty() || !keep(&rel) {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).unwrap();
        files.push((rel, buf));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(files.iter().any(|(p, _)| p == "perl.exe"), "perl.exe missing from engine");
    assert!(files.iter().any(|(p, _)| p == "exiftool.pl"), "exiftool.pl missing from engine");

    let mut raw = Vec::new();
    raw.extend_from_slice(b"MCEENG1\n");
    raw.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (p, data) in &files {
        raw.extend_from_slice(&(p.len() as u32).to_le_bytes());
        raw.extend_from_slice(p.as_bytes());
        raw.extend_from_slice(&(data.len() as u64).to_le_bytes());
        raw.extend_from_slice(data);
    }
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(&raw).unwrap();
    let packed = enc.finish().unwrap();
    std::fs::write(&out, &packed).unwrap();
    println!("cargo:rustc-env=MCE_ENGINE_SHA256={}", hex(&sha256(&packed)));
    println!(
        "cargo:warning=Embedded ExifTool {version}: {} files, {:.1} MB packed ({:.1} MB unpacked)",
        files.len(),
        packed.len() as f64 / 1048576.0,
        raw.len() as f64 / 1048576.0
    );
}

/// What to strip (spec 3.2: only what is safely unnecessary; keep all format modules).
fn keep(rel: &str) -> bool {
    let drop_prefixes = [
        "lib/Image/ExifTool/Lang/", // UI translations (we never pass -lang)
        "lib/TAP/",                 // Perl test harness
        "lib/Test/",
        "lib/Test2/",
    ];
    if drop_prefixes.iter().any(|p| rel.starts_with(p)) {
        return false;
    }
    if rel.ends_with(".pod") {
        return false; // documentation only
    }
    !matches!(rel, "windows_exiftool.txt") // HTML-ish manual copy
}

fn sha256(data: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    sha2::Sha256::digest(data).to_vec()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
