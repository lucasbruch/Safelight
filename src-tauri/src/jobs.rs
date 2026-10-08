//! Background work after a file lands: preview → AI for photos, poster →
//! playback proxy for videos. Runs on small worker pools so culling can start
//! while the import is still copying.

use crate::ai::Analyzer;
use crate::exiftool::ExifTool;
use crate::meta::Kind;
use crate::project::{Item, Project, VideoInfo};
use crossbeam_channel::{select_biased, unbounded, Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ItemEvent<'a> {
    project_root: String,
    item: &'a Item,
}

pub fn emit_item(app: &AppHandle, event: &str, project: &Project, item: &Item) {
    let _ = app.emit(event, ItemEvent { project_root: project.root.to_string_lossy().into_owned(), item });
}

enum Job {
    Preview(Arc<Project>, i64),
    Analyze(Arc<Project>, i64),
    Proxy(Arc<Project>, i64),
}

pub struct Jobs {
    preview_tx: Sender<Job>,
    ai_tx: Sender<Job>,
    proxy_tx: Sender<Job>,
    /// Proxies the user is waiting for (they opened the clip) jump the queue.
    proxy_urgent_tx: Sender<Job>,
    /// Jobs queued or running, so the same photo is never processed twice at once.
    inflight: Arc<Mutex<HashSet<(char, PathBuf, i64)>>>,
}

fn job_key(kind: char, p: &Project, id: i64) -> (char, PathBuf, i64) {
    (kind, p.root.clone(), id)
}

impl Jobs {
    pub fn start(app: AppHandle, exif: Arc<ExifTool>, analyzer: Arc<Analyzer>) -> Self {
        let (preview_tx, preview_rx) = unbounded::<Job>();
        let (ai_tx, ai_rx) = unbounded::<Job>();
        let (proxy_tx, proxy_rx) = unbounded::<Job>();
        let (proxy_urgent_tx, proxy_urgent_rx) = unbounded::<Job>();
        let inflight: Arc<Mutex<HashSet<(char, PathBuf, i64)>>> = Arc::default();
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        for i in 0..(cpus / 2).clamp(2, 6) {
            let ctx = PreviewCtx {
                app: app.clone(),
                exif: exif.clone(),
                ai_tx: ai_tx.clone(),
                proxy_tx: proxy_tx.clone(),
                analyzer: analyzer.clone(),
                inflight: inflight.clone(),
            };
            let rx = preview_rx.clone();
            std::thread::Builder::new()
                .name(format!("preview-{i}"))
                .spawn(move || preview_worker(ctx, rx))
                .expect("spawn preview worker");
        }
        // AI models are internally multi-threaded; one worker keeps the UI responsive.
        {
            let (app, inflight) = (app.clone(), inflight.clone());
            std::thread::Builder::new()
                .name("ai".into())
                .spawn(move || ai_worker(app, ai_rx, analyzer, inflight))
                .expect("spawn ai worker");
        }
        // Proxies decode 4K HEVC: one at a time so culling stays smooth.
        {
            let (app, inflight) = (app.clone(), inflight.clone());
            std::thread::Builder::new()
                .name("proxy".into())
                .spawn(move || proxy_worker(app, proxy_urgent_rx, proxy_rx, inflight))
                .expect("spawn proxy worker");
        }
        Self { preview_tx, ai_tx, proxy_tx, proxy_urgent_tx, inflight }
    }

    pub fn enqueue_preview(&self, project: Arc<Project>, id: i64) {
        if self.inflight.lock().insert(job_key('p', &project, id)) {
            let _ = self.preview_tx.send(Job::Preview(project, id));
        }
    }

    pub fn enqueue_ai(&self, project: Arc<Project>, id: i64) {
        if self.inflight.lock().insert(job_key('a', &project, id)) {
            let _ = self.ai_tx.send(Job::Analyze(project, id));
        }
    }

    /// Makes a clip's playback copy next in line (the user is looking at it).
    /// If it's also queued normally, the worker skips whichever copy comes second.
    pub fn urgent_proxy(&self, project: Arc<Project>, id: i64) {
        self.inflight.lock().insert(job_key('v', &project, id));
        let _ = self.proxy_urgent_tx.send(Job::Proxy(project, id));
    }

    fn enqueue_proxy(&self, project: Arc<Project>, id: i64) {
        if self.inflight.lock().insert(job_key('v', &project, id)) {
            let _ = self.proxy_tx.send(Job::Proxy(project, id));
        }
    }

    /// Re-queues anything left unfinished (app closed mid-import, AI turned on later…).
    pub fn resume(&self, project: &Arc<Project>, ai_enabled: bool) {
        if let Ok(items) = project.items() {
            for it in items {
                let pending_proxy = it.video.as_ref().map(|v| v.playback == "pending").unwrap_or(false);
                let stale_video = it.kind == Kind::Video && it.video.as_ref().map(|v| v.rev < VideoInfo::REV).unwrap_or(true);
                if it.preview_state != 1 || stale_video {
                    self.enqueue_preview(project.clone(), it.id);
                } else if pending_proxy {
                    self.enqueue_proxy(project.clone(), it.id);
                } else if ai_enabled && it.kind == Kind::Photo && it.preview_state == 1 && it.ai_state == 0 {
                    self.enqueue_ai(project.clone(), it.id);
                }
            }
        }
    }
}

type Inflight = Arc<Mutex<HashSet<(char, PathBuf, i64)>>>;

/// Runs one job, so a panic on one bad file fails that job instead of killing the worker.
fn guarded(what: &str, id: i64, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        log::error!("{what} crashed on item {id}; skipped");
    }
}

/// Clears a job's in-flight mark once it's finished, however it finishes.
struct Done<'a>(&'a Inflight, (char, PathBuf, i64));
impl Drop for Done<'_> {
    fn drop(&mut self) {
        self.0.lock().remove(&self.1);
    }
}

/// What a preview worker needs to hand work on.
struct PreviewCtx {
    app: AppHandle,
    exif: Arc<ExifTool>,
    ai_tx: Sender<Job>,
    proxy_tx: Sender<Job>,
    analyzer: Arc<Analyzer>,
    inflight: Inflight,
}

fn preview_worker(ctx: PreviewCtx, rx: Receiver<Job>) {
    while let Ok(Job::Preview(project, id)) = rx.recv() {
        let _done = Done(&ctx.inflight, job_key('p', &project, id));
        guarded("preview", id, || preview_one(&ctx, project, id));
    }
}

fn preview_one(ctx: &PreviewCtx, project: Arc<Project>, id: i64) {
    let Ok(item) = project.item(id) else { return };
    let state = if item.kind == Kind::Video {
        match video_poster(&ctx.exif, &project, &item) {
            Ok(info) => {
                let pending = info.playback == "pending";
                let _ = project.set_video(id, &info);
                if pending && ctx.inflight.lock().insert(job_key('v', &project, id)) {
                    let _ = ctx.proxy_tx.send(Job::Proxy(project.clone(), id));
                }
                1
            }
            Err(e) => {
                log::warn!("video preview failed for {}: {e:#}", item.file_name);
                let _ = project.set_video(id, &VideoInfo { playback: "failed".into(), ..Default::default() });
                2
            }
        }
    } else {
        let src = project.abs(&item.rel_path, item.moved_to_rejected);
        match crate::preview::build(&ctx.exif, &project, id, &src, item.meta.orientation) {
            Ok(()) => 1,
            Err(e) => {
                log::warn!("preview failed for {}: {e:#}", item.file_name);
                2
            }
        }
    };
    let _ = project.set_preview_state(id, state);
    if let Ok(item) = project.item(id) {
        emit_item(&ctx.app, "item-updated", &project, &item);
    }
    if state == 1 && item.kind == Kind::Photo && ctx.analyzer.enabled() && ctx.inflight.lock().insert(job_key('a', &project, id)) {
        let _ = ctx.ai_tx.send(Job::Analyze(project, id));
    }
}

/// Poster frame (through the log viewing LUT if needed) and a playback decision.
fn video_poster(exif: &ExifTool, project: &Project, item: &Item) -> anyhow::Result<VideoInfo> {
    let src = project.abs(&item.rel_path, item.moved_to_rejected);
    let tags = exif.read_json_full(std::slice::from_ref(&src), &["CanonLogVersion", "ColorSpace2", "Duration"]).unwrap_or_default();
    let tag = |k: &str| {
        tags.first().and_then(|v| v.get(k)).and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    };
    let sony_xml = project
        .sidecars(item.id)
        .unwrap_or_default()
        .iter()
        .filter(|r| r.to_ascii_lowercase().ends_with(".xml"))
        .find_map(|r| std::fs::read_to_string(project.abs(r, item.moved_to_rejected)).ok());
    let log = crate::video::detect_log(tag("CanonLogVersion").as_deref(), tag("ColorSpace2").as_deref(), sony_xml.as_deref());
    let duration = item.meta.duration.or_else(|| tags.first().and_then(|v| v.get("Duration")).and_then(|v| v.as_f64()));

    let frame = project.cache_dir().join(format!("{}_frame.jpg", item.id));
    crate::video::poster(&src, &frame, &project.lut_dir(), log, duration)?;
    let img = image::open(&frame)?;
    let _ = std::fs::remove_file(&frame);
    crate::preview::save_previews(project, item.id, img)?;

    // (Re)making the poster means the old playback copy may be stale: rebuild it.
    let _ = std::fs::remove_file(project.proxy_path(item.id));
    // Log footage always gets a proxy so it plays with the viewing LUT applied.
    let playback = if log.is_none() && crate::video::directly_playable(&src).unwrap_or(false) {
        "original"
    } else {
        "pending"
    };
    Ok(VideoInfo {
        log: log.map(|l| l.label().to_string()),
        gamut: log.map(|l| l.gamut_name()),
        playback: playback.into(),
        rev: VideoInfo::REV,
    })
}

fn proxy_worker(app: AppHandle, urgent: Receiver<Job>, normal: Receiver<Job>, inflight: Inflight) {
    loop {
        let job = select_biased! {
            recv(urgent) -> j => j,
            recv(normal) -> j => j,
        };
        let Ok(Job::Proxy(project, id)) = job else { break };
        let _done = Done(&inflight, job_key('v', &project, id));
        guarded("playback copy", id, || proxy_one(&app, &project, id));
    }
}

fn proxy_one(app: &AppHandle, project: &Arc<Project>, id: i64) {
    let Ok(item) = project.item(id) else { return };
    let Some(mut info) = item.video.clone() else { return };
    if info.playback != "pending" {
        return; // already made (it was queued twice)
    }
    let src = project.abs(&item.rel_path, item.moved_to_rejected);
    let log = info.log.as_deref().and_then(|l| crate::video::LogProfile::parse(l, info.gamut.as_deref()));
    info.playback = match crate::video::proxy(&src, &project.proxy_path(id), &project.lut_dir(), log) {
        Ok(()) => "proxy".into(),
        Err(e) => {
            log::warn!("proxy failed for {}: {e:#}", item.file_name);
            "failed".into()
        }
    };
    let _ = project.set_video(id, &info);
    if let Ok(item) = project.item(id) {
        emit_item(app, "item-updated", project, &item);
    }
}

/// How long the AI queue must stay empty before the cross-photo passes run.
const SETTLE: Duration = Duration::from_secs(2);
/// During a long import the queue is rarely idle that long; refresh at least this often.
const REFRESH: Duration = Duration::from_secs(30);

fn ai_worker(app: AppHandle, rx: Receiver<Job>, analyzer: Arc<Analyzer>, inflight: Inflight) {
    // Projects with new results since their last cross-photo pass (bursts, people,
    // suggestions). That pass reads and may rewrite the whole project, so it's
    // batched instead of run after every photo.
    let mut dirty: HashMap<PathBuf, Arc<Project>> = HashMap::new();
    let mut last_pass = Instant::now();
    let passes = |dirty: &mut HashMap<PathBuf, Arc<Project>>, last_pass: &mut Instant| {
        for (_, p) in dirty.drain() {
            guarded("project pass", 0, || analyzer.finish_project(&app, &p));
        }
        *last_pass = Instant::now();
    };
    loop {
        let job = if dirty.is_empty() {
            match rx.recv() {
                Ok(j) => j,
                Err(_) => break,
            }
        } else {
            match rx.recv_timeout(SETTLE) {
                Ok(j) => j,
                Err(RecvTimeoutError::Timeout) => {
                    passes(&mut dirty, &mut last_pass);
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        };
        let Job::Analyze(project, id) = job else { continue };
        {
            let _done = Done(&inflight, job_key('a', &project, id));
            if !analyzer.enabled() {
                continue;
            }
            guarded("analysis", id, || analyzer.analyze(&app, &project, id));
        }
        dirty.insert(project.root.clone(), project);
        if rx.is_empty() && last_pass.elapsed() >= REFRESH {
            passes(&mut dirty, &mut last_pass);
        }
    }
}
