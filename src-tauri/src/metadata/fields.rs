//! Curated, friendly fields (spec 7): reading them from any file and building the ExifTool
//! assignments that write them to the right groups for each format ("keep in sync").
//! Shared by the Edit flow and by Copy when the two files use different formats.

use super::datetime::{self, DeviceTz, Resolved, TzContext, fmt_exif, fmt_offset};
use super::tags::{FormatClass, Kind, TagMap};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    DateTaken,
    Location,
    Make,
    Model,
    Lens,
    Title,
    Description,
    Keywords,
    Rating,
    Author,
    Copyright,
}

pub const ALL_FIELDS: [Field; 11] = [
    Field::DateTaken,
    Field::Location,
    Field::Make,
    Field::Model,
    Field::Lens,
    Field::Title,
    Field::Description,
    Field::Keywords,
    Field::Rating,
    Field::Author,
    Field::Copyright,
];

impl Field {
    /// Copy-checklist category this field belongs to.
    pub fn category(self) -> &'static str {
        match self {
            Field::DateTaken => "dates",
            Field::Location => "location",
            Field::Make | Field::Model | Field::Lens => "camera",
            Field::Title | Field::Description | Field::Keywords | Field::Rating => "descriptive",
            Field::Author | Field::Copyright => "author",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DateValue {
    /// Local wall-clock time, `YYYY-MM-DDTHH:MM:SS`.
    pub local: String,
    /// `+HH:MM`, if known or assumed.
    pub offset: Option<String>,
    /// False when the offset was assumed (nothing in the file said).
    pub offset_known: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
    pub alt: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fields {
    pub date_taken: Option<DateValue>,
    pub location: Option<Location>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub rating: Option<u8>,
    pub author: Option<String>,
    pub copyright: Option<String>,
}

/// The capture instant, trying the date tags native to the file's kind first.
pub fn resolve_date(m: &TagMap, kind: Kind, ctx: &TzContext) -> Option<Resolved> {
    match kind {
        Kind::Video | Kind::Audio => datetime::resolve_video(m, ctx).or_else(|| datetime::resolve_photo(m, ctx)),
        _ => datetime::resolve_photo(m, ctx).or_else(|| datetime::resolve_video(m, ctx)),
    }
}

/// Display form of a resolved date for the UI.
pub fn date_value(r: &Resolved) -> DateValue {
    DateValue {
        local: r.local.format("%Y-%m-%dT%H:%M:%S").to_string(),
        offset: Some(fmt_offset(r.offset)),
        offset_known: !matches!(r.source, datetime::TzSource::Assumed),
    }
}

/// Reads the curated fields from a file's tags, preferring the most widely used tag for each.
pub fn extract(m: &TagMap, kind: Kind, device: &dyn DeviceTz) -> Fields {
    let location = match (m.num(&["Composite:*:GPSLatitude", "*:*:GPSLatitude"]), m.num(&["Composite:*:GPSLongitude", "*:*:GPSLongitude"])) {
        (Some(lat), Some(lon)) if lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0) => {
            Some(Location { lat, lon, alt: m.num(&["Composite:*:GPSAltitude", "*:*:GPSAltitude"]) })
        }
        _ => None,
    };
    let mut keywords = m.list(&["XMP:XMP-dc:Subject", "IPTC:*:Keywords", "QuickTime:*:Keywords", "PDF:*:Keywords", "EXIF:IFD0:XPKeywords"]);
    if keywords.len() == 1 && (keywords[0].contains(';') || keywords[0].contains(',')) {
        keywords = keywords[0].split([';', ',']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    Fields {
        date_taken: resolve_date(m, kind, &TzContext { override_offset: None, device }).map(|r| date_value(&r)),
        location,
        make: m.str(&["EXIF:IFD0:Make", "QuickTime:Keys:Make", "QuickTime:*:Make", "XMP:XMP-tiff:Make", "*:*:Make"]),
        model: m.str(&["EXIF:IFD0:Model", "QuickTime:Keys:Model", "QuickTime:*:Model", "XMP:XMP-tiff:Model", "*:*:Model"]),
        lens: m.str(&["EXIF:ExifIFD:LensModel", "XMP:XMP-exifEX:LensModel", "XMP:XMP-aux:Lens", "*:*:LensModel"]),
        title: m.str(&["XMP:XMP-dc:Title", "QuickTime:ItemList:Title", "QuickTime:*:Title", "PDF:*:Title", "EXIF:IFD0:XPTitle", "IPTC:*:ObjectName"]),
        description: m.str(&[
            "XMP:XMP-dc:Description",
            "EXIF:IFD0:ImageDescription",
            "IPTC:*:Caption-Abstract",
            "QuickTime:ItemList:Description",
            "QuickTime:*:Description",
            "PDF:*:Subject",
        ]),
        keywords,
        rating: m.num(&["XMP:XMP-xmp:Rating", "EXIF:IFD0:Rating"]).map(|r| r.clamp(0.0, 5.0) as u8),
        author: m.str(&[
            "XMP:XMP-dc:Creator",
            "EXIF:IFD0:Artist",
            "IPTC:*:By-line",
            "QuickTime:ItemList:Artist",
            "QuickTime:*:Artist",
            "PDF:*:Author",
            "EXIF:IFD0:XPAuthor",
        ]),
        copyright: m.str(&["XMP:XMP-dc:Rights", "EXIF:IFD0:Copyright", "IPTC:*:CopyrightNotice", "QuickTime:ItemList:Copyright", "QuickTime:*:Copyright"]),
    }
}

/// Whether a file of this class/kind can store the field (spec 7: disable with a tooltip otherwise).
pub fn supported(field: Field, class: FormatClass, kind: Kind) -> bool {
    match class {
        FormatClass::Unwritable => false,
        FormatClass::Pdf => !matches!(field, Field::Location | Field::Make | Field::Model | Field::Lens),
        FormatClass::QuickTime if kind == Kind::Audio => !matches!(field, Field::Location | Field::Make | Field::Model | Field::Lens),
        _ => true,
    }
}

/// A new value for one field (None = clear it).
#[derive(Clone, Debug, PartialEq)]
pub enum NewValue {
    Date(Resolved),
    Location(Location),
    Text(String),
    List(Vec<String>),
    Rating(u8),
}

fn text_targets(field: Field, class: FormatClass) -> Vec<&'static str> {
    use FormatClass::*;
    match (field, class) {
        (Field::Make, ExifFull | ExifXmp) => vec!["IFD0:Make"],
        (Field::Make, QuickTime) => vec!["Keys:Make"],
        (Field::Make, _) => vec!["XMP-tiff:Make"],
        (Field::Model, ExifFull | ExifXmp) => vec!["IFD0:Model"],
        (Field::Model, QuickTime) => vec!["Keys:Model"],
        (Field::Model, _) => vec!["XMP-tiff:Model"],
        (Field::Lens, ExifFull | ExifXmp) => vec!["ExifIFD:LensModel"],
        (Field::Lens, _) => vec!["XMP-exifEX:LensModel"],
        (Field::Title, ExifFull) => vec!["XMP-dc:Title", "IFD0:XPTitle", "IPTC:ObjectName"],
        (Field::Title, ExifXmp) => vec!["XMP-dc:Title", "IFD0:XPTitle"],
        (Field::Title, QuickTime) => vec!["ItemList:Title", "XMP-dc:Title"],
        (Field::Title, Pdf) => vec!["PDF:Title", "XMP-dc:Title"],
        (Field::Title, _) => vec!["XMP-dc:Title"],
        (Field::Description, ExifFull) => vec!["IFD0:ImageDescription", "XMP-dc:Description", "IPTC:Caption-Abstract"],
        (Field::Description, ExifXmp) => vec!["IFD0:ImageDescription", "XMP-dc:Description"],
        (Field::Description, QuickTime) => vec!["ItemList:Description", "XMP-dc:Description"],
        (Field::Description, Pdf) => vec!["PDF:Subject", "XMP-dc:Description"],
        (Field::Description, _) => vec!["XMP-dc:Description"],
        (Field::Author, ExifFull) => vec!["IFD0:Artist", "XMP-dc:Creator", "IPTC:By-line", "IFD0:XPAuthor"],
        (Field::Author, ExifXmp) => vec!["IFD0:Artist", "XMP-dc:Creator", "IFD0:XPAuthor"],
        (Field::Author, QuickTime) => vec!["ItemList:Artist", "XMP-dc:Creator"],
        (Field::Author, Pdf) => vec!["PDF:Author", "XMP-dc:Creator"],
        (Field::Author, _) => vec!["XMP-dc:Creator"],
        (Field::Copyright, ExifFull) => vec!["IFD0:Copyright", "XMP-dc:Rights", "IPTC:CopyrightNotice"],
        (Field::Copyright, ExifXmp) => vec!["IFD0:Copyright", "XMP-dc:Rights"],
        (Field::Copyright, QuickTime) => vec!["ItemList:Copyright", "XMP-dc:Rights"],
        (Field::Copyright, _) => vec!["XMP-dc:Rights"],
        _ => vec![],
    }
}

/// ExifTool assignments for one field. `existing` decides whether optional mirrors (XMP/IPTC
/// date copies that already exist) are kept in sync. Returns `-TAG=value` arguments.
pub fn assignments(field: Field, value: Option<&NewValue>, class: FormatClass, existing: &TagMap) -> Vec<String> {
    let set = |t: &str, v: &str| format!("-{t}={v}");
    let clear = |t: &str| format!("-{t}=");
    let has = |p: &str| existing.find(&[p]).is_some();
    let mut out = vec![];
    match field {
        Field::DateTaken => match value {
            Some(NewValue::Date(r)) => {
                let local = fmt_exif(r.local);
                let with_off = r.local_with_offset();
                match class {
                    FormatClass::ExifFull | FormatClass::ExifXmp => {
                        for (t, v) in datetime::photo_date_assignments(r, class) {
                            out.push(set(&t, &v));
                        }
                        for t in ["XMP-exif:DateTimeOriginal", "XMP-xmp:CreateDate", "XMP-photoshop:DateCreated", "XMP-xmp:ModifyDate"] {
                            if has(t) {
                                out.push(set(t, &with_off));
                            }
                        }
                        if class.has_iptc() && has("IPTC:DateCreated") {
                            out.push(set("IPTC:DateCreated", &local[..10]));
                            out.push(set("IPTC:TimeCreated", &format!("{}{}", &local[11..], fmt_offset(r.offset))));
                        }
                    }
                    FormatClass::XmpOnly => {
                        for t in ["XMP-exif:DateTimeOriginal", "XMP-xmp:CreateDate", "XMP-photoshop:DateCreated"] {
                            out.push(set(t, &with_off));
                        }
                    }
                    FormatClass::QuickTime => {
                        for (t, v) in datetime::video_date_assignments(r) {
                            out.push(set(&t, &v));
                        }
                        for t in ["XMP-exif:DateTimeOriginal", "XMP-xmp:CreateDate", "XMP-photoshop:DateCreated"] {
                            if has(t) {
                                out.push(set(t, &with_off));
                            }
                        }
                    }
                    FormatClass::Pdf => {
                        out.push(set("PDF:CreateDate", &with_off));
                        if has("XMP-xmp:CreateDate") {
                            out.push(set("XMP-xmp:CreateDate", &with_off));
                        }
                    }
                    FormatClass::Unwritable => {}
                }
            }
            _ => {
                for t in [
                    "DateTimeOriginal",
                    "CreateDate",
                    "ModifyDate",
                    "OffsetTime",
                    "OffsetTimeOriginal",
                    "OffsetTimeDigitized",
                    "SubSecTimeOriginal",
                    "XMP-photoshop:DateCreated",
                    "Keys:CreationDate",
                    "IPTC:DateCreated",
                    "IPTC:TimeCreated",
                ] {
                    out.push(clear(t));
                }
                if class == FormatClass::QuickTime {
                    for t in ["TrackCreateDate", "TrackModifyDate", "MediaCreateDate", "MediaModifyDate"] {
                        out.push(clear(t));
                    }
                }
            }
        },
        Field::Location => match value {
            Some(NewValue::Location(l)) => {
                let alt = l.alt;
                match class {
                    FormatClass::ExifFull | FormatClass::ExifXmp => {
                        out.push(set("GPS:GPSLatitude", &l.lat.abs().to_string()));
                        out.push(set("GPS:GPSLatitudeRef", if l.lat < 0.0 { "S" } else { "N" }));
                        out.push(set("GPS:GPSLongitude", &l.lon.abs().to_string()));
                        out.push(set("GPS:GPSLongitudeRef", if l.lon < 0.0 { "W" } else { "E" }));
                        match alt {
                            Some(a) => {
                                out.push(set("GPS:GPSAltitude", &a.abs().to_string()));
                                out.push(format!("-GPS:GPSAltitudeRef#={}", if a < 0.0 { 1 } else { 0 }));
                            }
                            None => {
                                out.push(clear("GPS:GPSAltitude"));
                                out.push(clear("GPS:GPSAltitudeRef"));
                            }
                        }
                        if has("XMP-exif:GPSLatitude") {
                            out.push(set("XMP-exif:GPSLatitude", &l.lat.to_string()));
                            out.push(set("XMP-exif:GPSLongitude", &l.lon.to_string()));
                        }
                    }
                    FormatClass::XmpOnly => {
                        out.push(set("XMP-exif:GPSLatitude", &l.lat.to_string()));
                        out.push(set("XMP-exif:GPSLongitude", &l.lon.to_string()));
                        if let Some(a) = alt {
                            out.push(set("XMP-exif:GPSAltitude", &a.abs().to_string()));
                            out.push(format!("-XMP-exif:GPSAltitudeRef#={}", if a < 0.0 { 1 } else { 0 }));
                        }
                    }
                    FormatClass::QuickTime => {
                        let v = match alt {
                            Some(a) => format!("{}, {}, {}", l.lat, l.lon, a),
                            None => format!("{}, {}", l.lat, l.lon),
                        };
                        out.push(set("Keys:GPSCoordinates", &v));
                        out.push(set("UserData:GPSCoordinates", &v));
                    }
                    _ => {}
                }
            }
            _ => {
                out.push(clear("GPS:all"));
                for t in [
                    "XMP-exif:GPSLatitude",
                    "XMP-exif:GPSLongitude",
                    "XMP-exif:GPSAltitude",
                    "XMP-exif:GPSAltitudeRef",
                    "Keys:GPSCoordinates",
                    "UserData:GPSCoordinates",
                ] {
                    out.push(clear(t));
                }
            }
        },
        Field::Keywords => {
            let mut targets: Vec<&str> = vec!["XMP-dc:Subject"];
            if class.has_iptc() {
                targets.push("IPTC:Keywords");
            }
            match value {
                Some(NewValue::List(list)) if !list.is_empty() => {
                    for t in &targets {
                        for k in list {
                            out.push(set(t, k));
                        }
                    }
                    if class.has_exif() {
                        out.push(set("IFD0:XPKeywords", &list.join(";")));
                    }
                    if class == FormatClass::Pdf {
                        out.push(set("PDF:Keywords", &list.join(", ")));
                    }
                }
                _ => {
                    for t in &targets {
                        out.push(clear(t));
                    }
                    if class.has_exif() {
                        out.push(clear("IFD0:XPKeywords"));
                    }
                    if class == FormatClass::Pdf {
                        out.push(clear("PDF:Keywords"));
                    }
                }
            }
        }
        Field::Rating => match value {
            Some(NewValue::Rating(r)) => {
                let r = (*r).min(5);
                out.push(set("XMP-xmp:Rating", &r.to_string()));
                if class.has_exif() {
                    out.push(set("IFD0:Rating", &r.to_string()));
                    let pct = [0, 1, 25, 50, 75, 99][r as usize];
                    out.push(set("IFD0:RatingPercent", &pct.to_string()));
                }
            }
            _ => {
                out.push(clear("XMP-xmp:Rating"));
                if class.has_exif() {
                    out.push(clear("IFD0:Rating"));
                    out.push(clear("IFD0:RatingPercent"));
                }
            }
        },
        _ => {
            let targets = text_targets(field, class);
            match value {
                Some(NewValue::Text(s)) if !s.trim().is_empty() => {
                    for t in targets {
                        out.push(set(t, s));
                    }
                    // Video files from Android/other cameras keep make/model in UserData too.
                    if class == FormatClass::QuickTime && matches!(field, Field::Make | Field::Model) {
                        let ud = if field == Field::Make { "UserData:Make" } else { "UserData:Model" };
                        if has(ud) {
                            out.push(set(ud, s));
                        }
                    }
                }
                _ => {
                    for t in targets {
                        out.push(clear(t));
                    }
                }
            }
        }
    }
    // IPTC needs an explicit UTF-8 declaration so non-Latin text survives.
    if out.iter().any(|a| a.starts_with("-IPTC:") && !a.ends_with('=')) {
        out.insert(0, "-IPTC:CodedCharacterSet=UTF8".into());
        out.insert(0, "iptc=UTF8".into());
        out.insert(0, "-charset".into());
    }
    out
}

/// Field value as a `NewValue` (for Copy translations).
pub fn as_new_value(f: &Fields, field: Field, resolved: Option<&Resolved>) -> Option<NewValue> {
    match field {
        Field::DateTaken => resolved.cloned().map(NewValue::Date),
        Field::Location => f.location.map(NewValue::Location),
        Field::Make => f.make.clone().map(NewValue::Text),
        Field::Model => f.model.clone().map(NewValue::Text),
        Field::Lens => f.lens.clone().map(NewValue::Text),
        Field::Title => f.title.clone().map(NewValue::Text),
        Field::Description => f.description.clone().map(NewValue::Text),
        Field::Keywords => (!f.keywords.is_empty()).then(|| NewValue::List(f.keywords.clone())),
        Field::Rating => f.rating.map(NewValue::Rating),
        Field::Author => f.author.clone().map(NewValue::Text),
        Field::Copyright => f.copyright.clone().map(NewValue::Text),
    }
}

/// Short display string for a field (Preview diffs and Review screen).
pub fn display(f: &Fields, field: Field) -> Option<String> {
    match field {
        Field::DateTaken => f.date_taken.as_ref().map(|d| {
            let t = d.local.replace('T', " ");
            match (&d.offset, d.offset_known) {
                (Some(o), true) => format!("{t} ({o})"),
                (Some(o), false) => format!("{t} ({o}, assumed)"),
                _ => t,
            }
        }),
        Field::Location => f.location.map(|l| match l.alt {
            Some(a) => format!("{:.6}, {:.6} · {:.0} m", l.lat, l.lon, a),
            None => format!("{:.6}, {:.6}", l.lat, l.lon),
        }),
        Field::Make => f.make.clone(),
        Field::Model => f.model.clone(),
        Field::Lens => f.lens.clone(),
        Field::Title => f.title.clone(),
        Field::Description => f.description.clone(),
        Field::Keywords => (!f.keywords.is_empty()).then(|| f.keywords.join(", ")),
        Field::Rating => f.rating.map(|r| format!("{}{}", "★".repeat(r as usize), "☆".repeat(5 - r as usize))),
        Field::Author => f.author.clone(),
        Field::Copyright => f.copyright.clone(),
    }
}

/// Sets one field in `f` to `v` (None clears it).
pub fn set_field(f: &mut Fields, field: Field, v: Option<&NewValue>) {
    match (field, v) {
        (Field::DateTaken, Some(NewValue::Date(r))) => f.date_taken = Some(date_value(r)),
        (Field::DateTaken, _) => f.date_taken = None,
        (Field::Location, Some(NewValue::Location(l))) => f.location = Some(*l),
        (Field::Location, _) => f.location = None,
        (Field::Keywords, Some(NewValue::List(l))) => f.keywords = l.clone(),
        (Field::Keywords, _) => f.keywords = vec![],
        (Field::Rating, Some(NewValue::Rating(r))) => f.rating = Some(*r),
        (Field::Rating, _) => f.rating = None,
        (field, v) => {
            let s = match v {
                Some(NewValue::Text(s)) if !s.trim().is_empty() => Some(s.clone()),
                _ => None,
            };
            match field {
                Field::Make => f.make = s,
                Field::Model => f.model = s,
                Field::Lens => f.lens = s,
                Field::Title => f.title = s,
                Field::Description => f.description = s,
                Field::Author => f.author = s,
                Field::Copyright => f.copyright = s,
                _ => {}
            }
        }
    }
}

/// Field name as shown in diffs and messages.
pub fn field_label(field: Field) -> &'static str {
    match field {
        Field::DateTaken => "Date taken",
        Field::Location => "Location",
        Field::Make => "Camera make",
        Field::Model => "Camera model",
        Field::Lens => "Lens",
        Field::Title => "Title",
        Field::Description => "Description",
        Field::Keywords => "Keywords",
        Field::Rating => "Rating",
        Field::Author => "Author",
        Field::Copyright => "Copyright",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::datetime::{FixedTz, TzSource};
    use chrono::{FixedOffset, NaiveDateTime};
    use serde_json::json;

    #[test]
    fn writes_keep_groups_in_sync() {
        let empty = TagMap::default();
        let a = assignments(Field::Description, Some(&NewValue::Text("Hi".into())), FormatClass::ExifFull, &empty);
        assert!(a.contains(&"-IFD0:ImageDescription=Hi".to_string()));
        assert!(a.contains(&"-XMP-dc:Description=Hi".to_string()));
        assert!(a.contains(&"-IPTC:Caption-Abstract=Hi".to_string()));
        assert!(a.contains(&"-IPTC:CodedCharacterSet=UTF8".to_string()));
        let a = assignments(Field::Description, Some(&NewValue::Text("Hi".into())), FormatClass::ExifXmp, &empty);
        assert!(!a.iter().any(|x| x.contains("IPTC")));
    }

    #[test]
    fn video_dates_are_utc_plus_keys_offset() {
        let r = Resolved {
            local: NaiveDateTime::parse_from_str("2024-03-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            offset: FixedOffset::east_opt(3600).unwrap(),
            utc: NaiveDateTime::parse_from_str("2024-03-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            source: TzSource::Explicit,
            zone: None,
        };
        let a = assignments(Field::DateTaken, Some(&NewValue::Date(r)), FormatClass::QuickTime, &TagMap::default());
        assert!(a.contains(&"-QuickTime:CreateDate=2024:03:01 09:00:00".to_string()));
        assert!(a.contains(&"-Keys:CreationDate=2024:03:01 10:00:00+01:00".to_string()));
    }

    #[test]
    fn location_writes_refs_and_video_coordinates() {
        let l = Location { lat: -33.5, lon: 151.25, alt: None };
        let a = assignments(Field::Location, Some(&NewValue::Location(l)), FormatClass::ExifFull, &TagMap::default());
        assert!(a.contains(&"-GPS:GPSLatitudeRef=S".to_string()));
        assert!(a.contains(&"-GPS:GPSLongitude=151.25".to_string()));
        let a = assignments(Field::Location, Some(&NewValue::Location(l)), FormatClass::QuickTime, &TagMap::default());
        assert!(a.contains(&"-Keys:GPSCoordinates=-33.5, 151.25".to_string()));
    }

    #[test]
    fn extracts_curated_fields() {
        let m = TagMap::from_json(json!({
            "EXIF:IFD0:Make": "Apple",
            "EXIF:ExifIFD:DateTimeOriginal": "2023:05:01 14:22:10",
            "EXIF:ExifIFD:OffsetTimeOriginal": "+02:00",
            "Composite:GPSLatitude": 48.85,
            "Composite:GPSLongitude": 2.35,
            "XMP:XMP-dc:Subject": ["paris", "trip"],
            "XMP:XMP-xmp:Rating": 4
        }));
        let f = extract(&m, Kind::Photo, &FixedTz(FixedOffset::east_opt(0).unwrap()));
        assert_eq!(f.make.as_deref(), Some("Apple"));
        assert_eq!(f.date_taken.as_ref().unwrap().local, "2023-05-01T14:22:10");
        assert_eq!(f.date_taken.as_ref().unwrap().offset.as_deref(), Some("+02:00"));
        assert_eq!(f.keywords, vec!["paris", "trip"]);
        assert_eq!(f.rating, Some(4));
        assert_eq!(display(&f, Field::Rating).unwrap(), "★★★★☆");
    }
}
