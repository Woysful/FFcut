//! Preview of the video effects: the same filter graph the export uses, run by mpv (`vf=lavfi`).

use crate::app::App;
use eframe::egui;
use ffcut_core::fonts::Font;
use ffcut_core::videofx::{self, PreviewChain};
use ffcut_core::Rotation;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A graph tried in ffmpeg before the player gets it.
#[derive(Clone)]
enum Trial {
    Running,
    Passed,
    Failed(String),
}

/// A video chain that was handed to mpv but has not delivered its first picture yet.
struct Pending {
    /// Caption bars the layout switches to once the new picture is there.
    bars: (u32, u32),
    /// Size of mpv's output picture when the chain was handed over.
    before: Option<(u64, u64)>,
    since: Instant,
}

/// The longest a picture is held back while waiting for mpv's new one.
const PENDING_MAX: Duration = Duration::from_millis(1500);
/// Held at least this long even when the picture keeps its shape (mpv drops the paused frame on a chain change).
const PENDING_MIN: Duration = Duration::from_millis(300);

/// How long a trial may take before the plugin is taken for hung.
const TRIAL_TIMEOUT: Duration = Duration::from_secs(15);

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
    /// This is what the layout uses. It follows the picture mpv really delivers: switching it right away
    /// squeezed the old picture into the new shape (black bars above and below) until the new one arrived.
    pub bars: (u32, u32),
    pending: Option<Pending>,
    hw_copy: bool,
    pub error: Option<String>,
    /// ffmpeg's messages since the chain was applied (mpv reports a failed graph only in its log).
    log: Vec<String>,
    /// Graphs with a frei0r plugin, run once in ffmpeg first: a plugin that crashes takes down the process
    /// it runs in, and the player runs in this one.
    trials: Arc<Mutex<HashMap<String, Trial>>>,
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
            pending: None,
            hw_copy: false,
            error: None,
            log: Vec::new(),
            trials: Default::default(),
        }
    }
}

/// Runs `graph` over a few frames of a test picture of the frame's size.
fn trial(ffmpeg: &std::path::Path, graph: &str, w: u32, h: u32) -> Trial {
    let src = format!("testsrc2=size={}x{}:rate=25", w.max(16), h.max(16));
    let child = ffcut_core::run::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-f", "lavfi", "-i", &src, "-vf", graph, "-frames:v", "3", "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Trial::Failed(format!("cannot start ffmpeg: {e}")),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                if st.success() {
                    return Trial::Passed;
                }
                if let Some(c) = ffcut_core::run::crash_text(&st) {
                    return Trial::Failed(c.to_string());
                }
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut err);
                }
                return Trial::Failed(err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("ffmpeg failed").to_string());
            }
            Ok(None) if start.elapsed() > TRIAL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Trial::Failed("a plugin did not finish its first frames".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Trial::Failed(e.to_string()),
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
        self.pending = None;
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

    /// mpv accepts any graph and only says in its log that it could not build it (then plays without it).
    fn watch_filter_log(&mut self) {
        let log = self.player.filter_log();
        self.vfx.log.extend(log.errors);
        let n = self.vfx.log.len();
        self.vfx.log.drain(..n.saturating_sub(8));
        let ours = matches!(self.vfx.applied, Some(Some(_)));
        if log.failed && ours && self.vfx.error.is_none() {
            // the last line that names the cause ("Error creating filters" and the like only say that it failed)
            let why = self.vfx.log.iter().rev().find(|l| !l.starts_with("Error ")).or(self.vfx.log.last());
            self.vfx.error = Some(why.cloned().unwrap_or_else(|| "the player could not build the filters".into()));
        }
    }

    /// Switches the layout to the new caption bars (and shows live frames again) once mpv delivers the picture
    /// of the new chain: its output size changes, or enough time has passed for a chain that keeps the shape.
    fn commit_pending(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.vfx.pending else { return };
        let el = p.since.elapsed();
        let out = self.player.out_size();
        let reshaped = p.bars != self.vfx.bars;
        let arrived = if reshaped { out.is_some() && out != p.before } else { el >= PENDING_MIN };
        if arrived || el >= PENDING_MAX {
            self.vfx.bars = p.bars;
            self.vfx.pending = None;
            self.player.release_hold();
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Keeps mpv's video filter in step with the settings. Waits until the user stops dragging.
    pub fn sync_video_preview(&mut self, ctx: &egui::Context) {
        if self.clip.is_none() {
            return;
        }
        self.watch_filter_log();
        self.commit_pending(ctx);
        let (fw, fh) = self.frame_dims();
        let want = videofx::preview_chain(&self.s.vfx, self.total_rotation(), self.s.flip_h, self.s.flip_v, fw, fh, self.settings.preview_div());
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
        let mut want = self.vfx.want.clone();
        let mut trial_error = None;
        if let (Some(c), Some(ff)) = (&want, &self.ffmpeg) {
            if c.graph.contains("frei0r=") {
                let state = self.vfx.trials.lock().unwrap().get(&c.graph).cloned();
                match state {
                    None => {
                        self.vfx.trials.lock().unwrap().insert(c.graph.clone(), Trial::Running);
                        let (trials, ctx, ff, graph) = (self.vfx.trials.clone(), ctx.clone(), ff.clone(), c.graph.clone());
                        std::thread::spawn(move || {
                            let r = trial(&ff, &graph, fw, fh);
                            trials.lock().unwrap().insert(graph, r);
                            ctx.request_repaint();
                        });
                        return;
                    }
                    Some(Trial::Running) => return,
                    Some(Trial::Passed) => {}
                    Some(Trial::Failed(e)) => {
                        trial_error = Some(format!("a plugin failed when tried in ffmpeg, so the preview plays without the effects: {e}"));
                        want = None;
                    }
                }
            }
        }
        let keep = self.player.real_time();
        let before = self.player.out_size();
        let new_bars = if want.as_ref().is_some_and(|c| c.rotates) { videofx::bars(&self.s.vfx, fw) } else { (0, 0) };
        // from here until the new chain delivers its first picture mpv may draw black or the old picture in a
        // new shape: keep showing the last good frame (while playing the frames keep coming anyway)
        if self.player.paused() || new_bars != self.vfx.bars {
            self.player.hold();
        }
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
        self.vfx.error = trial_error.or(result.err().map(|e| format!("{e}")));
        self.vfx.log.clear();
        self.vfx.rotates = want.as_ref().is_some_and(|c| c.rotates);
        self.vfx.pending = Some(Pending { bars: new_bars, before, since: Instant::now() });
        self.vfx.applied = Some(self.vfx.want.clone());
        self.apply_preview_transform();
        // mpv answers a filter change with a seek of its own: put the playhead back (see `watch_audio_jump`)
        self.player.seek(keep);
        self.audio_hold = Some((keep, Instant::now(), self.player.seek_gen()));
    }
}
