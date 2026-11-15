//! Per-pair Copy planning from metadata alone (D6, D7, T7): the clean-slate ExifTool
//! arguments, per-category counts, the predicted before/after diff, timezone and verification
//! expectations. Pure, so it is unit-tested without files.

use super::guards::{mismatch, orientation_decision};
use super::{DiffRow, Issue, diff_rows, issue};
use crate::engine::path_arg;
use crate::metadata::categories::{self, IDS};
use crate::metadata::datetime::{self, DeviceTz, Resolved, TzContext, TzSource, fmt_offset};
use crate::metadata::fields::{self, ALL_FIELDS, Field, Fields};
use crate::metadata::tags::{FormatClass, Kind, Tag, TagMap, ext_lower, format_class, kind_of};
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

/// What happens to one checklist category for a pair (or, summed, for the batch).
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatCount {
    pub id: &'static str,
    pub enabled: bool,
    pub copied: usize,
    pub not_storable: Vec<String>,
    /// Tags in the Converted file this category leaves untouched (unticked, D7).
    pub kept: usize,
}

/// The timezone used to convert dates, for the Preview.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TzView {
    pub source: TzSource,
    pub offset: String,
    pub zone: Option<String>,
}

/// Groups that never take part in copying or clearing.
const SKIP_GROUPS: &[&str] = &["File", "System", "Composite", "ExifTool"];
/// Groups kept intact when clearing the Converted file (structural blocks).
const KEEP_BLOCKS: &[&str] = &["ICC_Profile", "JFIF", "Adobe"];

fn skip_group(t: &Tag) -> bool {
    SKIP_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(&t.g0))
}

fn is_writable(t: &Tag, writable: &HashSet<String>) -> bool {
    writable.is_empty() || writable.contains(&t.name.to_ascii_lowercase())
}

/// Tag names a cross-format translation re-creates in the destination's own groups.
const TRANSLATED: &[&str] = &[
    "datetimeoriginal",
    "createdate",
    "modifydate",
    "offsettime",
    "offsettimeoriginal",
    "offsettimedigitized",
    "creationdate",
    "gpslatitude",
    "gpslongitude",
    "gpsaltitude",
    "gpslatituderef",
    "gpslongituderef",
    "gpsaltituderef",
    "gpscoordinates",
    "make",
    "model",
    "lensmodel",
    "title",
    "xptitle",
    "objectname",
    "imagedescription",
    "description",
    "caption-abstract",
    "artist",
    "creator",
    "by-line",
    "xpauthor",
    "copyright",
    "rights",
    "copyrightnotice",
    "keywords",
    "subject",
    "xpkeywords",
    "rating",
    "ratingpercent",
];

/// Everything the planner needs to know about one pair.
pub struct PairInputs<'a> {
    pub source: &'a Path,
    pub converted: &'a Path,
    pub src: &'a TagMap,
    pub conv: &'a TagMap,
    /// Ticked category ids.
    pub enabled: &'a HashSet<String>,
    /// Writable tag names (lower case); empty means "assume writable".
    pub writable: &'a HashSet<String>,
    pub tz_override: Option<chrono::FixedOffset>,
    pub device: &'a dyn DeviceTz,
}

/// Everything about one pair that depends only on metadata.
pub struct PairBuild {
    /// ExifTool arguments, before `-o TEMP CONVERTED`.
    pub args: Vec<String>,
    /// Tag → value checks run on the written temp file (T9).
    pub expect: Vec<(String, String)>,
    pub diff: Vec<DiffRow>,
    pub categories: Vec<CatCount>,
    /// Shown only when dates must be converted between photo and video conventions.
    pub tz: Option<TzView>,
    pub warnings: Vec<Issue>,
    pub problems: Vec<Issue>,
}

/// Facts derived once and shared by the planning steps.
struct Facts<'p, 'a> {
    i: &'p PairInputs<'a>,
    src_kind: Kind,
    dst_kind: Kind,
    dst_class: FormatClass,
    /// Photo ↔ video: dates change convention (local vs UTC).
    cross_kind: bool,
    /// Different format or kind: curated fields are also written into the destination's groups.
    translate: bool,
    resolved: Option<Resolved>,
    src_fields: Fields,
    orient_copy: bool,
}

impl Facts<'_, '_> {
    fn enabled(&self, category: &str) -> bool {
        self.i.enabled.contains(category)
    }
}

/// Plans one pair: arguments, expectations, diff, counts, timezone and warnings.
pub fn build(i: &PairInputs) -> PairBuild {
    let (src_kind, dst_kind) = (kind_of(i.source), kind_of(i.converted));
    let (src_class, dst_class) = (format_class(i.source), format_class(i.converted));
    let mut warnings = vec![];
    let mut problems = vec![];
    if dst_class == FormatClass::Unwritable {
        let ext = ext_lower(i.converted).to_uppercase();
        problems
            .push(issue("unwritable", format!("Metadata can't be written into .{ext} files. Skip this pair, or convert to a format like MP4, JPEG or PNG.")));
    }

    let resolved = fields::resolve_date(i.src, src_kind, &TzContext { override_offset: i.tz_override, device: i.device });
    let cross_kind = (src_kind == Kind::Video) != (dst_kind == Kind::Video);
    // The timezone only matters when the conversion has to change how dates are stored.
    let tz = if cross_kind && i.enabled.contains("dates") {
        resolved.as_ref().map(|r| TzView { source: r.source, offset: fmt_offset(r.offset), zone: r.zone.clone() })
    } else {
        None
    };
    if let Some(t) = tz.as_ref().filter(|t| t.source == TzSource::Assumed) {
        warnings.push(issue("tzAssumed", format!("Timezone assumed: {} (this computer's). Nothing in the file said where it was taken.", t.offset)));
    }

    let (orient_copy, orient_note) = orientation_decision(i.src, i.conv, if src_kind == Kind::Video { Kind::Video } else { Kind::Photo });
    if let Some(n) = orient_note {
        warnings.push(issue("orientation", n));
    }
    warnings.extend(mismatch(i.src, i.conv));

    let f = Facts {
        i,
        src_kind,
        dst_kind,
        dst_class,
        cross_kind,
        translate: src_class != dst_class || src_kind != dst_kind,
        src_fields: fields::extract(i.src, src_kind, i.device),
        resolved,
        orient_copy,
    };
    PairBuild { args: arguments(&f), expect: expectations(&f), diff: predicted_diff(&f), categories: category_counts(&f), tz, warnings, problems }
}

/// The clean-slate copy (D6, D7, T7) as one ExifTool command.
fn arguments(f: &Facts) -> Vec<String> {
    let i = f.i;
    // 1) Clear the Converted file's metadata, keeping structural blocks.
    let mut a: Vec<String> = vec!["-all=".into()];
    for g in KEEP_BLOCKS {
        a.push(format!("--{g}:all"));
    }
    if !f.enabled("makernotes") {
        a.push("--MakerNotes:all".into());
    }
    // 2) Put back from the Converted file itself: structural tags plus unticked categories.
    let mut keep = BTreeSet::new();
    for t in &i.conv.tags {
        if skip_group(t) || KEEP_BLOCKS.iter().any(|g| g.eq_ignore_ascii_case(&t.g0)) || t.g0.eq_ignore_ascii_case("MakerNotes") {
            continue;
        }
        if !is_writable(t, i.writable) {
            continue;
        }
        if categories::is_structural(t) || !f.enabled(categories::classify(t)) {
            keep.insert(t.spec());
        }
    }
    if !keep.is_empty() {
        a.push("-tagsFromFile".into());
        a.push("@".into());
        a.extend(keep.iter().map(|s| format!("-{s}")));
    }
    // 3) Copy everything from the Source to the same groups, minus structural and unticked.
    a.push("-tagsFromFile".into());
    a.push(path_arg(i.source));
    a.push("-all:all".into());
    // Computed (Composite) tags are never copied: some are writable (e.g. Composite
    // GPSLatitude writes EXIF GPS + Ref), which would bypass the category exclusions below.
    a.push("--Composite:all".into());
    let mut excl = BTreeSet::new();
    for t in &i.src.tags {
        if skip_group(t) || t.g0.eq_ignore_ascii_case("MakerNotes") {
            continue;
        }
        let rot = t.name.eq_ignore_ascii_case("Orientation") || t.name.eq_ignore_ascii_case("Rotation");
        if categories::is_structural(t) {
            if !(rot && f.orient_copy) {
                excl.insert(t.spec());
            }
        } else if !f.enabled(categories::classify(t)) {
            excl.insert(t.spec());
        }
    }
    if !f.enabled("makernotes") {
        a.push("--MakerNotes:all".into());
    }
    a.extend(excl.iter().map(|s| format!("--{s}")));
    if f.orient_copy {
        // Orientation is copied explicitly (it is excluded from wildcard copies above).
        a.push(if f.src_kind == Kind::Video { "-Rotation".into() } else { "-Orientation".into() });
    }
    // 4) Different formats: write the curated fields into the destination's own groups.
    if f.translate && f.dst_class != FormatClass::Unwritable {
        for field in ALL_FIELDS {
            if !f.enabled(field.category()) || !fields::supported(field, f.dst_class, f.dst_kind) {
                continue;
            }
            if field == Field::DateTaken {
                if let Some(r) = &f.resolved {
                    a.extend(date_translation(r, f.dst_class, f.cross_kind));
                }
                continue;
            }
            if let Some(v) = fields::as_new_value(&f.src_fields, field, f.resolved.as_ref()) {
                a.extend(fields::assignments(field, Some(&v), f.dst_class, i.conv));
            }
        }
    }
    a
}

/// Per-category counts: copied, can't be stored in the destination, kept (unticked).
fn category_counts(f: &Facts) -> Vec<CatCount> {
    let i = f.i;
    let mut cats: Vec<CatCount> = IDS.iter().map(|id| CatCount { id, enabled: f.enabled(id), ..Default::default() }).collect();
    let mut seen = HashSet::new();
    for t in &i.src.tags {
        if skip_group(t) || categories::is_structural(t) {
            continue;
        }
        if !t.g0.eq_ignore_ascii_case("MakerNotes") && !is_writable(t, i.writable) {
            continue;
        }
        let id = categories::classify(t);
        let Some(c) = cats.iter_mut().find(|c| c.id == id) else { continue };
        if !c.enabled {
            continue;
        }
        let lname = t.name.to_ascii_lowercase();
        if f.dst_class.supports(&t.g0) || (f.translate && TRANSLATED.contains(&lname.as_str())) {
            c.copied += 1;
        } else if seen.insert((id, lname)) {
            c.not_storable.push(t.name.clone());
        }
    }
    for t in &i.conv.tags {
        if skip_group(t) || categories::is_structural(t) || !is_writable(t, i.writable) {
            continue;
        }
        let id = categories::classify(t);
        if let Some(c) = cats.iter_mut().find(|c| c.id == id && !c.enabled) {
            c.kept += 1;
        }
    }
    cats
}

/// The Converted file's curated fields now versus after the copy.
fn predicted_diff(f: &Facts) -> Vec<DiffRow> {
    let conv_fields = fields::extract(f.i.conv, f.dst_kind, f.i.device);
    let mut predicted = conv_fields.clone();
    let mut notes: Vec<(Field, String)> = vec![];
    for field in ALL_FIELDS {
        if !f.enabled(field.category()) {
            continue;
        }
        if !fields::supported(field, f.dst_class, f.dst_kind) {
            fields::set_field(&mut predicted, field, None);
            if fields::as_new_value(&f.src_fields, field, f.resolved.as_ref()).is_some() {
                notes.push((field, format!("can't be stored in this {}", f.dst_class.label())));
            }
            continue;
        }
        if field == Field::DateTaken {
            // The Source's date in its resolved form, and only when the Source has a date.
            predicted.date_taken = f.src_fields.date_taken.as_ref().map(|d| f.resolved.as_ref().map(fields::date_value).unwrap_or_else(|| d.clone()));
        } else {
            fields::set_field(&mut predicted, field, fields::as_new_value(&f.src_fields, field, f.resolved.as_ref()).as_ref());
        }
    }
    diff_rows(&conv_fields, &predicted, &notes)
}

/// Checks the written temp file must pass before the swap (T9).
fn expectations(f: &Facts) -> Vec<(String, String)> {
    let i = f.i;
    let mut expect = vec![];
    if let Some(ft) = i.conv.file_type() {
        expect.push(("File:*:FileType".to_string(), ft));
    }
    if f.enabled("dates")
        && f.dst_class != FormatClass::Unwritable
        && let Some(r) = &f.resolved
    {
        if f.dst_class == FormatClass::QuickTime && f.src_kind != Kind::Video && f.src_kind != Kind::Audio {
            expect.push(("QuickTime:*:CreateDate".into(), datetime::fmt_exif(r.utc)));
        } else if f.dst_class.has_exif()
            && let Some(v) = i.src.str(&["EXIF:ExifIFD:DateTimeOriginal"])
        {
            expect.push(("EXIF:ExifIFD:DateTimeOriginal".into(), v));
        }
    }
    if f.enabled("camera")
        && fields::supported(Field::Make, f.dst_class, f.dst_kind)
        && let Some(m) = &f.src_fields.make
    {
        expect.push(("*:*:Make".into(), m.clone()));
    }
    expect
}

/// Date tags for a destination with a different format (spec 5.2 rules 2–4).
fn date_translation(r: &Resolved, dst: FormatClass, cross_kind: bool) -> Vec<String> {
    let mut out = vec![];
    match dst {
        FormatClass::QuickTime => {
            for (t, v) in datetime::video_date_assignments(r) {
                out.push(format!("-{t}={v}"));
            }
        }
        FormatClass::ExifFull | FormatClass::ExifXmp | FormatClass::XmpOnly => {
            // Same-kind copies keep values as they are: never invent an offset that was assumed.
            let offset_ok = cross_kind || r.source != TzSource::Assumed;
            let local = datetime::fmt_exif(r.local);
            if offset_ok {
                for (t, v) in datetime::photo_date_assignments(r, dst) {
                    out.push(format!("-{t}={v}"));
                }
            } else if dst.has_exif() {
                for t in ["ExifIFD:DateTimeOriginal", "ExifIFD:CreateDate", "IFD0:ModifyDate"] {
                    out.push(format!("-{t}={local}"));
                }
            } else {
                out.push(format!("-XMP-exif:DateTimeOriginal={local}"));
            }
        }
        FormatClass::Pdf => out.push(format!("-PDF:CreateDate={}", r.local_with_offset())),
        FormatClass::Unwritable => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::datetime::FixedTz;
    use chrono::FixedOffset;
    use serde_json::json;

    fn all() -> HashSet<String> {
        IDS.iter().map(|s| s.to_string()).collect()
    }

    fn built(src: serde_json::Value, conv: serde_json::Value, s: &str, c: &str, enabled: HashSet<String>) -> PairBuild {
        let (src, conv) = (TagMap::from_json(src), TagMap::from_json(conv));
        let dev = FixedTz(FixedOffset::east_opt(0).unwrap());
        let w = HashSet::new();
        build(&PairInputs {
            source: Path::new(s),
            converted: Path::new(c),
            src: &src,
            conv: &conv,
            enabled: &enabled,
            writable: &w,
            tz_override: None,
            device: &dev,
        })
    }

    #[test]
    fn clean_slate_copy_keeps_structure() {
        let b = built(
            json!({"EXIF:IFD0:Make":"Canon","EXIF:ExifIFD:ExifImageWidth":6000,"EXIF:ExifIFD:DateTimeOriginal":"2020:01:01 10:00:00"}),
            json!({"EXIF:ExifIFD:ExifImageWidth":1200,"EXIF:IFD0:Software":"Squoosh"}),
            "s/a.jpg",
            "c/a.jpg",
            all(),
        );
        assert_eq!(b.args[0], "-all=");
        assert!(b.args.contains(&"--ICC_Profile:all".to_string()));
        // Converted's own pixel size is restored from itself, Source's is excluded.
        let at = b.args.iter().position(|x| x == "@").unwrap();
        assert!(b.args[at..].contains(&"-ExifIFD:ExifImageWidth".to_string()));
        assert!(b.args.contains(&"--ExifIFD:ExifImageWidth".to_string()));
        // Converter's Software tag is not put back (Source is ground truth).
        assert!(!b.args.contains(&"-IFD0:Software".to_string()));
        assert!(b.args.contains(&"-all:all".to_string()));
        // Writable Composite tags (e.g. GPSLatitude) must never bypass category exclusions.
        assert!(b.args.contains(&"--Composite:all".to_string()));
    }

    #[test]
    fn unticked_category_is_left_untouched() {
        let mut en = all();
        en.remove("location");
        let b = built(json!({"EXIF:GPS:GPSLatitude":1.0,"EXIF:IFD0:Make":"Canon"}), json!({"EXIF:GPS:GPSLatitude":2.0}), "s/a.jpg", "c/a.jpg", en);
        assert!(b.args.contains(&"-GPS:GPSLatitude".to_string()), "converted GPS is put back");
        assert!(b.args.contains(&"--GPS:GPSLatitude".to_string()), "source GPS is not copied");
        let loc = b.categories.iter().find(|c| c.id == "location").unwrap();
        assert!(!loc.enabled);
        assert_eq!(loc.kept, 1);
    }

    #[test]
    fn photo_to_video_writes_utc_dates() {
        let b = built(
            json!({"EXIF:ExifIFD:DateTimeOriginal":"2023:05:01 14:22:10","EXIF:ExifIFD:OffsetTimeOriginal":"+02:00","EXIF:IFD0:Make":"Apple"}),
            json!({"QuickTime:QuickTime:CreateDate":"2024:01:01 00:00:00"}),
            "s/IMG.HEIC",
            "c/IMG.mp4",
            all(),
        );
        assert!(b.args.contains(&"-QuickTime:CreateDate=2023:05:01 12:22:10".to_string()));
        assert!(b.args.contains(&"-Keys:CreationDate=2023:05:01 14:22:10+02:00".to_string()));
        assert!(b.args.contains(&"-Keys:Make=Apple".to_string()));
        assert_eq!(b.tz.as_ref().unwrap().source, TzSource::Explicit);
        let dates = b.categories.iter().find(|c| c.id == "dates").unwrap();
        assert!(dates.not_storable.is_empty(), "translated dates are not reported as lost");
    }

    #[test]
    fn heic_to_png_counts_unstorable_iptc() {
        let b = built(json!({"IPTC:IPTC:Keywords":["x"],"EXIF:IFD0:Make":"Apple","IPTC:IPTC:Urgency":5}), json!({}), "s/a.jpg", "c/a.png", all());
        let d = b.categories.iter().find(|c| c.id == "descriptive").unwrap();
        assert!(d.not_storable.contains(&"Urgency".to_string()));
        assert!(!d.not_storable.contains(&"Keywords".to_string()), "keywords are translated to XMP");
    }

    #[test]
    fn orientation_copy_adds_explicit_tag_and_no_exclusion() {
        let b = built(
            json!({"EXIF:IFD0:Orientation":6,"File:File:ImageWidth":4000,"File:File:ImageHeight":3000}),
            json!({"File:File:ImageWidth":2000,"File:File:ImageHeight":1500}),
            "s/a.jpg",
            "c/a.jpg",
            all(),
        );
        assert!(b.args.contains(&"-Orientation".to_string()));
        assert!(!b.args.contains(&"--IFD0:Orientation".to_string()));
    }

    #[test]
    fn mismatch_warns_but_never_blocks() {
        let b = built(
            json!({"File:File:ImageWidth":4000,"File:File:ImageHeight":3000}),
            json!({"File:File:ImageWidth":1000,"File:File:ImageHeight":1000}),
            "s/a.jpg",
            "c/a.jpg",
            all(),
        );
        assert!(b.warnings.iter().any(|w| w.code == "mismatch"));
        assert!(b.problems.is_empty());
    }
}
