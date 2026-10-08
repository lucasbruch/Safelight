//! Card detection and scanning.

use crate::exiftool::ExifTool;
use crate::ledger::{self, Ledger, Seen};
use crate::meta::{self, Kind, Meta};
use crate::project::{Project, REJECTED_DIR};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub mount: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Camera cards: mounted volumes with a DCIM folder (stills) or PRIVATE/M4ROOT
/// (Sony video), excluding the system drive and any drive holding one of
/// `exclude` (the projects and backup folders, which may well contain a DCIM copy).
pub fn detect(exclude: &[String]) -> Vec<Card> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let system_root = std::env::var("SystemDrive").map(|d| format!("{d}\\")).unwrap_or_else(|_| "/".into());
    let mut cards: Vec<Card> = disks
        .iter()
        .filter_map(|d| {
            let mount = d.mount_point();
            let m = mount.to_string_lossy().to_string();
            if m.eq_ignore_ascii_case(&system_root) || m == "/" || m.starts_with("/System") {
                return None;
            }
            if !looks_like_card(mount) {
                return None;
            }
            let name = d.name().to_string_lossy().trim().to_string();
            Some(Card {
                label: if name.is_empty() { m.clone() } else { name },
                mount: m,
                total_bytes: d.total_space(),
                free_bytes: d.available_space(),
            })
        })
        .collect();
    #[cfg(windows)]
    for c in windows_drive_letters() {
        if !cards.iter().any(|k| k.mount.eq_ignore_ascii_case(&c.mount)) {
            cards.push(c);
        }
    }
    cards.retain(|c| !exclude.iter().any(|e| on_volume(e, &c.mount)));
    cards.sort_by(|a, b| a.mount.cmp(&b.mount));
    cards.dedup_by(|a, b| a.mount == b.mount);
    cards
}

/// True if `path` lives on the volume mounted at `mount`.
fn on_volume(path: &str, mount: &str) -> bool {
    let (p, m) = (path.trim(), mount.trim_end_matches(['\\', '/']));
    if p.is_empty() || m.is_empty() {
        return false;
    }
    let (p, m) = if cfg!(windows) { (p.to_lowercase(), m.to_lowercase()) } else { (p.to_string(), m.to_string()) };
    p == m || p.strip_prefix(&m).is_some_and(|rest| rest.starts_with(['\\', '/']))
}

/// Some card readers (and `subst` drives) aren't listed as volumes; check the
/// drive letters of removable and local drives directly.
#[cfg(windows)]
fn windows_drive_letters() -> Vec<Card> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetVolumeInformationW(root: *const u16, name: *mut u16, name_len: u32, serial: *mut u32, max_comp: *mut u32, flags: *mut u32, fs: *mut u16, fs_len: u32) -> i32;
        fn GetDiskFreeSpaceExW(root: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let mask = unsafe { GetLogicalDrives() };
    let mut out = vec![];
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        if format!("{letter}:") == system {
            continue;
        }
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = std::ffi::OsStr::new(&root).encode_wide().chain(Some(0)).collect();
        let ty = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if ty != DRIVE_REMOVABLE && ty != DRIVE_FIXED {
            continue; // skip network, optical, RAM drives: slow or irrelevant
        }
        if !looks_like_card(Path::new(&root)) {
            continue;
        }
        let mut name = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(wide.as_ptr(), name.as_mut_ptr(), name.len() as u32, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0)
        };
        let label = if ok != 0 {
            String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(0)])
        } else {
            String::new()
        };
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
        out.push(Card {
            label: if label.trim().is_empty() { format!("Card ({letter}:)") } else { label },
            mount: root,
            total_bytes: total,
            free_bytes: avail,
        });
    }
    out
}

pub fn looks_like_card(root: &Path) -> bool {
    root.join("DCIM").is_dir() || root.join("PRIVATE").join("M4ROOT").is_dir()
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub kind: Kind,
    pub size: u64,
    pub meta: Meta,
    pub camera_folder: String,
    pub camera_label: String,
    pub sidecars: Vec<PathBuf>,
    pub key: String,
    pub already: Option<Seen>,
}

impl SourceFile {
    pub fn file_name(&self) -> String {
        self.path.file_name().unwrap_or_default().to_string_lossy().into_owned()
    }
    pub fn captured_iso(&self) -> Option<String> {
        self.meta.captured_at.map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3f").to_string())
    }
}

pub struct Scan {
    pub source: PathBuf,
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub source: String,
    pub label: String,
    pub photos: usize,
    pub videos: usize,
    pub bytes: u64,
    pub new_photos: usize,
    pub new_videos: usize,
    pub new_bytes: u64,
    pub already_imported: usize,
    /// Of those, rejects you sent to the Trash from Safelight.
    pub trashed: usize,
    /// First capture date among the files that aren't imported yet (names new projects).
    pub first_new_date: Option<String>,
    /// Projects that already contain some of these files.
    pub already_in: Vec<String>,
    pub cameras: Vec<String>,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

impl Scan {
    pub fn summary(&self, label: &str) -> ScanSummary {
        let mut s = ScanSummary {
            source: self.source.to_string_lossy().into_owned(),
            label: label.to_string(),
            photos: 0,
            videos: 0,
            bytes: 0,
            new_photos: 0,
            new_videos: 0,
            new_bytes: 0,
            already_imported: 0,
            trashed: 0,
            first_new_date: None,
            already_in: vec![],
            cameras: vec![],
            first_date: None,
            last_date: None,
        };
        let mut cams = BTreeSet::new();
        let mut projects = BTreeSet::new();
        let mut dates = vec![];
        let mut new_dates = vec![];
        for f in &self.files {
            let is_new = f.already.is_none();
            match f.kind {
                Kind::Photo => {
                    s.photos += 1;
                    s.new_photos += is_new as usize;
                }
                Kind::Video => {
                    s.videos += 1;
                    s.new_videos += is_new as usize;
                }
            }
            s.bytes += f.size;
            if is_new {
                s.new_bytes += f.size;
            } else {
                s.already_imported += 1;
                s.trashed += f.already.as_ref().is_some_and(|a| a.trashed) as usize;
                if let Some(a) = f.already.as_ref().filter(|a| !a.trashed) {
                    projects.insert(
                        Path::new(&a.project_root).file_name().unwrap_or_default().to_string_lossy().into_owned(),
                    );
                }
            }
            cams.insert(f.camera_label.clone());
            if let Some(d) = f.meta.captured_at {
                dates.push(d);
                if is_new {
                    new_dates.push(d);
                }
            }
        }
        dates.sort();
        s.first_date = dates.first().map(|d| d.format("%Y-%m-%d").to_string());
        s.last_date = dates.last().map(|d| d.format("%Y-%m-%d").to_string());
        s.first_new_date = new_dates.iter().min().map(|d| d.format("%Y-%m-%d").to_string());
        s.cameras = cams.into_iter().collect();
        s.already_in = projects.into_iter().collect();
        s
    }
}

fn walk_roots(source: &Path) -> Vec<PathBuf> {
    let known: Vec<PathBuf> = ["DCIM", "PRIVATE", "CLIP", "XDROOT"]
        .iter()
        .map(|d| source.join(d))
        .filter(|p| p.is_dir())
        .collect();
    if known.is_empty() {
        vec![source.to_path_buf()]
    } else {
        known
    }
}

fn skip_entry(e: &walkdir::DirEntry) -> bool {
    let n = e.file_name().to_string_lossy();
    e.depth() > 0 && (n.starts_with('.') || n == REJECTED_DIR || n == "$RECYCLE.BIN" || n == "System Volume Information")
}

pub fn scan(
    source: &Path,
    exif: &ExifTool,
    ledger: &Ledger,
    mut progress: impl FnMut(usize, usize),
) -> Result<Scan> {
    // 1. Enumerate media and sidecars.
    let mut media: Vec<(PathBuf, Kind, u64)> = vec![];
    let mut sidecars: Vec<PathBuf> = vec![];
    for root in walk_roots(source) {
        for e in WalkDir::new(&root).max_depth(8).into_iter().filter_entry(|e| !skip_entry(e)).flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            let p = e.path();
            if let Some(k) = meta::classify(p) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                media.push((p.to_path_buf(), k, size));
            } else if meta::is_sidecar(p) {
                sidecars.push(p.to_path_buf());
            }
        }
    }
    media.sort_by(|a, b| a.0.cmp(&b.0));

    // 2. Attach sidecars: same folder, and the sidecar's stem starts with the
    //    media stem (IMG_0001.THM, C0001M01.XML for C0001.MP4).
    let mut by_dir: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, (p, _, _)) in media.iter().enumerate() {
        by_dir.entry(p.parent().unwrap_or(source).to_path_buf()).or_default().push(i);
    }
    let mut attached: Vec<Vec<PathBuf>> = vec![vec![]; media.len()];
    for sc in sidecars {
        let stem = stem_lower(&sc);
        if let Some(cands) = by_dir.get(sc.parent().unwrap_or(source)) {
            let best = cands
                .iter()
                .filter(|&&i| stem.starts_with(&stem_lower(&media[i].0)))
                // Longest matching stem wins; between a RAW and its JPEG, the RAW gets the sidecar.
                .max_by_key(|&&i| (stem_lower(&media[i].0).len(), meta::is_raw(&media[i].0)));
            if let Some(&i) = best {
                attached[i].push(sc);
            }
        }
    }

    // 3. Metadata in batches (plus XML sidecars for video clips, which carry the camera model).
    let total = media.len();
    let mut files = Vec::with_capacity(total);
    progress(0, total);
    for (chunk_i, chunk) in media.chunks(64).enumerate() {
        let paths: Vec<PathBuf> = chunk.iter().map(|m| m.0.clone()).collect();
        // A failure here means ExifTool itself is broken or missing. Stop rather than
        // import everything as "Unknown Camera" with file dates (and different duplicate keys).
        let json = exif.read_json(&paths, meta::TAGS).context("Couldn't read the photos' metadata (ExifTool failed)")?;
        let by_src: HashMap<String, &serde_json::Value> = json
            .iter()
            .filter_map(|v| Some((norm(v.get("SourceFile")?.as_str()?), v)))
            .collect();
        for (j, (path, kind, size)) in chunk.iter().enumerate() {
            let idx = chunk_i * 64 + j;
            let mut m = by_src
                .get(&norm(&path.to_string_lossy()))
                .map(|v| Meta::from_exif(v))
                .unwrap_or_default();
            if *kind == Kind::Video && (m.model.is_none() || m.captured_at.is_none()) {
                for sc in &attached[idx] {
                    if sc.extension().map(|e| e.eq_ignore_ascii_case("xml")).unwrap_or(false) {
                        if let Ok(xml) = std::fs::read_to_string(sc) {
                            m.merge_missing(&sony_clip_xml(&xml));
                        }
                    }
                }
            }
            if m.captured_at.is_none() {
                m.captured_at = meta::file_mtime(path);
            }
            let camera_folder = meta::camera_folder(m.make.as_deref(), m.model.as_deref());
            let camera_label = meta::camera_label(m.make.as_deref(), m.model.as_deref());
            let mut f = SourceFile {
                path: path.clone(),
                kind: *kind,
                size: *size,
                meta: m,
                camera_folder,
                camera_label,
                sidecars: std::mem::take(&mut attached[idx]),
                key: String::new(),
                already: None,
            };
            f.key = ledger::key(&f.file_name(), f.size, f.captured_iso().as_deref());
            // Deleted rejects count as imported: you already decided against them.
            f.already = ledger.get(&f.key)?.filter(|s| s.trashed || still_exists(s));
            files.push(f);
        }
        progress(files.len(), total);
    }
    Ok(Scan { source: source.to_path_buf(), files })
}

/// A ledger entry only counts if the imported file is still on disk.
fn still_exists(s: &Seen) -> bool {
    let root = Path::new(&s.project_root);
    if !Project::is_project(root) {
        return false;
    }
    let rel: PathBuf = s.rel_path.split('/').collect();
    root.join(&rel).exists() || root.join(REJECTED_DIR).join(&rel).exists()
}

/// Sony clip metadata (`C0001M01.XML`):
/// `<Device manufacturer="Sony" modelName="ILCE-7M4" serialNo="…"/>`,
/// `<CreationDate value="2023-10-19T16:13:48+02:00"/>`.
fn sony_clip_xml(xml: &str) -> Meta {
    fn attr(xml: &str, tag: &str, name: &str) -> Option<String> {
        let start = xml.find(&format!("<{tag} "))?;
        let el = &xml[start..start + xml[start..].find('>')?];
        let key = format!("{name}=\"");
        let v = &el[el.find(&key)? + key.len()..];
        Some(v[..v.find('"')?].to_string()).filter(|s| !s.is_empty())
    }
    Meta {
        make: attr(xml, "Device", "manufacturer"),
        model: attr(xml, "Device", "modelName"),
        serial: attr(xml, "Device", "serialNo"),
        captured_at: attr(xml, "CreationDate", "value").and_then(|d| meta::parse_date(&d)),
        ..Default::default()
    }
}

fn stem_lower(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase()
}

/// exiftool echoes paths with forward slashes; normalise for lookups.
fn norm(p: &str) -> String {
    p.replace('\\', "/").to_lowercase()
}

pub fn eject(mount: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let drive = mount.trim_end_matches(['\\', '/']);
        // It's pasted into a PowerShell command, so only ever accept a bare drive letter.
        let b = drive.as_bytes();
        anyhow::ensure!(b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':', "{mount} isn't a card drive");
        let script = format!(
            "$d = (New-Object -ComObject Shell.Application).Namespace(17).ParseName('{drive}'); if ($d) {{ $d.InvokeVerb('Eject') }} else {{ exit 1 }}"
        );
        let st = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .status()?;
        anyhow::ensure!(st.success(), "Windows could not eject {drive}");
        // InvokeVerb returns before the volume is gone; wait briefly for it.
        for _ in 0..20 {
            if !Path::new(mount).exists() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        anyhow::bail!("The card is still in use. Close any window showing it and try again.")
    }
    #[cfg(target_os = "macos")]
    {
        anyhow::ensure!(mount.starts_with("/Volumes/"), "{mount} isn't a card");
        let st = std::process::Command::new("diskutil").args(["eject", mount]).status()?;
        anyhow::ensure!(st.success(), "macOS could not eject the card (is a file on it still open?)");
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let st = std::process::Command::new("umount").arg(mount).status()?;
        anyhow::ensure!(st.success(), "could not unmount {mount}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes() {
        assert!(on_volume(r"E:\Photos\Lib", r"E:\"));
        assert_eq!(on_volume(r"e:\", r"E:\"), cfg!(windows));
        assert!(!on_volume(r"F:\Photos", r"E:\"));
        assert!(on_volume("/Volumes/Backup/Safelight", "/Volumes/Backup"));
        assert!(!on_volume("/Volumes/BackupTwo", "/Volumes/Backup"));
        assert!(!on_volume("", "/Volumes/Backup"));
    }

    /// Runs against the sample cards in `test-fixtures/` (not in git); skipped if absent.
    #[test]
    fn scans_fixture_cards() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-fixtures");
        if !root.exists() {
            return;
        }
        let exif = ExifTool::new(crate::exiftool::locate(None), 1);
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&dir.path().join("l.sqlite")).unwrap();

        let canon = scan(&root.join("card_canon"), &exif, &ledger, |_, _| {}).unwrap();
        let s = canon.summary("canon");
        assert_eq!((s.photos, s.videos), (4, 0));
        assert!(s.cameras.contains(&"Canon EOS R5".to_string()), "{:?}", s.cameras);
        assert!(canon.files.iter().all(|f| f.meta.captured_at.is_some()));

        let sony = scan(&root.join("card_sony"), &exif, &ledger, |_, _| {}).unwrap();
        let s = sony.summary("sony");
        assert_eq!((s.photos, s.videos), (3, 1));
        let clip = sony.files.iter().find(|f| f.kind == Kind::Video).unwrap();
        assert_eq!(clip.sidecars.len(), 1, "XML sidecar attached to C0001.MP4");
        assert_eq!(clip.camera_folder, "Sony-ILCE-7M4", "camera model taken from the XML");
        assert!(sony.files.iter().any(|f| f.camera_folder == "Sony-ILCE-7M3"));
    }
}
