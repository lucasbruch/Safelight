use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Where new projects are created.
    pub library_root: String,
    /// Optional second copy of every import; empty = off.
    pub backup_root: String,
    pub artist: String,
    pub copyright: String,
    /// Run the local AI checks after previews are ready.
    pub ai_enabled: bool,
    pub resolve_hint_seen: bool,
    /// Projects opened from outside the library folder.
    pub other_projects: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let pictures = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
        // Installs from before the rename keep their existing folder.
        let legacy = pictures.join("GrabIt");
        let library = if legacy.is_dir() { legacy } else { pictures.join("Safelight") };
        Self {
            library_root: library.to_string_lossy().into_owned(),
            backup_root: String::new(),
            artist: String::new(),
            copyright: String::new(),
            ai_enabled: true,
            resolve_hint_seen: false,
            other_projects: vec![],
        }
    }
}

impl Settings {
    fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        // Write then rename: a crash mid-write must not leave a half file, which
        // `load` would silently replace with defaults.
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, Self::path(dir))?;
        Ok(())
    }
}
