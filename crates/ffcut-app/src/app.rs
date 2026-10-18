//! Application state and the per-frame glue. Widgets live in `ui/*`.

use crate::display::Backend;
use crate::player::Player;
use ffcut_app::audiomix;
use crate::settings::{self, Settings};
use crate::theme;
use eframe::egui::{self, Key, Modifiers};
use ffcut_core::cmdline;
use ffcut_core::presets::{self, Presets};
use ffcut_core::probe::{self, Encoders};
use ffcut_core::run::{Event, Job};
use ffcut_core::tools::find_tool;
use ffcut_core::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const CONTAINERS: [&str; 4] = ["mp4", "mkv", "mov", "webm"];

/// Two kinds of fullscreen that replace each other (never stack):
/// `App` = the whole interface fills the screen (F11), `Video` = only the picture does (Ctrl+F).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FsMode {
    Off,
    App,
    Video,
}

/// What the preview plays: one chosen track, or every audible track of the timeline mixed together.
#[derive(Clone, PartialEq)]
pub enum PreviewAudio {
    Single(usize),
    /// (track, volume in 1/1000)
    Mix(Vec<(TrackSource, u32)>),
}

/// Waveform of one audio track, computed in the background.
pub enum Wave {
    Loading,
    Ready(Vec<f32>),
    Failed,
}

/// What a drag on an audio track lane is doing.
#[derive(Clone, Copy, PartialEq)]
pub enum TrkDrag {
    Seek,
    TrimStart,
    TrimEnd,
    Move,
    FadeIn,
    FadeOut,
    /// A volume keyframe (index into the track's sorted `envelope`).
    EnvPt(usize),
}

pub struct Clip {
    pub path: PathBuf,
    pub info: SourceInfo,
}

pub struct Running {
    pub job: Job,
    pub started: Instant,
    pub percent: Option<f32>,
    pub speed: Option<f32>,
    pub out_time: f64,
    pub output: PathBuf,
}

pub enum Status {
    Info(String),
    Done(PathBuf),
    Error(String),
}

#[derive(Clone, Copy, PartialEq)]
pub enum TlDrag {
    Seek,
    Start,
    End,
}

pub struct App {
    pub player: Player,
    pub settings: Settings,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub ffmpeg_version: String,
    pub encoders: Arc<Mutex<Option<Encoders>>>,

    pub clip: Option<Clip>,
    pub s: ExportSettings,
    pub container: String,
    pub out_str: String,

    pub trim_start: f64,
    pub trim_end: f64,
    pub loop_on: bool,
    pub lock_trim: bool,
    pub tl_drag: Option<TlDrag>,

    pub crop_on: bool,
    /// Crop edges in frame pixels (x0, y0, x1, y1) of the rotated+flipped frame. Floats so slow drags work.
    pub crop_edges: [f32; 4],

    pub preview_audio: usize,
    pub volume: f32,
    pub muted: bool,

    pub cmd_text: String,
    pub cmd_edited: bool,
    pub cmd_stale: bool,
    pub last_auto: String,

    pub job: Option<Running>,
    pub status: Option<Status>,
    pub error_window: Option<(String, String)>,

    pub presets: Presets,
    pub presets_path: PathBuf,
    pub preset_name: String,
    pub show_save_preset: bool,
    pub show_settings: bool,
    pub show_about: bool,
    pub meter: crate::ui::MeterState,
    pub fx: crate::fx::FxState,
    /// After an audio change: (position to hold, when, seek counter). See `watch_audio_jump`.
    pub audio_hold: Option<(f64, Instant, u64)>,
    pub settings_tab: usize,
    pub window_title: String,
    pub show_update: bool,
    /// A newer release than this one (None = up to date / unknown).
    pub update_found: Option<crate::update::Release>,
    pub update_rx: Option<std::sync::mpsc::Receiver<crate::update::Outcome>>,
    /// Result line of the last manual "Check now" (Settings / About).
    pub update_msg: Option<String>,
    pub update_manual: bool,
    pub fs: FsMode,
    /// Timeline shows one lane per audio track (waveforms, per-track edits).
    pub multitrack: bool,
    pub waves: Arc<Mutex<HashMap<String, Wave>>>,
    pub trk_drag: Option<(usize, TrkDrag)>,
    /// What mpv was last told to play (see `sync_preview_audio`).
    pub applied_audio: Option<PreviewAudio>,
    /// Windowing system in use (Linux: Wayland or X11).
    pub backend: Backend,
    pub show_presets: bool,
    /// (old name, text being typed) while a preset is being renamed in the editor.
    pub preset_rename: Option<(String, String)>,
    pub preset_msg: Option<String>,
    /// File from the command line: opened after the first frame so mpv has a running render loop.
    pub pending_open: Option<PathBuf>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>, backend: Backend) -> Result<Self, String> {
        theme::apply(&cc.egui_ctx);
        let player = Player::new(cc)?;
        let settings = Settings::load();
        let presets_path = settings::config_dir().join("presets.json");

        let mut app = App {
            player,
            ffmpeg: None,
            ffprobe: None,
            ffmpeg_version: String::new(),
            encoders: Arc::new(Mutex::new(None)),
            clip: None,
            s: ExportSettings::default(),
            container: "mp4".into(),
            out_str: String::new(),
            trim_start: 0.0,
            trim_end: 0.0,
            loop_on: false,
            lock_trim: false,
            tl_drag: None,
            crop_on: false,
            crop_edges: [0.0; 4],
            preview_audio: 0,
            volume: settings.volume.unwrap_or(100.0),
            muted: settings.muted,
            cmd_text: String::new(),
            cmd_edited: false,
            cmd_stale: false,
            last_auto: String::new(),
            job: None,
            status: None,
            error_window: None,
            presets: presets::load(&presets_path),
            presets_path,
            preset_name: String::new(),
            show_save_preset: false,
            show_settings: false,
            show_about: false,
            meter: Default::default(),
            fx: Default::default(),
            audio_hold: None,
            settings_tab: 0,
            window_title: String::new(),
            show_update: false,
            update_found: None,
            update_rx: None,
            update_msg: None,
            update_manual: false,
            fs: FsMode::Off,
            multitrack: false,
            waves: Arc::new(Mutex::new(HashMap::new())),
            trk_drag: None,
            applied_audio: None,
            backend,
            show_presets: false,
            preset_rename: None,
            preset_msg: None,
            pending_open: open,
            settings,
        };
        app.locate_tools(&cc.egui_ctx);
        if !app.settings.no_update_check && std::env::var_os("FFCUT_NO_UPDATE").is_none() {
            app.start_update_check(&cc.egui_ctx, false);
        }
        app.player.set_volume(app.volume as f64);
        app.player.set_mute(app.muted);
        Ok(app)
    }

    // ---------------------------------------------------------------- updates

    /// `manual`: the user pressed "Check now", so say how it went even when nothing is new.
    pub fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update_rx.is_some() {
            return;
        }
        if !crate::update::configured() {
            if manual {
                self.update_msg = Some("The update source is not set up in this build.".into());
            }
            return;
        }
        self.update_manual = manual;
        if manual {
            self.update_msg = Some("Checking…".into());
        }
        let ctx = ctx.clone();
        self.update_rx = Some(crate::update::check_in_background(move || ctx.request_repaint()));
    }

    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.update_rx = None;
        match res {
            Ok(Some(r)) => {
                self.update_msg = Some(format!("Version {} is available.", r.version));
                if self.update_manual {
                    self.settings.skipped_version.clear(); // asked for it explicitly: show it again
                    self.show_update = true;
                }
                if r.version != self.settings.skipped_version {
                    self.update_found = Some(r);
                }
            }
            Ok(None) => {
                self.update_found = None;
                self.update_msg = Some(format!("You have the latest version ({}).", crate::update::VERSION));
            }
            Err(e) => self.update_msg = if self.update_manual { Some(format!("Could not check: {e}")) } else { None },
        }
        self.update_manual = false;
    }

    // ---------------------------------------------------------------- tools

    pub fn locate_tools(&mut self, ctx: &egui::Context) {
        let ov = |s: &str| if s.trim().is_empty() { None } else { Some(PathBuf::from(s.trim())) };
        self.ffmpeg = find_tool("ffmpeg", ov(&self.settings.ffmpeg_path).as_deref());
        self.ffprobe = find_tool("ffprobe", ov(&self.settings.ffprobe_path).as_deref());
        self.ffmpeg_version = self
            .ffmpeg
            .as_ref()
            .and_then(|p| ffcut_core::run::command(p).arg("-version").output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string())
            .unwrap_or_default();

        // the encoder list takes a moment: fetch it off the UI thread
        *self.encoders.lock().unwrap() = None;
        if let Some(ff) = self.ffmpeg.clone() {
            let slot = self.encoders.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                *slot.lock().unwrap() = probe::list_encoders(&ff);
                ctx.request_repaint();
            });
        }
    }

    pub fn tools_ok(&self) -> bool {
        self.ffmpeg.is_some() && self.ffprobe.is_some()
    }

    // ---------------------------------------------------------------- clip

    pub fn open_file(&mut self, path: &Path) {
        let path = &std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let Some(ffprobe) = self.ffprobe.clone() else {
            self.error_window = Some(("ffprobe not found".into(), "Set the path to ffmpeg/ffprobe in File → Settings.".into()));
            return;
        };
        let info = match probe::probe(&ffprobe, path) {
            Ok(i) => i,
            Err(e) => {
                self.error_window = Some((format!("Cannot open {}", path.file_name().unwrap_or_default().to_string_lossy()), e.to_string()));
                return;
            }
        };

        // clip-specific state is reset; the user's encoding choices (codec, quality...) stay
        self.s.trim_start = 0.0;
        self.s.trim_end = None;
        self.s.rotation = Rotation::None;
        self.s.flip_h = false;
        self.s.flip_v = false;
        self.s.crop = None;
        self.s.audio_mode = AudioMode::Separate;
        self.s.tracks = info.audio.iter().map(|a| TrackEdit::original(a.index, crate::ui::audio_label(a), info.duration)).collect();
        self.waves.lock().unwrap().clear();
        self.fx.clear();
        self.trk_drag = None;
        self.applied_audio = None;
        self.trim_start = 0.0;
        self.trim_end = info.duration;
        self.crop_on = false;
        self.crop_edges = [0.0, 0.0, info.width as f32, info.height as f32];
        self.preview_audio = 0;
        self.loop_on = false;
        self.cmd_edited = false;
        self.cmd_stale = false;
        self.status = None;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        self.container = if CONTAINERS.contains(&ext.as_str()) { ext } else { "mp4".into() };
        self.out_str = default_output(path, &self.container).to_string_lossy().into_owned();

        self.player.open(path, info.duration);
        self.clip = Some(Clip { path: path.to_path_buf(), info });
        self.apply_preview_transform();
        self.player.set_loop(None);
    }

    pub fn open_dialog(&mut self) {
        let dlg = rfd::FileDialog::new()
            .set_title("Open video")
            .add_filter("Video", &["mp4", "mkv", "mov", "webm", "avi", "m4v", "flv", "wmv", "ts", "mts", "m2ts", "mpg", "mpeg", "3gp", "ogv"])
            .add_filter("All files", &["*"]);
        if let Some(p) = dlg.pick_file() {
            self.open_file(&p);
        }
    }

    pub fn dur(&self) -> f64 {
        self.clip.as_ref().map_or(0.0, |c| c.info.duration)
    }

    /// Size of the frame the user sees (after rotation) — the coordinate space of the crop.
    pub fn frame_dims(&self) -> (u32, u32) {
        let Some(c) = &self.clip else { return (16, 9) };
        match self.s.rotation {
            Rotation::Cw90 | Rotation::Ccw90 => (c.info.height, c.info.width),
            _ => (c.info.width, c.info.height),
        }
    }

    pub fn set_rotation(&mut self, r: Rotation) {
        self.s.rotation = r;
        let (w, h) = self.frame_dims();
        self.crop_edges = [0.0, 0.0, w as f32, h as f32]; // the frame changed shape: restart the crop
        self.apply_preview_transform();
    }

    pub fn rotate_by(&mut self, quarter_turns_cw: i32) {
        let cur = match self.s.rotation {
            Rotation::None => 0,
            Rotation::Cw90 => 1,
            Rotation::R180 => 2,
            Rotation::Ccw90 => 3,
        };
        let n = (cur + quarter_turns_cw).rem_euclid(4);
        self.set_rotation([Rotation::None, Rotation::Cw90, Rotation::R180, Rotation::Ccw90][n as usize]);
    }

    /// Pushes rotate/flip to mpv so the preview matches what ffmpeg will do.
    /// ffmpeg: rotate, then flip (in the picture you see). mpv rotates on the GPU; the flip is done by
    /// the player when it draws the frame, so it is in the same displayed frame and needs no video filter.
    pub fn apply_preview_transform(&self) {
        let user = match self.s.rotation {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::R180 => 180,
            Rotation::Ccw90 => 270,
        };
        let container = self.clip.as_ref().map_or(0, |c| c.info.rotation as i64);
        let total = (user + container) % 360;
        self.player.set_transform(total, self.s.flip_h, self.s.flip_v);
    }

    pub fn crop_rect(&self) -> Crop {
        let (fw, fh) = self.frame_dims();
        let e = self.crop_edges;
        let even = |v: f32| ((v / 2.0).round() * 2.0).max(0.0) as u32;
        let x0 = even(e[0].min(e[2])).min(fw.saturating_sub(2));
        let y0 = even(e[1].min(e[3])).min(fh.saturating_sub(2));
        let x1 = even(e[0].max(e[2])).clamp(x0 + 2, fw.max(x0 + 2));
        let y1 = even(e[1].max(e[3])).clamp(y0 + 2, fh.max(y0 + 2));
        Crop { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    pub fn reset_crop(&mut self) {
        let (w, h) = self.frame_dims();
        self.crop_edges = [0.0, 0.0, w as f32, h as f32];
    }

    // ------------------------------------------------------------ playback

    pub fn play_pause(&mut self) {
        if self.clip.is_none() {
            return;
        }
        if self.player.paused() {
            let t = self.player.time();
            if t < self.trim_start - 0.01 || t >= self.trim_end - 0.02 {
                self.player.seek(self.trim_start);
            }
            self.player.set_paused(false);
        } else {
            self.player.set_paused(true);
        }
    }

    pub fn sync_loop(&self) {
        if self.loop_on && self.trim_end > self.trim_start {
            self.player.set_loop(Some((self.trim_start, self.trim_end)));
        } else {
            self.player.set_loop(None);
        }
    }

    /// Keeps playback inside the selected range.
    fn enforce_range(&self) {
        if self.clip.is_none() || self.player.paused() || self.loop_on {
            return;
        }
        if self.player.time() >= self.trim_end - 0.01 && self.trim_end < self.dur() - 0.01 {
            self.player.set_paused(true);
            self.player.seek(self.trim_end);
        }
    }

    // ------------------------------------------------------------- command

    /// Pulls UI state into `ExportSettings` and rebuilds the auto command.
    /// A command the user edited by hand is never overwritten: it is marked stale instead.
    pub fn refresh_command(&mut self) {
        let Some(clip) = &self.clip else { return };
        let dur = clip.info.duration;
        self.s.trim_start = self.trim_start;
        self.s.trim_end = if self.trim_end >= dur - 0.01 { None } else { Some(self.trim_end) };
        self.s.crop = if self.crop_on { Some(self.crop_rect()) } else { None };

        let auto = cmdline::to_text(&build_args(&clip.info, &self.s, &clip.path, Path::new(&self.out_str)));
        if auto != self.last_auto {
            if self.cmd_edited {
                self.cmd_stale = true;
            } else {
                self.cmd_text = auto.clone();
            }
            self.last_auto = auto;
        }
        if self.cmd_edited && self.cmd_text == self.last_auto {
            self.cmd_edited = false;
            self.cmd_stale = false;
        }
    }

    pub fn reset_command(&mut self) {
        self.cmd_text = self.last_auto.clone();
        self.cmd_edited = false;
        self.cmd_stale = false;
    }

    pub fn set_container(&mut self, ext: &str) {
        self.container = ext.to_string();
        let mut p = PathBuf::from(&self.out_str);
        p.set_extension(ext);
        self.out_str = p.to_string_lossy().into_owned();
    }

    // -------------------------------------------------------------- export

    /// Export button: asks where to save, then runs the command with that output path.
    pub fn export_clicked(&mut self) {
        let Some(clip) = &self.clip else { return };
        let mut argv = match cmdline::parse(&self.cmd_text) {
            Ok(a) => a,
            Err(e) => {
                self.status = Some(Status::Error(format!("Command error: {e}")));
                return;
            }
        };
        let Some(current) = argv.last().filter(|l| argv.len() >= 4 && !l.starts_with('-')).cloned() else {
            self.status = Some(Status::Error("The command must end with an output file name".into()));
            return;
        };

        let cur = PathBuf::from(&current);
        let mut dlg = rfd::FileDialog::new().set_title("Export video").add_filter("Video", &CONTAINERS);
        if let Some(d) = cur.parent().filter(|d| d.is_dir()).or_else(|| clip.path.parent()) {
            dlg = dlg.set_directory(d);
        }
        if let Some(n) = cur.file_name() {
            dlg = dlg.set_file_name(n.to_string_lossy());
        }
        // FFCUT_EXPORT_TO: debug/automation only, skips the dialog
        let picked = std::env::var_os("FFCUT_EXPORT_TO").map(PathBuf::from).or_else(|| dlg.save_file());
        let Some(mut out) = picked else { return }; // cancelled
        if out.extension().is_none() {
            out.set_extension(&self.container); // the dialog confirmed overwriting already
        }
        if same_file(&out, &clip.path) {
            self.status = Some(Status::Error("Output file is the same as the input — pick another name".into()));
            return;
        }

        // the chosen path goes into the command; a hand-edited command keeps all its other changes
        if let Some(ext) = out.extension().and_then(|e| e.to_str()) {
            if CONTAINERS.contains(&ext.to_ascii_lowercase().as_str()) {
                self.container = ext.to_ascii_lowercase();
            }
        }
        self.out_str = out.to_string_lossy().into_owned();
        *argv.last_mut().unwrap() = self.out_str.clone();
        if self.cmd_edited {
            self.cmd_text = cmdline::to_text(&argv);
        }
        self.refresh_command();
        if self.cmd_edited {
            self.cmd_stale = false; // only the path changed, and we just applied it
        }
        self.run_export(argv);
    }

    pub fn run_export(&mut self, mut argv: Vec<String>) {
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.status = Some(Status::Error("ffmpeg not found — see File → Settings".into()));
            return;
        };
        if !argv.iter().any(|a| a == "-y" || a == "-n") {
            argv.insert(1, "-y".into()); // overwrite was confirmed
        }
        let output = PathBuf::from(argv.last().cloned().unwrap_or_default());
        let expected = (self.trim_end - self.trim_start).max(0.0);
        match Job::spawn(&ffmpeg, &argv, Some(expected)) {
            Ok(job) => {
                self.status = None;
                self.job = Some(Running { job, started: Instant::now(), percent: None, speed: None, out_time: 0.0, output });
            }
            Err(e) => self.status = Some(Status::Error(format!("Cannot start ffmpeg: {e}"))),
        }
    }

    fn poll_job(&mut self) {
        let Some(run) = &mut self.job else { return };
        let mut finished: Option<Event> = None;
        while let Ok(ev) = run.job.events.try_recv() {
            match ev {
                Event::Progress { percent, out_time, speed, .. } => {
                    run.percent = percent;
                    run.out_time = out_time;
                    run.speed = speed;
                }
                e => finished = Some(e),
            }
        }
        if let Some(ev) = finished {
            let output = run.output.clone();
            self.job = None;
            self.status = Some(match ev {
                Event::Finished => Status::Done(output),
                Event::Cancelled => Status::Info("Export cancelled".into()),
                Event::Failed(msg) => {
                    self.error_window = Some(("Export failed".into(), msg));
                    Status::Error("Export failed".into())
                }
                Event::Progress { .. } => unreachable!(),
            });
        }
    }

    pub fn set_fs(&mut self, ctx: &egui::Context, mode: FsMode) {
        self.fs = mode;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(mode != FsMode::Off));
    }

    /// Keeps mpv's audio in step with the interface: in the track view every audible track plays
    /// together (volume applies; mute / solo decide who is audible), otherwise the chosen single track.
    /// mpv answers an audio change with a seek of its own a moment later (to the next keyframe, which can be
    /// seconds away). For a short while after a change, put the playhead back if that happened.
    fn watch_audio_jump(&mut self, ctx: &egui::Context) {
        let Some((pos, at, gen)) = self.audio_hold else { return };
        let el = at.elapsed().as_secs_f64();
        if self.player.seek_gen() != gen || el > 1.5 {
            self.audio_hold = None; // the user moved the playhead, or nothing happened
            return;
        }
        let expected = if self.player.paused() { pos } else { pos + el };
        if el > 0.12 && (self.player.real_time() - expected).abs() > 0.6 {
            self.player.seek(expected);
            self.audio_hold = Some((expected, Instant::now(), self.player.seek_gen()));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
    }

    pub fn sync_preview_audio(&mut self, ctx: &egui::Context) {
        self.watch_audio_jump(ctx);
        if self.clip.is_none() {
            return;
        }
        let want = if self.multitrack {
            self.want_mix(ctx)
        } else {
            // one chosen track; if it carries edits (shift, trim, effects) it is played rendered, like in the track view
            let pick = self.s.tracks.iter().find(|t| t.source == TrackSource::Original(self.preview_audio) && !t.deleted).filter(|t| t.needs_bake(self.dur()) || (t.volume - 1.0).abs() > 0.001).cloned();
            match pick {
                Some(t) => PreviewAudio::Mix(vec![(self.preview_source(ctx, &t), (t.volume.max(0.0) * 1000.0).round() as u32)]),
                None => PreviewAudio::Single(self.preview_audio),
            }
        };
        if self.applied_audio.as_ref() == Some(&want) {
            return;
        }
        // not while a slider is being dragged (every step would restart mpv's audio), and not before the file is open
        let m = self.player.mpv();
        let ready = m.get_property::<i64>("track-list/count").unwrap_or(0) > 0;
        if !ready || !self.settled() || ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
            return;
        }
        // changing the audio graph while playing makes mpv jump ahead (to the end of its read-ahead
        // buffer, seconds away); put the playhead back where it was
        let keep = self.player.real_time();
        let result = match &want {
            PreviewAudio::Single(i) => audiomix::single_track(m, *i as i64 + 1),
            PreviewAudio::Mix(parts) => {
                let mut ids = Vec::new();
                for (src, v) in parts {
                    let id = match src {
                        TrackSource::Original(i) => Some(*i as i64 + 1),
                        TrackSource::External { path, .. } => audiomix::ensure_external(m, path),
                    };
                    if let Some(id) = id {
                        ids.push((id, *v as f32 / 1000.0));
                    }
                }
                audiomix::set_mix(m, &ids)
            }
        };
        if let Err(e) = result {
            eprintln!("audio preview: {e}");
        }
        self.player.seek(keep);
        self.audio_hold = Some((keep, Instant::now(), self.player.seek_gen()));
        self.applied_audio = Some(want);
    }

    // --------------------------------------------------------------- audio tracks

    pub fn wave_key(&self, src: &TrackSource) -> String {
        match (src, &self.clip) {
            (TrackSource::Original(i), Some(c)) => format!("{}#{i}", c.path.display()),
            (TrackSource::External { path, .. }, _) => format!("{path}#0"),
            _ => String::new(),
        }
    }

    /// Starts computing a waveform in the background (once per track).
    pub fn want_wave(&self, ctx: &egui::Context, src: &TrackSource) {
        let (Some(ffmpeg), Some(clip)) = (self.ffmpeg.clone(), &self.clip) else { return };
        let key = self.wave_key(src);
        {
            let mut w = self.waves.lock().unwrap();
            if w.contains_key(&key) {
                return;
            }
            w.insert(key.clone(), Wave::Loading);
        }
        let (file, ordinal) = match src {
            TrackSource::Original(i) => (clip.path.clone(), *i),
            TrackSource::External { path, .. } => (PathBuf::from(path), 0),
        };
        let (waves, ctx) = (self.waves.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = match ffcut_core::waveform::peaks(&ffmpeg, &file, ordinal) {
                Ok(p) => Wave::Ready(p),
                Err(_) => Wave::Failed,
            };
            waves.lock().unwrap().insert(key, r);
            ctx.request_repaint();
        });
    }

    fn pick_audio_file(&mut self, title: &str) -> Option<(PathBuf, probe::AudioFileInfo)> {
        let ffprobe = self.ffprobe.clone()?;
        // FFCUT_PICK_AUDIO: debug/automation only, skips the dialog
        let p = std::env::var_os("FFCUT_PICK_AUDIO").map(PathBuf::from).or_else(|| {
            rfd::FileDialog::new()
                .set_title(title)
                .add_filter("Audio / video", &["wav", "mp3", "flac", "m4a", "aac", "ogg", "opus", "wma", "ac3", "mka", "mp4", "mkv", "mov", "webm"])
                .add_filter("All files", &["*"])
                .pick_file()
        })?;
        match probe::probe_audio(&ffprobe, &p) {
            Ok(i) => Some((p, i)),
            Err(e) => {
                self.error_window = Some((format!("Cannot use {}", p.file_name().unwrap_or_default().to_string_lossy()), e.to_string()));
                None
            }
        }
    }

    pub fn add_audio_track(&mut self) {
        if let Some((p, info)) = self.pick_audio_file("Add audio track") {
            self.s.tracks.push(TrackEdit::external(p.to_string_lossy().into_owned(), info.label, info.duration));
        }
    }

    pub fn replace_track(&mut self, idx: usize) {
        if let Some((p, info)) = self.pick_audio_file("Replace audio with…") {
            if let Some(t) = self.s.tracks.get_mut(idx) {
                let vol = t.volume;
                *t = TrackEdit::external(p.to_string_lossy().into_owned(), info.label, info.duration);
                t.volume = vol;
            }
        }
    }

    pub fn reset_tracks(&mut self) {
        let Some(c) = &self.clip else { return };
        self.s.tracks = c.info.audio.iter().map(|a| TrackEdit::original(a.index, crate::ui::audio_label(a), c.info.duration)).collect();
    }

    /// Anything on the track list differs from the untouched file (removed, muted, solo, trimmed ...).
    pub fn tracks_edited(&self) -> bool {
        let d = self.dur();
        self.s.tracks.iter().any(|t| t.is_modified(d))
    }

    /// Do the timeline edits force audio through ffmpeg filters (= re-encode)?
    /// Removing a track on its own does not: it only changes which streams are copied.
    pub fn tracks_need_filters(&self) -> bool {
        let d = self.dur();
        audible(&self.s.tracks).iter().any(|t| t.needs_filter(d))
    }

    // --------------------------------------------------------------- input

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(p) = dropped.first().map(|f| f.path().to_path_buf()) {
            self.open_file(&p);
        }
    }

    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        let k = |ctx: &egui::Context, m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
        // global shortcuts work even while typing
        if k(ctx, Modifiers::COMMAND, Key::O) || k(ctx, Modifiers::COMMAND, Key::N) {
            self.open_dialog();
        }
        if (k(ctx, Modifiers::COMMAND, Key::E) || k(ctx, Modifiers::COMMAND, Key::S)) && self.job.is_none() {
            self.export_clicked();
        }
        if k(ctx, Modifiers::NONE, Key::F11) {
            self.set_fs(ctx, if self.fs == FsMode::App { FsMode::Off } else { FsMode::App });
        }
        if k(ctx, Modifiers::COMMAND, Key::F) && self.clip.is_some() {
            self.set_fs(ctx, if self.fs == FsMode::Video { FsMode::Off } else { FsMode::Video });
        }
        if self.fs == FsMode::Video && k(ctx, Modifiers::NONE, Key::Escape) {
            self.set_fs(ctx, FsMode::Off);
        }
        if ctx.egui_wants_keyboard_input() || self.clip.is_none() {
            return;
        }
        if k(ctx, Modifiers::NONE, Key::Space) {
            self.play_pause();
        }
        let fast = Modifiers::SHIFT;
        if k(ctx, fast, Key::ArrowLeft) {
            self.player.seek((self.player.time() - 5.0).max(0.0));
        } else if k(ctx, fast, Key::ArrowRight) {
            self.player.seek(self.player.time() + 5.0);
        }
        if k(ctx, Modifiers::NONE, Key::ArrowLeft) || k(ctx, Modifiers::NONE, Key::Comma) {
            self.player.step_frame(false);
        }
        if k(ctx, Modifiers::NONE, Key::ArrowRight) || k(ctx, Modifiers::NONE, Key::Period) {
            self.player.step_frame(true);
        }
        if !self.lock_trim {
            if k(ctx, Modifiers::NONE, Key::Q) {
                self.trim_start = self.player.time().min(self.trim_end - 0.05).max(0.0);
                self.sync_loop();
            }
            if k(ctx, Modifiers::NONE, Key::E) {
                self.trim_end = self.player.time().max(self.trim_start + 0.05).min(self.dur());
                self.sync_loop();
            }
            if k(ctx, Modifiers::NONE, Key::R) {
                self.trim_start = 0.0;
                self.trim_end = self.dur();
                self.sync_loop();
            }
        }
        if k(ctx, Modifiers::NONE, Key::C) {
            self.crop_on = !self.crop_on;
        }
        if k(ctx, Modifiers::SHIFT, Key::A) {
            self.lock_trim = !self.lock_trim;
        } else if k(ctx, Modifiers::NONE, Key::A) {
            self.loop_on = !self.loop_on;
            self.sync_loop();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(p) = self.pending_open.take() {
            self.open_file(&p);
        }
        self.handle_drops(&ctx);
        self.handle_hotkeys(&ctx);
        self.poll_job();
        self.poll_update();
        self.track_changes(&ctx);
        self.drive_measures(&ctx);
        self.sync_preview_audio(&ctx);
        self.enforce_range();
        self.refresh_command();
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.player.settling() || !self.player.paused() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }

        let title = crate::layout::render_title(&self.settings.title_template, self.clip.as_ref().map(|c| (c.path.as_path(), &c.info)), &self.settings.path_sep);
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        if self.fs != FsMode::Video {
            // panel order matters: the sidebar claims its strip first, so the timeline (added
            // next) only spans the space left of it
            self.menu_bar(ui);
            self.side_panel(ui);
            self.transport_panel(ui);
        }
        self.video_panel(ui);
        self.windows(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.volume = Some(self.volume);
        self.settings.muted = self.muted;
        self.settings.save();
        if let Some(r) = &self.job {
            r.job.cancel();
        }
    }
}

pub fn default_output(input: &Path, ext: &str) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    input.with_file_name(format!("{stem}_cut.{ext}"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub fn fmt_time(t: f64) -> String {
    let t = t.max(0.0);
    let ms = ((t - t.floor()) * 1000.0).round() as u32 % 1000;
    let s = t.floor() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{ms:03}")
    } else {
        format!("{m:02}:{s:02}.{ms:03}")
    }
}

/// Accepts `90`, `1:30`, `1:02:03.5`.
pub fn parse_time(s: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in s.trim().split(':') {
        total = total * 60.0 + part.trim().replace(',', ".").parse::<f64>().ok()?;
    }
    Some(total)
}

pub fn open_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(Path::new("."))).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_roundtrip() {
        assert_eq!(fmt_time(75.5), "01:15.500");
        assert_eq!(fmt_time(3723.25), "1:02:03.250");
        assert_eq!(parse_time("1:15.5"), Some(75.5));
        assert_eq!(parse_time("1:02:03.25"), Some(3723.25));
        assert_eq!(parse_time("abc"), None);
    }
}
