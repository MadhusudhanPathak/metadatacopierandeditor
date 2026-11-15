//! Parsed ExifTool output for one file, plus file-kind and format-capability helpers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// One tag as reported by `exiftool -json -a -G0:1 -n`.
#[derive(Clone, Debug)]
pub struct Tag {
    pub g0: String,
    pub g1: String,
    pub name: String,
    pub value: Value,
}

impl Tag {
    /// `Group1:Name`, the form used for group-specific ExifTool arguments.
    pub fn spec(&self) -> String {
        format!("{}:{}", self.g1, self.name)
    }
}

/// All tags of one file, in ExifTool's output order.
#[derive(Clone, Debug, Default)]
pub struct TagMap {
    pub tags: Vec<Tag>,
}

impl TagMap {
    /// Parses one object of `exiftool -json -G0:1` output (keys `G0:G1:Name`, `G0:Name` or `Name`).
    pub fn from_json(v: Value) -> Self {
        let mut tags = vec![];
        if let Value::Object(map) = v {
            for (k, value) in map {
                if k == "SourceFile" {
                    continue;
                }
                let parts: Vec<&str> = k.split(':').collect();
                let (g0, g1, name) = match parts.as_slice() {
                    [g0, g1, name] => (*g0, *g1, *name),
                    [g0, name] => (*g0, *g0, *name),
                    [name] => ("", "", *name),
                    _ => {
                        let name = parts[parts.len() - 1];
                        (parts[0], parts[1], name)
                    }
                };
                tags.push(Tag { g0: g0.into(), g1: g1.into(), name: name.into(), value });
            }
        }
        TagMap { tags }
    }

    /// Finds the first tag matching any of the patterns `G0:G1:Name`, `G1:Name`, or `Name`
    /// (`*` matches any group). Patterns are tried in order.
    pub fn find(&self, patterns: &[&str]) -> Option<&Tag> {
        for p in patterns {
            let parts: Vec<&str> = p.split(':').collect();
            let (g0, g1, name) = match parts.as_slice() {
                [g0, g1, n] => (*g0, *g1, *n),
                [g1, n] => ("*", *g1, *n),
                [n] => ("*", "*", *n),
                _ => continue,
            };
            if let Some(t) = self.tags.iter().find(|t| {
                t.name.eq_ignore_ascii_case(name)
                    && (g0 == "*" || t.g0.eq_ignore_ascii_case(g0))
                    && (g1 == "*" || t.g1.eq_ignore_ascii_case(g1) || (g1.ends_with('*') && t.g1.starts_with(&g1[..g1.len() - 1])))
                    && !is_empty_value(&t.value)
            }) {
                return Some(t);
            }
        }
        None
    }

    /// First matching value as text (non-empty).
    pub fn str(&self, patterns: &[&str]) -> Option<String> {
        self.find(patterns).map(|t| value_to_string(&t.value)).filter(|s| !s.trim().is_empty())
    }

    /// First matching value as a number (`-n` output is numeric).
    pub fn num(&self, patterns: &[&str]) -> Option<f64> {
        self.find(patterns).and_then(|t| value_to_f64(&t.value))
    }

    /// First matching value as a list (a single value becomes a one-item list).
    pub fn list(&self, patterns: &[&str]) -> Vec<String> {
        match self.find(patterns).map(|t| &t.value) {
            Some(Value::Array(a)) => a.iter().map(value_to_string).filter(|s| !s.is_empty()).collect(),
            Some(v) => {
                let s = value_to_string(v);
                if s.is_empty() { vec![] } else { vec![s] }
            }
            None => vec![],
        }
    }

    /// True if any tag belongs to the family-0 group (e.g. `MakerNotes`).
    pub fn has_group0(&self, g0: &str) -> bool {
        self.tags.iter().any(|t| t.g0.eq_ignore_ascii_case(g0))
    }

    /// ExifTool's error for the file, if it couldn't be read properly.
    pub fn error(&self) -> Option<String> {
        self.str(&["ExifTool:ExifTool:Error", "ExifTool:Error"])
    }

    /// ExifTool's warning for the file, shown under "Details".
    pub fn warning(&self) -> Option<String> {
        self.str(&["ExifTool:ExifTool:Warning", "ExifTool:Warning"])
    }

    /// Detected file type (e.g. `JPEG`), independent of the extension.
    pub fn file_type(&self) -> Option<String> {
        self.str(&["File:File:FileType", "File:FileType"])
    }

    /// Stored pixel dimensions (before any orientation/rotation is applied).
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        let w = self.num(&[
            "File:File:ImageWidth",
            "PNG:*:ImageWidth",
            "RIFF:*:ImageWidth",
            "QuickTime:Track*:ImageWidth",
            "QuickTime:*:ImageWidth",
            "GIF:*:ImageWidth",
            "EXIF:*:ImageWidth",
            "EXIF:*:ExifImageWidth",
            "*:*:ImageWidth",
        ])?;
        let h = self.num(&[
            "File:File:ImageHeight",
            "PNG:*:ImageHeight",
            "RIFF:*:ImageHeight",
            "QuickTime:Track*:ImageHeight",
            "QuickTime:*:ImageHeight",
            "GIF:*:ImageHeight",
            "EXIF:*:ImageHeight",
            "EXIF:*:ExifImageHeight",
            "*:*:ImageHeight",
        ])?;
        if w > 0.0 && h > 0.0 { Some((w as u32, h as u32)) } else { None }
    }

    /// Media duration in seconds, if positive.
    pub fn duration(&self) -> Option<f64> {
        self.num(&["QuickTime:*:Duration", "Composite:*:Duration", "*:*:Duration"]).filter(|d| *d > 0.0)
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// JSON value as display text (arrays joined with ", ").
pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(a) => a.iter().map(value_to_string).collect::<Vec<_>>().join(", "),
        Value::Object(_) => v.to_string(),
    }
}

fn value_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Broad media kind, used for pairing (D3) and date conversion (spec 5.2).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Photo,
    Video,
    Audio,
    Other,
}

/// Media kind by extension.
pub fn kind_of(path: &Path) -> Kind {
    let ext = ext_lower(path);
    match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" | "heic" | "heif" | "hif" | "avif" | "png" | "webp" | "gif" | "tif" | "tiff" | "bmp" | "jxl" | "psd" | "dng"
        | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "orf" | "rw2" | "raf" | "pef" | "srw" | "x3f" | "3fr" | "erf" | "kdc" | "mrw"
        | "mos" | "iiq" | "rwl" | "dcr" | "raw" => Kind::Photo,
        "mp4" | "m4v" | "mov" | "qt" | "3gp" | "3g2" | "avi" | "mkv" | "webm" | "mts" | "m2ts" | "wmv" | "flv" | "insv" | "mpg" | "mpeg" | "lrv" => Kind::Video,
        "m4a" | "mp3" | "wav" | "flac" | "ogg" | "opus" | "aac" | "aif" | "aiff" | "wma" | "ape" => Kind::Audio,
        _ => Kind::Other,
    }
}

/// Lower-case extension without the dot.
pub fn ext_lower(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// What ExifTool can write into a file, by extension.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FormatClass {
    /// JPEG, TIFF and camera RAW: EXIF + IPTC + XMP (+ MakerNotes).
    ExifFull,
    /// PNG, WebP, HEIC/HEIF/AVIF, JXL: EXIF + XMP.
    ExifXmp,
    /// GIF and SVG: XMP only.
    XmpOnly,
    /// MP4/MOV family (also M4A): QuickTime + XMP.
    QuickTime,
    /// PDF: PDF info dictionary + XMP.
    Pdf,
    /// ExifTool cannot write this format (MP3, WAV, AVI, MKV, BMP, ...).
    Unwritable,
}

/// Write capabilities by extension.
pub fn format_class(path: &Path) -> FormatClass {
    match ext_lower(path).as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" | "tif" | "tiff" | "dng" | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "orf" | "rw2" | "raf"
        | "pef" | "srw" | "x3f" | "3fr" | "erf" | "kdc" | "mrw" | "mos" | "iiq" | "rwl" | "dcr" | "psd" => FormatClass::ExifFull,
        "png" | "webp" | "heic" | "heif" | "hif" | "avif" | "jxl" => FormatClass::ExifXmp,
        "gif" | "svg" => FormatClass::XmpOnly,
        "mp4" | "m4v" | "mov" | "qt" | "3gp" | "3g2" | "m4a" | "insv" | "lrv" => FormatClass::QuickTime,
        "pdf" => FormatClass::Pdf,
        _ => FormatClass::Unwritable,
    }
}

impl FormatClass {
    /// Family-0 groups this format can store.
    fn groups(self) -> &'static [&'static str] {
        match self {
            FormatClass::ExifFull => &["EXIF", "IPTC", "XMP", "MakerNotes", "Photoshop", "File", "ICC_Profile", "JFIF"],
            FormatClass::ExifXmp => &["EXIF", "XMP", "MakerNotes", "PNG", "ICC_Profile"],
            FormatClass::XmpOnly => &["XMP", "File"],
            FormatClass::QuickTime => &["QuickTime", "XMP"],
            FormatClass::Pdf => &["PDF", "XMP"],
            FormatClass::Unwritable => &[],
        }
    }

    /// True if the format can store tags of this family-0 group.
    pub fn supports(self, g0: &str) -> bool {
        self.groups().iter().any(|g| g.eq_ignore_ascii_case(g0))
    }

    pub fn has_exif(self) -> bool {
        matches!(self, FormatClass::ExifFull | FormatClass::ExifXmp)
    }

    pub fn has_iptc(self) -> bool {
        matches!(self, FormatClass::ExifFull)
    }

    /// Word used in messages like "can't be stored in this video".
    pub fn label(self) -> &'static str {
        match self {
            FormatClass::ExifFull | FormatClass::ExifXmp | FormatClass::XmpOnly => "image",
            FormatClass::QuickTime => "video",
            FormatClass::Pdf => "PDF",
            FormatClass::Unwritable => "file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_group_keys_and_finds() {
        let m = TagMap::from_json(json!({
            "SourceFile": "x.jpg",
            "EXIF:IFD0:Make": "Canon",
            "XMP:XMP-dc:Subject": ["a", "b"],
            "Composite:GPSLatitude": 12.5,
            "QuickTime:Track1:ImageWidth": 1920,
            "QuickTime:Track1:ImageHeight": 1080
        }));
        assert_eq!(m.str(&["EXIF:IFD0:Make"]).as_deref(), Some("Canon"));
        assert_eq!(m.str(&["IFD0:Make"]).as_deref(), Some("Canon"));
        assert_eq!(m.list(&["XMP-dc:Subject"]), vec!["a", "b"]);
        assert_eq!(m.num(&["Composite:*:GPSLatitude"]), Some(12.5));
        assert_eq!(m.dimensions(), Some((1920, 1080)));
    }

    #[test]
    fn kinds_and_classes() {
        assert_eq!(kind_of(Path::new("a/IMG_1.HEIC")), Kind::Photo);
        assert_eq!(kind_of(Path::new("clip.MOV")), Kind::Video);
        assert_eq!(kind_of(Path::new("song.m4a")), Kind::Audio);
        assert_eq!(format_class(Path::new("x.mp4")), FormatClass::QuickTime);
        assert_eq!(format_class(Path::new("x.avi")), FormatClass::Unwritable);
        assert!(FormatClass::ExifXmp.supports("exif"));
        assert!(!FormatClass::ExifXmp.supports("IPTC"));
    }
}
