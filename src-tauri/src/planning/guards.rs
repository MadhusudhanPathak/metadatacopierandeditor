//! Safety guards for a Copy pair: orientation (T8) and "do these files belong together?" hints.

use super::{Issue, issue};
use crate::metadata::tags::{Kind, TagMap};

/// Allowed relative difference in aspect ratio or duration before a pair looks mismatched.
const MISMATCH_TOLERANCE: f64 = 0.05;

/// Orientation guard (spec 5.1): copy only when it can't double-rotate.
/// Returns (copy?, note shown in the Preview when skipped).
pub fn orientation_decision(src: &TagMap, conv: &TagMap, kind: Kind) -> (bool, Option<String>) {
    let (so, co, swap, half) = if kind == Kind::Video {
        let r = |m: &TagMap| m.num(&["QuickTime:*:Rotation", "Composite:*:Rotation"]).map(|v| (v as i64).rem_euclid(360));
        let so = r(src).unwrap_or(0);
        let co = r(conv).unwrap_or(0);
        (so, co, so == 90 || so == 270, so == 180)
    } else {
        let o = |m: &TagMap| m.num(&["EXIF:IFD0:Orientation", "XMP:XMP-tiff:Orientation", "*:*:Orientation"]).map(|v| v as i64);
        let so = o(src).unwrap_or(1);
        let co = o(conv).unwrap_or(1);
        (so, co, (5..=8).contains(&so), (2..=4).contains(&so))
    };
    let neutral = if kind == Kind::Video { 0 } else { 1 };
    if so == neutral {
        return (false, None);
    }
    if co != neutral {
        // The converter kept its own orientation, which matches its pixels.
        return (
            false,
            if co == so { None } else { Some("Orientation not copied: the converted file already has its own, so the picture won't be turned twice.".into()) },
        );
    }
    if half {
        return (
            false,
            Some(
                "Orientation not copied: we can't tell whether the converted file was already turned upside-down or mirrored. Check that it looks right."
                    .into(),
            ),
        );
    }
    if swap {
        return match (src.dimensions(), conv.dimensions()) {
            (Some((sw, sh)), Some((cw, ch))) if sw != sh && cw != ch => {
                if (sw > sh) == (cw > ch) {
                    (true, None)
                } else {
                    (false, Some("Orientation not copied: the converted file is already rotated, so copying it would turn the picture twice.".into()))
                }
            }
            _ => (false, Some("Orientation not copied: the picture's shape doesn't show whether it was already rotated. Check that it looks right.".into())),
        };
    }
    (false, None)
}

/// Shape/duration mismatch hints (spec 11: warn, never block).
pub fn mismatch(src: &TagMap, conv: &TagMap) -> Vec<Issue> {
    let mut v = vec![];
    if let (Some((sw, sh)), Some((cw, ch))) = (src.dimensions(), conv.dimensions()) {
        let ratio = |w: u32, h: u32| w.max(h) as f64 / w.min(h).max(1) as f64;
        let (rs, rc) = (ratio(sw, sh), ratio(cw, ch));
        if (rs - rc).abs() / rs > MISMATCH_TOLERANCE {
            v.push(issue("mismatch", format!("These look like different pictures ({sw}×{sh} vs {cw}×{ch}). Make sure they really belong together.")));
        }
    }
    if let (Some(ds), Some(dc)) = (src.duration(), conv.duration())
        && (ds - dc).abs() > (ds * MISMATCH_TOLERANCE).max(1.0)
    {
        v.push(issue("mismatch", format!("These videos have different lengths ({ds:.0}s vs {dc:.0}s). Make sure they really belong together.")));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn orientation_guard() {
        let src = TagMap::from_json(json!({"EXIF:IFD0:Orientation":6,"File:File:ImageWidth":4000,"File:File:ImageHeight":3000}));
        let not_rotated = TagMap::from_json(json!({"File:File:ImageWidth":2000,"File:File:ImageHeight":1500}));
        let rotated = TagMap::from_json(json!({"File:File:ImageWidth":1500,"File:File:ImageHeight":2000}));
        assert!(orientation_decision(&src, &not_rotated, Kind::Photo).0);
        let (copy, note) = orientation_decision(&src, &rotated, Kind::Photo);
        assert!(!copy && note.is_some());
        let upside = TagMap::from_json(json!({"EXIF:IFD0:Orientation":3}));
        assert!(!orientation_decision(&upside, &not_rotated, Kind::Photo).0);
        let has_own = TagMap::from_json(json!({"EXIF:IFD0:Orientation":6,"File:File:ImageWidth":2000,"File:File:ImageHeight":1500}));
        assert!(!orientation_decision(&src, &has_own, Kind::Photo).0);
    }
}
