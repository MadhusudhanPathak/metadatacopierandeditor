//! End-to-end tests against the real bundled ExifTool (spec 13 "must-pass behaviours").
//! Fixtures come from ExifTool's own sample images: `npm run fetch-engine -- all` then
//! `npm run fixtures`. Run with `npm run test:e2e`. Tests skip (with a note) if fixtures
//! are missing.

use mce_lib::engine::{Engine, Launch, extract};
use mce_lib::metadata::cache::MetaCache;
use mce_lib::metadata::datetime::{FixedTz, Shift};
use mce_lib::metadata::tags::{TagMap, value_to_string};
use mce_lib::ops::executor::{self, Ctx, Progress};
use mce_lib::ops::items::{copy_items, edit_items};
use mce_lib::ops::journal::{ItemStatus, Journal, Op, OpKind, new_id};
use mce_lib::planning::PairState;
use mce_lib::planning::copy::{self, CopyReq, PairReq};
use mce_lib::planning::edit::{self, Changes, DateChange, EditReq, TextChange};
use mce_lib::platform::fsops::{self, FileTimes};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

const ALL: [&str; 9] = ["dates", "location", "camera", "exposure", "descriptive", "author", "makernotes", "container", "other"];

fn fixtures() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
    d.join("Canon.jpg").exists().then_some(d)
}

macro_rules! need_fixtures {
    () => {
        match fixtures() {
            Some(f) => f,
            None => {
                eprintln!("SKIPPED: fixtures missing (npm run fetch-engine -- all && npm run fixtures)");
                return;
            }
        }
    };
}

fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| {
        let base = std::env::temp_dir().join("mce-e2e-engine");
        let dir = extract::ensure_engine(&base).expect("engine extract");
        Engine::new(Launch::for_dir(&dir))
    })
}

fn dev() -> FixedTz {
    FixedTz(chrono::FixedOffset::east_opt(0).unwrap())
}

fn read(p: &Path) -> TagMap {
    engine().read_one(p, &[]).expect("read")
}

fn run_et(args: &[&str]) {
    let out = engine().run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
    assert!(!out.stderr.contains("Error"), "exiftool error: {}", out.stderr);
}

/// Copies a fixture with all metadata stripped (a "converted" file).
fn stripped(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    run_et(&["-all=", "-o", &dst.to_string_lossy(), &src.to_string_lossy()]);
}

fn put(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    std::fs::copy(src, dst).unwrap();
}

fn hash(p: &Path) -> String {
    fsops::sha256_file(p).unwrap()
}

/// The updated file must be a normal, visible file.
fn assert_visible(p: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        let attrs = std::fs::metadata(fsops::long_path(p)).unwrap().file_attributes();
        assert_eq!(attrs & HIDDEN, 0, "{} must not be hidden", p.display());
    }
}

/// No hidden app folders are left anywhere under `dir` (D15).
fn assert_no_hidden_folders(dir: &Path) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        assert!(!name.starts_with(".mce"), "unexpected {name} in {}", dir.display());
        if e.file_type().unwrap().is_dir() {
            assert_no_hidden_folders(&e.path());
        }
    }
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    journal: Journal,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let journal = Journal::new(&root.join("appdata"));
    World { _tmp: tmp, root, journal }
}

fn with_ctx<T>(w: &World, f: impl FnOnce(&Ctx) -> T) -> T {
    let cancel = AtomicBool::new(false);
    let progress = |_p: Progress| {};
    f(&Ctx { engine: engine(), journal: &w.journal, cancel: &cancel, stop_on_failure: false, progress: &progress })
}

fn ctx_run(w: &World, op: &mut Op) {
    with_ctx(w, |ctx| executor::run(ctx, op)).unwrap();
}

fn ctx_undo(w: &World, op: &mut Op) -> Result<(), String> {
    with_ctx(w, |ctx| executor::undo(ctx, op))
}

fn plan_copy(pairs: &[(&Path, &Path)], cats: &[&str]) -> copy::CopyPlan {
    let req = CopyReq {
        pairs: pairs
            .iter()
            .enumerate()
            .map(|(i, (s, c))| PairReq { id: format!("p{i}"), source: s.to_string_lossy().into(), converted: c.to_string_lossy().into(), final_name: None })
            .collect(),
        categories: cats.iter().map(|s| s.to_string()).collect(),
        tz_override: None,
    };
    copy::plan(engine(), &MetaCache::default(), &req, &dev(), new_id())
}

fn copy_op(plan: &copy::CopyPlan) -> Op {
    let id = new_id();
    let items = copy_items(plan, &id, &[]);
    Op::new(id, OpKind::Copy, "t".into(), items)
}

fn assert_ready(plan: &copy::CopyPlan) {
    for p in &plan.pairs {
        assert_eq!(p.state, PairState::Ready, "pair not ready: problems {:?} conflict {:?}", p.problems, p.conflict_with);
    }
}

#[test]
fn jpeg_copy_swap_and_undo() {
    let fx = need_fixtures!();
    let w = world();
    // Unicode + emoji + spaces in names (spec 13).
    let src = w.root.join("photos").join("Été 📷 IMG 001.jpg");
    let conv = w.root.join("photos").join("converted").join("Été 📷 IMG 001.jpg");
    put(&fx.join("Canon.jpg"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    let src_times = FileTimes { created_ms: Some(1_000_000_000_000), modified_ms: 1_100_000_000_000 };
    fsops::set_times(&src, &src_times).unwrap();
    let (h_src, h_conv) = (hash(&src), hash(&conv));
    let conv_times = fsops::read_times(&conv).unwrap();
    let before = read(&src);

    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    let p = &plan.pairs[0];
    assert!(p.final_path.ends_with("Été 📷 IMG 001.jpg"));
    assert!(p.source_renamed.ends_with("Été 📷 IMG 001_original.jpg"));

    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);

    // Swap (spec 5.4): converted takes the Source's place and name; Source kept as _original.
    let renamed = w.root.join("photos").join("Été 📷 IMG 001_original.jpg");
    assert!(!conv.exists());
    assert_eq!(hash(&renamed), h_src, "source content untouched");
    let after = read(&src);
    for t in ["EXIF:IFD0:Make", "EXIF:IFD0:Model", "EXIF:ExifIFD:DateTimeOriginal", "EXIF:ExifIFD:FNumber"] {
        assert_eq!(after.str(&[t]), before.str(&[t]), "{t} copied");
    }
    assert!(after.has_group0("MakerNotes"), "maker notes copied");
    // File-system dates follow the Source (spec 5.3).
    let ft = fsops::read_times(&src).unwrap();
    assert_eq!(ft.modified_ms, src_times.modified_ms);
    if fsops::CAN_SET_CREATED {
        assert_eq!(ft.created_ms, src_times.created_ms);
    }
    // D15: only `_original` + the updated file; nothing hidden, no hidden copies.
    assert_visible(&src);
    assert_no_hidden_folders(&w.root);

    // Undo: the original comes back byte-identical with its name and dates; the converted
    // file goes back to its own folder and name, keeping the copied metadata (D15).
    let mut op = w.journal.load(&op.id).unwrap();
    ctx_undo(&w, &mut op).unwrap();
    assert_eq!(hash(&src), h_src);
    assert!(conv.exists());
    assert_ne!(hash(&conv), h_conv, "converted keeps the copied metadata");
    assert_eq!(read(&conv).str(&["EXIF:IFD0:Make"]), before.str(&["EXIF:IFD0:Make"]));
    assert!(!renamed.exists());
    assert_eq!(fsops::read_times(&src).unwrap().modified_ms, src_times.modified_ms);
    assert_eq!(fsops::read_times(&conv).unwrap().modified_ms, conv_times.modified_ms);
    assert_no_hidden_folders(&w.root);
}

#[test]
fn existing_original_gets_numbered_suffix() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("IMG.jpg");
    let conv = w.root.join("c").join("IMG.jpg");
    put(&fx.join("Canon.jpg"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    std::fs::write(w.root.join("IMG_original.jpg"), b"someone else's file").unwrap();
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert!(plan.pairs[0].source_renamed.ends_with("IMG_original_2.jpg"));
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    assert_eq!(std::fs::read(w.root.join("IMG_original.jpg")).unwrap(), b"someone else's file");
    assert!(w.root.join("IMG_original_2.jpg").exists());
}

#[test]
fn heic_to_jpeg_keeps_converted_extension() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("IMG_7.HEIC");
    let conv = w.root.join("converted").join("IMG_7.jpg");
    put(&fx.join("QuickTime.heic"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    assert!(w.root.join("IMG_7.jpg").exists(), "Bs.Ec");
    assert!(w.root.join("IMG_7_original.HEIC").exists());
}

#[test]
fn photo_to_video_writes_utc_and_keys_offset() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("clip.jpg");
    let conv = w.root.join("c").join("clip.mov");
    put(&fx.join("Canon.jpg"), &src);
    run_et(&["-overwrite_original", "-DateTimeOriginal=2023:05:01 14:22:10", "-OffsetTimeOriginal=+02:00", &src.to_string_lossy()]);
    put(&fx.join("QuickTime.mov"), &conv);
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    let m = read(&w.root.join("clip.mov"));
    assert_eq!(m.str(&["QuickTime:*:CreateDate"]).as_deref(), Some("2023:05:01 12:22:10"));
    assert_eq!(m.str(&["QuickTime:Keys:CreationDate"]).as_deref(), Some("2023:05:01 14:22:10+02:00"));
}

#[test]
fn iphone_video_to_photo_uses_local_time_and_offset() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("IMG_9.MOV");
    let conv = w.root.join("c").join("IMG_9.jpg");
    put(&fx.join("QuickTime.mov"), &src);
    run_et(&["-overwrite_original", "-QuickTime:CreateDate=2023:05:01 12:22:10", "-Keys:CreationDate=2023:05:01 14:22:10+02:00", &src.to_string_lossy()]);
    stripped(&fx.join("Canon.jpg"), &conv);
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    let m = read(&w.root.join("IMG_9.jpg"));
    assert_eq!(m.str(&["EXIF:ExifIFD:DateTimeOriginal"]).as_deref(), Some("2023:05:01 14:22:10"));
    assert_eq!(m.str(&["EXIF:ExifIFD:OffsetTimeOriginal"]).as_deref(), Some("+02:00"));
}

#[test]
fn unticked_location_leaves_converted_gps_alone() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("a.jpg");
    let conv = w.root.join("c").join("a.jpg");
    put(&fx.join("GPS.jpg"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    run_et(&["-overwrite_original", "-GPSLatitude=10.5", "-GPSLatitudeRef=N", "-GPSLongitude=20.25", "-GPSLongitudeRef=E", &conv.to_string_lossy()]);
    let cats: Vec<&str> = ALL.iter().copied().filter(|c| *c != "location").collect();
    let plan = plan_copy(&[(&src, &conv)], &cats);
    assert_ready(&plan);
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    let m = read(&src);
    let lat = m.num(&["Composite:*:GPSLatitude"]).unwrap();
    assert!((lat - 10.5).abs() < 1e-6, "converted GPS kept, got {lat}");
}

#[test]
fn unwritable_and_readonly_are_blocked_with_reasons() {
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("song.mp3");
    let conv = w.root.join("c").join("song.mp3");
    put(&fx.join("MP3.mp3"), &src);
    put(&fx.join("MP3.mp3"), &conv);
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_eq!(plan.pairs[0].state, PairState::Blocked);
    assert!(plan.pairs[0].problems.iter().any(|p| p.code == "unwritable"));

    let src = w.root.join("ro.jpg");
    let conv = w.root.join("c").join("ro.jpg");
    put(&fx.join("Canon.jpg"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    let mut perm = std::fs::metadata(&conv).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&conv, perm.clone()).unwrap();
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert!(plan.pairs[0].problems.iter().any(|p| p.code == "readOnly"));
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&conv, perm).unwrap();
}

#[test]
fn very_long_paths_work() {
    let fx = need_fixtures!();
    let w = world();
    let mut dir = w.root.clone();
    for i in 0..6 {
        dir = dir.join(format!("a very long folder name number {i} with ünïcødé ✨"));
    }
    let src = dir.join("long name photo.jpg");
    let conv = dir.join("converted").join("long name photo.jpg");
    assert!(src.to_string_lossy().len() > 260, "path should exceed MAX_PATH");
    std::fs::create_dir_all(fsops::long_path(conv.parent().unwrap())).unwrap();
    std::fs::copy(fx.join("Canon.jpg"), fsops::long_path(&src)).unwrap();
    std::fs::copy(fx.join("Canon.jpg"), fsops::long_path(&conv)).unwrap();
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    assert!(fsops::long_path(&dir.join("long name photo_original.jpg")).exists());
}

fn edit_plan(paths: &[&Path], changes: Changes, set_dates: bool) -> edit::EditPlan {
    let req = EditReq { paths: paths.iter().map(|p| p.to_string_lossy().into()).collect(), changes, set_file_dates: set_dates };
    edit::plan(engine(), &MetaCache::default(), &req, &dev(), new_id())
}

fn edit_op(plan: &edit::EditPlan) -> Op {
    let id = new_id();
    let items = edit_items(plan, &id);
    Op::new(id, OpKind::Edit, "t".into(), items)
}

/// Every tag except the ones a write legitimately touches.
fn snapshot(m: &TagMap, ignore: &[&str]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = m
        .tags
        .iter()
        .filter(|t| !matches!(t.g0.as_str(), "File" | "System" | "ExifTool" | "Composite"))
        .filter(|t| !ignore.iter().any(|i| i.eq_ignore_ascii_case(&t.name)))
        .map(|t| (format!("{}:{}:{}", t.g0, t.g1, t.name), value_to_string(&t.value)))
        .collect();
    v.sort();
    v
}

#[test]
fn bulk_edit_writes_only_touched_fields_and_undoes() {
    let fx = need_fixtures!();
    let w = world();
    let a = w.root.join("a.jpg");
    let b = w.root.join("b.jpg");
    put(&fx.join("XMP.jpg"), &a);
    put(&fx.join("IPTC.jpg"), &b);
    let (ha, hb) = (hash(&a), hash(&b));
    // Fields touched + bookkeeping ExifTool maintains itself: IPTC record headers created
    // alongside the first IPTC tag, the IPTC digest, the XMP toolkit stamp, and the byte
    // offset of the embedded thumbnail (the file layout shifts when anything is rewritten).
    let ignore = [
        "Artist",
        "Creator",
        "By-line",
        "XPAuthor",
        "CodedCharacterSet",
        "EnvelopeRecordVersion",
        "ApplicationRecordVersion",
        "XMPToolkit",
        "CurrentIPTCDigest",
        "IPTCDigest",
        "ThumbnailOffset",
    ];
    // When a file has no EXIF block yet, writing Artist creates one and ExifTool adds the
    // four tags EXIF requires in IFD0. They describe the block, not the user's metadata.
    let required_ifd0 = ["XResolution", "YResolution", "ResolutionUnit", "YCbCrPositioning"];
    let (sa, sb) = (snapshot(&read(&a), &ignore), snapshot(&read(&b), &ignore));

    let ch = Changes { author: Some(TextChange::Set { value: "Ada Lövelace 👩‍💻".into() }), ..Default::default() };
    let plan = edit_plan(&[&a, &b], ch, false);
    assert_eq!(plan.ready, 2, "{:?}", plan.items.iter().map(|i| &i.problems).collect::<Vec<_>>());
    let mut op = edit_op(&plan);
    ctx_run(&w, &mut op);
    for it in &op.items {
        assert_eq!(it.status, ItemStatus::Done, "{:?}", it.message);
    }
    for (p, snap) in [(&a, &sa), (&b, &sb)] {
        let m = read(p);
        assert_eq!(m.str(&["XMP:XMP-dc:Creator", "EXIF:IFD0:Artist"]).as_deref(), Some("Ada Lövelace 👩‍💻"));
        let had_exif = snap.iter().any(|(k, _)| k.starts_with("EXIF:IFD0:"));
        let now: Vec<_> =
            snapshot(&m, &ignore).into_iter().filter(|(k, _)| had_exif || !required_ifd0.iter().any(|r| k == &format!("EXIF:IFD0:{r}"))).collect();
        let added: Vec<_> = now.iter().filter(|x| !snap.contains(x)).collect();
        let removed: Vec<_> = snap.iter().filter(|x| !now.contains(x)).collect();
        assert!(added.is_empty() && removed.is_empty(), "untouched tags changed in {}:\n  added {added:?}\n  removed {removed:?}", p.display());
    }
    // The untouched originals sit next to the updated files (D15).
    assert_eq!(hash(&w.root.join("a_original.jpg")), ha);
    assert_eq!(hash(&w.root.join("b_original.jpg")), hb);

    let mut op = w.journal.load(&op.id).unwrap();
    ctx_undo(&w, &mut op).unwrap();
    assert_eq!(hash(&a), ha);
    assert_eq!(hash(&b), hb);
    assert!(!w.root.join("a_original.jpg").exists());
    assert_no_hidden_folders(&w.root);
}

#[test]
fn shift_dates_and_set_file_dates() {
    let fx = need_fixtures!();
    let w = world();
    let a = w.root.join("a.jpg");
    put(&fx.join("Canon.jpg"), &a);
    run_et(&["-overwrite_original", "-DateTimeOriginal=2020:01:01 10:00:00", "-OffsetTimeOriginal=+00:00", &a.to_string_lossy()]);
    let ch = Changes { date_taken: Some(DateChange::Shift { shift: Shift { days: 1, hours: 2, minutes: 30 } }), ..Default::default() };
    let plan = edit_plan(&[&a], ch, true);
    let mut op = edit_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    let m = read(&a);
    assert_eq!(m.str(&["EXIF:ExifIFD:DateTimeOriginal"]).as_deref(), Some("2020:01:02 12:30:00"));
    let want = chrono::NaiveDateTime::parse_from_str("2020-01-02 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap().and_utc().timestamp_millis();
    assert_eq!(fsops::read_times(&a).unwrap().modified_ms, want);
    // Always: the untouched file as a_original.jpg next to the updated one, both visible.
    assert!(w.root.join("a_original.jpg").exists());
    assert_visible(&a);
    assert_no_hidden_folders(&w.root);
}

#[test]
fn set_title_on_png_webp_gif_pdf_and_mov() {
    let fx = need_fixtures!();
    let w = world();
    let files: Vec<PathBuf> = ["PNG.png", "RIFF.webp", "GIF.gif", "PDF.pdf", "QuickTime.mov", "DNG.dng"]
        .iter()
        .map(|f| {
            let p = w.root.join(f);
            put(&fx.join(f), &p);
            p
        })
        .collect();
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    let ch = Changes { title: Some(TextChange::Set { value: "Hello, world".into() }), ..Default::default() };
    let plan = edit_plan(&refs, ch, false);
    let mut op = edit_op(&plan);
    ctx_run(&w, &mut op);
    for it in &op.items {
        assert_eq!(it.status, ItemStatus::Done, "{}: {:?}", it.label, it.message);
    }
    for p in &files {
        let m = read(p);
        assert_eq!(m.str(&["XMP:XMP-dc:Title", "QuickTime:ItemList:Title", "PDF:*:Title"]).as_deref(), Some("Hello, world"), "{}", p.display());
    }
}

#[test]
fn tricky_values_survive_exactly() {
    let fx = need_fixtures!();
    let w = world();
    let a = w.root.join("$weird @name.jpg");
    put(&fx.join("Canon.jpg"), &a);
    let text = "Line 1\nLine 2 costs $5 @ C:\\temp\\x — ok 📷\ttab";
    let ch = Changes { description: Some(TextChange::Set { value: text.into() }), ..Default::default() };
    let plan = edit_plan(&[&a], ch, false);
    let mut op = edit_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    let m = read(&a);
    assert_eq!(m.str(&["XMP:XMP-dc:Description"]).as_deref(), Some(text));
    assert_eq!(m.str(&["EXIF:IFD0:ImageDescription"]).as_deref(), Some(text));
}

#[test]
fn converted_next_to_original_with_same_name_is_replaced_in_place() {
    // Source IMG.jpeg and Converted IMG.jpg in the same folder: the final name IMG.jpg is the
    // Converted file itself, so it is replaced atomically.
    let fx = need_fixtures!();
    let w = world();
    let src = w.root.join("IMG.jpeg");
    let conv = w.root.join("IMG.jpg");
    put(&fx.join("Canon.jpg"), &src);
    stripped(&fx.join("Canon.jpg"), &conv);
    let h_src = hash(&src);
    let plan = plan_copy(&[(&src, &conv)], &ALL);
    assert_ready(&plan);
    assert_eq!(Path::new(&plan.pairs[0].final_path), conv.as_path());
    let mut op = copy_op(&plan);
    ctx_run(&w, &mut op);
    assert_eq!(op.items[0].status, ItemStatus::Done, "{:?}", op.items[0].message);
    assert_eq!(read(&conv).str(&["EXIF:IFD0:Make"]).as_deref(), Some("Canon"));
    assert_eq!(hash(&w.root.join("IMG_original.jpeg")), h_src);
    assert!(!src.exists());
    let names: Vec<String> =
        std::fs::read_dir(&w.root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n != "appdata").collect();
    assert_eq!(names.len(), 2, "exactly the original and the updated file: {names:?}");

    let mut op = w.journal.load(&op.id).unwrap();
    ctx_undo(&w, &mut op).unwrap();
    assert_eq!(hash(&src), h_src);
    assert!(conv.exists());
    assert!(!w.root.join("IMG_original.jpeg").exists());
}

#[test]
fn offline_place_search_uses_the_bundled_city_database() {
    let base = std::env::temp_dir().join("mce-e2e-engine");
    let dir = extract::ensure_engine(&base).expect("engine extract");
    let db = mce_lib::geo::Db::load(&mce_lib::geo::db_path(&dir)).expect("load Geolocation.dat");
    assert!(db.len() > 100_000, "expected the full city list, got {}", db.len());
    let r = db.search("paris", 5);
    assert_eq!(r[0].country, "France");
    assert!((r[0].lat - 48.85).abs() < 0.1 && (r[0].lon - 2.35).abs() < 0.1, "{:?}", r[0]);
    let r = db.search("sao paulo, brazil", 3);
    assert_eq!(r[0].country_code, "BR");
    let r = db.search("Kingston, Canada", 3);
    assert!(r.iter().all(|p| p.country_code == "CA") && !r.is_empty());
}
