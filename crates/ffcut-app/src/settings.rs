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
    /// Window title template, e.g. "{file} — {app}" (empty = default). See `layout::TITLE_VARS`.
    pub title_template: String,
    /// Fields of the file-info line in the menu bar, in order.
    pub info_items: Vec<crate::layout::InfoItem>,
    /// The timer above the timeline shows frame numbers instead of time.
    pub show_frames: bool,
    /// Shown instead of `/` and `\\` in paths (title, info line). Empty = the path as it is.
    pub path_sep: String,
    /// Changed keyboard shortcuts (see `hotkeys`); anything missing keeps its default.
    pub hotkeys: crate::hotkeys::Overrides,
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
        std::fs::read_to_string(Self::file()).ok().and_then(|t| serde_json::from_str::<Settings>(&t).ok()).unwrap_or_default().normalized()
    }
    fn normalized(mut self) -> Self {
        crate::layout::normalize_info_items(&mut self.info_items);
        self
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        let tmp = Self::file().with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap()).is_ok() {
            let _ = std::fs::rename(tmp, Self::file());
        }
    }
}
