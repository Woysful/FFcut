//! App settings (JSON in the user's config dir). Corrupt/missing file = defaults, never a crash.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Explicit paths; empty = look next to the app, then in PATH.
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    /// Linux: "auto" (default), "wayland" or "x11". Applied at the next start.
    pub display_server: String,
    /// true = do not ask GitHub for a newer release at startup.
    pub no_update_check: bool,
    /// A release the user chose to ignore (the lamp stays off for it).
    pub skipped_version: String,
    pub volume: Option<f32>,
    pub muted: bool,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("ffcut")
}

impl Settings {
    fn file() -> PathBuf {
        config_dir().join("settings.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        let tmp = Self::file().with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap()).is_ok() {
            let _ = std::fs::rename(tmp, Self::file());
        }
    }
}
