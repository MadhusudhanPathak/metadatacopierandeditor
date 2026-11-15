//! Date/time and timezone rules (spec 5.2, D8). Everything here is pure and timezone-independent:
//! the device timezone is injected, so tests give identical results on any machine.

use super::tags::{FormatClass, TagMap};
use chrono::{Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone};
use serde::{Deserialize, Serialize};

/// A parsed ExifTool date: wall-clock time plus the offset, if the value carried one.
#[derive(Clone, Debug, PartialEq)]
pub struct ExifDt {
    pub local: NaiveDateTime,
    pub offset: Option<FixedOffset>,
}

/// Parses ExifTool date strings: `YYYY:MM:DD HH:MM:SS[.fff][Z|+HH:MM|-HHMM]`
/// (also accepts `-` date separators and a `T`). Returns None for zero/partial dates.
pub fn parse_exif_dt(s: &str) -> Option<ExifDt> {
    let s = s.trim();
    if s.starts_with("0000") {
        return None;
    }
    // `get` (unlike `split_at`) returns None instead of panicking on a non-ASCII prefix.
    let head = s.get(..19)?;
    let rest = &s[19..];
    let norm: String = head
        .char_indices()
        .map(|(i, c)| match (i, c) {
            (4 | 7, '-') => ':',
            (10, 'T') => ' ',
            _ => c,
        })
        .collect();
    let local = NaiveDateTime::parse_from_str(&norm, "%Y:%m:%d %H:%M:%S").ok()?;
    let mut rest = rest.trim_start();
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = if rest.is_empty() { None } else { parse_offset(rest) };
    Some(ExifDt { local, offset })
}

/// Parses `Z`, `+HH:MM`, `-HHMM`, `+HH`. Rejects offsets beyond ±14:59.
pub fn parse_offset(s: &str) -> Option<FixedOffset> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("z") {
        return FixedOffset::east_opt(0);
    }
    let (sign, body) = match s.chars().next()? {
        '+' => (1, &s[1..]),
        '-' => (-1, &s[1..]),
        _ => return None,
    };
    let digits: String = body.chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match digits.len() {
        2 => (digits.parse::<i32>().ok()?, 0),
        4 => (digits[..2].parse::<i32>().ok()?, digits[2..].parse::<i32>().ok()?),
        _ => return None,
    };
    if h > 14 || m > 59 {
        return None;
    }
    FixedOffset::east_opt(sign * (h * 3600 + m * 60))
}

/// `YYYY:MM:DD HH:MM:SS`, the ExifTool date format.
pub fn fmt_exif(dt: NaiveDateTime) -> String {
    dt.format("%Y:%m:%d %H:%M:%S").to_string()
}

/// `+HH:MM` / `-HH:MM`.
pub fn fmt_offset(o: FixedOffset) -> String {
    let secs = o.local_minus_utc();
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.abs();
    format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60)
}

/// Where the UTC offset came from.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TzSource {
    /// An explicit offset stored in the file (OffsetTime*, Keys:CreationDate, XMP date with zone).
    Explicit,
    /// Derived from the GPS timestamp (UTC) versus the local capture time.
    GpsTime,
    /// Derived from the GPS position's timezone (ExifTool's offline geolocation database).
    GpsZone,
    /// Nothing in the file: the device timezone was assumed.
    Assumed,
    /// The user chose the timezone for this batch.
    Override,
}

/// A capture instant with its resolved UTC offset and where that offset came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub local: NaiveDateTime,
    pub offset: FixedOffset,
    pub utc: NaiveDateTime,
    pub source: TzSource,
    pub zone: Option<String>,
}

impl Resolved {
    pub fn from_local(local: NaiveDateTime, offset: FixedOffset, source: TzSource, zone: Option<String>) -> Self {
        let utc = local - Duration::seconds(offset.local_minus_utc() as i64);
        Self { local, offset, utc, source, zone }
    }
    pub fn from_utc(utc: NaiveDateTime, offset: FixedOffset, source: TzSource, zone: Option<String>) -> Self {
        let local = utc + Duration::seconds(offset.local_minus_utc() as i64);
        Self { local, offset, utc, source, zone }
    }
    /// `YYYY:MM:DD HH:MM:SS+HH:MM`.
    pub fn local_with_offset(&self) -> String {
        format!("{}{}", fmt_exif(self.local), fmt_offset(self.offset))
    }
}

/// Device timezone lookup, injected so tests are machine-independent.
pub trait DeviceTz {
    fn offset_at_local(&self, local: NaiveDateTime) -> FixedOffset;
    fn offset_at_utc(&self, utc: NaiveDateTime) -> FixedOffset;
}

/// The real device timezone.
pub struct SystemTz;
impl DeviceTz for SystemTz {
    fn offset_at_local(&self, local: NaiveDateTime) -> FixedOffset {
        chrono::Local.from_local_datetime(&local).earliest().map(|d| d.offset().fix()).unwrap_or_else(|| chrono::Local::now().offset().fix())
    }
    fn offset_at_utc(&self, utc: NaiveDateTime) -> FixedOffset {
        chrono::Local.from_utc_datetime(&utc).offset().fix()
    }
}

/// A constant device timezone (tests).
pub struct FixedTz(pub FixedOffset);
impl DeviceTz for FixedTz {
    fn offset_at_local(&self, _: NaiveDateTime) -> FixedOffset {
        self.0
    }
    fn offset_at_utc(&self, _: NaiveDateTime) -> FixedOffset {
        self.0
    }
}

/// Inputs for timezone resolution besides the file's own metadata.
pub struct TzContext<'a> {
    /// Batch override chosen by the user; replaces only *assumed* offsets.
    pub override_offset: Option<FixedOffset>,
    pub device: &'a dyn DeviceTz,
}

fn zone_of(m: &TagMap) -> Option<chrono_tz::Tz> {
    m.str(&["Composite:*:GeolocationTimeZone", "*:*:GeolocationTimeZone"])?.parse().ok()
}

fn zone_offset_at_local(z: chrono_tz::Tz, local: NaiveDateTime) -> Option<FixedOffset> {
    z.from_local_datetime(&local).earliest().map(|d| d.offset().fix())
}

fn fallback(ctx: &TzContext, at_local: Option<NaiveDateTime>, at_utc: Option<NaiveDateTime>) -> (FixedOffset, TzSource) {
    if let Some(o) = ctx.override_offset {
        return (o, TzSource::Override);
    }
    let o = match (at_local, at_utc) {
        (Some(l), _) => ctx.device.offset_at_local(l),
        (None, Some(u)) => ctx.device.offset_at_utc(u),
        _ => FixedOffset::east_opt(0).unwrap(),
    };
    (o, TzSource::Assumed)
}

const PHOTO_DATE_TAGS: &[&str] = &[
    "EXIF:ExifIFD:DateTimeOriginal",
    "XMP:XMP-exif:DateTimeOriginal",
    "XMP:XMP-photoshop:DateCreated",
    "EXIF:ExifIFD:CreateDate",
    "XMP:XMP-xmp:CreateDate",
    "Composite:*:DateTimeCreated",
    "*:*:DateTimeOriginal",
    "EXIF:IFD0:ModifyDate",
];

/// Resolves a photo's capture instant: local time plus the best available offset.
pub fn resolve_photo(m: &TagMap, ctx: &TzContext) -> Option<Resolved> {
    let dt = PHOTO_DATE_TAGS.iter().find_map(|p| m.str(&[p]).and_then(|s| parse_exif_dt(&s)))?;
    let local = dt.local;
    if let Some(o) = dt.offset {
        return Some(Resolved::from_local(local, o, TzSource::Explicit, None));
    }
    for p in ["*:*:OffsetTimeOriginal", "*:*:OffsetTime", "*:*:OffsetTimeDigitized"] {
        if let Some(o) = m.str(&[p]).and_then(|s| parse_offset(&s)) {
            return Some(Resolved::from_local(local, o, TzSource::Explicit, None));
        }
    }
    if let Some(utc) = gps_utc(m)
        && let Some(o) = offset_from_gps_time(local, utc)
    {
        return Some(Resolved::from_local(local, o, TzSource::GpsTime, None));
    }
    if let Some(z) = zone_of(m)
        && let Some(o) = zone_offset_at_local(z, local)
    {
        return Some(Resolved::from_local(local, o, TzSource::GpsZone, Some(z.name().to_string())));
    }
    let (o, src) = fallback(ctx, Some(local), None);
    Some(Resolved::from_local(local, o, src, None))
}

/// Resolves a video's capture instant. QuickTime dates are UTC (spec 5.2); iPhone-style
/// `Keys:CreationDate` carries a local time with an explicit offset and wins.
pub fn resolve_video(m: &TagMap, ctx: &TzContext) -> Option<Resolved> {
    if let Some(dt) = m.str(&["QuickTime:Keys:CreationDate", "*:Keys:CreationDate", "QuickTime:*:CreationDate"]).and_then(|s| parse_exif_dt(&s))
        && let Some(o) = dt.offset
    {
        return Some(Resolved::from_local(dt.local, o, TzSource::Explicit, None));
    }
    let utc = ["QuickTime:QuickTime:CreateDate", "QuickTime:*:CreateDate", "QuickTime:*:MediaCreateDate", "QuickTime:*:TrackCreateDate"]
        .iter()
        .find_map(|p| m.str(&[p]).and_then(|s| parse_exif_dt(&s)))?;
    // A value carrying its own offset (rare in QuickTime) is already an instant.
    let utc_naive = match utc.offset {
        Some(o) => utc.local - Duration::seconds(o.local_minus_utc() as i64),
        None => utc.local,
    };
    if let Some(z) = zone_of(m) {
        let o = z.from_utc_datetime(&utc_naive).offset().fix();
        return Some(Resolved::from_utc(utc_naive, o, TzSource::GpsZone, Some(z.name().to_string())));
    }
    let (o, src) = fallback(ctx, None, Some(utc_naive));
    Some(Resolved::from_utc(utc_naive, o, src, None))
}

fn gps_utc(m: &TagMap) -> Option<NaiveDateTime> {
    if let Some(dt) = m.str(&["Composite:*:GPSDateTime"]).and_then(|s| parse_exif_dt(&s)) {
        return Some(dt.local);
    }
    let date = m.str(&["*:*:GPSDateStamp"])?;
    let time = m.str(&["*:*:GPSTimeStamp"])?;
    let d = NaiveDate::parse_from_str(date.trim(), "%Y:%m:%d").ok()?;
    let t = NaiveTime::parse_from_str(time.trim().split('.').next()?, "%H:%M:%S").ok()?;
    Some(d.and_time(t))
}

/// Offset = local − GPS UTC, accepted only if it lands within 5 minutes of a 15-minute step
/// (GPS fixes can be stale; a large residual means the timestamp is unreliable).
fn offset_from_gps_time(local: NaiveDateTime, utc: NaiveDateTime) -> Option<FixedOffset> {
    let diff_min = (local - utc).num_seconds() as f64 / 60.0;
    let rounded = (diff_min / 15.0).round() * 15.0;
    if (diff_min - rounded).abs() > 5.0 || rounded.abs() > 14.0 * 60.0 {
        return None;
    }
    FixedOffset::east_opt(rounded as i32 * 60)
}

/// Tags to write when copying photo → video (spec 5.2 rule 2).
pub fn video_date_assignments(r: &Resolved) -> Vec<(String, String)> {
    let utc = fmt_exif(r.utc);
    let mut v: Vec<(String, String)> = [
        "QuickTime:CreateDate",
        "QuickTime:ModifyDate",
        "QuickTime:TrackCreateDate",
        "QuickTime:TrackModifyDate",
        "QuickTime:MediaCreateDate",
        "QuickTime:MediaModifyDate",
    ]
    .iter()
    .map(|t| (t.to_string(), utc.clone()))
    .collect();
    v.push(("Keys:CreationDate".into(), r.local_with_offset()));
    v
}

/// Tags to write when copying video → photo (spec 5.2 rule 3).
pub fn photo_date_assignments(r: &Resolved, class: FormatClass) -> Vec<(String, String)> {
    let local = fmt_exif(r.local);
    let off = fmt_offset(r.offset);
    if class.has_exif() {
        vec![
            ("ExifIFD:DateTimeOriginal".into(), local.clone()),
            ("ExifIFD:CreateDate".into(), local.clone()),
            ("IFD0:ModifyDate".into(), local),
            ("ExifIFD:OffsetTimeOriginal".into(), off.clone()),
            ("ExifIFD:OffsetTimeDigitized".into(), off.clone()),
            ("ExifIFD:OffsetTime".into(), off),
        ]
    } else {
        vec![("XMP-exif:DateTimeOriginal".into(), r.local_with_offset()), ("XMP-xmp:CreateDate".into(), r.local_with_offset())]
    }
}

/// A relative shift for the Edit flow's "Shift by" (D10). Values come from the user, so every
/// computation is checked: an out-of-range shift yields None instead of panicking.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shift {
    pub days: i64,
    pub hours: i64,
    pub minutes: i64,
}

impl Shift {
    pub fn total_minutes(&self) -> Option<i64> {
        self.days.checked_mul(1440)?.checked_add(self.hours.checked_mul(60)?)?.checked_add(self.minutes)
    }

    pub fn is_zero(&self) -> bool {
        self.total_minutes() == Some(0)
    }

    /// ExifTool shift argument: (operator, value), e.g. ("+=", "0:0:1 2:30:0").
    pub fn exiftool(&self) -> Option<(&'static str, String)> {
        let total = self.total_minutes()?;
        let op = if total < 0 { "-=" } else { "+=" };
        let a = total.unsigned_abs();
        let (d, h, m) = (a / 1440, (a % 1440) / 60, a % 60);
        Some((op, format!("0:0:{d} {h}:{m}:0")))
    }

    /// `dt` moved by the shift, or None if the result is out of range.
    pub fn apply(&self, dt: NaiveDateTime) -> Option<NaiveDateTime> {
        dt.checked_add_signed(Duration::try_minutes(self.total_minutes()?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tags::TagMap;
    use serde_json::json;

    fn off(h: i32, m: i32) -> FixedOffset {
        FixedOffset::east_opt(h.signum() * (h.abs() * 3600 + m * 60)).unwrap()
    }
    fn ndt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }
    fn ctx_fixed(dev: &FixedTz) -> TzContext<'_> {
        TzContext { override_offset: None, device: dev }
    }

    #[test]
    fn parses_dates_and_offsets() {
        let d = parse_exif_dt("2023:05:01 14:22:10").unwrap();
        assert_eq!(d.local, ndt("2023-05-01 14:22:10"));
        assert_eq!(d.offset, None);
        let d = parse_exif_dt("2023:05:01 14:22:10.123+05:30").unwrap();
        assert_eq!(d.offset, Some(off(5, 30)));
        let d = parse_exif_dt("2023-05-01T14:22:10Z").unwrap();
        assert_eq!(d.offset, Some(off(0, 0)));
        assert!(parse_exif_dt("0000:00:00 00:00:00").is_none());
        assert!(parse_exif_dt("2023:05:01").is_none());
        assert!(parse_exif_dt("2023:05:01 14:22:1é").is_none(), "non-ASCII must not panic");
        assert_eq!(parse_offset("-0800"), Some(off(-8, 0)));
        assert_eq!(parse_offset("+09"), Some(off(9, 0)));
        assert_eq!(fmt_offset(off(-3, 30).neg_fix()), "+03:30");
        assert_eq!(fmt_offset(FixedOffset::east_opt(-(3 * 3600 + 1800)).unwrap()), "-03:30");
    }

    trait NegFix {
        fn neg_fix(self) -> FixedOffset;
    }
    impl NegFix for FixedOffset {
        fn neg_fix(self) -> FixedOffset {
            FixedOffset::east_opt(-self.local_minus_utc()).unwrap()
        }
    }

    #[test]
    fn photo_with_explicit_offset_to_video_is_utc() {
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal": "2023:05:01 14:22:10",
            "EXIF:ExifIFD:OffsetTimeOriginal": "+02:00"
        }));
        let dev = FixedTz(off(-7, 0)); // device timezone must not matter
        let r = resolve_photo(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::Explicit);
        assert_eq!(r.utc, ndt("2023-05-01 12:22:10"));
        let a = video_date_assignments(&r);
        assert!(a.contains(&("QuickTime:CreateDate".into(), "2023:05:01 12:22:10".into())));
        assert!(a.contains(&("Keys:CreationDate".into(), "2023:05:01 14:22:10+02:00".into())));
    }

    #[test]
    fn photo_offset_from_gps_timestamp() {
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal": "2023:05:01 21:30:00",
            "EXIF:GPS:GPSDateStamp": "2023:05:01",
            "EXIF:GPS:GPSTimeStamp": "12:28:41"
        }));
        let dev = FixedTz(off(0, 0));
        let r = resolve_photo(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::GpsTime);
        assert_eq!(r.offset, off(9, 0));
    }

    #[test]
    fn stale_gps_timestamp_is_ignored() {
        // 37 minutes off any 15-minute step: unreliable, fall back to device (assumed).
        assert_eq!(offset_from_gps_time(ndt("2023-05-01 12:37:00"), ndt("2023-05-01 12:00:00")), None);
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal": "2023:05:01 12:37:00",
            "Composite:GPSDateTime": "2023:05:01 12:00:00Z"
        }));
        let dev = FixedTz(off(1, 0));
        let r = resolve_photo(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::Assumed);
        assert_eq!(r.offset, off(1, 0));
    }

    #[test]
    fn photo_offset_from_gps_zone_handles_dst() {
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal": "2023:07:01 10:00:00",
            "Composite:GeolocationTimeZone": "Europe/London"
        }));
        let dev = FixedTz(off(9, 0));
        let r = resolve_photo(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::GpsZone);
        assert_eq!(r.offset, off(1, 0)); // BST in July
        assert_eq!(r.zone.as_deref(), Some("Europe/London"));
    }

    #[test]
    fn assumed_offset_uses_device_or_override() {
        let m = TagMap::from_json(json!({ "EXIF:ExifIFD:DateTimeOriginal": "2023:01:10 08:00:00" }));
        let dev = FixedTz(off(5, 30));
        let r = resolve_photo(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::Assumed);
        assert_eq!(r.utc, ndt("2023-01-10 02:30:00"));
        let ctx = TzContext { override_offset: Some(off(-5, 0)), device: &dev };
        let r = resolve_photo(&m, &ctx).unwrap();
        assert_eq!(r.source, TzSource::Override);
        assert_eq!(r.utc, ndt("2023-01-10 13:00:00"));
    }

    #[test]
    fn override_never_replaces_explicit_offset() {
        let m = TagMap::from_json(json!({
            "EXIF:ExifIFD:DateTimeOriginal": "2023:01:10 08:00:00",
            "EXIF:ExifIFD:OffsetTimeOriginal": "+01:00"
        }));
        let dev = FixedTz(off(0, 0));
        let ctx = TzContext { override_offset: Some(off(-5, 0)), device: &dev };
        assert_eq!(resolve_photo(&m, &ctx).unwrap().offset, off(1, 0));
    }

    #[test]
    fn iphone_video_to_photo_uses_keys_offset() {
        let m = TagMap::from_json(json!({
            "QuickTime:QuickTime:CreateDate": "2023:05:01 12:22:10",
            "QuickTime:Keys:CreationDate": "2023:05:01 14:22:10+02:00"
        }));
        let dev = FixedTz(off(-8, 0));
        let r = resolve_video(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::Explicit);
        assert_eq!(r.local, ndt("2023-05-01 14:22:10"));
        let a = photo_date_assignments(&r, FormatClass::ExifFull);
        assert!(a.contains(&("ExifIFD:DateTimeOriginal".into(), "2023:05:01 14:22:10".into())));
        assert!(a.contains(&("ExifIFD:OffsetTimeOriginal".into(), "+02:00".into())));
    }

    #[test]
    fn plain_video_utc_to_local_with_gps_zone() {
        let m = TagMap::from_json(json!({
            "QuickTime:QuickTime:CreateDate": "2024:01:15 03:00:00",
            "Composite:GeolocationTimeZone": "Asia/Tokyo"
        }));
        let dev = FixedTz(off(0, 0));
        let r = resolve_video(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::GpsZone);
        assert_eq!(r.local, ndt("2024-01-15 12:00:00"));
    }

    #[test]
    fn plain_video_assumed_offset() {
        let m = TagMap::from_json(json!({ "QuickTime:QuickTime:CreateDate": "2024:01:15 03:00:00" }));
        let dev = FixedTz(off(-5, 0));
        let r = resolve_video(&m, &ctx_fixed(&dev)).unwrap();
        assert_eq!(r.source, TzSource::Assumed);
        assert_eq!(r.local, ndt("2024-01-14 22:00:00"));
        assert!(resolve_video(&TagMap::from_json(json!({"QuickTime:QuickTime:CreateDate": "0000:00:00 00:00:00"})), &ctx_fixed(&dev)).is_none());
    }

    #[test]
    fn photo_to_photo_without_exif_uses_xmp_with_offset() {
        let r = Resolved::from_local(ndt("2023-05-01 14:22:10"), off(2, 0), TzSource::Explicit, None);
        let a = photo_date_assignments(&r, FormatClass::XmpOnly);
        assert_eq!(a[0], ("XMP-exif:DateTimeOriginal".into(), "2023:05:01 14:22:10+02:00".into()));
    }

    #[test]
    fn shift_arguments() {
        let s = Shift { days: 1, hours: 2, minutes: 30 };
        assert_eq!(s.exiftool(), Some(("+=", "0:0:1 2:30:0".to_string())));
        let s = Shift { days: 0, hours: -1, minutes: 0 };
        assert_eq!(s.exiftool(), Some(("-=", "0:0:0 1:0:0".to_string())));
        assert_eq!(s.apply(ndt("2023-01-01 00:30:00")), Some(ndt("2022-12-31 23:30:00")));
    }

    #[test]
    fn huge_shifts_are_rejected_not_panicking() {
        let s = Shift { days: i64::MAX / 2, hours: 0, minutes: 0 };
        assert_eq!(s.total_minutes(), None);
        assert_eq!(s.exiftool(), None);
        let s = Shift { days: 1_000_000_000_000, hours: 0, minutes: 0 };
        assert_eq!(s.apply(ndt("2023-01-01 00:00:00")), None);
    }
}
