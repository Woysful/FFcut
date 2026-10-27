//! Undo / redo of the edits: the export settings (trim points, tracks, effects, keyframes, crop, …).
//!
//! The state is compared every frame. A change is written to the history once it has settled (the mouse is up
//! and nothing moved for a frame), so a whole drag, or a typed value, is one step.

use crate::app::App;
use crate::state::CropState;
use eframe::egui;
use ffcut_core::ExportSettings;

const LIMIT: usize = 200;

#[derive(Clone, PartialEq)]
pub struct Snap {
    s: ExportSettings,
    trim: (f64, f64),
    crop: CropState,
}

#[derive(Default)]
pub struct History {
    /// The last state that is in the history (what Undo returns to is below it).
    base: Option<Snap>,
    /// The state of the previous frame: tells whether something is still changing.
    prev: Option<Snap>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
}

impl History {
    pub fn clear(&mut self) {
        *self = History::default();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn pending(&self) -> bool {
        matches!((&self.base, &self.prev), (Some(b), Some(p)) if b != p)
    }
}

impl App {
    fn snap(&self) -> Snap {
        Snap { s: self.s.clone(), trim: (self.trim_start, self.trim_end), crop: self.crop }
    }

    fn same_as(&self, o: &Snap) -> bool {
        (self.trim_start, self.trim_end) == o.trim && self.crop == o.crop && self.s == o.s
    }

    /// Once per frame, before the UI draws.
    pub fn track_history(&mut self, ctx: &egui::Context) {
        if self.clip.is_none() {
            self.hist.clear();
            return;
        }
        if self.hist.base.is_none() {
            let s = self.snap();
            self.hist.base = Some(s.clone());
            self.hist.prev = Some(s);
            return;
        }
        let moving = self.hist.prev.as_ref().is_none_or(|p| !self.same_as(p));
        if moving {
            self.hist.prev = Some(self.snap());
            ctx.request_repaint(); // one more frame to see that it stopped
            return;
        }
        let held = ctx.input(|i| i.pointer.any_down());
        if !held && self.hist.pending() {
            let now = self.hist.prev.clone().unwrap();
            if let Some(b) = self.hist.base.replace(now) {
                self.hist.undo.push(b);
                if self.hist.undo.len() > LIMIT {
                    self.hist.undo.remove(0);
                }
            }
            self.hist.redo.clear();
        }
    }

    fn apply_snap(&mut self, sn: Snap) {
        self.s = sn.s.clone();
        (self.trim_start, self.trim_end) = sn.trim;
        self.crop = sn.crop;
        self.trk_drag = None;
        self.fx.env_edit = None;
        self.sync_loop();
        self.apply_preview_transform();
        self.hist.base = Some(sn.clone());
        self.hist.prev = Some(sn);
    }

    pub fn undo(&mut self, ctx: &egui::Context) {
        if self.clip.is_none() {
            return;
        }
        // unsettled changes count as a step of their own
        self.track_history_now();
        let Some(back) = self.hist.undo.pop() else { return };
        if let Some(cur) = self.hist.base.take() {
            self.hist.redo.push(cur);
        }
        self.apply_snap(back);
        ctx.request_repaint();
    }

    pub fn redo(&mut self, ctx: &egui::Context) {
        let Some(fwd) = self.hist.redo.pop() else { return };
        if let Some(cur) = self.hist.base.take() {
            self.hist.undo.push(cur);
        }
        self.apply_snap(fwd);
        ctx.request_repaint();
    }

    /// Writes a change that has not settled yet into the history right away.
    fn track_history_now(&mut self) {
        let cur = self.snap();
        if self.hist.base.as_ref().is_some_and(|b| *b != cur) {
            if let Some(b) = self.hist.base.replace(cur.clone()) {
                self.hist.undo.push(b);
            }
            self.hist.prev = Some(cur);
            self.hist.redo.clear();
        }
    }
}
