//! Everything the UI can ask the backend to do. Anything that touches the disk
//! is `async` and runs on a blocking pool: Tauri runs plain commands on the main
//! thread, where a slow drive would freeze the window.

use crate::card::{self, Card, ScanSummary};
use crate::handoff;
use crate::ingest::{self, ImportRequest, Progress};
use crate::meta::Kind;
use crate::project::{Item, Project, Summary};
use crate::settings::Settings;
use crate::state::AppState;
use crate::xmp::{self, XmpState};
use anyhow::Context;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    // anyhow's alternate format includes the context chain.
    format!("{e:#}")
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?.map_err(err)
}

type St<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn get_settings(state: St) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn save_settings(state: St, settings: Settings) -> Res<()> {
    settings.save(&state.data_dir).map_err(err)?;
    state.analyzer().set_enabled(settings.ai_enabled);
    *state.settings.lock() = settings;
    Ok(())
}

#[tauri::command]
pub async fn list_cards(state: St<'_>) -> Res<Vec<Card>> {
    let exclude = state.card_exclusions();
    blocking(move || Ok(card::detect(&exclude))).await
}

#[tauri::command]
pub async fn scan_source(app: AppHandle, state: St<'_>, source: String, label: Option<String>) -> Res<ScanSummary> {
    let st = state.inner().clone();
    blocking(move || {
        let path = PathBuf::from(&source);
        anyhow::ensure!(path.is_dir(), "{source} is not a folder or is no longer connected.");
        let scan = card::scan(&path, &st.exif, &st.ledger, |done, total| {
            let _ = app.emit("scan-progress", serde_json::json!({ "source": source, "done": done, "total": total }));
        })?;
        let label = label.unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| source.clone()));
        let summary = scan.summary(&label);
        st.scans.lock().insert(source.clone(), Arc::new(scan));
        Ok(summary)
    })
    .await
}

#[tauri::command]
pub fn start_import(app: AppHandle, state: St, request: ImportRequest) -> Res<String> {
    // Held until the job is registered, so two quick clicks can't start two imports.
    let mut slot = state.import.lock();
    if slot.is_some() {
        return Err("An import is already running.".into());
    }
    let scan = state
        .scans
        .lock()
        .get(&request.source)
        .cloned()
        .ok_or("Please scan the card again.")?;
    let root = ingest::resolve_root(&state, &scan, &request).map_err(err)?;
    let project = state.project(&root).map_err(err)?;
    remember_project(&state, &root);
    let root_s = root.to_string_lossy().into_owned();
    *slot = Some(Progress {
        project_root: root_s.clone(),
        project_name: project.name(),
        ..Default::default()
    });
    drop(slot);
    // Reset here, not in the thread, so a "Stop" pressed right away still counts.
    state.cancel_import.store(false, Ordering::Relaxed);
    let st = state.inner().clone();
    let spawned = std::thread::Builder::new().name("import".into()).spawn(move || {
        let run = std::panic::AssertUnwindSafe(|| ingest::run(app.clone(), st.clone(), scan, project, request));
        if std::panic::catch_unwind(run).is_err() {
            *st.import.lock() = None;
            let _ = app.emit("import-error", "The import stopped unexpectedly. Files copied so far are safe; import the card again to finish.");
        }
    });
    if let Err(e) = spawned {
        *state.import.lock() = None;
        return Err(err(e));
    }
    Ok(root_s)
}

#[tauri::command]
pub fn cancel_import(state: St) {
    state.cancel_import.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn import_status(state: St) -> Option<Progress> {
    state.import.lock().clone()
}

fn remember_project(state: &AppState, root: &Path) {
    let mut s = state.settings.lock();
    let r = root.to_string_lossy().into_owned();
    let lib = PathBuf::from(&s.library_root);
    if root.parent() != Some(lib.as_path()) && !s.other_projects.contains(&r) {
        s.other_projects.push(r);
        let _ = s.save(&state.data_dir);
    }
}

#[tauri::command]
pub async fn list_projects(state: St<'_>) -> Res<Vec<Summary>> {
    let st = state.inner().clone();
    blocking(move || {
        let s = st.settings.lock().clone();
        let mut roots: Vec<PathBuf> = std::fs::read_dir(&s.library_root)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| Project::is_project(p)).collect())
            .unwrap_or_default();
        roots.extend(s.other_projects.iter().map(PathBuf::from).filter(|p| Project::is_project(p)));
        roots.sort();
        roots.dedup();
        let mut out: Vec<Summary> = roots
            .iter()
            .filter_map(|r| st.existing_project(r).ok()?.summary().ok())
            .collect();
        out.sort_by(|a, b| b.last_date.cmp(&a.last_date).then(b.name.cmp(&a.name)));
        Ok(out)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectData {
    root: String,
    name: String,
    items: Vec<Item>,
}

#[tauri::command]
pub async fn open_project(state: St<'_>, root: String) -> Res<ProjectData> {
    let st = state.inner().clone();
    blocking(move || {
        let path = PathBuf::from(&root);
        anyhow::ensure!(path.is_dir(), "This project folder can't be found. Is its drive connected?");
        let p = st.existing_project(&path)?;
        remember_project(&st, &path);
        let ai = st.settings.lock().ai_enabled;
        st.jobs.resume(&p, ai);
        Ok(ProjectData { root, name: p.name(), items: p.items()? })
    })
    .await
}

fn write_xmp(state: &AppState, p: &Project, item: &Item) {
    let media = p.abs(&item.rel_path, item.moved_to_rejected);
    if item.kind != Kind::Photo || !xmp::wants_sidecar(&media) {
        return;
    }
    let s = state.settings.lock();
    state.xmp.write(
        xmp::sidecar_for(&media),
        XmpState {
            rating: item.rating,
            flag: item.flag,
            artist: s.artist.clone(),
            copyright: s.copyright.clone(),
            tags: item.ai.as_ref().map(|a| a.tags.clone()).unwrap_or_default(),
        },
    );
}

#[tauri::command]
pub async fn set_rating(state: St<'_>, root: String, ids: Vec<i64>, rating: i64) -> Res<Vec<Item>> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        p.set_rating(&ids, rating)?;
        let items: Vec<Item> = ids.iter().filter_map(|id| p.item(*id).ok()).collect();
        for it in &items {
            write_xmp(&st, &p, it);
        }
        Ok(items)
    })
    .await
}

#[tauri::command]
pub async fn set_flag(state: St<'_>, root: String, ids: Vec<i64>, flag: i64) -> Res<Vec<Item>> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        let flag = flag.clamp(-1, 1);
        let mut ok = vec![];
        let mut failed = vec![];
        let mut flushed = false;
        for id in ids {
            let Ok(it) = p.item(id) else { continue };
            // Un-rejecting a photo that was already moved away brings its files back
            // first. The flag only changes once they're back, so a failed move can't
            // leave a photo you meant to keep inside `_Rejected/`.
            if it.moved_to_rejected && flag != -1 {
                if !flushed {
                    st.xmp.flush();
                    flushed = true;
                }
                if let Err(e) = move_item(&p, &it, false) {
                    failed.push(format!("{}: {e:#}", it.file_name));
                    continue;
                }
            }
            ok.push(id);
        }
        p.set_flag(&ok, flag)?;
        let items: Vec<Item> = ok.iter().filter_map(|id| p.item(*id).ok()).collect();
        for it in &items {
            write_xmp(&st, &p, it);
        }
        anyhow::ensure!(failed.is_empty(), "Couldn't bring back {} from _Rejected: {}", failed.len(), failed.join("; "));
        Ok(items)
    })
    .await
}

/// An item's files, relative to the project: the media, its sidecars and (for RAWs) the XMP Safelight writes.
fn item_rels(p: &Project, it: &Item) -> anyhow::Result<Vec<String>> {
    let mut rels = vec![it.rel_path.clone()];
    rels.extend(p.sidecars(it.id)?);
    // Only RAWs own an XMP; a JPEG must never take its RAW twin's XMP along.
    if xmp::wants_sidecar(Path::new(&it.rel_path)) {
        let xmp_rel = Path::new(&it.rel_path).with_extension("xmp").to_string_lossy().replace('\\', "/");
        if !rels.contains(&xmp_rel) {
            rels.push(xmp_rel);
        }
    }
    Ok(rels)
}

/// Moves an item's files between its folder and `_Rejected/`. All or nothing:
/// if any file can't move, the ones already moved go back and the database is untouched.
fn move_item(p: &Project, it: &Item, to_rejected: bool) -> anyhow::Result<()> {
    let rels = item_rels(p, it)?;
    let mut done: Vec<(PathBuf, PathBuf)> = vec![];
    let res = (|| -> anyhow::Result<()> {
        for rel in &rels {
            let from = p.abs(rel, !to_rejected);
            let to = p.abs(rel, to_rejected);
            if !from.exists() {
                continue;
            }
            anyhow::ensure!(!to.exists(), "{} already exists", to.display());
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&from, &to).with_context(|| format!("moving {}", from.display()))?;
            done.push((from, to));
        }
        p.set_moved(it.id, to_rejected)
    })();
    if let Err(e) = res {
        for (from, to) in done.iter().rev() {
            let _ = std::fs::rename(to, from);
        }
        return Err(e);
    }
    for (from, _) in &done {
        remove_empty_dirs(from.parent(), &p.root);
    }
    Ok(())
}

fn remove_empty_dirs(mut dir: Option<&Path>, stop: &Path) {
    while let Some(d) = dir {
        if d == stop || !d.starts_with(stop) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    moved: usize,
    failed: Vec<String>,
}

#[tauri::command]
pub async fn move_rejects(state: St<'_>, root: String) -> Res<MoveReport> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        // Queued XMP writes must land before their files move, or they'd recreate
        // stray XMPs in the old folders.
        st.xmp.flush();
        let mut report = MoveReport { moved: 0, failed: vec![] };
        for it in p.items()?.into_iter().filter(|i| i.flag == -1 && !i.moved_to_rejected) {
            match move_item(&p, &it, true) {
                Ok(()) => report.moved += 1,
                Err(e) => report.failed.push(format!("{}: {e:#}", it.file_name)),
            }
        }
        Ok(report)
    })
    .await
}

/// Sends the moved rejects to the system trash (recoverable there).
#[tauri::command]
pub async fn trash_rejects(state: St<'_>, root: String) -> Res<usize> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        st.xmp.flush();
        // Only what is both rejected and moved away: a photo flagged as a keeper is
        // never trashed, even if its files are still in `_Rejected/`.
        let doomed: Vec<Item> = p.items()?.into_iter().filter(|i| i.moved_to_rejected && i.flag == -1).collect();
        let mut files = vec![];
        for it in &doomed {
            files.extend(item_rels(&p, it)?.iter().map(|r| p.abs(r, true)).filter(|f| f.exists()));
        }
        files.sort();
        files.dedup();
        if !files.is_empty() {
            trash::delete_all(&files)?;
        }
        p.delete_items(&doomed.iter().map(|i| i.id).collect::<Vec<_>>())?;
        for it in &doomed {
            p.remove_cache(it.id);
        }
        for f in &files {
            remove_empty_dirs(f.parent(), &p.root);
        }
        Ok(doomed.len())
    })
    .await
}

#[tauri::command]
pub async fn full_image(state: St<'_>, root: String, id: i64) -> Res<String> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        let it = p.item(id)?;
        let src = p.abs(&it.rel_path, it.moved_to_rejected);
        let path = crate::preview::full(&st.exif, &p, id, &src, it.meta.width, it.meta.orientation)?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub async fn eject_card(mount: String) -> Res<()> {
    blocking(move || {
        anyhow::ensure!(card::looks_like_card(Path::new(&mount)), "{mount} doesn't look like a camera card.");
        card::eject(&mount)
    })
    .await
}

/// The user opened a clip whose playback copy isn't ready: make it next.
#[tauri::command]
pub async fn request_proxy(state: St<'_>, root: String, id: i64) -> Res<()> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        st.jobs.urgent_proxy(p, id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn rerun_ai(state: St<'_>, root: String) -> Res<usize> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        st.analyzer().reset_models();
        let ids = p.ids_needing("ai_state", 99)?;
        for id in &ids {
            st.jobs.enqueue_ai(p.clone(), *id);
        }
        Ok(ids.len())
    })
    .await
}

#[tauri::command]
pub fn ai_status(state: St) -> crate::ai::models::Status {
    crate::ai::models::status(state.analyzer().models_dir())
}

#[tauri::command]
pub async fn download_models(app: AppHandle, state: St<'_>) -> Res<()> {
    let st = state.inner().clone();
    blocking(move || {
        crate::ai::models::download(st.analyzer().models_dir(), |done, total, name| {
            let _ = app.emit("models-progress", serde_json::json!({ "done": done, "total": total, "name": name }));
        })?;
        st.analyzer().reset_models();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn handoff_status() -> Res<handoff::Status> {
    blocking(|| Ok(handoff::status())).await
}

#[tauri::command]
pub async fn send_to_lightroom(state: St<'_>, root: String, ids: Vec<i64>) -> Res<String> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        let photos = handoff::lr_photos(&p, &ids)?;
        st.xmp.flush();
        let (artist, copyright) = {
            let s = st.settings.lock();
            (s.artist.clone(), s.copyright.clone())
        };
        handoff::lightroom(&p.name(), &photos, &artist, &copyright)
    })
    .await
}

#[tauri::command]
pub async fn send_to_resolve(state: St<'_>, root: String, ids: Vec<i64>) -> Res<String> {
    let st = state.inner().clone();
    blocking(move || {
        let p = st.existing_project(Path::new(&root))?;
        let items: Vec<Item> = ids.iter().filter_map(|id| p.item(*id).ok()).collect();
        handoff::resolve(&p, &items)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Meta;
    use crate::project::NewItem;

    fn add(p: &Project, rel: &str, sidecars: &[String]) -> Item {
        let path = p.abs(rel, false);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, rel).unwrap();
        for s in sidecars {
            std::fs::write(p.abs(s, false), s).unwrap();
        }
        let meta = Meta::default();
        let name = rel.rsplit('/').next().unwrap();
        let id = p
            .insert(&NewItem { rel_path: rel, kind: Kind::Photo, file_name: name, size: 1, hash: "h", camera: "c", meta: &meta, sidecars })
            .unwrap();
        p.item(id).unwrap()
    }

    #[test]
    fn moving_a_jpeg_leaves_its_raw_twins_xmp_alone() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let _raw = add(&p, "d/c/IMG_1.CR3", &[]);
        std::fs::write(p.abs("d/c/IMG_1.xmp", false), "raw's xmp").unwrap();
        let jpg = add(&p, "d/c/IMG_1.JPG", &[]);
        move_item(&p, &jpg, true).unwrap();
        assert!(p.abs("d/c/IMG_1.JPG", true).exists());
        assert!(p.abs("d/c/IMG_1.xmp", false).exists(), "the RAW keeps its XMP");
        assert!(p.item(jpg.id).unwrap().moved_to_rejected);
    }

    #[test]
    fn a_failed_move_is_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let it = add(&p, "d/c/C0001.MP4", &["d/c/C0001M01.XML".to_string()]);
        // Something already sits where the sidecar would go.
        std::fs::create_dir_all(p.abs("d/c", true)).unwrap();
        std::fs::write(p.abs("d/c/C0001M01.XML", true), "in the way").unwrap();
        assert!(move_item(&p, &it, true).is_err());
        assert!(p.abs("d/c/C0001.MP4", false).exists(), "media moved back");
        assert!(!p.abs("d/c/C0001.MP4", true).exists());
        assert!(!p.item(it.id).unwrap().moved_to_rejected, "database untouched");
    }

    #[test]
    fn round_trip_through_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let it = add(&p, "d/c/IMG_2.CR3", &["d/c/IMG_2.THM".to_string()]);
        move_item(&p, &it, true).unwrap();
        assert!(!p.abs("d", false).exists(), "empty folders are tidied up");
        let it = p.item(it.id).unwrap();
        move_item(&p, &it, false).unwrap();
        assert!(p.abs("d/c/IMG_2.CR3", false).exists() && p.abs("d/c/IMG_2.THM", false).exists());
        assert!(!dir.path().join(crate::project::REJECTED_DIR).exists());
    }
}
