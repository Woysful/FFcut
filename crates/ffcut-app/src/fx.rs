//! Background work behind the track effects: rendering what a track sounds like (for the preview) and
//! measuring loudness (for normalization). Both run ffmpeg with exactly the filters the export uses.

use crate::app::{App, PreviewAudio};
use eframe::egui;
use ffcut_core::audiofx;
use ffcut_core::{EffectKind, Measured, TrackEdit, TrackSource};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

pub enum Render {
    Loading,
    Ready(PathBuf),
    Failed(String),
}

pub enum Measure {
    Loading,
    Done(Result<Measured, String>),
}

/// Temporary folder for the rendered previews; removed when the app closes.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct FxState {
    /// Keyed by `audiofx::render_key`.
    pub renders: Arc<Mutex<HashMap<u64, Render>>>,
    /// Keyed by `audiofx::norm_key`.
    pub measures: Arc<Mutex<HashMap<u64, Measure>>>,
    dir: TempDir,
    counter: Arc<std::sync::atomic::AtomicUsize>,
    /// Track whose volume keyframes are being edited on its lane.
    pub env_edit: Option<usize>,
    /// Keyframe the lane's context menu was opened on.
    pub ctx_point: Option<usize>,
    /// Selected equalizer band: (effect, band).
    pub eq_sel: Option<(usize, usize)>,
    /// The track list as of the last frame, and when it last changed (see `settled`).
    snapshot: Vec<TrackEdit>,
    changed_at: std::time::Instant,
}

/// Folders left behind by a crashed run (Linux: the process is gone).
fn remove_stale_dirs() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(pid) = name.strip_prefix("ffcut-preview-").and_then(|p| p.parse::<u32>().ok()) {
            if cfg!(target_os = "linux") && pid != std::process::id() && !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

impl Default for FxState {
    fn default() -> Self {
        remove_stale_dirs();
        let dir = std::env::temp_dir().join(format!("ffcut-preview-{}", std::process::id()));
        FxState {
            renders: Default::default(),
            measures: Default::default(),
            dir: TempDir(dir),
            counter: Default::default(),
            env_edit: None,
            ctx_point: None,
            eq_sel: None,
            snapshot: Vec::new(),
            changed_at: std::time::Instant::now(),
        }
    }
}

impl FxState {
    /// A new file was opened: forget everything rendered for the old one.
    pub fn clear(&mut self) {
        self.renders.lock().unwrap().clear();
        self.measures.lock().unwrap().clear();
        self.env_edit = None;
        self.eq_sel = None;
    }
}

impl App {
    /// Notices edits of the track list. Expensive follow-ups (rendering a preview, measuring, switching mpv's
    /// audio) wait until the edits have stopped for a moment, so dragging a slider or a point stays smooth.
    pub fn track_changes(&mut self, ctx: &egui::Context) {
        if self.fx.snapshot != self.s.tracks {
            self.fx.snapshot = self.s.tracks.clone();
            self.fx.changed_at = std::time::Instant::now();
        }
        if !self.settled() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    pub fn settled(&self) -> bool {
        self.fx.changed_at.elapsed() > std::time::Duration::from_millis(450)
    }

    /// (file, audio stream ordinal) a track reads from.
    fn track_input(&self, t: &TrackEdit) -> Option<(PathBuf, usize)> {
        match &t.source {
            TrackSource::Original(i) => self.clip.as_ref().map(|c| (c.path.clone(), *i)),
            TrackSource::External { path, .. } => Some((PathBuf::from(path), 0)),
        }
    }

    /// What mpv should read for this track in the preview: the track itself when nothing but volume /
    /// mute / solo was changed, otherwise the same track rendered with its trim, shift and effects
    /// (until that is ready, the plain track).
    pub fn preview_source(&self, ctx: &egui::Context, t: &TrackEdit) -> TrackSource {
        let dur = self.dur();
        if !t.needs_bake(dur) {
            return t.source.clone();
        }
        let key = audiofx::render_key(t, dur);
        match self.fx.renders.lock().unwrap().get(&key) {
            Some(Render::Ready(p)) => return TrackSource::External { path: p.to_string_lossy().into_owned(), duration: dur },
            Some(_) => return t.source.clone(),
            None => {}
        }
        // not while a slider / handle is being dragged: every step would start another render
        if !self.settled() || ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
            return t.source.clone();
        }
        let (Some(ffmpeg), Some((file, ordinal))) = (self.ffmpeg.clone(), self.track_input(t)) else { return t.source.clone() };
        self.fx.renders.lock().unwrap().insert(key, Render::Loading);
        let n = self.fx.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = std::fs::create_dir_all(&self.fx.dir.0);
        let out = self.fx.dir.0.join(format!("t{n}.flac"));
        let args = audiofx::preview_render_args(&file, ordinal, t, &out);
        let (renders, ctx) = (self.fx.renders.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = ffcut_core::run::command(&ffmpeg).args(&args).stdin(Stdio::null()).output();
            let state = match r {
                Ok(o) if o.status.success() => Render::Ready(out),
                Ok(o) => Render::Failed(match ffcut_core::run::crash_text(&o.status) {
                    Some(c) => c.to_string(),
                    None => String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg failed").to_string(),
                }),
                Err(e) => Render::Failed(e.to_string()),
            };
            renders.lock().unwrap().insert(key, state);
            ctx.request_repaint();
        });
        t.source.clone()
    }

    /// The rendered preview file of a track (trim, shift, effects and fades already in it), once it exists.
    pub fn baked_path(&self, t: &TrackEdit) -> Option<PathBuf> {
        let dur = self.dur();
        if !t.needs_bake(dur) {
            return None;
        }
        match self.fx.renders.lock().unwrap().get(&audiofx::render_key(t, dur)) {
            Some(Render::Ready(p)) => Some(p.clone()),
            _ => None,
        }
    }

    /// Is a rendered preview of this track being made right now?
    pub fn preview_rendering(&self, t: &TrackEdit) -> bool {
        let dur = self.dur();
        t.needs_bake(dur) && matches!(self.fx.renders.lock().unwrap().get(&audiofx::render_key(t, dur)), Some(Render::Loading) | None)
    }

    /// Why the preview of this track could not be rendered, if it could not.
    pub fn preview_error(&self, t: &TrackEdit) -> Option<String> {
        match self.fx.renders.lock().unwrap().get(&audiofx::render_key(t, self.dur())) {
            Some(Render::Failed(e)) => Some(e.clone()),
            _ => None,
        }
    }

    /// Normalization needs a measurement of the clip: start it when missing, and file the result into the track.
    pub fn drive_measures(&mut self, ctx: &egui::Context) {
        if self.clip.is_none() {
            return;
        }
        let quiet = self.settled() && !ctx.input(|i| i.pointer.any_down());
        for i in 0..self.s.tracks.len() {
            for idx in 0..self.s.tracks[i].effects.len() {
                let t = self.s.tracks[i].clone();
                let e = &t.effects[idx];
                if !e.on || !matches!(e.kind, EffectKind::Norm(_)) || audiofx::valid_measure(&t, idx).is_some() {
                    continue;
                }
                let key = audiofx::norm_key(&t, idx);
                let found = match self.fx.measures.lock().unwrap().get(&key) {
                    Some(Measure::Done(Ok(m))) => Some(m.clone()),
                    Some(_) => continue,
                    None => None,
                };
                if let Some(m) = found {
                    if let EffectKind::Norm(n) = &mut self.s.tracks[i].effects[idx].kind {
                        n.measured = Some(m);
                    }
                    continue;
                }
                if !quiet {
                    continue;
                }
                let (Some(ffmpeg), Some((file, ordinal))) = (self.ffmpeg.clone(), self.track_input(&t)) else { continue };
                self.fx.measures.lock().unwrap().insert(key, Measure::Loading);
                let (measures, ctx) = (self.fx.measures.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let r = ffcut_core::analysis::measure(&ffmpeg, &file, ordinal, &t, idx).map_err(|e| e.to_string());
                    measures.lock().unwrap().insert(key, Measure::Done(r));
                    ctx.request_repaint();
                });
            }
        }
    }

    /// Why the measurement of the normalization at `idx` failed, if it did.
    pub fn measure_error(&self, t: &TrackEdit, idx: usize) -> Option<String> {
        match self.fx.measures.lock().unwrap().get(&audiofx::norm_key(t, idx)) {
            Some(Measure::Done(Err(e))) => Some(e.clone()),
            _ => None,
        }
    }

    /// What mpv should play in the track view: every audible track, with its rendered edits.
    pub fn want_mix(&self, ctx: &egui::Context) -> PreviewAudio {
        PreviewAudio::Mix(
            ffcut_core::audible(&self.s.tracks).iter().map(|t| (self.preview_source(ctx, t), (t.volume.max(0.0) * 1000.0).round() as u32)).collect(),
        )
    }
}
