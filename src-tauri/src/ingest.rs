//! The import job: copy + verify every new file from a scan into the project,
//! record it, and hand it to the preview/AI pipeline as soon as it lands.

use crate::card::{Scan, SourceFile};
use crate::copy;
use crate::meta::{sanitize_component, Kind};
use crate::project::{NewItem, Project};
use crate::state::AppState;
use crate::xmp;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub source: String,
    /// Import into this existing project…
    pub project_root: Option<String>,
    /// …or create a new one called `<first date>_<name>`.
    pub new_name: Option<String>,
    #[serde(default)]
    pub include_duplicates: bool,
    /// Set when importing from a detected card, so the report can offer "Eject".
    pub card_mount: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub project_root: String,
    pub project_name: String,
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
    pub skipped: usize,
    pub failed: usize,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileError {
    pub file: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub source: String,
    pub card_mount: Option<String>,
    pub project_root: String,
    pub project_name: String,
    pub started_at: String,
    pub finished_at: String,
    pub seconds: f64,
    pub copied: usize,
    pub photos: usize,
    pub videos: usize,
    pub skipped: usize,
    pub bytes: u64,
    pub failed: Vec<FileError>,
    pub backup_root: Option<String>,
    pub backup_failed: Vec<FileError>,
    pub cancelled: bool,
}

/// Picks the project folder for a request (creating nothing yet).
pub fn resolve_root(state: &AppState, scan: &Scan, req: &ImportRequest) -> Result<PathBuf> {
    if let Some(r) = &req.project_root {
        return Ok(PathBuf::from(r));
    }
    let name = req.new_name.as_deref().unwrap_or("").trim();
    anyhow::ensure!(!name.is_empty(), "Please give the project a name.");
    let first_date = scan
        .files
        .iter()
        .filter(|f| f.already.is_none() || req.include_duplicates)
        .filter_map(|f| f.meta.captured_at)
        .min()
        .unwrap_or_else(|| chrono::Local::now().naive_local())
        .format("%Y-%m-%d")
        .to_string();
    let library = PathBuf::from(&state.settings.lock().library_root);
    Ok(library.join(folder_name(name, &first_date)))
}

/// `<first date>_<name>`, unless the name already starts with a date.
fn folder_name(name: &str, first_date: &str) -> String {
    // `get` rather than slicing: byte 10 can fall inside a multi-byte character.
    let has_date = name.get(..10).is_some_and(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok());
    if has_date {
        sanitize_component(name)
    } else {
        sanitize_component(&format!("{first_date}_{name}"))
    }
}

/// A sidecar follows its media file's (possibly de-duplicated) name:
/// `C0001M01.XML` for `C0001.MP4` becomes `C0001_1M01.XML` for `C0001_1.MP4`.
fn renamed_sidecar(sc_name: &str, orig_stem: &str, new_stem: &str) -> String {
    match sc_name.get(..orig_stem.len()) {
        Some(head) if head.eq_ignore_ascii_case(orig_stem) => format!("{new_stem}{}", &sc_name[orig_stem.len()..]),
        _ => sc_name.to_string(),
    }
}

fn rel_for(f: &SourceFile, file_name: &str) -> String {
    let date = f
        .meta
        .captured_at
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Unknown-Date".into());
    match f.kind {
        Kind::Photo => format!("{date}/{}/{file_name}", f.camera_folder),
        Kind::Video => format!("{date}/{}/Video/{file_name}", f.camera_folder),
    }
}

fn rel_to_path(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// Never overwrite: `IMG_0001.CR3` → `IMG_0001_1.CR3` if the name is taken
/// (counter rollover, or a second body with the same file naming), in the
/// project or in the backup folder.
fn free_name(project: &Project, backup: Option<&Path>, f: &SourceFile) -> Result<(String, String)> {
    let orig = f.file_name();
    let stem = f.path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = f.path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut name = orig.clone();
    let mut n = 1;
    loop {
        let rel = rel_for(f, &name);
        let taken = project.rel_exists(&rel)?
            || rel_to_path(&project.root, &rel).exists()
            || backup.is_some_and(|b| rel_to_path(b, &rel).exists());
        if !taken {
            return Ok((rel, name));
        }
        name = format!("{stem}_{n}{ext}");
        n += 1;
    }
}

pub fn run(app: AppHandle, state: Arc<AppState>, scan: Arc<Scan>, project: Arc<Project>, req: ImportRequest) {
    let started = chrono::Local::now();
    let t0 = Instant::now();
    let settings = state.settings.lock().clone();
    let backup_root = Some(settings.backup_root.trim().to_string())
        .filter(|b| !b.is_empty())
        .map(|b| PathBuf::from(b).join(project.name()));
    // Reset by `start_import` before this thread starts, so an early "Stop" isn't lost.
    let cancel = state.cancel_import.clone();

    let mut todo: Vec<&SourceFile> = scan
        .files
        .iter()
        .filter(|f| f.already.is_none() || req.include_duplicates)
        .collect();
    todo.sort_by(|a, b| a.meta.captured_at.cmp(&b.meta.captured_at).then(a.path.cmp(&b.path)));

    let mut p = Progress {
        project_root: project.root.to_string_lossy().into_owned(),
        project_name: project.name(),
        total: todo.len(),
        skipped: scan.files.len() - todo.len(),
        bytes_total: todo.iter().map(|f| f.size + sidecar_bytes(f)).sum(),
        ..Default::default()
    };
    let import_id = project.begin_import(&req.source).ok();
    let mut failed = vec![];
    let mut backup_failed = vec![];
    let (mut photos, mut videos) = (0, 0);
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let emit = |p: &Progress, state: &AppState| {
        *state.import.lock() = Some(p.clone());
        let _ = app.emit("import-progress", p);
    };
    emit(&p, &state);

    for f in todo {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        p.current = f.file_name();
        let res = (|| -> Result<Option<i64>> {
            let (rel, name) = free_name(&project, backup_root.as_deref(), f)?;
            let mut dests = vec![rel_to_path(&project.root, &rel)];
            if let Some(b) = &backup_root {
                dests.push(rel_to_path(b, &rel));
            }
            let out = copy::copy_verified(&f.path, &dests, &cancel, |n| {
                p.bytes_done += n;
                if last_emit.elapsed() > Duration::from_millis(150) {
                    last_emit = Instant::now();
                    emit(&p, &state);
                }
            })?;
            let mut results = out.results.into_iter();
            if let Some(Err(e)) = results.next() {
                return Err(e);
            }
            if let Some(Err(e)) = results.next() {
                backup_failed.push(FileError { file: f.file_name(), error: format!("{e:#}") });
            }

            // Sidecars follow the media file's (possibly de-duplicated) name.
            let orig_stem = f.path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let new_stem = Path::new(&name).file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let mut sidecar_rels = vec![];
            for sc in &f.sidecars {
                let sc_name = sc.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let renamed = renamed_sidecar(&sc_name, &orig_stem, &new_stem);
                let sc_rel = format!("{}/{renamed}", rel.rsplit_once('/').map(|x| x.0).unwrap_or(""));
                let mut d = vec![rel_to_path(&project.root, &sc_rel)];
                if let Some(b) = &backup_root {
                    d.push(rel_to_path(b, &sc_rel));
                }
                match copy::copy_verified(sc, &d, &cancel, |n| p.bytes_done += n) {
                    Ok(o) => {
                        let mut r = o.results.into_iter();
                        match r.next() {
                            Some(Err(e)) => failed.push(FileError { file: sc_name.clone(), error: format!("{e:#}") }),
                            _ => sidecar_rels.push(sc_rel),
                        }
                        if let Some(Err(e)) = r.next() {
                            backup_failed.push(FileError { file: sc_name.clone(), error: format!("{e:#}") });
                        }
                    }
                    Err(e) => failed.push(FileError { file: sc_name, error: format!("{e:#}") }),
                }
            }

            let id = project.insert(&NewItem {
                rel_path: &rel,
                kind: f.kind,
                file_name: &name,
                size: f.size,
                hash: &out.hash,
                camera: &f.camera_label,
                meta: &f.meta,
                sidecars: &sidecar_rels,
            })?;
            state.ledger.record(&f.key, &p.project_root, &rel, &out.hash)?;
            let media = rel_to_path(&project.root, &rel);
            if xmp::wants_sidecar(&media) && (!settings.artist.trim().is_empty() || !settings.copyright.trim().is_empty()) {
                state.xmp.write(
                    xmp::sidecar_for(&media),
                    xmp::XmpState {
                        artist: settings.artist.clone(),
                        copyright: settings.copyright.clone(),
                        ..Default::default()
                    },
                );
            }
            Ok(Some(id))
        })();

        match res {
            Ok(Some(id)) => {
                match f.kind {
                    Kind::Photo => photos += 1,
                    Kind::Video => videos += 1,
                }
                if let Ok(item) = project.item(id) {
                    crate::jobs::emit_item(&app, "item-added", &project, &item);
                }
                state.jobs.enqueue_preview(project.clone(), id);
            }
            Ok(None) => {}
            Err(e) => {
                if !cancel.load(Ordering::Relaxed) {
                    p.failed += 1;
                    failed.push(FileError { file: f.file_name(), error: format!("{e:#}") });
                }
            }
        }
        p.done += 1;
        emit(&p, &state);
    }

    let cancelled = cancel.load(Ordering::Relaxed);
    let report = Report {
        source: req.source.clone(),
        card_mount: req.card_mount.clone(),
        project_root: p.project_root.clone(),
        project_name: p.project_name.clone(),
        started_at: started.to_rfc3339(),
        finished_at: chrono::Local::now().to_rfc3339(),
        seconds: t0.elapsed().as_secs_f64(),
        copied: photos + videos,
        photos,
        videos,
        skipped: p.skipped,
        bytes: p.bytes_done,
        failed,
        backup_root: backup_root.map(|b| b.to_string_lossy().into_owned()),
        backup_failed,
        cancelled,
    };
    if let Some(id) = import_id {
        let _ = project.finish_import(id, &serde_json::to_value(&report).unwrap_or_default());
    }
    p.finished = true;
    p.current.clear();
    emit(&p, &state);
    *state.import.lock() = None;
    let _ = app.emit("import-finished", &report);
}

fn sidecar_bytes(f: &SourceFile) -> u64 {
    f.sidecars.iter().filter_map(|s| std::fs::metadata(s).ok()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_with_any_characters() {
        assert_eq!(folder_name("Wedding Smith", "2026-05-01"), "2026-05-01_Wedding-Smith");
        assert_eq!(folder_name("2026-04-30 Party", "2026-05-01"), "2026-04-30-Party");
        // Byte 10 falls inside a character in both of these; this used to crash the app.
        assert_eq!(folder_name("結婚式の写真", "2026-05-01"), "2026-05-01_結婚式の写真");
        assert_eq!(folder_name("Hochzeitsüberraschung", "2026-05-01"), "2026-05-01_Hochzeitsüberraschung");
    }

    #[test]
    fn sidecars_follow_the_new_name() {
        assert_eq!(renamed_sidecar("C0001M01.XML", "C0001", "C0001_1"), "C0001_1M01.XML");
        assert_eq!(renamed_sidecar("img_0001.thm", "IMG_0001", "IMG_0001_2"), "IMG_0001_2.thm");
        assert_eq!(renamed_sidecar("ÄB.xml", "Ä", "X"), "XB.xml");
        assert_eq!(renamed_sidecar("ab", "Äbc", "X"), "ab", "no panic when the stem doesn't fit");
    }
}
