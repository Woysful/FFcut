//! The sidebar: one scrolling column of sections that all look the same, with the Export button pinned below.
//!
//! Order follows the work: what the file is → which part → how the picture is changed → effects → sound →
//! what comes out → the exact command. Every section remembers whether it was left open.

use super::widgets::{self, caption, field, hint, num, resettable, warn};
use super::{audio_label, codecs};
use crate::app::{fmt_time, parse_time, App, AUDIO_EXPORT, CONTAINERS};
use crate::hotkeys::Action;
use crate::theme::{self, size};
use eframe::egui::{self, pos2, vec2, Color32, DragValue, RichText, Sense, Stroke};
use ffcut_core::*;

pub const SPEEDS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
pub const PIX_FMTS: [&str; 7] = ["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "nv12", "p010le"];

/// Sidebar sections; the key is what settings.json remembers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    File,
    Trim,
    Transform,
    VideoFx,
    Audio,
    Volume,
    Output,
    Command,
}

impl Section {
    pub fn key(self) -> &'static str {
        match self {
            Section::File => "file",
            Section::Trim => "trim",
            Section::Transform => "transform",
            Section::VideoFx => "video-fx",
            Section::Audio => "audio",
            Section::Volume => "volume",
            Section::Output => "output",
            Section::Command => "command",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Section::File => "File",
            Section::Trim => "Trim",
            Section::Transform => "Crop & transform",
            Section::VideoFx => "Video effects & text",
            Section::Audio => "Audio",
            Section::Volume => "Volume",
            Section::Output => "Output",
            Section::Command => "Command",
        }
    }

    fn default_open(self) -> bool {
        !matches!(self, Section::File | Section::VideoFx)
    }
}

/// Sidebar state that is not part of the edit.
#[derive(Default)]
pub struct SideState {
    /// Open this section and scroll to it on the next frame (something elsewhere pointed at it).
    pub reveal: Option<Section>,
    /// The audio track whose effects the sidebar shows.
    pub fx_track: Option<usize>,
}

/// A time value as a time code, or as a frame number when `fps` is given.
pub(super) fn time_field(ui: &mut egui::Ui, v: &mut f64, min: f64, max: f64, fps: Option<f64>) -> egui::Response {
    let dv = DragValue::new(v).range(min..=max);
    let dv = match fps {
        None => dv.speed(0.02).custom_formatter(|n, _| fmt_time(n)).custom_parser(parse_time),
        Some(fps) => dv
            .speed(1.0 / fps)
            .custom_formatter(move |n, _| format!("{}", (n * fps).round() as i64))
            // a plain number is a frame number; "1:30" is still read as a time
            .custom_parser(move |s| if s.contains(':') { parse_time(s) } else { s.trim().parse::<f64>().ok().map(|f| f / fps) }),
    };
    widgets::value_sized(ui, [110.0, ui.spacing().interact_size.y], dv)
}

/// Width × height with 0 = "auto" (keeps the proportions). True when changed.
pub(super) fn size_fields(ui: &mut egui::Ui, scale: &mut Scale) -> bool {
    let (mut w, mut h) = (scale.w.unwrap_or(0), scale.h.unwrap_or(0));
    let fmt = |n: f64, _: std::ops::RangeInclusive<usize>| if n == 0.0 { "auto".to_string() } else { format!("{n:.0}") };
    let rw = num(ui, DragValue::new(&mut w).range(0..=16384).speed(2.0).custom_formatter(fmt)).on_hover_text("Width in pixels (auto keeps the proportions)");
    let rw = resettable(rw, &mut w, 0);
    let rh = num(ui, DragValue::new(&mut h).range(0..=16384).speed(2.0).custom_formatter(fmt)).on_hover_text("Height in pixels (auto keeps the proportions)");
    let rh = resettable(rh, &mut h, 0);
    let changed = rw.changed() || rh.changed();
    if changed {
        *scale = Scale { w: (w > 0).then_some(w), h: (h > 0).then_some(h) };
    }
    changed
}

/// Constant quality or target bitrate. True when changed.
pub(super) fn quality_fields(ui: &mut egui::Ui, q: &mut Quality) -> bool {
    let mut changed = false;
    let mut crf = matches!(q, Quality::Crf(_));
    if ui.radio_value(&mut crf, true, "Quality").on_hover_text("Constant quality (CRF): the size follows the content").clicked()
        && !matches!(q, Quality::Crf(_))
    {
        *q = Quality::Crf(23);
        changed = true;
    }
    if ui.radio_value(&mut crf, false, "Bitrate").on_hover_text("A target bitrate: the size is predictable").clicked() && !matches!(q, Quality::Bitrate(_)) {
        *q = Quality::Bitrate(8000);
        changed = true;
    }
    changed
}

fn chevron(p: &egui::Painter, c: egui::Pos2, open: f32, col: Color32) {
    // a small triangle that turns from ▸ to ▾
    let a = open * std::f32::consts::FRAC_PI_2;
    let rot = |x: f32, y: f32| pos2(c.x + x * a.cos() - y * a.sin(), c.y + x * a.sin() + y * a.cos());
    p.add(egui::Shape::convex_polygon(vec![rot(-2.5, -4.0), rot(-2.5, 4.0), rot(3.5, 0.0)], col, Stroke::NONE));
}

impl App {
    pub fn is_open(&self, s: Section) -> bool {
        self.settings.sections.get(s.key()).copied().unwrap_or(s.default_open())
    }

    /// Opens a section and scrolls the sidebar to it.
    pub fn reveal(&mut self, s: Section) {
        self.side.reveal = Some(s);
    }

    /// Shows the effects of one audio track in the sidebar.
    pub fn show_track_fx(&mut self, track: usize) {
        self.side.fx_track = Some(track);
        self.reveal(Section::Audio);
    }

    /// One sidebar section: a header row (title, a short summary of what is set, open / closed) and the body.
    pub(super) fn section(&mut self, ui: &mut egui::Ui, s: Section, summary: Option<String>, body: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let reveal = self.side.reveal == Some(s);
        let mut open = self.is_open(s) || reveal;

        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        if resp.clicked() {
            open = !open;
        }
        if reveal {
            resp.scroll_to_me(Some(egui::Align::TOP));
            self.side.reveal = None;
        }
        let t = ui.ctx().animate_bool(resp.id, open);
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect.expand2(vec2(4.0, 0.0)), 6.0, Color32::from_white_alpha(6));
        }
        let col = if resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT };
        chevron(p, pos2(rect.left() + 6.0, rect.center().y), t, theme::TEXT_DIM);
        let title = p.layout_no_wrap(s.title().to_string(), egui::FontId::proportional(size::TITLE), col);
        let title_w = title.size().x;
        p.galley(pos2(rect.left() + 18.0, rect.center().y - title.size().y / 2.0), title, col);
        if let Some(sum) = summary.filter(|s| !s.is_empty()) {
            // the summary takes what the title leaves, cut at the left so its end (the newest part) stays
            let room = rect.width() - title_w - 34.0;
            if room > 30.0 {
                let job = egui::text::LayoutJob {
                    wrap: egui::text::TextWrapping::truncate_at_width(room),
                    ..egui::text::LayoutJob::simple_singleline(sum, egui::FontId::proportional(size::HINT), theme::TEXT_DIM)
                };
                let g = ui.fonts_mut(|f| f.layout_job(job));
                p.galley(pos2(rect.right() - g.size().x - 2.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT_DIM);
            }
        }
        if open != self.is_open(s) {
            self.settings.sections.insert(s.key().to_string(), open);
            self.settings.save();
        }
        if open {
            ui.add_space(2.0);
            body(self, ui);
            ui.add_space(10.0);
        }
        let y = ui.cursor().top();
        ui.painter().hline(rect.left()..=rect.right(), y, Stroke::new(1.0, theme::LINE_FAINT));
        ui.add_space(1.0);
    }

    pub fn side_panel(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        egui::Panel::right("sidebar").default_size(360.0).size_range(320.0..=600.0).frame(egui::Frame::NONE.fill(theme::PANEL)).show(ui, |ui| {
            egui::Panel::bottom("sidebar-footer")
                .frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 12 }))
                .show(ui, |ui| self.export_footer(ui));
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 16, top: 6, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.audio_mode() {
                            self.audio_file_sections(ui);
                        } else {
                            self.video_file_sections(ui);
                        }
                    });
                });
            });
        });
    }

    fn video_file_sections(&mut self, ui: &mut egui::Ui) {
        self.file_section(ui);
        self.trim_section(ui);
        let sum = self.transform_summary();
        self.section(ui, Section::Transform, Some(sum), |a, ui| a.transform_body(ui));
        let n = self.s.vfx.count_on();
        self.section(ui, Section::VideoFx, (n > 0).then(|| format!("{n} on")), |a, ui| a.vfx_body(ui));
        self.vfx.window = self.is_open(Section::VideoFx);
        let sum = self.audio_summary();
        self.section(ui, Section::Audio, Some(sum), |a, ui| a.audio_body(ui));
        let sum = self.output_summary();
        self.section(ui, Section::Output, Some(sum), |a, ui| a.output_body(ui));
        self.command_section(ui);
    }

    /// Audio-file mode: only what concerns sound. One track, so its effects live right here.
    fn audio_file_sections(&mut self, ui: &mut egui::Ui) {
        self.file_section(ui);
        self.trim_section(ui);
        let vol = self.s.tracks.first().map(|t| format!("{:.0}%", t.volume * 100.0));
        self.section(ui, Section::Volume, vol, |a, ui| a.volume_body(ui));
        let fx = self.s.tracks.first().map_or(0, |t| t.effects.iter().filter(|e| e.on).count());
        self.section(ui, Section::Audio, (fx > 0).then(|| format!("{fx} effect(s)")), |a, ui| {
            a.fx_presets_row(ui, 0);
            ui.add_space(4.0);
            a.fx_sidebar(ui, 0);
        });
        let sum = self.output_summary();
        self.section(ui, Section::Output, Some(sum), |a, ui| a.output_body(ui));
        self.command_section(ui);
    }

    // ------------------------------------------------------------------ file

    fn file_section(&mut self, ui: &mut egui::Ui) {
        let Some(c) = &self.clip else { return };
        let i = &c.info;
        let sum =
            if i.audio_only { format!("{} · {}", i.format, fmt_time(i.duration)) } else { format!("{}×{} · {}", i.width, i.height, fmt_time(i.duration)) };
        self.section(ui, Section::File, Some(sum), |a, ui| {
            let Some(c) = &a.clip else { return };
            let i = &c.info;
            ui.add(egui::Label::new(RichText::new(c.path.file_name().unwrap_or_default().to_string_lossy()).strong().color(theme::TEXT_STRONG)).wrap());
            ui.add_space(2.0);
            let mut rows = Vec::new();
            if !i.audio_only {
                rows.push(("Video", format!("{} · {}×{} · {:.2} fps", i.video_codec, i.width, i.height, i.fps)));
                rows.push(("Pixel format", i.pix_fmt.clone()));
            }
            rows.push(("Duration", fmt_time(i.duration)));
            rows.push(("Size", format!("{:.1} MB", i.file_size as f64 / 1_048_576.0)));
            rows.push(("Container", i.format.clone()));
            if i.rotation != 0 {
                rows.push(("Rotation tag", format!("{}°", i.rotation)));
            }
            widgets::grid("file-info").spacing([8.0, 3.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(k).color(theme::TEXT_LABEL));
                    ui.label(v);
                    ui.end_row();
                }
                for (n, a) in i.audio.iter().enumerate() {
                    ui.label(RichText::new(if n == 0 { "Audio" } else { "" }).color(theme::TEXT_LABEL));
                    ui.label(audio_label(a));
                    ui.end_row();
                }
            });
        });
    }

    // ------------------------------------------------------------------ trim

    fn trim_section(&mut self, ui: &mut egui::Ui) {
        let fps = self.frame_fps();
        let len = self.trim_end - self.trim_start;
        let sum = match fps {
            Some(f) => format!("{} frames", (len * f).round() as i64),
            None => fmt_time(len),
        };
        self.section(ui, Section::Trim, Some(sum), |a, ui| {
            let dur = a.dur();
            field(ui, "Start", |ui| {
                let mut v = a.trim_start;
                let r = time_field(ui, &mut v, 0.0, (a.trim_end - 0.05).max(0.0), fps)
                    .on_hover_text(a.tip("Right-click to reset. Set to the playhead", Action::TrimStart));
                if resettable(r, &mut v, 0.0).changed() {
                    a.trim_start = v;
                    a.player.seek(v);
                    a.sync_loop();
                }
            });
            field(ui, "End", |ui| {
                let mut v = a.trim_end;
                let r =
                    time_field(ui, &mut v, a.trim_start + 0.05, dur, fps).on_hover_text(a.tip("Right-click to reset. Set to the playhead", Action::TrimEnd));
                if resettable(r, &mut v, dur).changed() {
                    a.trim_end = v;
                    a.player.seek(v);
                    a.sync_loop();
                }
            });
        });
    }

    // ------------------------------------------------------------------ crop & transform

    /// "crop · 90° · flip H · 720p", or "none".
    fn transform_summary(&self) -> String {
        let mut v: Vec<String> = Vec::new();
        if self.crop.on {
            let c = self.crop_rect();
            v.push(format!("crop {}×{}", c.w, c.h));
        }
        match self.s.rotation {
            Rotation::None => {}
            Rotation::Cw90 => v.push("90°".into()),
            Rotation::R180 => v.push("180°".into()),
            Rotation::Ccw90 => v.push("270°".into()),
        }
        if self.s.flip_h {
            v.push("flip H".into());
        }
        if self.s.flip_v {
            v.push("flip V".into());
        }
        let sc = self.s.scale;
        match (sc.w, sc.h) {
            (None, None) => {}
            (None, Some(h)) => v.push(format!("{h}p")),
            (w, h) => v.push(format!("{}×{}", w.map_or("auto".into(), |x| x.to_string()), h.map_or("auto".into(), |x| x.to_string()))),
        }
        if v.is_empty() {
            "none".into()
        } else {
            v.join(" · ")
        }
    }

    fn transform_body(&mut self, ui: &mut egui::Ui) {
        const BTN: [f32; 2] = [84.0, 26.0];
        field(ui, "Crop", |ui| {
            let on = self.crop.on;
            let label = if on { "On" } else { "Off" };
            if ui
                .add_sized(BTN, egui::Button::new(label).selected(on))
                .on_hover_text(self.tip("Crop the picture: drag the frame on the video", Action::Crop))
                .clicked()
            {
                self.set_crop(!on);
            }
            if self.crop.on && ui.button("Reset").on_hover_text("The whole frame").clicked() {
                self.reset_crop();
            }
        });
        if self.crop.on {
            let (fw, fh) = self.frame_dims();
            let c = self.crop_rect();
            let (mut x, mut y, mut w, mut h) = (c.x as f32, c.y as f32, c.w as f32, c.h as f32);
            let mut edited = false;
            let snap = "Values snap to even numbers (required by most encoders)";
            field(ui, "Position", |ui| {
                let r = num(ui, DragValue::new(&mut x).range(0.0..=(fw - 2) as f32).speed(2.0).fixed_decimals(0).prefix("x "));
                edited |= resettable(r, &mut x, 0.0).changed();
                let r = num(ui, DragValue::new(&mut y).range(0.0..=(fh - 2) as f32).speed(2.0).fixed_decimals(0).prefix("y "));
                edited |= resettable(r, &mut y, 0.0).changed();
            });
            field(ui, "Size", |ui| {
                let r = num(ui, DragValue::new(&mut w).range(2.0..=fw as f32).speed(2.0).fixed_decimals(0)).on_hover_text(format!("Width. {snap}"));
                edited |= resettable(r, &mut w, fw as f32).changed();
                let r = num(ui, DragValue::new(&mut h).range(2.0..=fh as f32).speed(2.0).fixed_decimals(0)).on_hover_text(format!("Height. {snap}"));
                edited |= resettable(r, &mut h, fh as f32).changed();
            });
            if edited {
                x = x.min(fw as f32 - 2.0);
                y = y.min(fh as f32 - 2.0);
                self.crop.edges = [x, y, (x + w).min(fw as f32), (y + h).min(fh as f32)];
            }
        }
        ui.add_space(2.0);
        field(ui, "Rotate", |ui| {
            if ui.add_sized(BTN, egui::Button::new("⟲ 90°")).on_hover_text("Rotate counter-clockwise").clicked() {
                self.rotate_by(-1);
            }
            if ui.add_sized(BTN, egui::Button::new("⟳ 90°")).on_hover_text("Rotate clockwise").clicked() {
                self.rotate_by(1);
            }
        });
        let mut flipped = false;
        field(ui, "Flip", |ui| {
            flipped |= ui.add_sized(BTN, egui::Button::new("↔ Horiz.").selected(self.s.flip_h)).on_hover_text("Mirror left to right").clicked() && {
                self.s.flip_h = !self.s.flip_h;
                true
            };
            flipped |= ui.add_sized(BTN, egui::Button::new("↕ Vert.").selected(self.s.flip_v)).on_hover_text("Mirror top to bottom").clicked() && {
                self.s.flip_v = !self.s.flip_v;
                true
            };
        });
        if flipped {
            self.apply_preview_transform();
        }
        ui.add_space(2.0);
        field(ui, "Resize", |ui| size_fields(ui, &mut self.s.scale));
        field(ui, "", |ui| {
            for (label, hh) in [("1080p", 1080u32), ("720p", 720), ("480p", 480)] {
                let preset = Scale { w: None, h: Some(hh) };
                if ui
                    .add(egui::Button::new(label).small().selected(self.s.scale == preset))
                    .on_hover_text(format!("{hh} pixels high, width to match"))
                    .clicked()
                {
                    self.s.scale = preset;
                }
            }
        });
    }

    // ------------------------------------------------------------------ audio

    fn audio_summary(&self) -> String {
        if self.s.tracks.is_empty() {
            return "no audio".into();
        }
        let n = audible(&self.s.tracks).len();
        let total = self.s.tracks.len();
        let mode = match self.s.audio_mode {
            AudioMode::None => return "off".into(),
            AudioMode::Merge if n > 1 => " · mixed",
            _ => "",
        };
        if n == total {
            format!("{n} track{}{mode}", if n == 1 { "" } else { "s" })
        } else {
            format!("{n} of {total} tracks{mode}")
        }
    }

    /// The selected track for effects, kept on a track that still exists.
    fn fx_track(&mut self) -> Option<usize> {
        let usable: Vec<usize> = (0..self.s.tracks.len()).filter(|&i| !self.s.tracks[i].deleted).collect();
        let cur = self.side.fx_track.filter(|i| usable.contains(i)).or(usable.first().copied());
        self.side.fx_track = cur;
        cur
    }

    /// The track list (the same tracks as the multi-track timeline: ticking a box here is the × there), then
    /// the effects of the selected track.
    fn audio_body(&mut self, ui: &mut egui::Ui) {
        if self.s.tracks.is_empty() {
            hint(ui, "This file has no audio.");
        }
        let heard: Vec<bool> = {
            let h = audible(&self.s.tracks);
            self.s.tracks.iter().map(|t| h.iter().any(|x| std::ptr::eq(*x, t))).collect()
        };
        let sel = self.fx_track();
        let mut toggle = None;
        let mut pick = None;
        for (i, t) in self.s.tracks.iter().enumerate() {
            let selected = sel == Some(i) && self.s.tracks.len() > 1;
            let stroke = if selected { Stroke::new(1.5, theme::ACCENT) } else { Stroke::new(1.0, theme::LINE) };
            // the row's click sense is registered before its contents, so the checkbox stays on top of it
            let row = egui::UiBuilder::new().sense(Sense::click());
            let r = ui.scope_builder(row, |ui| {
                let frame = widgets::box_frame().stroke(stroke).inner_margin(egui::Margin::symmetric(8, 3)).corner_radius(theme::radius::WIDGET);
                frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let mut on = !t.deleted;
                        if ui.checkbox(&mut on, "").on_hover_text("Include this track in the export").changed() {
                            toggle = Some(i);
                        }
                        let dim = t.deleted || !heard[i];
                        let ext = matches!(t.source, TrackSource::External { .. });
                        let name = format!("{}{}", if ext { "+ " } else { "" }, t.label);
                        ui.add(egui::Label::new(RichText::new(name).color(if dim { theme::TEXT_OFF } else { theme::TEXT })).truncate()).on_hover_text(&t.label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if t.has_fx() && !t.deleted {
                                widgets::badge(ui, "FX", theme::ACCENT);
                            }
                            let state = if t.deleted {
                                ""
                            } else if t.solo {
                                "solo"
                            } else if t.muted {
                                "muted"
                            } else if !heard[i] {
                                "silent"
                            } else {
                                ""
                            };
                            if !state.is_empty() {
                                widgets::badge(ui, state, theme::WARN).on_hover_text("Changed on the track view (S / M buttons)");
                            }
                        });
                    });
                });
            });
            if !t.deleted && r.response.clicked() {
                pick = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = toggle {
            let t = &mut self.s.tracks[i];
            t.deleted = !t.deleted;
        }
        if let Some(i) = pick {
            self.side.fx_track = Some(i);
        }
        let w = ui.available_width();
        if ui.add_sized([w, 26.0], egui::Button::new("+ Add audio track…")).on_hover_text("Use the sound of another audio or video file").clicked() {
            self.add_audio_track();
        }

        let Some(cur) = self.fx_track() else { return };
        ui.add_space(8.0);
        if self.s.tracks.len() > 1 {
            let name = self.s.tracks[cur].label.clone();
            ui.label(RichText::new(format!("Track {} · {name}", cur + 1)).strong().color(theme::TEXT_LABEL))
                .on_hover_text("Click a track above to edit its fades and effects");
        }
        self.fx_presets_row(ui, cur);
        ui.add_space(4.0);
        self.fx_sidebar(ui, cur);
    }

    /// Audio-file mode: the level of the one track.
    fn volume_body(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.s.tracks.first_mut() else { return };
        field(ui, "Level", |ui| {
            let mut pct = t.volume * 100.0;
            let r = num(ui, DragValue::new(&mut pct).range(0.0..=300.0).speed(1.0).suffix("%").fixed_decimals(0));
            if resettable(r, &mut pct, 100.0).changed() {
                t.volume = pct / 100.0;
            }
            if !t.envelope.is_empty() && ui.small_button("Clear keyframes").clicked() {
                t.envelope.clear();
            }
        });
        hint(ui, "Double-click the waveform to add a volume keyframe; drag it to change the level over time.");
    }

    /// Named effect stacks: apply one to the track, save the track's effects as a new one, or manage them.
    fn fx_presets_row(&mut self, ui: &mut egui::Ui, track: usize) {
        field(ui, "Presets", |ui| {
            egui::ComboBox::from_id_salt("audio_presets").selected_text("Choose…").width(ui.available_width()).show_ui(ui, |ui| {
                if ui.add_enabled(!self.s.tracks[track].effects.is_empty(), egui::Button::selectable(false, "Save these effects…")).clicked() {
                    self.presets_ui.name.clear();
                    self.presets_ui.fx_save_track = Some(track);
                }
                if ui.selectable_label(false, "Manage presets…").clicked() {
                    self.presets_ui.tab = crate::state::PresetsTab::Audio;
                    self.dialogs.presets = true;
                }
                ui.separator();
                if self.presets.fx.is_empty() {
                    ui.weak("No audio presets yet");
                }
                let mut chosen = None;
                for (name, fx) in &self.presets.fx {
                    if ui.selectable_label(false, name).on_hover_text("Replaces the effects of this track").clicked() {
                        chosen = Some(fx.clone());
                    }
                }
                if let Some(fx) = chosen {
                    self.s.tracks[track].effects = fx;
                }
            });
        });
    }

    // ------------------------------------------------------------------ output

    /// ".mp4 · libx264 CRF 23 · aac".
    fn output_summary(&self) -> String {
        let mut v = vec![format!(".{}", self.container)];
        if !self.audio_mode() && !self.s.audio_only {
            v.push(match &self.s.video_codec {
                VideoCodec::Copy => "copy".into(),
                VideoCodec::Auto => "auto".into(),
                VideoCodec::Encoder(e) => e.clone(),
            });
        }
        if self.s.audio_mode != AudioMode::None && !self.s.tracks.is_empty() {
            v.push(match &self.s.audio_codec {
                AudioCodec::Copy => "audio copy".into(),
                AudioCodec::Encoder(e) => e.clone(),
            });
        }
        v.join(" · ")
    }

    fn output_body(&mut self, ui: &mut egui::Ui) {
        let enc = self.encoders.lock().unwrap().clone();
        if !self.audio_mode() {
            field(ui, "Export", |ui| {
                let before = self.s.audio_only;
                let mut audio_only = before;
                ui.selectable_value(&mut audio_only, false, "Video");
                ui.selectable_value(&mut audio_only, true, "Audio only").on_hover_text("Just the sound of this video");
                if audio_only != before {
                    self.set_audio_only(audio_only);
                }
            });
        }
        let act = field(ui, "Container", |ui| {
            let cur = self.container.clone();
            let r = egui::ComboBox::from_id_salt("container").selected_text(format!(".{cur}")).width(ui.available_width()).show_ui(ui, |ui| {
                for c in self.containers().iter().copied() {
                    if ui.selectable_label(cur == c, format!(".{c}")).clicked() {
                        self.set_container(c);
                    }
                }
            });
            let r = r.response.on_hover_text(if self.audio_mode() {
                "Copy keeps the original codec when the container can hold it; otherwise the audio is re-encoded."
            } else {
                "Not every codec fits every container (e.g. HEVC in .webm). ffmpeg will tell you if it does not."
            });
            // sound-only outputs (audio files, "Audio only") have their own default
            let builtin = if self.s.audio_only { self.sound_container().to_string() } else { self.builtin_container() };
            let sound = self.audio_mode() || self.s.audio_only;
            let d = &mut self.settings.defaults;
            let own = if sound { &mut d.audio_container } else { &mut d.container };
            widgets::default_menu(&r, &self.container, own, &builtin, |c| format!(".{c}"))
        });
        if let Some(c) = self.default_act(act).filter(|c| self.containers().contains(&c.as_str())) {
            self.set_container(&c);
        }
        if !self.audio_mode() && !self.s.audio_only {
            ui.add_space(6.0);
            caption(ui, "Video");
            self.video_codec_rows(ui, &enc);
        }
        if !self.s.tracks.is_empty() {
            ui.add_space(6.0);
            caption(ui, "Audio");
            self.audio_codec_rows(ui, &enc);
        }
        ui.add_space(4.0);
        hint(ui, "Right-click a dropdown to make its choice the default for new files.");
    }

    fn video_codec_rows(&mut self, ui: &mut egui::Ui, enc: &Option<probe::Encoders>) {
        let act = field(ui, "Codec", |ui| {
            let w = ui.available_width();
            let r = codecs::video_codec_combo(ui, "vcodec", &mut self.s.video_codec, enc, w);
            widgets::default_menu(&r, &self.s.video_codec, &mut self.settings.defaults.video_codec, &VideoCodec::Auto, codecs::video_text)
        });
        if let Some(c) = self.default_act(act) {
            self.s.video_codec = c;
        }
        let filters =
            self.s.vfx.is_active() || self.s.rotation != Rotation::None || self.s.flip_h || self.s.flip_v || self.crop.on || self.s.scale != Scale::default();
        if matches!(self.s.video_codec, VideoCodec::Copy) {
            if filters {
                warn(ui, "Crop, rotation, resize and effects need re-encoding: choose a codec, or they are left out.");
            }
            if self.trimmed() {
                warn(
                    ui,
                    "Copy with a trim: the cut snaps to keyframes, so it is not frame-exact, and the start of the file can show \
                     artifacts (frozen or broken frames). Choose a codec (e.g. Auto) for an exact cut.",
                );
            } else if !filters {
                hint(ui, "The video is copied as it is: fast and lossless.");
            }
            return;
        }
        let name = match &self.s.video_codec {
            VideoCodec::Encoder(e) => e.clone(),
            _ => builder::auto_encoder(&self.clip.as_ref().map(|c| c.info.video_codec.clone()).unwrap_or_default()).to_string(),
        };
        field(ui, "Rate", |ui| quality_fields(ui, &mut self.s.quality));
        match &mut self.s.quality {
            Quality::Crf(v) => {
                field(ui, "CRF", |ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(60.0);
                    let r =
                        super::widgets::value(ui, egui::Slider::new(v, 0..=51)).on_hover_text("Lower = better quality, bigger file. 18–23 is a good range.");
                    resettable(r, v, 23);
                });
            }
            Quality::Bitrate(k) => {
                field(ui, "Bitrate", |ui| {
                    let r = num(ui, DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps"));
                    resettable(r, k, 8000);
                });
            }
        }
        if matches!(name.as_str(), "libx264" | "libx265") {
            let act = field(ui, "Speed", |ui| {
                let r = egui::ComboBox::from_id_salt("preset")
                    .selected_text(&self.s.preset)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for p in SPEEDS {
                            ui.selectable_value(&mut self.s.preset, p.to_string(), p);
                        }
                    })
                    .response
                    .on_hover_text("Encoder preset: slower makes a smaller file at the same quality, but takes longer.");
                widgets::default_menu(&r, &self.s.preset, &mut self.settings.defaults.speed, &"medium".to_string(), |p| p.clone())
            });
            if let Some(p) = self.default_act(act) {
                self.s.preset = p;
            }
        }
        let act = field(ui, "Pixel format", |ui| {
            let cur = self.s.pix_fmt.clone().unwrap_or_else(|| "auto".into());
            let r = egui::ComboBox::from_id_salt("pixfmt").selected_text(&cur).width(ui.available_width()).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.s.pix_fmt, None, "auto");
                for p in PIX_FMTS {
                    ui.selectable_value(&mut self.s.pix_fmt, Some(p.to_string()), p);
                }
            });
            widgets::default_menu(&r.response, &cur, &mut self.settings.defaults.pix_fmt, &"auto".to_string(), |p| p.clone())
        });
        if let Some(p) = self.default_act(act) {
            self.s.pix_fmt = (p != "auto").then_some(p);
        }
    }

    /// Track packing, codec and bitrate (the tracks themselves are in the Audio section).
    fn audio_codec_rows(&mut self, ui: &mut egui::Ui, enc: &Option<probe::Encoders>) {
        let audible_n = audible(&self.s.tracks).len();
        if !self.audio_mode() {
            let act = field(ui, "Tracks", |ui| {
                let r = egui::ComboBox::from_id_salt("audio_mode").selected_text(audio_mode_label(self.s.audio_mode)).width(ui.available_width()).show_ui(
                    ui,
                    |ui| {
                        for (m, tip) in [
                            (AudioMode::Separate, "Every audible track stays a separate audio stream"),
                            (AudioMode::Merge, "All audible tracks are mixed into a single stream"),
                            (AudioMode::None, "No audio in the output"),
                        ] {
                            ui.selectable_value(&mut self.s.audio_mode, m, audio_mode_label(m)).on_hover_text(tip);
                        }
                    },
                );
                widgets::default_menu(&r.response, &self.s.audio_mode, &mut self.settings.defaults.audio_mode, &AudioMode::Separate, |m| {
                    audio_mode_label(*m).to_string()
                })
            });
            if let Some(m) = self.default_act(act) {
                self.s.audio_mode = m;
            }
        }
        if self.s.audio_mode == AudioMode::None || audible_n == 0 {
            return;
        }
        let act = field(ui, "Codec", |ui| {
            let w = ui.available_width();
            let r = codecs::audio_codec_combo(ui, "acodec", &mut self.s.audio_codec, enc, w);
            widgets::default_menu(&r, &self.s.audio_codec, &mut self.settings.defaults.audio_codec, &AudioCodec::Copy, codecs::audio_text)
        });
        if let Some(c) = self.default_act(act) {
            self.s.audio_codec = c;
        }
        if let AudioCodec::Encoder(c) = &self.s.audio_codec {
            if !codecs::is_lossless_audio(c) {
                field(ui, "Bitrate", |ui| {
                    let r = num(ui, DragValue::new(&mut self.s.audio_bitrate_kbps).range(32..=1536).speed(2.0).suffix(" kbps"));
                    resettable(r, &mut self.s.audio_bitrate_kbps, 192);
                });
            }
        }
        if self.s.audio_mode == AudioMode::Merge && audible_n > 1 {
            warn(ui, "Mixing re-encodes the audio.");
        } else if self.tracks_need_filters() {
            warn(ui, "Track edits or effects are active: the audio is re-encoded.");
        }
        if self.s.audio_only && audible_n > 1 && self.s.audio_mode == AudioMode::Separate {
            let ext = &self.container;
            if builder::multi_stream_container(ext) {
                hint(ui, format!("{audible_n} tracks stay separate streams of one .{ext} file."));
            } else {
                hint(ui, format!("{audible_n} tracks → {audible_n} files (name_1.{ext}, name_2.{ext}…). Pick .mka, .mkv or .mp4 to keep them in one file."));
            }
        }
    }

    /// Switches between exporting the video and only its sound, with a container that suits it.
    fn set_audio_only(&mut self, on: bool) {
        self.s.audio_only = on;
        if !on && !CONTAINERS.contains(&self.container.as_str()) {
            let c = self.settings.defaults.container.clone().filter(|c| CONTAINERS.contains(&c.as_str()));
            self.set_container(c.as_deref().unwrap_or("mp4"));
        }
        if on && !self.cmd.edited {
            // a plain video container is rarely what one wants for sound only
            let own = self.settings.defaults.audio_container.clone().filter(|c| AUDIO_EXPORT.contains(&c.as_str()));
            let c = own.unwrap_or_else(|| self.sound_container().to_string());
            self.set_container(&c);
        }
    }

    /// The built-in container of "Audio only": one that keeps several tracks apart when there are several.
    fn sound_container(&self) -> &'static str {
        if self.s.tracks.iter().filter(|t| !t.deleted).count() > 1 {
            "mka"
        } else {
            "m4a"
        }
    }

    /// Saves the settings when a dropdown's default changed; returns the value to go back to, if asked.
    fn default_act<T>(&mut self, act: widgets::DefaultAct<T>) -> Option<T> {
        if act.saved {
            self.settings.save();
        }
        act.reset
    }

    /// The trim is not the whole file.
    fn trimmed(&self) -> bool {
        self.trim_start > 0.001 || self.trim_end < self.dur() - 0.001
    }
}

pub fn audio_mode_label(m: AudioMode) -> &'static str {
    match m {
        AudioMode::Separate => "Keep separate",
        AudioMode::Merge => "Mix into one",
        AudioMode::None => "No audio",
    }
}
