//! Groups of application state that belong together. `App` owns one of each.

use crate::update::{Outcome, Release};
use ffcut_core::presets::{self, FxPresets, Part, Presets};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// Which windows are open.
#[derive(Default)]
pub struct Dialogs {
    pub settings: bool,
    pub about: bool,
    pub update: bool,
    pub save_preset: bool,
    pub presets: bool,
    /// (title, message) of an error window.
    pub error: Option<(String, String)>,
}

/// The update check.
#[derive(Default)]
pub struct Updates {
    /// A newer release than this one (None = up to date / unknown).
    pub found: Option<Release>,
    pub rx: Option<Receiver<Outcome>>,
    /// Result line of the last manual "Check now" (Settings / About).
    pub msg: Option<String>,
    pub manual: bool,
    /// The update being installed (or installed, or failed) by "Update now".
    pub install: Option<crate::install::Shared>,
}

/// The saved presets and where they live.
pub struct PresetStore {
    pub export: Presets,
    pub fx: FxPresets,
    export_path: PathBuf,
    fx_path: PathBuf,
}

impl PresetStore {
    pub fn load(dir: &Path) -> Self {
        let export_path = dir.join("presets.json");
        let fx_path = dir.join("audio-presets.json");
        PresetStore { export: presets::load(&export_path), fx: presets::load_fx(&fx_path), export_path, fx_path }
    }

    pub fn save_export(&self) -> Result<(), String> {
        presets::save(&self.export_path, &self.export).map_err(|e| format!("Cannot save presets: {e}"))
    }

    pub fn save_fx(&self) -> Result<(), String> {
        presets::save_fx(&self.fx_path, &self.fx).map_err(|e| format!("Cannot save audio presets: {e}"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetsTab {
    #[default]
    Export,
    Audio,
}

/// The preset dialogs: what is being typed, renamed, ticked.
#[derive(Default)]
pub struct PresetsUi {
    pub tab: PresetsTab,
    /// Name typed in a "save as" dialog.
    pub name: String,
    /// Parts ticked in the save-preset dialog.
    pub save_parts: Vec<Part>,
    /// Track the "Save audio preset" dialog takes its effects from (Some = the dialog is open).
    pub fx_save_track: Option<usize>,
    /// (old name, text being typed) while a preset is being renamed in the editor.
    pub rename: Option<(String, String)>,
    pub msg: Option<String>,
}

/// The crop frame.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct CropState {
    pub on: bool,
    /// Edges in frame pixels (x0, y0, x1, y1) of the rotated+flipped frame. Floats so slow drags work.
    pub edges: [f32; 4],
}

/// The final command, as built from the settings and as the user may have edited it.
#[derive(Default)]
pub struct CmdState {
    pub text: String,
    /// The text differs from what the settings produce.
    pub edited: bool,
    /// Settings changed after the user edited the text.
    pub stale: bool,
    /// The command the settings produce now.
    pub last_auto: String,
}

/// The crop loupe: the last piece of the picture it was given, and what part of the picture that is.
#[derive(Default)]
pub struct Loupe {
    pub tex: Option<eframe::egui::TextureHandle>,
    pub region: [f32; 4],
    /// Its size / zoom changed and are not saved yet.
    pub dirty: bool,
}
