//! Preview of the video effects: the same filter graph the export uses, run by mpv (`vf=lavfi`).

use crate::app::App;
use eframe::egui;
use ffcut_core::fonts::Font;
use ffcut_core::videofx::{self, PreviewChain};
use ffcut_core::Rotation;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct VfxState {
    /// The effects window is open.
    pub window: bool,
    /// Effect whose area / text marker is shown on the video.
    pub sel: Option<usize>,
    /// System fonts, read in the background the first time the window opens.
    pub fonts: Arc<Mutex<Option<Vec<Font>>>>,
    fonts_started: bool,
    /// Curves editor: channel (0 master, 1 red, 2 green, 3 blue) and the selected point (effect, point).
    pub curve_ch: usize,
    pub curve_sel: Option<(usize, usize)>,
    /// What the settings ask for, and since when (a short pause lets the user finish a drag).
    want: Option<PreviewChain>,
    changed_at: Instant,
    /// What mpv runs now (`None` = nothing applied yet; `Some(None)` = plain playback).
    applied: Option<Option<PreviewChain>>,
    /// The chain in mpv rotates / flips the picture itself.
    pub rotates: bool,
    /// Caption bars (top, bottom) in the picture mpv shows now, in source pixels.
    pub bars: (u32, u32),
    hw_copy: bool,
    pub error: Option<String>,
}

impl Default for VfxState {
    fn default() -> Self {
        VfxState {
            window: false,
            sel: None,
            fonts: Default::default(),
            fonts_started: false,
            curve_ch: 0,
            curve_sel: None,
            want: None,
            changed_at: Instant::now(),
            applied: None,
            rotates: false,
            bars: (0, 0),
            hw_copy: false,
            error: None,
        }
    }
}

impl VfxState {
    /// A new file was opened.
    pub fn clear(&mut self) {
        self.sel = None;
        self.curve_sel = None;
        self.want = None;
        self.applied = None;
        self.rotates = false;
        self.bars = (0, 0);
        self.error = None;
    }
}

impl App {
    /// Starts reading the system fonts (once).
    pub fn load_fonts(&mut self, ctx: &egui::Context) {
        if self.vfx.fonts_started {
            return;
        }
        self.vfx.fonts_started = true;
        let (slot, ctx) = (self.vfx.fonts.clone(), ctx.clone());
        std::thread::spawn(move || {
            *slot.lock().unwrap() = Some(ffcut_core::fonts::scan());
            ctx.request_repaint();
        });
    }

    /// Size of the picture mpv shows: the frame plus caption bars.
    pub fn view_dims(&self) -> (u32, u32) {
        let (w, h) = self.frame_dims();
        (w, h + self.vfx.bars.0 + self.vfx.bars.1)
    }

    /// Clockwise degrees the picture is turned in total (the file's own rotation + the user's).
    pub fn total_rotation(&self) -> i64 {
        let user = match self.s.rotation {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::R180 => 180,
            Rotation::Ccw90 => 270,
        };
        (user + self.clip.as_ref().map_or(0, |c| c.info.rotation as i64)) % 360
    }

    /// Keeps mpv's video filter in step with the settings. Waits until the user stops dragging.
    pub fn sync_video_preview(&mut self, ctx: &egui::Context) {
        if self.clip.is_none() {
            return;
        }
        let (fw, fh) = self.frame_dims();
        let want = videofx::preview_chain(&self.s.vfx, self.total_rotation(), self.s.flip_h, self.s.flip_v, fw, fh);
        if want != self.vfx.want {
            self.vfx.want = want;
            self.vfx.changed_at = Instant::now();
        }
        if self.vfx.applied.as_ref() == Some(&self.vfx.want) {
            return;
        }
        let m = self.player.mpv();
        let ready = m.get_property::<i64>("track-list/count").unwrap_or(0) > 0;
        if !ready || self.vfx.changed_at.elapsed() < Duration::from_millis(250) || ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let want = self.vfx.want.clone();
        let keep = self.player.real_time();
        // a software filter cannot take frames that live in GPU memory: decode into system memory while it runs
        let result = match &want {
            Some(c) => {
                if !self.vfx.hw_copy {
                    let _ = m.set_property("hwdec", "auto-copy-safe");
                    self.vfx.hw_copy = true;
                }
                m.set_property("vf", format!("lavfi=graph=%{}%{}", c.graph.len(), c.graph))
            }
            None => {
                let r = m.set_property("vf", "");
                if self.vfx.hw_copy {
                    let _ = m.set_property("hwdec", "auto-safe");
                    self.vfx.hw_copy = false;
                }
                r
            }
        };
        self.vfx.error = result.err().map(|e| format!("{e}"));
        self.vfx.rotates = want.as_ref().is_some_and(|c| c.rotates);
        self.vfx.bars = if self.vfx.rotates { videofx::bars(&self.s.vfx, fw) } else { (0, 0) };
        self.vfx.applied = Some(want);
        self.apply_preview_transform();
        // mpv answers a filter change with a seek of its own: put the playhead back (see `watch_audio_jump`)
        self.player.seek(keep);
        self.audio_hold = Some((keep, Instant::now(), self.player.seek_gen()));
    }
}
