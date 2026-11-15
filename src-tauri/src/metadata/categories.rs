//! Data-driven category classification for the Copy checklist (spec 5.1).
//! The map itself lives in `categories.json`.

use super::tags::Tag;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Category ids in checklist order (mirrored by `CATEGORY_IDS` in the frontend).
pub const IDS: [&str; 9] = ["dates", "location", "camera", "exposure", "descriptive", "author", "makernotes", "container", "other"];

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Rule {
    id: String,
    tags: Vec<String>,
    prefixes: Vec<String>,
    suffixes: Vec<String>,
    except_prefixes: Vec<String>,
    groups0: Vec<String>,
    groups1: Vec<String>,
    catch_all: bool,
}

#[derive(Deserialize)]
struct Structural {
    groups0: Vec<String>,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct Map {
    structural: Structural,
    categories: Vec<Rule>,
}

struct Compiled {
    structural_groups: HashSet<String>,
    structural_tags: HashSet<String>,
    rules: Vec<Rule>,
}

fn compiled() -> &'static Compiled {
    static C: OnceLock<Compiled> = OnceLock::new();
    C.get_or_init(|| {
        let map: Map = serde_json::from_str(include_str!("categories.json")).expect("categories.json is invalid");
        let lower = |v: &Vec<String>| v.iter().map(|s| s.to_ascii_lowercase()).collect::<HashSet<_>>();
        let mut rules = map.categories;
        for r in &mut rules {
            for v in [&mut r.tags, &mut r.prefixes, &mut r.suffixes, &mut r.except_prefixes, &mut r.groups0, &mut r.groups1] {
                for s in v.iter_mut() {
                    *s = s.to_ascii_lowercase();
                }
            }
        }
        Compiled { structural_groups: lower(&map.structural.groups0), structural_tags: lower(&map.structural.tags), rules }
    })
}

/// True if the tag describes the file's actual pixels/streams (never copied or cleared).
pub fn is_structural(t: &Tag) -> bool {
    let c = compiled();
    c.structural_groups.contains(&t.g0.to_ascii_lowercase()) || c.structural_tags.contains(&t.name.to_ascii_lowercase())
}

/// Category id for a non-structural tag.
pub fn classify(t: &Tag) -> &'static str {
    let c = compiled();
    let name = t.name.to_ascii_lowercase();
    let g0 = t.g0.to_ascii_lowercase();
    let g1 = t.g1.to_ascii_lowercase();
    // Maker notes are copied as one block (ExifTool cannot split them), so every tag inside
    // them belongs to that category regardless of its name.
    if g0 == "makernotes" {
        return "makernotes";
    }
    for r in &c.rules {
        if r.catch_all {
            return id_static(&r.id);
        }
        if r.except_prefixes.iter().any(|p| name.starts_with(p.as_str())) {
            continue;
        }
        let hit = r.tags.contains(&name)
            || r.prefixes.iter().any(|p| name.starts_with(p.as_str()))
            || r.suffixes.iter().any(|s| name.ends_with(s.as_str()))
            || r.groups0.contains(&g0)
            || r.groups1.contains(&g1);
        if hit {
            return id_static(&r.id);
        }
    }
    "other"
}

fn id_static(id: &str) -> &'static str {
    IDS.iter().find(|x| **x == id).copied().unwrap_or("other")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn t(g0: &str, g1: &str, name: &str) -> Tag {
        Tag { g0: g0.into(), g1: g1.into(), name: name.into(), value: Value::Null }
    }

    #[test]
    fn classifies_common_tags() {
        assert_eq!(classify(&t("EXIF", "ExifIFD", "DateTimeOriginal")), "dates");
        assert_eq!(classify(&t("EXIF", "ExifIFD", "OffsetTimeOriginal")), "dates");
        assert_eq!(classify(&t("QuickTime", "QuickTime", "CreateDate")), "dates");
        assert_eq!(classify(&t("EXIF", "GPS", "GPSDateStamp")), "location");
        assert_eq!(classify(&t("EXIF", "GPS", "GPSLatitude")), "location");
        assert_eq!(classify(&t("QuickTime", "Keys", "GPSCoordinates")), "location");
        assert_eq!(classify(&t("EXIF", "IFD0", "Make")), "camera");
        assert_eq!(classify(&t("QuickTime", "Keys", "Model")), "camera");
        assert_eq!(classify(&t("EXIF", "ExifIFD", "ExposureTime")), "exposure");
        assert_eq!(classify(&t("XMP", "XMP-dc", "Subject")), "descriptive");
        assert_eq!(classify(&t("IPTC", "IPTC", "By-line")), "author");
        assert_eq!(classify(&t("MakerNotes", "Canon", "CanonModelID")), "makernotes");
        assert_eq!(classify(&t("QuickTime", "ItemList", "Encoder")), "container");
        assert_eq!(classify(&t("XMP", "XMP-xmpMM", "DocumentID")), "other");
    }

    #[test]
    fn structural_tags_are_recognised() {
        assert!(is_structural(&t("EXIF", "ExifIFD", "ExifImageWidth")));
        assert!(is_structural(&t("ICC_Profile", "ICC-header", "ProfileVersion")));
        assert!(is_structural(&t("EXIF", "IFD1", "ThumbnailImage")));
        assert!(is_structural(&t("EXIF", "IFD0", "Orientation")));
        assert!(!is_structural(&t("EXIF", "IFD0", "Make")));
    }
}
