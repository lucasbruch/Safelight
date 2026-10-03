//! A project is a folder `<library>/<YYYY-MM-DD_Name>/` holding the media plus
//! `.grabit/project.sqlite` and `.grabit/cache/` (previews). Everything a
//! project needs lives inside it, so it can be moved or copied as a whole.

use crate::meta::{Kind, Meta};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DIR: &str = ".grabit";
pub const REJECTED_DIR: &str = "_Rejected";

pub struct Project {
    pub root: PathBuf,
    pub db: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ai {
    pub sharpness: Option<f64>,
    /// 0 = sharp, 1 = slightly soft, 2 = blurry.
    pub blur_level: Option<i64>,
    pub motion_blur: Option<bool>,
    pub faces: Option<i64>,
    pub eyes_closed: Option<i64>,
    pub over_exposed: Option<f64>,
    pub under_exposed: Option<f64>,
    pub horizon_tilt: Option<f64>,
    pub aesthetic: Option<f64>,
    pub burst_id: Option<i64>,
    pub burst_size: Option<i64>,
    pub burst_best: Option<bool>,
    /// 1 = suggested keeper, -1 = suggested reject, 0 = no opinion.
    pub suggestion: Option<i64>,
    pub reasons: Vec<String>,
    pub tags: Vec<String>,
    pub people: Vec<i64>,
    /// 64-bit difference hash (hex) for near-duplicate detection without CLIP.
    pub phash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    /// e.g. "Canon Log 3" when the clip is log footage (previewed through a viewing LUT).
    pub log: Option<String>,
    /// Colour gamut of log footage, for rebuilding the viewing LUT.
    #[serde(default)]
    pub gamut: Option<String>,
    /// "original" (plays as-is), "pending", "proxy" (720p copy ready) or "failed".
    pub playback: String,
    /// Bumped when video handling changes, so older previews are redone.
    #[serde(default)]
    pub rev: u32,
}

impl VideoInfo {
    pub const REV: u32 = 2;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub rel_path: String,
    pub kind: Kind,
    pub file_name: String,
    pub size: i64,
    pub captured_at: Option<String>,
    pub camera: String,
    pub meta: Meta,
    pub rating: i64,
    /// 1 = pick, -1 = reject, 0 = unflagged.
    pub flag: i64,
    pub moved_to_rejected: bool,
    pub preview_state: i64,
    pub ai_state: i64,
    pub ai: Option<Ai>,
    pub video: Option<VideoInfo>,
    /// Tags you added yourself; kept apart from the AI's so re-running it never drops them.
    pub tags: Vec<String>,
}

impl Item {
    /// Your tags, then the AI's, without repeats: what editors receive as keywords.
    pub fn keywords(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for t in self.tags.iter().chain(self.ai.iter().flat_map(|a| a.tags.iter())) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
                out.push(t.clone());
            }
        }
        out
    }
}

/// A tag as typed, made safe for XMP, Lightroom and Resolve: one line, no commas
/// (editors split keyword lists on them), single spaces, at most 64 characters.
pub fn clean_tag(t: &str) -> Option<String> {
    let t: String = t.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(64).collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY,
    rel_path TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    file_name TEXT NOT NULL,
    size INTEGER NOT NULL,
    hash TEXT NOT NULL,
    captured_at TEXT,
    camera TEXT NOT NULL,
    meta TEXT NOT NULL,
    sidecars TEXT NOT NULL DEFAULT '[]',
    rating INTEGER NOT NULL DEFAULT 0,
    flag INTEGER NOT NULL DEFAULT 0,
    moved_to_rejected INTEGER NOT NULL DEFAULT 0,
    preview_state INTEGER NOT NULL DEFAULT 0,
    ai_state INTEGER NOT NULL DEFAULT 0,
    ai TEXT,
    embedding BLOB,
    face_embeddings BLOB,
    imported_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS items_captured ON items(captured_at);
CREATE TABLE IF NOT EXISTS imports (
    id INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    source TEXT NOT NULL,
    report TEXT
);
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

pub struct NewItem<'a> {
    pub rel_path: &'a str,
    pub kind: Kind,
    pub file_name: &'a str,
    pub size: u64,
    pub hash: &'a str,
    pub camera: &'a str,
    pub meta: &'a Meta,
    pub sidecars: &'a [String],
}

impl Project {
    pub fn open(root: &Path) -> Result<Self> {
        let dir = root.join(DIR);
        std::fs::create_dir_all(dir.join("cache"))
            .with_context(|| format!("creating {}", dir.display()))?;
        hide_dir(&dir);
        let conn = Connection::open(dir.join("project.sqlite"))?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA)?;
        // Columns added after the first release; "duplicate column" means it's already there.
        let _ = conn.execute("ALTER TABLE items ADD COLUMN video TEXT", []);
        let _ = conn.execute("ALTER TABLE items ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'", []);
        Ok(Self { root: root.to_path_buf(), db: Mutex::new(conn) })
    }

    pub fn is_project(root: &Path) -> bool {
        root.join(DIR).join("project.sqlite").exists()
    }

    pub fn name(&self) -> String {
        self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(DIR).join("cache")
    }

    pub fn thumb_path(&self, id: i64) -> PathBuf {
        self.cache_dir().join(format!("{id}_t.jpg"))
    }

    pub fn preview_path(&self, id: i64) -> PathBuf {
        self.cache_dir().join(format!("{id}_p.jpg"))
    }

    pub fn proxy_path(&self, id: i64) -> PathBuf {
        self.cache_dir().join(format!("{id}_proxy.mp4"))
    }

    pub fn lut_dir(&self) -> PathBuf {
        self.cache_dir().join("luts")
    }

    pub fn set_video(&self, id: i64, v: &VideoInfo) -> Result<()> {
        self.db.lock().execute("UPDATE items SET video = ?1 WHERE id = ?2", params![serde_json::to_string(v)?, id])?;
        Ok(())
    }

    pub fn full_path(&self, id: i64) -> PathBuf {
        self.cache_dir().join(format!("{id}_full.jpg"))
    }

    /// Absolute path of an item's media file, accounting for moved rejects.
    pub fn abs(&self, rel_path: &str, moved_to_rejected: bool) -> PathBuf {
        let mut p = self.root.clone();
        if moved_to_rejected {
            p.push(REJECTED_DIR);
        }
        for part in rel_path.split('/') {
            p.push(part);
        }
        p
    }

    pub fn rel_exists(&self, rel_path: &str) -> Result<bool> {
        let db = self.db.lock();
        Ok(db
            .query_row("SELECT 1 FROM items WHERE rel_path = ?1", [rel_path], |_| Ok(()))
            .optional()?
            .is_some())
    }

    pub fn insert(&self, n: &NewItem) -> Result<i64> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        // Ids are never reused (plain rowids would be, after emptying rejects), so
        // cache files and the UI's cached URLs can't point at a different photo.
        let id: i64 = tx.query_row(
            "SELECT MAX(COALESCE((SELECT CAST(value AS INTEGER) FROM kv WHERE key = 'last_id'), 0),
                        COALESCE((SELECT MAX(id) FROM items), 0)) + 1",
            [],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO items (id, rel_path, kind, file_name, size, hash, captured_at, camera, meta, sidecars, imported_at)
             VALUES (?11, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                n.rel_path,
                n.kind.as_str(),
                n.file_name,
                n.size as i64,
                n.hash,
                n.meta.captured_at.map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()),
                n.camera,
                serde_json::to_string(n.meta)?,
                serde_json::to_string(n.sidecars)?,
                chrono::Local::now().to_rfc3339(),
                id,
            ],
        )?;
        tx.execute("INSERT OR REPLACE INTO kv (key, value) VALUES ('last_id', ?1)", [id.to_string()])?;
        tx.commit()?;
        Ok(id)
    }

    fn row_to_item(r: &Row) -> rusqlite::Result<Item> {
        let kind: String = r.get("kind")?;
        let meta: String = r.get("meta")?;
        let ai: Option<String> = r.get("ai")?;
        let video: Option<String> = r.get("video")?;
        let tags: String = r.get("tags")?;
        Ok(Item {
            id: r.get("id")?,
            rel_path: r.get("rel_path")?,
            kind: if kind == "video" { Kind::Video } else { Kind::Photo },
            file_name: r.get("file_name")?,
            size: r.get("size")?,
            captured_at: r.get("captured_at")?,
            camera: r.get("camera")?,
            meta: serde_json::from_str(&meta).unwrap_or_default(),
            rating: r.get("rating")?,
            flag: r.get("flag")?,
            moved_to_rejected: r.get::<_, i64>("moved_to_rejected")? != 0,
            preview_state: r.get("preview_state")?,
            ai_state: r.get("ai_state")?,
            ai: ai.and_then(|a| serde_json::from_str(&a).ok()),
            video: video.and_then(|v| serde_json::from_str(&v).ok()),
            tags: serde_json::from_str(&tags).unwrap_or_default(),
        })
    }

    const ITEM_COLS: &'static str = "id, rel_path, kind, file_name, size, captured_at, camera, meta, rating, flag, moved_to_rejected, preview_state, ai_state, ai, video, tags";

    pub fn items(&self) -> Result<Vec<Item>> {
        let db = self.db.lock();
        let mut st = db.prepare(&format!(
            "SELECT {} FROM items ORDER BY captured_at IS NULL, captured_at, file_name",
            Self::ITEM_COLS
        ))?;
        let rows = st.query_map([], Self::row_to_item)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn item(&self, id: i64) -> Result<Item> {
        let db = self.db.lock();
        Ok(db.query_row(
            &format!("SELECT {} FROM items WHERE id = ?1", Self::ITEM_COLS),
            [id],
            Self::row_to_item,
        )?)
    }

    pub fn sidecars(&self, id: i64) -> Result<Vec<String>> {
        let db = self.db.lock();
        let s: String = db.query_row("SELECT sidecars FROM items WHERE id = ?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&s).unwrap_or_default())
    }

    /// Runs `sql` once per id with `value` as ?1 and the id as ?2, in one transaction
    /// (one disk sync instead of one per photo).
    fn update_each(&self, sql: &str, ids: &[i64], value: i64) -> Result<()> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare(sql)?;
            for id in ids {
                st.execute(params![value, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_rating(&self, ids: &[i64], rating: i64) -> Result<()> {
        self.update_each("UPDATE items SET rating = ?1 WHERE id = ?2", ids, rating.clamp(0, 5))
    }

    pub fn set_flag(&self, ids: &[i64], flag: i64) -> Result<()> {
        self.update_each("UPDATE items SET flag = ?1 WHERE id = ?2", ids, flag.clamp(-1, 1))
    }

    /// Adds and removes tags on each photo, in one transaction. Matching ignores case,
    /// so "Bride" isn't added next to "bride".
    pub fn edit_tags(&self, ids: &[i64], add: &[String], remove: &[String]) -> Result<()> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        {
            let mut get = tx.prepare("SELECT tags FROM items WHERE id = ?1")?;
            let mut put = tx.prepare("UPDATE items SET tags = ?1 WHERE id = ?2")?;
            for id in ids {
                let Some(cur) = get.query_row([id], |r| r.get::<_, String>(0)).optional()? else { continue };
                let mut tags: Vec<String> = serde_json::from_str(&cur).unwrap_or_default();
                tags.retain(|t| !remove.iter().any(|r| r.eq_ignore_ascii_case(t)));
                for a in add {
                    if !tags.iter().any(|t| t.eq_ignore_ascii_case(a)) {
                        tags.push(a.clone());
                    }
                }
                put.execute(params![serde_json::to_string(&tags)?, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_items(&self, ids: &[i64]) -> Result<()> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare("DELETE FROM items WHERE id = ?1")?;
            for id in ids {
                st.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Removes every cached file derived from an item (previews, full-size, proxy).
    pub fn remove_cache(&self, id: i64) {
        for p in [self.thumb_path(id), self.preview_path(id), self.full_path(id), self.proxy_path(id)] {
            let _ = std::fs::remove_file(p);
        }
    }

    pub fn set_moved(&self, id: i64, moved: bool) -> Result<()> {
        self.db.lock().execute(
            "UPDATE items SET moved_to_rejected = ?1 WHERE id = ?2",
            params![moved as i64, id],
        )?;
        Ok(())
    }

    pub fn set_preview_state(&self, id: i64, state: i64) -> Result<()> {
        self.db.lock().execute("UPDATE items SET preview_state = ?1 WHERE id = ?2", params![state, id])?;
        Ok(())
    }

    pub fn set_ai(&self, id: i64, ai: &Ai, state: i64) -> Result<()> {
        self.db.lock().execute(
            "UPDATE items SET ai = ?1, ai_state = ?2 WHERE id = ?3",
            params![serde_json::to_string(ai)?, state, id],
        )?;
        Ok(())
    }

    /// Writes many AI results at once, in one transaction.
    pub fn set_ai_many(&self, updates: &[(i64, Ai)], state: i64) -> Result<()> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare("UPDATE items SET ai = ?1, ai_state = ?2 WHERE id = ?3")?;
            for (id, ai) in updates {
                st.execute(params![serde_json::to_string(ai)?, state, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_embeddings(&self, id: i64, clip: Option<&[f32]>, faces: Option<&[Vec<f32>]>) -> Result<()> {
        let db = self.db.lock();
        if let Some(c) = clip {
            db.execute("UPDATE items SET embedding = ?1 WHERE id = ?2", params![f32_bytes(c), id])?;
        }
        if let Some(fs) = faces {
            db.execute(
                "UPDATE items SET face_embeddings = ?1 WHERE id = ?2",
                params![serde_json::to_vec(fs)?, id],
            )?;
        }
        Ok(())
    }

    /// (id, captured_at, clip embedding) for every photo with an embedding.
    pub fn embeddings(&self) -> Result<Vec<(i64, Option<String>, Vec<f32>)>> {
        let db = self.db.lock();
        let mut st = db.prepare(
            "SELECT id, captured_at, embedding FROM items WHERE embedding IS NOT NULL ORDER BY captured_at, file_name",
        )?;
        let rows = st.query_map([], |r| {
            let b: Vec<u8> = r.get(2)?;
            Ok((r.get(0)?, r.get(1)?, bytes_f32(&b)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn face_embeddings(&self) -> Result<Vec<(i64, Vec<Vec<f32>>)>> {
        let db = self.db.lock();
        let mut st = db.prepare("SELECT id, face_embeddings FROM items WHERE face_embeddings IS NOT NULL ORDER BY captured_at, id")?;
        let rows = st.query_map([], |r| {
            let b: Vec<u8> = r.get(1)?;
            Ok((r.get(0)?, serde_json::from_slice(&b).unwrap_or_default()))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn ids_needing(&self, column: &str, below: i64) -> Result<Vec<i64>> {
        let db = self.db.lock();
        let mut st = db.prepare(&format!(
            "SELECT id FROM items WHERE kind = 'photo' AND {column} < ?1 ORDER BY captured_at"
        ))?;
        let rows = st.query_map([below], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn begin_import(&self, source: &str) -> Result<i64> {
        let db = self.db.lock();
        db.execute(
            "INSERT INTO imports (started_at, source) VALUES (?1, ?2)",
            params![chrono::Local::now().to_rfc3339(), source],
        )?;
        Ok(db.last_insert_rowid())
    }

    pub fn finish_import(&self, id: i64, report: &serde_json::Value) -> Result<()> {
        self.db.lock().execute(
            "UPDATE imports SET finished_at = ?1, report = ?2 WHERE id = ?3",
            params![chrono::Local::now().to_rfc3339(), report.to_string(), id],
        )?;
        Ok(())
    }

    pub fn summary(&self) -> Result<Summary> {
        let db = self.db.lock();
        let (total, photos, videos, picks, rejects, first, last, cover): (i64, i64, i64, i64, i64, Option<String>, Option<String>, Option<i64>) = db.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(kind = 'photo'), 0),
                    COALESCE(SUM(kind = 'video'), 0),
                    COALESCE(SUM(flag = 1), 0),
                    COALESCE(SUM(flag = -1), 0),
                    MIN(captured_at), MAX(captured_at),
                    (SELECT id FROM items WHERE kind = 'photo' AND preview_state = 1
                        ORDER BY flag DESC, rating DESC, captured_at LIMIT 1)
             FROM items",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
        drop(db);
        Ok(Summary {
            root: self.root.to_string_lossy().into_owned(),
            name: self.name(),
            total,
            photos,
            videos,
            picks,
            rejects,
            first_date: first,
            last_date: last,
            cover: cover.map(|id| self.thumb_path(id).to_string_lossy().into_owned()),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub root: String,
    pub name: String,
    pub total: i64,
    pub photos: i64,
    pub videos: i64,
    pub picks: i64,
    pub rejects: i64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub cover: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(p: &Project, rel: &str) -> i64 {
        let meta = Meta::default();
        p.insert(&NewItem { rel_path: rel, kind: Kind::Photo, file_name: rel, size: 1, hash: "h", camera: "c", meta: &meta, sidecars: &[] })
            .unwrap()
    }

    #[test]
    fn ids_are_not_reused_after_deleting_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let (a, b) = (add(&p, "a.jpg"), add(&p, "b.jpg"));
        assert_eq!((a, b), (1, 2));
        p.delete_items(&[b]).unwrap();
        assert_eq!(add(&p, "c.jpg"), 3);
    }

    #[test]
    fn bulk_rating_and_flags() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let ids: Vec<i64> = (0..5).map(|i| add(&p, &format!("{i}.jpg"))).collect();
        p.set_rating(&ids, 9).unwrap();
        p.set_flag(&ids[..2], -1).unwrap();
        let items = p.items().unwrap();
        assert!(items.iter().all(|i| i.rating == 5));
        assert_eq!(items.iter().filter(|i| i.flag == -1).count(), 2);
    }

    #[test]
    fn tags_add_remove_and_ignore_case() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let (a, b) = (add(&p, "a.jpg"), add(&p, "b.jpg"));
        p.edit_tags(&[a, b], &["Bride".into(), "Venue".into()], &[]).unwrap();
        p.edit_tags(&[a], &["bride".into()], &["venue".into()]).unwrap();
        assert_eq!(p.item(a).unwrap().tags, ["Bride"]);
        assert_eq!(p.item(b).unwrap().tags, ["Bride", "Venue"]);
    }

    #[test]
    fn keywords_put_your_tags_first_without_repeats() {
        let dir = tempfile::tempdir().unwrap();
        let p = Project::open(dir.path()).unwrap();
        let id = add(&p, "a.jpg");
        p.edit_tags(&[id], &["Dog".into()], &[]).unwrap();
        let mut it = p.item(id).unwrap();
        it.ai = Some(Ai { tags: vec!["dog".into(), "beach".into()], ..Default::default() });
        assert_eq!(it.keywords(), ["Dog", "beach"]);
    }

    #[test]
    fn clean_tag_makes_one_safe_keyword() {
        assert_eq!(clean_tag("  golden\thour \n").as_deref(), Some("golden hour"));
        assert_eq!(clean_tag("a,b").as_deref(), Some("a b"));
        assert_eq!(clean_tag(" , ").as_deref(), None);
        assert_eq!(clean_tag(&"x".repeat(100)).map(|t| t.len()), Some(64));
    }
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bytes_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[cfg(windows)]
fn hide_dir(p: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    extern "system" {
        fn SetFileAttributesW(name: *const u16, attrs: u32) -> i32;
    }
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn hide_dir(_: &Path) {} // dot-folders are already hidden on macOS
