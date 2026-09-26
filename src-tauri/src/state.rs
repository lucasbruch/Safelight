use crate::ai::Analyzer;
use crate::card::Scan;
use crate::exiftool::ExifTool;
use crate::ingest::Progress;
use crate::jobs::Jobs;
use crate::ledger::Ledger;
use crate::project::Project;
use crate::settings::Settings;
use crate::xmp;
use anyhow::Result;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub app: AppHandle,
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub exif: Arc<ExifTool>,
    pub ledger: Arc<Ledger>,
    pub xmp: xmp::Writer,
    pub jobs: Jobs,
    pub analyzer: Arc<Analyzer>,
    pub projects: Mutex<HashMap<PathBuf, Arc<Project>>>,
    pub scans: Mutex<HashMap<String, Arc<Scan>>>,
    pub import: Mutex<Option<Progress>>,
    pub cancel_import: Arc<AtomicBool>,
}

impl AppState {
    pub fn analyzer(&self) -> &Analyzer {
        &self.analyzer
    }

    /// Opens (or returns the already-open) project at `root`, creating it if needed.
    /// Only for importing into a new project and opening one the user chose.
    pub fn project(&self, root: &Path) -> Result<Arc<Project>> {
        let mut projects = self.projects.lock();
        if let Some(p) = projects.get(root) {
            return Ok(p.clone());
        }
        let p = Arc::new(Project::open(root)?);
        // The web view may only load files from projects (see `assetProtocol` in tauri.conf.json).
        let _ = self.app.asset_protocol_scope().allow_directory(root, true);
        projects.insert(root.to_path_buf(), p.clone());
        Ok(p)
    }

    /// Like `project`, but refuses folders that aren't already Safelight projects,
    /// so a stray path from the UI can never create a `.grabit` folder somewhere.
    pub fn existing_project(&self, root: &Path) -> Result<Arc<Project>> {
        if let Some(p) = self.projects.lock().get(root) {
            return Ok(p.clone());
        }
        anyhow::ensure!(Project::is_project(root), "{} isn't a Safelight project.", root.display());
        self.project(root)
    }

    /// Folders whose drives must never be mistaken for a camera card.
    pub fn card_exclusions(&self) -> Vec<String> {
        let s = self.settings.lock();
        vec![s.library_root.clone(), s.backup_root.clone()]
    }
}
