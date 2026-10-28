//! App settings (JSON in the user's config dir). Corrupt/missing file = defaults, never a crash.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A modifier key that goes with the mouse wheel (zoom / scroll of the waveform).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModKey {
    None,
    Ctrl,
    Alt,
    Shift,
}

impl ModKey {
    pub const ALL: [ModKey; 4] = [ModKey::None, ModKey::Ctrl, ModKey::Alt, ModKey::Shift];

    pub fn label(self) -> &'static str {
        match self {
            ModKey::None => "Wheel alone",
            ModKey::Ctrl => "Ctrl + wheel",
            ModKey::Alt => "Alt + wheel",
            ModKey::Shift => "Shift + wheel",
        }
    }

    /// Exactly this modifier is held (and no other one).
    pub fn matches(self, m: &eframe::egui::Modifiers) -> bool {
        let ctrl = m.ctrl || m.command;
        match self {
            ModKey::None => !ctrl && !m.alt && !m.shift,
            ModKey::Ctrl => ctrl && !m.alt && !m.shift,
            ModKey::Alt => m.alt && !ctrl && !m.shift,
            ModKey::Shift => m.shift && !ctrl && !m.alt,
        }
    }
}

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
    /// Sidebar sections: key → open. What the user left open stays open at the next start.
    pub sections: std::collections::BTreeMap<String, bool>,
    /// Waveform: what zooms and what scrolls (defaults: the wheel zooms, Shift + wheel scrolls).
    pub zoom_mod: Option<ModKey>,
    pub scroll_mod: Option<ModKey>,
    /// The crop loupe: side of its window (points) and magnification.
    pub loupe_size: Option<f32>,
    pub loupe_zoom: Option<f32>,
    pub volume: Option<f32>,
    pub muted: bool,
    /// The user's own defaults for the output dropdowns (right-click a dropdown → "Use as default").
    pub defaults: Defaults,
    /// Folders with LADSPA, LV2 or frei0r plugins, searched before the usual ones.
    pub plugin_folders: Vec<String>,
}

/// Output choices the user made the default; `None` = the built-in one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    /// For video files.
    pub container: Option<String>,
    /// For audio files.
    pub audio_container: Option<String>,
    pub video_codec: Option<ffcut_core::VideoCodec>,
    pub audio_codec: Option<ffcut_core::AudioCodec>,
    /// The x264 / x265 speed preset.
    pub speed: Option<String>,
    /// "auto" or a pixel format.
    pub pix_fmt: Option<String>,
    pub audio_mode: Option<ffcut_core::AudioMode>,
}

impl Defaults {
    /// The encoding choices; they are set once at start and then stay from file to file.
    pub fn apply(&self, s: &mut ffcut_core::ExportSettings) {
        if let Some(c) = &self.video_codec {
            s.video_codec = c.clone();
        }
        if let Some(c) = &self.audio_codec {
            s.audio_codec = c.clone();
        }
        if let Some(p) = &self.speed {
            s.preset = p.clone();
        }
        if let Some(p) = &self.pix_fmt {
            s.pix_fmt = (p != "auto").then(|| p.clone());
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("ffcut")
}

impl Settings {
    pub fn zoom_key(&self) -> ModKey {
        self.zoom_mod.unwrap_or(ModKey::None)
    }
    pub fn scroll_key(&self) -> ModKey {
        self.scroll_mod.unwrap_or(ModKey::Shift)
    }

    /// Where plugins are looked for: the user's folders, then the usual ones.
    pub fn plugin_dirs(&self) -> ffcut_core::plugins::Dirs {
        let mine: Vec<PathBuf> = self.plugin_folders.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).map(PathBuf::from).collect();
        ffcut_core::plugins::Dirs::new(&mine)
    }

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
