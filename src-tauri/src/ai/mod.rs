//! Local, offline culling assistant. Per photo: sharpness, motion blur,
//! exposure, horizon, faces/blinks, CLIP embedding (tags, aesthetics). Across
//! the project: burst grouping, best-of-burst, people clusters, suggestions.
//! It only ever *suggests*; it never changes a rating or moves a file.

pub mod cv;
pub mod models;

use crate::project::{Ai, Item, Project};
use image::imageops::FilterType;
use models::Models;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub const SHARP: f64 = 45.0;
pub const SOFT: f64 = 32.0;

pub struct Analyzer {
    enabled: AtomicBool,
    models_dir: PathBuf,
    models: Mutex<Option<Arc<Models>>>,
    models_failed: AtomicBool,
    runtime: Option<PathBuf>,
}

impl Analyzer {
    pub fn new(enabled: bool, models_dir: PathBuf, runtime: Option<PathBuf>) -> Self {
        Self {
            runtime,
            enabled: AtomicBool::new(enabled),
            models_dir,
            models: Mutex::new(None),
            models_failed: AtomicBool::new(false),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }

    /// Loads the ONNX models once they're downloaded; the CV-only checks work without them.
    fn models(&self) -> Option<Arc<Models>> {
        let mut g = self.models.lock();
        if g.is_none() && !self.models_failed.load(Ordering::Relaxed) && models::available(&self.models_dir) {
            match models::init_runtime(self.runtime.as_deref()).and_then(|()| Models::load(&self.models_dir)) {
                Ok(m) => *g = Some(Arc::new(m)),
                Err(e) => {
                    eprintln!("[safelight] AI models failed to load: {e:#}");
                    self.models_failed.store(true, Ordering::Relaxed);
                }
            }
        }
        g.clone()
    }

    /// Forget a failed load so a fresh download gets another chance.
    pub fn reset_models(&self) {
        *self.models.lock() = None;
        self.models_failed.store(false, Ordering::Relaxed);
    }

    pub fn analyze(&self, app: &AppHandle, project: &Arc<Project>, id: i64) {
        let result = (|| -> anyhow::Result<()> {
            let img = image::open(project.preview_path(id))?.to_rgb8();
            let (w, h) = img.dimensions();
            let scale = cv::WORK_MAX as f32 / w.max(h) as f32;
            let work = if scale < 1.0 {
                image::imageops::resize(&img, (w as f32 * scale) as u32, (h as f32 * scale) as u32, FilterType::Triangle)
            } else {
                img.clone()
            };

            let mut ai = Ai::default();
            let mut face_embs: Vec<Vec<f32>> = vec![];
            let mut roi = None;
            let models = self.models();
            if let Some(m) = &models {
                let faces = m.faces(&img).unwrap_or_default();
                ai.faces = Some(faces.len() as i64);
                ai.eyes_closed = Some(faces.iter().filter(|f| f.eyes_closed).count() as i64);
                // Focus is judged on the biggest face, if any.
                if let Some(big) = faces.iter().max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h))) {
                    roi = Some(cv::Roi { x: big.x, y: big.y, w: big.w, h: big.h });
                }
                face_embs = faces.into_iter().filter_map(|f| f.embedding).collect();
                if let Ok(clip) = m.clip(&img) {
                    ai.aesthetic = m.aesthetic(&clip);
                    ai.tags = m.tags(&clip);
                    let _ = project.set_embeddings(id, Some(&clip), None);
                }
            }
            if !face_embs.is_empty() {
                let _ = project.set_embeddings(id, None, Some(&face_embs));
            }

            let m = cv::measure(&work, roi);
            let eff = m.roi_sharpness.map(|r| r.max(m.sharpness * 0.85).min(r + 10.0)).unwrap_or(m.sharpness);
            ai.sharpness = Some(round1(eff));
            ai.blur_level = Some(if eff >= SHARP { 0 } else if eff >= SOFT { 1 } else { 2 });
            ai.motion_blur = Some(m.coherence > 0.5 && eff < SHARP);
            ai.over_exposed = Some(round3(m.over_exposed));
            ai.under_exposed = Some(round3(m.under_exposed));
            ai.horizon_tilt = m.horizon_tilt.filter(|t| t.abs() >= 1.5).map(round1);
            ai.phash = Some(format!("{:016x}", m.phash));
            suggest_single(&mut ai);
            project.set_ai(id, &ai, 1)?;
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("[safelight] analysis failed for item {id}: {e:#}");
            let _ = project.set_ai(id, &Ai::default(), 2);
        }
        if let Ok(item) = project.item(id) {
            crate::jobs::emit_item(app, "item-updated", project, &item);
        }
    }

    /// Cross-photo passes (bursts, people, suggestions). The AI worker batches
    /// these; only photos whose results changed are written and sent to the UI.
    pub fn finish_project(&self, app: &AppHandle, project: &Arc<Project>) {
        let Ok(items) = project.items() else { return };
        let clip: HashMap<i64, Vec<f32>> = project
            .embeddings()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, _, e)| (id, e))
            .collect();
        let people = models::cluster_people(&project.face_embeddings().unwrap_or_default());
        let mut updated = group_bursts(&items, &clip);
        for (id, ai) in updated.iter_mut() {
            ai.people = people.get(id).cloned().unwrap_or_default();
        }
        let before: HashMap<i64, &Item> = items.iter().map(|i| (i.id, i)).collect();
        updated.retain(|(id, ai)| before.get(id).and_then(|i| i.ai.as_ref()) != Some(ai));
        if updated.is_empty() || project.set_ai_many(&updated, 1).is_err() {
            return;
        }
        let changed: Vec<Item> = updated.iter().filter_map(|(id, _)| project.item(*id).ok()).collect();
        let _ = app.emit(
            "items-updated",
            serde_json::json!({ "projectRoot": project.root.to_string_lossy(), "items": changed }),
        );
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Quality used to pick the best shot of a burst.
fn quality(ai: &Ai) -> f64 {
    let mut q = ai.sharpness.unwrap_or(0.0);
    q -= 25.0 * ai.eyes_closed.unwrap_or(0) as f64;
    q -= 40.0 * (ai.over_exposed.unwrap_or(0.0) - 0.02).max(0.0);
    q += 3.0 * ai.aesthetic.unwrap_or(5.0);
    q
}

/// Per-photo reasons and suggestion (burst context is added later).
fn suggest_single(ai: &mut Ai) {
    let mut reasons = vec![];
    let mut verdict = 0;
    match ai.blur_level {
        Some(2) => {
            reasons.push(if ai.motion_blur == Some(true) { "Motion blur" } else { "Blurry" }.to_string());
            verdict = -1;
        }
        Some(1) => reasons.push("Slightly soft".into()),
        _ => {}
    }
    if ai.eyes_closed.unwrap_or(0) > 0 {
        reasons.push(if ai.eyes_closed == Some(1) { "Eyes closed".into() } else { format!("{} people blinking", ai.eyes_closed.unwrap()) });
        verdict = -1;
    }
    if ai.over_exposed.unwrap_or(0.0) > 0.25 {
        reasons.push("Highlights blown".into());
        verdict = -1;
    } else if ai.over_exposed.unwrap_or(0.0) > 0.04 {
        reasons.push("Some highlights clipped".into());
    }
    if ai.under_exposed.unwrap_or(0.0) > 0.5 {
        reasons.push("Very dark".into());
        verdict = -1;
    }
    if let Some(t) = ai.horizon_tilt {
        reasons.push(format!("Tilted {:.1}°", t.abs()));
    }
    if verdict == 0 && ai.blur_level == Some(0) && ai.aesthetic.map(|a| a >= 5.0).unwrap_or(true) {
        verdict = 1;
    }
    ai.reasons = reasons;
    ai.suggestion = Some(verdict);
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

fn secs(ts: &Option<String>) -> Option<f64> {
    let t = ts.as_deref()?;
    let d = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.3f").ok()?;
    Some(d.and_utc().timestamp_millis() as f64 / 1000.0)
}

/// Consecutive shots from the same camera, ≤2 s apart and visually alike,
/// form a burst. The best one is marked; clearly weaker ones get a reject hint.
fn group_bursts(items: &[Item], clip: &HashMap<i64, Vec<f32>>) -> Vec<(i64, Ai)> {
    let photos: Vec<&Item> = items.iter().filter(|i| i.ai.is_some() && i.ai_state == 1).collect();
    let alike = |a: &Item, b: &Item| -> bool {
        if let (Some(x), Some(y)) = (clip.get(&a.id), clip.get(&b.id)) {
            return cosine(x, y) >= 0.88;
        }
        let h = |i: &Item| i.ai.as_ref().and_then(|a| a.phash.as_ref()).and_then(|p| u64::from_str_radix(p, 16).ok());
        match (h(a), h(b)) {
            (Some(x), Some(y)) => cv::hamming(x, y) <= 14,
            _ => false,
        }
    };
    let mut groups: Vec<Vec<&Item>> = vec![];
    for it in photos {
        let joins = groups.last().and_then(|g| g.last()).map(|prev| {
            prev.camera == it.camera
                && matches!((secs(&prev.captured_at), secs(&it.captured_at)), (Some(a), Some(b)) if (b - a).abs() <= 2.0)
                && alike(prev, it)
        });
        if joins == Some(true) {
            groups.last_mut().unwrap().push(it);
        } else {
            groups.push(vec![it]);
        }
    }

    let mut out = vec![];
    for g in groups {
        let size = g.len() as i64;
        let best_id = g
            .iter()
            .max_by(|a, b| quality(a.ai.as_ref().unwrap()).total_cmp(&quality(b.ai.as_ref().unwrap())))
            .map(|i| i.id);
        let best_q = g.iter().find(|i| Some(i.id) == best_id).map(|i| quality(i.ai.as_ref().unwrap())).unwrap_or(0.0);
        for it in &g {
            let mut ai = it.ai.clone().unwrap();
            suggest_single(&mut ai);
            if size >= 2 {
                ai.burst_id = Some(g[0].id);
                ai.burst_size = Some(size);
                ai.burst_best = Some(Some(it.id) == best_id);
                if Some(it.id) == best_id {
                    ai.reasons.insert(0, format!("Best of {size}"));
                    if ai.suggestion == Some(0) && ai.blur_level != Some(2) {
                        ai.suggestion = Some(1);
                    }
                } else {
                    if ai.suggestion == Some(1) {
                        ai.suggestion = Some(0);
                    }
                    if quality(&ai) < best_q - 8.0 && ai.suggestion != Some(-1) {
                        ai.reasons.push("Weaker than best in burst".into());
                        ai.suggestion = Some(-1);
                    }
                }
            } else {
                ai.burst_id = None;
                ai.burst_size = None;
                ai.burst_best = None;
            }
            out.push((it.id, ai));
        }
    }
    out
}
