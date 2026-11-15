//! Edit planner (spec 4.3, 7, D10, D11, D15): reads curated fields and plans single or bulk
//! edits that write only the fields the user touched. The edited file is always kept as
//! `name_original` next to the updated one.

use super::preflight::{file_access_problem, folder_problem, space_needed};
use super::{DiffRow, Issue, PairState, diff_rows, issue};
use crate::engine::Engine;
use crate::metadata::cache::MetaCache;
use crate::metadata::datetime::{DeviceTz, Resolved, Shift, TzContext, TzSource, fmt_exif, parse_exif_dt, parse_offset};
use crate::metadata::fields::{self, Field, Fields, Location, NewValue};
use crate::metadata::tags::{FormatClass, Kind, TagMap, ext_lower, format_class, kind_of, value_to_string};
use crate::platform::fsops::{self, FileTimes, Stamp, file_name, path_key};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DateChange {
    /// `local` is `YYYY-MM-DDTHH:MM[:SS]`, `offset` is `+HH:MM`.
    Set {
        local: String,
        offset: String,
    },
    Shift {
        shift: Shift,
    },
    Clear,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum LocChange {
    Set { lat: f64, lon: f64, alt: Option<f64> },
    Clear,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum TextChange {
    Set { value: String },
    Clear,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ListChange {
    Set { values: Vec<String> },
    Clear,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum RatingChange {
    Set { value: u8 },
    Clear,
}

/// Only fields present here were touched by the user; nothing else is written.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Changes {
    pub date_taken: Option<DateChange>,
    pub location: Option<LocChange>,
    pub make: Option<TextChange>,
    pub model: Option<TextChange>,
    pub lens: Option<TextChange>,
    pub title: Option<TextChange>,
    pub description: Option<TextChange>,
    pub keywords: Option<ListChange>,
    pub rating: Option<RatingChange>,
    pub author: Option<TextChange>,
    pub copyright: Option<TextChange>,
}

impl Changes {
    /// The touched fields, in display order.
    pub fn touched(&self) -> Vec<Field> {
        fields::ALL_FIELDS.iter().copied().filter(|f| self.is_touched(*f)).collect()
    }

    fn is_touched(&self, f: Field) -> bool {
        match f {
            Field::DateTaken => self.date_taken.is_some(),
            Field::Location => self.location.is_some(),
            Field::Keywords => self.keywords.is_some(),
            Field::Rating => self.rating.is_some(),
            _ => self.text(f).is_some(),
        }
    }

    fn text(&self, f: Field) -> Option<&TextChange> {
        match f {
            Field::Make => self.make.as_ref(),
            Field::Model => self.model.as_ref(),
            Field::Lens => self.lens.as_ref(),
            Field::Title => self.title.as_ref(),
            Field::Description => self.description.as_ref(),
            Field::Author => self.author.as_ref(),
            Field::Copyright => self.copyright.as_ref(),
            Field::DateTaken | Field::Location | Field::Keywords | Field::Rating => None,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EditReq {
    pub paths: Vec<String>,
    pub changes: Changes,
    pub set_file_dates: bool,
}

/// One file as shown in the editor.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileFields {
    pub path: String,
    pub name: String,
    pub kind: Kind,
    pub class: FormatClass,
    pub size: u64,
    pub file_type: Option<String>,
    pub dimensions: Option<(u32, u32)>,
    pub duration: Option<f64>,
    pub fields: Fields,
    /// Fields this file's format can store (others are disabled in the editor).
    pub supported: Vec<Field>,
    pub error: Option<String>,
}

/// Reads the curated fields of every file for the editor.
pub fn read_fields(engine: &Engine, cache: &MetaCache, paths: &[String], device: &dyn DeviceTz) -> Vec<FileFields> {
    let pbs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    cache.prefetch(engine, &pbs);
    pbs.iter()
        .map(|p| {
            let kind = kind_of(p);
            let class = format_class(p);
            let meta = cache.get(p);
            FileFields {
                path: p.to_string_lossy().into_owned(),
                name: file_name(p),
                kind,
                class,
                size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                file_type: meta.as_ref().and_then(TagMap::file_type),
                dimensions: meta.as_ref().and_then(TagMap::dimensions),
                duration: meta.as_ref().and_then(TagMap::duration),
                fields: meta.as_ref().map(|m| fields::extract(m, kind, device)).unwrap_or_default(),
                supported: fields::ALL_FIELDS.iter().copied().filter(|f| fields::supported(*f, class, kind)).collect(),
                error: match &meta {
                    Some(m) => m.error(),
                    None if p.is_file() => Some("This file couldn't be read.".into()),
                    None => Some("This file couldn't be found.".into()),
                },
            }
        })
        .collect()
}

/// Execution data for one file, kept server-side (not sent to the UI).
#[derive(Clone, Debug, Default)]
pub struct EditExec {
    pub args: Vec<String>,
    pub expect: Vec<(String, String)>,
    pub times: FileTimes,
    pub stamp: Stamp,
    pub final_times: FileTimes,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EditItemPlan {
    pub path: String,
    pub name: String,
    pub state: PairState,
    pub no_change: bool,
    pub problems: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub diff: Vec<DiffRow>,
    /// Where the untouched file will be kept (`name_original`, `name_original_2`, ...).
    pub backup_path: String,
    #[serde(skip)]
    pub exec: EditExec,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    pub plan_id: String,
    pub items: Vec<EditItemPlan>,
    pub ready: usize,
    pub blocked: usize,
    pub unchanged: usize,
    pub set_file_dates: bool,
}

/// The planned edit of one file, from its metadata alone.
#[derive(Debug, Default)]
pub struct FileEdit {
    pub args: Vec<String>,
    pub warnings: Vec<Issue>,
    pub diff: Vec<DiffRow>,
    /// File dates to apply afterwards: the new Date taken, or the file's current dates.
    pub final_times: FileTimes,
    pub expect: Vec<(String, String)>,
}

fn parse_local(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().replace(' ', "T");
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M")).ok()
}

/// Date tags shifted by "Shift by" (only those present in the file are touched).
const SHIFT_TAGS: &[&str] = &[
    "DateTimeOriginal",
    "CreateDate",
    "ModifyDate",
    "DateCreated",
    "TrackCreateDate",
    "TrackModifyDate",
    "MediaCreateDate",
    "MediaModifyDate",
    "CreationDate",
];

/// `-GROUP:Tag+=…` for every shiftable date present in the file (each tag once).
fn shift_args(m: &TagMap, (op, v): &(&str, String)) -> Vec<String> {
    let mut seen = HashSet::new();
    m.tags
        .iter()
        .filter(|t| matches!(t.g0.as_str(), "EXIF" | "XMP" | "QuickTime" | "PDF"))
        .filter(|t| SHIFT_TAGS.iter().any(|n| n.eq_ignore_ascii_case(&t.name)) && parse_exif_dt(&value_to_string(&t.value)).is_some())
        .map(|t| t.spec())
        .filter(|spec| seen.insert(spec.clone()))
        .map(|spec| format!("-{spec}{op}{v}"))
        .collect()
}

/// The new value for a touched field. `Err` carries a warning when the change is left out.
fn new_value(f: Field, ch: &Changes) -> Result<Option<NewValue>, Issue> {
    Ok(match f {
        Field::DateTaken => match &ch.date_taken {
            Some(DateChange::Set { local, offset }) => {
                let (Some(l), Some(o)) = (parse_local(local), parse_offset(offset)) else {
                    return Err(issue("badDate", "That date or timezone isn't valid — left out."));
                };
                Some(NewValue::Date(Resolved::from_local(l, o, TzSource::Explicit, None)))
            }
            _ => None,
        },
        Field::Location => match &ch.location {
            Some(LocChange::Set { lat, lon, alt }) if lat.abs() <= 90.0 && lon.abs() <= 180.0 => {
                Some(NewValue::Location(Location { lat: *lat, lon: *lon, alt: *alt }))
            }
            Some(LocChange::Set { .. }) => {
                return Err(issue("badLocation", "Latitude must be between -90 and 90 and longitude between -180 and 180 — left out."));
            }
            _ => None,
        },
        Field::Keywords => match &ch.keywords {
            Some(ListChange::Set { values }) => {
                let v: Vec<String> = values.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                (!v.is_empty()).then_some(NewValue::List(v))
            }
            _ => None,
        },
        Field::Rating => match &ch.rating {
            Some(RatingChange::Set { value }) if *value > 0 => Some(NewValue::Rating((*value).min(5))),
            _ => None,
        },
        _ => match ch.text(f) {
            Some(TextChange::Set { value }) if !value.trim().is_empty() => Some(NewValue::Text(value.clone())),
            _ => None,
        },
    })
}

/// Plans the edit of one file from its metadata (no file access, so it is unit-testable).
fn plan_file(path: &Path, m: &TagMap, ch: &Changes, set_file_dates: bool, times: FileTimes, device: &dyn DeviceTz) -> FileEdit {
    let kind = kind_of(path);
    let class = format_class(path);
    let before = fields::extract(m, kind, device);
    let mut after = before.clone();
    let mut out = FileEdit::default();
    let old_resolved = fields::resolve_date(m, kind, &TzContext { override_offset: None, device });
    let mut new_resolved = old_resolved.clone();
    if let Some(ft) = m.file_type() {
        out.expect.push(("File:*:FileType".to_string(), ft));
    }
    let touched = ch.touched();
    for &f in &touched {
        if !fields::supported(f, class, kind) {
            out.warnings.push(issue("unsupported", format!("{} can't be stored in this {} — left out.", fields::field_label(f), class.label())));
            continue;
        }
        if let Some(DateChange::Shift { shift }) = &ch.date_taken
            && f == Field::DateTaken
        {
            if shift.is_zero() {
                continue;
            }
            let Some(r) = &old_resolved else {
                out.warnings.push(issue("noDate", "This file has no date to shift — left out."));
                continue;
            };
            let (Some(local), Some(utc), Some(arg)) = (shift.apply(r.local), shift.apply(r.utc), shift.exiftool()) else {
                out.warnings.push(issue("badShift", "That shift is too large — left out."));
                continue;
            };
            let shifted = Resolved { local, utc, ..r.clone() };
            out.args.extend(shift_args(m, &arg));
            fields::set_field(&mut after, f, Some(&NewValue::Date(shifted.clone())));
            new_resolved = Some(shifted);
            continue;
        }
        let value = match new_value(f, ch) {
            Ok(v) => v,
            Err(w) => {
                out.warnings.push(w);
                continue;
            }
        };
        if f == Field::DateTaken {
            match &value {
                Some(NewValue::Date(r)) => {
                    if class == FormatClass::QuickTime {
                        out.expect.push(("QuickTime:*:CreateDate".into(), fmt_exif(r.utc)));
                    } else if class.has_exif() {
                        out.expect.push(("EXIF:ExifIFD:DateTimeOriginal".into(), fmt_exif(r.local)));
                    }
                    new_resolved = Some(r.clone());
                }
                _ => new_resolved = None,
            }
        }
        out.args.extend(fields::assignments(f, value.as_ref(), class, m));
        fields::set_field(&mut after, f, value.as_ref());
    }
    out.final_times = match new_resolved.filter(|_| set_file_dates) {
        Some(r) => {
            let ms = r.utc.and_utc().timestamp_millis();
            FileTimes { created_ms: Some(ms), modified_ms: ms }
        }
        None => times,
    };
    out.diff = diff_rows(&before, &after, &[]).into_iter().filter(|r| touched.contains(&r.field)).collect();
    out
}

/// File-level checks that block an edit, most important first (only the first applies).
fn file_problem(p: &Path, class: FormatClass) -> Option<Issue> {
    if !p.is_file() {
        return Some(issue("missing", format!("{} couldn't be found.", file_name(p))));
    }
    if class == FormatClass::Unwritable {
        return Some(issue("unwritable", format!("Metadata can't be written into .{} files.", ext_lower(p).to_uppercase())));
    }
    file_access_problem(p)
}

/// Builds the Review for every file. `_original` names are reserved across the batch.
pub fn plan(engine: &Engine, cache: &MetaCache, req: &EditReq, device: &dyn DeviceTz, plan_id: String) -> EditPlan {
    let pbs: Vec<PathBuf> = req.paths.iter().map(PathBuf::from).collect();
    cache.prefetch(engine, &pbs);
    let mut plan = EditPlan { plan_id, set_file_dates: req.set_file_dates, ..Default::default() };
    let mut reserved: HashSet<String> = HashSet::new();
    for p in &pbs {
        let name = file_name(p);
        let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut problems: Vec<Issue> = file_problem(p, format_class(p)).into_iter().collect();
        if p.is_file() {
            problems.extend(folder_problem(&dir));
        }
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if fsops::free_space(&dir).is_some_and(|free| free < space_needed(size)) {
            problems.push(issue("noSpace", "Not enough free space on that drive to save a new copy of this file."));
        }
        let times = fsops::read_times(p).unwrap_or_default();
        let stamp = fsops::stamp(p).unwrap_or_default();
        let edit = match cache.get(p) {
            Some(m) => {
                if let Some(e) = m.error() {
                    problems.push(issue("corrupt", format!("{name} couldn't be read: {e}")));
                }
                plan_file(p, &m, &req.changes, req.set_file_dates, times, device)
            }
            None => FileEdit { final_times: times, ..Default::default() },
        };
        let no_change = problems.is_empty() && edit.args.is_empty();
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = p.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let backup = fsops::unique_original_name(&dir, &stem, &ext, |c| c.exists() || reserved.contains(&path_key(c)));
        reserved.insert(path_key(&backup));
        let state = if problems.is_empty() { PairState::Ready } else { PairState::Blocked };
        match (state, no_change) {
            (PairState::Blocked, _) => plan.blocked += 1,
            (_, true) => plan.unchanged += 1,
            _ => plan.ready += 1,
        }
        plan.items.push(EditItemPlan {
            path: p.to_string_lossy().into_owned(),
            name,
            state,
            no_change,
            problems,
            warnings: edit.warnings,
            diff: edit.diff,
            backup_path: backup.to_string_lossy().into_owned(),
            exec: EditExec { args: edit.args, expect: edit.expect, times, stamp, final_times: edit.final_times },
        });
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::datetime::FixedTz;
    use chrono::FixedOffset;
    use serde_json::json;

    fn dev() -> FixedTz {
        FixedTz(FixedOffset::east_opt(0).unwrap())
    }

    #[test]
    fn bulk_edit_writes_only_touched_fields() {
        let m = TagMap::from_json(json!({"EXIF:IFD0:Make":"Canon","EXIF:IFD0:Artist":"Old","XMP:XMP-dc:Title":"Keep me"}));
        let ch = Changes { author: Some(TextChange::Set { value: "New".into() }), ..Default::default() };
        let e = plan_file(Path::new("a.jpg"), &m, &ch, false, FileTimes::default(), &dev());
        assert!(e.warnings.is_empty());
        assert!(
            e.args.iter().all(|a| a.contains("Artist")
                || a.contains("Creator")
                || a.contains("By-line")
                || a.contains("XPAuthor")
                || a.contains("charset")
                || a.contains("CodedCharacterSet")
                || a == "iptc=UTF8"),
            "{:?}",
            e.args
        );
        assert!(!e.args.iter().any(|a| a.contains("Title") || a.contains("Make")));
        assert_eq!(e.diff.len(), 1);
        assert_eq!(e.diff[0].after.as_deref(), Some("New"));
    }

    #[test]
    fn shift_only_touches_existing_dates_and_moves_file_dates() {
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal":"2023:05:01 10:00:00",
            "EXIF:ExifIFD:OffsetTimeOriginal":"+00:00",
            "EXIF:IFD0:ModifyDate":"2023:05:01 10:00:00"
        }));
        let ch = Changes { date_taken: Some(DateChange::Shift { shift: Shift { days: 0, hours: 1, minutes: 30 } }), ..Default::default() };
        let e = plan_file(Path::new("a.jpg"), &m, &ch, true, FileTimes::default(), &dev());
        assert!(e.args.contains(&"-ExifIFD:DateTimeOriginal+=0:0:0 1:30:0".to_string()));
        assert!(e.args.contains(&"-IFD0:ModifyDate+=0:0:0 1:30:0".to_string()));
        assert!(!e.args.iter().any(|a| a.contains("OffsetTime")));
        assert!(e.diff[0].after.as_deref().unwrap().starts_with("2023-05-01 11:30:00"));
        let want = NaiveDateTime::parse_from_str("2023-05-01 11:30:00", "%Y-%m-%d %H:%M:%S").unwrap().and_utc().timestamp_millis();
        assert_eq!(e.final_times.modified_ms, want);
    }

    #[test]
    fn out_of_range_shift_is_left_out_with_a_warning() {
        let m = TagMap::from_json(json!({"EXIF:ExifIFD:DateTimeOriginal":"2023:05:01 10:00:00"}));
        let ch = Changes { date_taken: Some(DateChange::Shift { shift: Shift { days: 1_000_000_000_000, hours: 0, minutes: 0 } }), ..Default::default() };
        let e = plan_file(Path::new("a.jpg"), &m, &ch, true, FileTimes::default(), &dev());
        assert!(e.args.is_empty());
        assert_eq!(e.warnings[0].code, "badShift");
    }

    #[test]
    fn unsupported_fields_are_reported_not_written() {
        let ch = Changes { make: Some(TextChange::Set { value: "X".into() }), ..Default::default() };
        let e = plan_file(Path::new("doc.pdf"), &TagMap::default(), &ch, false, FileTimes::default(), &dev());
        assert!(e.args.is_empty());
        assert_eq!(e.warnings[0].code, "unsupported");
    }

    #[test]
    fn set_date_on_video_is_utc() {
        let ch = Changes { date_taken: Some(DateChange::Set { local: "2024-03-01T10:00".into(), offset: "+01:00".into() }), ..Default::default() };
        let e = plan_file(Path::new("v.mp4"), &TagMap::default(), &ch, false, FileTimes::default(), &dev());
        assert!(e.args.contains(&"-QuickTime:CreateDate=2024:03:01 09:00:00".to_string()));
        assert!(e.args.contains(&"-Keys:CreationDate=2024:03:01 10:00:00+01:00".to_string()));
    }

    #[test]
    fn file_dates_preserved_when_toggle_off() {
        let t = FileTimes { created_ms: Some(5), modified_ms: 6 };
        let ch = Changes { title: Some(TextChange::Set { value: "x".into() }), ..Default::default() };
        let e = plan_file(Path::new("a.jpg"), &TagMap::default(), &ch, false, t, &dev());
        assert_eq!(e.final_times, t);
    }
}
