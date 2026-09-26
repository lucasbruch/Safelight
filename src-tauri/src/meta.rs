//! Media classification, metadata parsing and folder naming.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Photo,
    Video,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Photo => "photo",
            Kind::Video => "video",
        }
    }
}

const PHOTO_EXT: &[&str] = &["cr3", "cr2", "crw", "arw", "srf", "sr2", "jpg", "jpeg", "hif", "heif", "heic", "dng", "tif", "tiff"];
/// Proprietary RAW formats: the only ones whose metadata Lightroom reads from an
/// `.xmp` sidecar (JPEG, HEIC, TIFF and DNG carry their XMP inside the file).
const RAW_EXT: &[&str] = &["cr3", "cr2", "crw", "arw", "srf", "sr2"];
const VIDEO_EXT: &[&str] = &["mp4", "mov", "mxf", "crm", "avi", "mts", "m2ts"];
/// Files that belong to a media file with the same stem (or stem prefix for Sony clips).
const SIDECAR_EXT: &[&str] = &["xml", "thm", "xmp", "wav", "lrf"];

pub fn classify(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if PHOTO_EXT.contains(&ext.as_str()) {
        Some(Kind::Photo)
    } else if VIDEO_EXT.contains(&ext.as_str()) {
        Some(Kind::Video)
    } else {
        None
    }
}

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn is_sidecar(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SIDECAR_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// The tags we ask exiftool for (media files and Sony clip XML alike).
pub const TAGS: &[&str] = &[
    "Make", "Model", "DateTimeOriginal", "SubSecTimeOriginal", "CreateDate", "LensModel", "Lens",
    "ISO", "ExposureTime", "FNumber", "FocalLength", "Flash", "Orientation", "ImageWidth",
    "ImageHeight", "GPSLatitude", "GPSLongitude", "SerialNumber", "InternalSerialNumber",
    "Duration",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub make: Option<String>,
    pub model: Option<String>,
    pub captured_at: Option<NaiveDateTime>,
    pub lens: Option<String>,
    pub iso: Option<f64>,
    pub shutter: Option<f64>,
    pub aperture: Option<f64>,
    pub focal: Option<f64>,
    pub flash: Option<bool>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub serial: Option<String>,
    pub duration: Option<f64>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(x) if !x.trim().is_empty() => Some(x.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn f(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(x) => x.trim().parse().ok(),
        _ => None,
    }
}

/// Parses exiftool dates like `2021:05:15 13:08:54`, `2021:05:15 14:08:54+02:00`
/// or ISO `2021-05-15T14:08:54+02:00`. Time zone suffixes are dropped: we want
/// the wall-clock time the camera showed.
pub fn parse_date(raw: &str) -> Option<NaiveDateTime> {
    let t = raw.trim();
    if t.starts_with("0000") {
        return None;
    }
    // `get` rather than slicing: corrupt metadata can put a multi-byte character here.
    let head = t.get(..19)?.replace('T', " ").replace('-', ":");
    NaiveDateTime::parse_from_str(&head, "%Y:%m:%d %H:%M:%S").ok()
}

impl Meta {
    pub fn from_exif(v: &Value) -> Self {
        let mut captured_at = s(v, "DateTimeOriginal")
            .and_then(|d| parse_date(&d))
            .or_else(|| s(v, "CreateDate").and_then(|d| parse_date(&d)));
        if let (Some(dt), Some(sub)) = (captured_at.as_mut(), s(v, "SubSecTimeOriginal")) {
            // "83" means .83 s; keep millisecond precision for burst ordering.
            let digits: String = sub.chars().filter(|c| c.is_ascii_digit()).take(3).collect();
            if let Ok(ms) = format!("{digits:0<3}").parse::<i64>() {
                *dt += chrono::Duration::milliseconds(ms);
            }
        }
        Self {
            make: s(v, "Make"),
            model: s(v, "Model"),
            captured_at,
            lens: s(v, "LensModel").or_else(|| s(v, "Lens")),
            iso: f(v, "ISO"),
            shutter: f(v, "ExposureTime"),
            aperture: f(v, "FNumber"),
            focal: f(v, "FocalLength"),
            // EXIF Flash bit 0 = fired.
            flash: f(v, "Flash").map(|x| (x as i64) & 1 == 1),
            width: f(v, "ImageWidth").map(|x| x as i64),
            height: f(v, "ImageHeight").map(|x| x as i64),
            orientation: f(v, "Orientation").map(|x| x as i64),
            gps_lat: f(v, "GPSLatitude"),
            gps_lon: f(v, "GPSLongitude"),
            serial: s(v, "SerialNumber").or_else(|| s(v, "InternalSerialNumber")),
            duration: f(v, "Duration"),
        }
    }

    /// Fills gaps from another source (e.g. a Sony clip's XML sidecar).
    pub fn merge_missing(&mut self, other: &Meta) {
        macro_rules! fill { ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = other.$f.clone(); } )* } }
        fill!(make, model, captured_at, lens, serial, duration);
    }
}

pub fn file_mtime(path: &Path) -> Option<NaiveDateTime> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = m.into();
    Some(dt.naive_local())
}

fn pretty_make(make: &str) -> String {
    let first = make.split_whitespace().next().unwrap_or(make);
    let mut c = first.chars();
    match c.next() {
        Some(h) => h.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Makes a string safe as a single path component on Windows and macOS.
pub fn sanitize_component(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c if c.is_whitespace() => '-',
            c => c,
        })
        .collect();
    let mut out = String::new();
    for ch in cleaned.chars() {
        if ch == '-' && out.ends_with('-') {
            continue;
        }
        out.push(ch);
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if out.is_empty() {
        "Untitled".into()
    } else {
        out
    }
}

/// Folder name for a camera: "Canon EOS R5" → `Canon-EOS-R5`, Sony "ILCE-7M4" → `Sony-ILCE-7M4`.
pub fn camera_folder(make: Option<&str>, model: Option<&str>) -> String {
    let make = make.map(str::trim).filter(|m| !m.is_empty());
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    let name = match (make, model) {
        (_, None) => "Unknown Camera".to_string(),
        (Some(mk), Some(md)) => {
            let pm = pretty_make(mk);
            if md.to_lowercase().starts_with(&pm.to_lowercase()) {
                md.to_string()
            } else {
                format!("{pm} {md}")
            }
        }
        (None, Some(md)) => md.to_string(),
    };
    sanitize_component(&name)
}

/// Human-friendly camera label for the UI ("Canon EOS R5", "Sony ILCE-7M4").
pub fn camera_label(make: Option<&str>, model: Option<&str>) -> String {
    camera_folder(make, model).replace('-', " ").replace("ILCE ", "ILCE-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_names() {
        assert_eq!(camera_folder(Some("Canon"), Some("Canon EOS R5")), "Canon-EOS-R5");
        assert_eq!(camera_folder(Some("SONY"), Some("ILCE-7M4")), "Sony-ILCE-7M4");
        assert_eq!(camera_folder(None, None), "Unknown-Camera");
        assert_eq!(camera_folder(Some("NIKON CORPORATION"), Some("NIKON Z 6")), "NIKON-Z-6");
    }

    #[test]
    fn dates() {
        let d = parse_date("2021:05:15 14:08:54+02:00").unwrap();
        assert_eq!(d.to_string(), "2021-05-15 14:08:54");
        assert!(parse_date("0000:00:00 00:00:00").is_none());
        assert!(parse_date("2023-10-19T16:13:48+01:00").is_some());
        assert!(parse_date("2023:10:19 16:13:4\u{e9}xyz").is_none(), "no panic on a multi-byte char at byte 19");
        assert!(parse_date("short").is_none());
    }

    #[test]
    fn raw_formats() {
        assert!(is_raw(Path::new("a/IMG_0001.CR3")));
        assert!(!is_raw(Path::new("a/IMG_0001.JPG")));
        assert!(!is_raw(Path::new("a/IMG_0001.dng")));
    }

    #[test]
    fn subseconds_and_flash() {
        let v: Value = serde_json::json!({"DateTimeOriginal":"2017:01:26 09:36:26","SubSecTimeOriginal":83,"Flash":16});
        let m = Meta::from_exif(&v);
        assert_eq!(m.captured_at.unwrap().format("%H:%M:%S%.3f").to_string(), "09:36:26.830");
        assert_eq!(m.flash, Some(false));
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_component("  Wedding: Smith / Jones "), "Wedding-Smith-Jones");
        assert_eq!(sanitize_component("..."), "Untitled");
    }
}
