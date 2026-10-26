use super::{audio_label, codecs, field};
use crate::app::{fmt_time, parse_time, App, CONTAINERS};
use crate::hotkeys::Action;
use crate::theme;
use eframe::egui::{self, Color32, DragValue, RichText};
use ffcut_core::*;

const PRESETS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
const PIX_FMTS: [&str; 7] = ["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "nv12", "p010le"];

/// The sections whose open / closed state is remembered between sessions.
pub const SECTION_KEYS: [&str; 9] = ["File", "Trim", "Transform", "vfx-section", "Video", "Audio", "Export", "Volume", "Audio effects"];

/// A section header that starts as the user left it last time. (The saved states travel through egui's
/// temporary data, so the section code needs no access to the settings.)
pub(super) fn saved_header(ui: &egui::Ui, text: RichText, key: &str, default_open: bool) -> egui::CollapsingHeader {
    let saved = ui.ctx().data(|d| d.get_temp::<std::collections::BTreeMap<String, bool>>(egui::Id::new("sections-saved"))).and_then(|m| m.get(key).copied());
    egui::CollapsingHeader::new(text).id_salt(key).default_open(saved.unwrap_or(default_open))
}

/// Tells the side panel what the user has now (it saves it when it changed).
pub(super) fn note_section_state(ui: &egui::Ui, key: &str, open: bool) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("section-now", key)), open));
}

/// Asks the sidebar to open a section on the next frame (used when something elsewhere points at it).
pub(super) fn open_section(ctx: &egui::Context, title: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("open-section"), title.to_string()));
}

fn section<R>(ui: &mut egui::Ui, title: &str, default_open: bool, f: impl FnOnce(&mut egui::Ui) -> R) {
    let force = ui.ctx().data(|d| d.get_temp::<String>(egui::Id::new("open-section"))).as_deref() == Some(title);
    let mut header = saved_header(ui, RichText::new(title).strong().size(13.5), title, default_open);
    if force {
        ui.ctx().data_mut(|d| d.remove_temp::<String>(egui::Id::new("open-section")));
        header = header.open(Some(true));
    }
    let r = header.show(ui, |ui| {
        ui.add_space(2.0);
        f(ui);
        ui.add_space(6.0);
    });
    note_section_state(ui, title, r.openness >= 0.5);
}

/// A group inside a group (Export → Video / Audio): its rows line up with the rows of the group around it.
fn subsection<R>(ui: &mut egui::Ui, title: &str, f: impl FnOnce(&mut egui::Ui) -> R) {
    let r = saved_header(ui, RichText::new(title).size(12.5).color(Color32::from_gray(200)), title, true).show_unindented(ui, |ui| {
        ui.add_space(2.0);
        f(ui);
        ui.add_space(4.0);
    });
    note_section_state(ui, title, r.openness >= 0.5);
}

/// A time value as a time code, or as a frame number when `fps` is given.
pub(super) fn time_field(ui: &mut egui::Ui, v: &mut f64, min: f64, max: f64, fps: Option<f64>) -> bool {
    match fps {
        None => ui.add(DragValue::new(v).range(min..=max).speed(0.02).custom_formatter(|n, _| fmt_time(n)).custom_parser(parse_time)).changed(),
        Some(fps) => ui
            .add(
                DragValue::new(v)
                    .range(min..=max)
                    .speed(1.0 / fps)
                    .custom_formatter(move |n, _| format!("{}", (n * fps).round() as i64))
                    // a plain number is a frame number; "1:30" is still read as a time
                    .custom_parser(move |s| if s.contains(':') { parse_time(s) } else { s.trim().parse::<f64>().ok().map(|f| f / fps) }),
            )
            .changed(),
    }
}

impl App {
    pub fn side_panel(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        let saved = self.settings.sections.clone();
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("sections-saved"), saved));
        egui::Panel::right("settings").default_size(340.0).size_range(300.0..=560.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(4.0);
                // keep clear of the scroll bar
                ui.set_max_width(ui.available_width() - 14.0);
                if self.audio_mode() {
                    self.audio_file_sidebar(ui);
                    return;
                }
                self.info_section(ui);
                self.trim_section(ui);
                self.transform_section(ui);
                self.vfx_section(ui);
                self.audio_fx_section(ui);
                self.export_section(ui);
            });
        });
        self.remember_sections(ui.ctx());
    }

    /// Writes the open / closed state of the sections to the settings when the user changed one.
    fn remember_sections(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        for key in SECTION_KEYS {
            if let Some(now) = ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("section-now", key)))) {
                if self.settings.sections.get(key) != Some(&now) {
                    self.settings.sections.insert(key.to_string(), now);
                    changed = true;
                }
            }
        }
        if changed {
            self.settings.save();
        }
    }

    pub(super) fn info_section(&mut self, ui: &mut egui::Ui) {
        let Some(c) = &self.clip else { return };
        let i = &c.info;
        section(ui, "File", false, |ui| {
            ui.label(RichText::new(c.path.file_name().unwrap_or_default().to_string_lossy()).strong());
            let mut rows = if i.audio_only {
                vec![]
            } else {
                vec![("Video", format!("{} · {}×{} · {:.2} fps", i.video_codec, i.width, i.height, i.fps)), ("Pixel format", i.pix_fmt.clone())]
            };
            rows.extend([
                ("Duration", fmt_time(i.duration)),
                ("Size", format!("{:.1} MB", i.file_size as f64 / 1_048_576.0)),
                ("Container", i.format.clone()),
            ]);
            if i.rotation != 0 {
                rows.push(("Rotation tag", format!("{}°", i.rotation)));
            }
            egui::Grid::new("info").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.weak(k);
                    ui.label(v);
                    ui.end_row();
                }
            });
            for a in &i.audio {
                ui.weak(format!("Audio {}", audio_label(a)));
            }
        });
    }

    pub(super) fn trim_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Trim", true, |ui| {
            let dur = self.dur();
            let fps = self.frame_fps();
            ui.checkbox(&mut self.lock_trim, "Lock trim handles").on_hover_text(self.keys(Action::LockTrim));
            ui.add_enabled_ui(!self.lock_trim, |ui| {
                field(ui, "Start", |ui| {
                    let mut v = self.trim_start;
                    if time_field(ui, &mut v, 0.0, (self.trim_end - 0.05).max(0.0), fps) {
                        self.trim_start = v;
                        self.player.seek(v);
                        self.sync_loop();
                    }
                    if ui.small_button("Set").on_hover_text(self.tip("Set to playhead", Action::TrimStart)).clicked() {
                        self.trim_start = self.player.time().min(self.trim_end - 0.05).max(0.0);
                        self.sync_loop();
                    }
                });
                field(ui, "End", |ui| {
                    let mut v = self.trim_end;
                    if time_field(ui, &mut v, self.trim_start + 0.05, dur, fps) {
                        self.trim_end = v;
                        self.player.seek(v);
                        self.sync_loop();
                    }
                    if ui.small_button("Set").on_hover_text(self.tip("Set to playhead", Action::TrimEnd)).clicked() {
                        self.trim_end = self.player.time().max(self.trim_start + 0.05).min(dur);
                        self.sync_loop();
                    }
                });
                field(ui, "Length", |ui| {
                    let len = self.trim_end - self.trim_start;
                    ui.label(match fps {
                        Some(f) => format!("{} frames", (len * f).round() as i64),
                        None => fmt_time(len),
                    });
                    if ui.small_button(self.tip("Reset", Action::ResetTrim)).clicked() {
                        self.trim_start = 0.0;
                        self.trim_end = dur;
                        self.sync_loop();
                    }
                });
            });
        });
    }

    fn transform_section(&mut self, ui: &mut egui::Ui) {
        const BTN: [f32; 2] = [92.0, 26.0];
        section(ui, "Transform", true, |ui| {
            // ---- crop
            field(ui, "Crop", |ui| {
                if ui.checkbox(&mut self.crop_on, "").on_hover_text(self.tip("Toggle", Action::Crop)).changed() && self.crop_on {
                    let c = self.crop_rect();
                    if c.w < 4 || c.h < 4 {
                        self.reset_crop();
                    }
                }
                if self.crop_on && ui.small_button("Reset").clicked() {
                    self.reset_crop();
                }
            });
            if self.crop_on {
                let (fw, fh) = self.frame_dims();
                let c = self.crop_rect();
                let (mut x, mut y, mut w, mut h) = (c.x as f32, c.y as f32, c.w as f32, c.h as f32);
                let mut edited = false;
                let snap = "Values snap to even numbers (required by most encoders)";
                field(ui, "Position", |ui| {
                    edited |= super::num(ui, DragValue::new(&mut x).range(0.0..=(fw - 2) as f32).speed(2.0).fixed_decimals(0)).on_hover_text("X").changed();
                    edited |= super::num(ui, DragValue::new(&mut y).range(0.0..=(fh - 2) as f32).speed(2.0).fixed_decimals(0)).on_hover_text("Y").changed();
                });
                field(ui, "Size", |ui| {
                    edited |= super::num(ui, DragValue::new(&mut w).range(2.0..=fw as f32).speed(2.0).fixed_decimals(0)).on_hover_text(format!("Width. {snap}")).changed();
                    edited |= super::num(ui, DragValue::new(&mut h).range(2.0..=fh as f32).speed(2.0).fixed_decimals(0)).on_hover_text(format!("Height. {snap}")).changed();
                });
                if edited {
                    x = x.min(fw as f32 - 2.0);
                    y = y.min(fh as f32 - 2.0);
                    self.crop_edges = [x, y, (x + w).min(fw as f32), (y + h).min(fh as f32)];
                }
            }

            let mut changed = false;
            field(ui, "Rotate", |ui| {
                if ui.add_sized(BTN, egui::Button::new("⟲ 90°")).on_hover_text("Rotate counter-clockwise").clicked() {
                    self.rotate_by(-1);
                }
                if ui.add_sized(BTN, egui::Button::new("⟳ 90°")).on_hover_text("Rotate clockwise").clicked() {
                    self.rotate_by(1);
                }
            });
            field(ui, "Flip", |ui| {
                if ui.add_sized(BTN, egui::Button::new("↔ Flip H").selected(self.s.flip_h)).on_hover_text("Mirror horizontally").clicked() {
                    self.s.flip_h = !self.s.flip_h;
                    changed = true;
                }
                if ui.add_sized(BTN, egui::Button::new("↕ Flip V").selected(self.s.flip_v)).on_hover_text("Mirror vertically").clicked() {
                    self.s.flip_v = !self.s.flip_v;
                    changed = true;
                }
            });
            if changed {
                self.apply_preview_transform();
            }

            // ---- scale
            field(ui, "Resize", |ui| {
                let mut w = self.s.scale.w.unwrap_or(0);
                let mut h = self.s.scale.h.unwrap_or(0);
                let fmt = |n: f64, _: std::ops::RangeInclusive<usize>| if n == 0.0 { "auto".to_string() } else { format!("{n:.0}") };
                let rw = super::num(ui, DragValue::new(&mut w).range(0..=16384).speed(2.0).custom_formatter(fmt)).on_hover_text("Width (auto keeps the proportions)");
                let rh = super::num(ui, DragValue::new(&mut h).range(0..=16384).speed(2.0).custom_formatter(fmt)).on_hover_text("Height (auto keeps the proportions)");
                if rw.changed() || rh.changed() {
                    self.s.scale = Scale { w: (w > 0).then_some(w), h: (h > 0).then_some(h) };
                }
            });
            field(ui, "", |ui| {
                for (label, hh) in [("1080p", 1080u32), ("720p", 720), ("480p", 480)] {
                    if ui.add(egui::Button::new(label).small().selected(self.s.scale == (Scale { w: None, h: Some(hh) }))).clicked() {
                        self.s.scale = Scale { w: None, h: Some(hh) };
                    }
                }
                if ui.add_enabled(self.s.scale != Scale::default(), egui::Button::new("Off").small()).on_hover_text("No resize").clicked() {
                    self.s.scale = Scale::default();
                }
            });
            if self.s.vfx.is_active() || self.s.rotation != Rotation::None || self.s.flip_h || self.s.flip_v || self.crop_on || self.s.scale != Scale::default() {
                ui.colored_label(theme::WARN, "Filters force re-encoding (no stream copy).");
            }
        });
    }

    fn video_section(&mut self, ui: &mut egui::Ui) {
        let enc = self.encoders.lock().unwrap().clone();
        subsection(ui, "Video", |ui| {
            field(ui, "Codec", |ui| {
                let w = ui.available_width();
                codecs::video_codec_combo(ui, "vcodec", &mut self.s.video_codec, &enc, w);
            });
            if matches!(self.s.video_codec, VideoCodec::Copy) {
                return; // nothing else applies: the stream is copied as it is (cuts snap to keyframes)
            }
            let name = match &self.s.video_codec {
                VideoCodec::Encoder(e) => e.clone(),
                _ => builder::auto_encoder(&self.clip.as_ref().map(|c| c.info.video_codec.clone()).unwrap_or_default()).to_string(),
            };

            field(ui, "Rate", |ui| {
                let mut crf = matches!(self.s.quality, Quality::Crf(_));
                if ui.radio_value(&mut crf, true, "Quality").clicked() && !matches!(self.s.quality, Quality::Crf(_)) {
                    self.s.quality = Quality::Crf(23);
                }
                if ui.radio_value(&mut crf, false, "Bitrate").clicked() && !matches!(self.s.quality, Quality::Bitrate(_)) {
                    self.s.quality = Quality::Bitrate(8000);
                }
            });
            match &mut self.s.quality {
                Quality::Crf(v) => {
                    field(ui, "CRF", |ui| {
                        ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(60.0);
                        ui.add(egui::Slider::new(v, 0..=51)).on_hover_text("Lower = better quality, bigger file. 18–23 is a good range.");
                    });
                }
                Quality::Bitrate(k) => {
                    field(ui, "Bitrate", |ui| {
                        ui.add(DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps"));
                    });
                }
            }
            if matches!(name.as_str(), "libx264" | "libx265") {
                field(ui, "Encode preset", |ui| {
                    egui::ComboBox::from_id_salt("preset").selected_text(&self.s.preset).width(ui.available_width()).show_ui(ui, |ui| {
                        for p in PRESETS {
                            ui.selectable_value(&mut self.s.preset, p.to_string(), p);
                        }
                    })
                    .response
                    .on_hover_text("How much effort the encoder spends. Slower presets make a smaller file at the same quality, but take longer to export.");
                });
            }
            field(ui, "Pixel format", |ui| {
                let cur = self.s.pix_fmt.clone().unwrap_or_else(|| "auto".into());
                egui::ComboBox::from_id_salt("pixfmt").selected_text(cur).width(ui.available_width()).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.s.pix_fmt, None, "auto");
                    for p in PIX_FMTS {
                        ui.selectable_value(&mut self.s.pix_fmt, Some(p.to_string()), p);
                    }
                });
            });
        });
    }

    /// Audio. The track list is the single source of truth, shared with the multi-track timeline:
    /// the checkboxes here and the remove / mute / solo buttons there edit the same tracks, and
    /// tracks added from files show up in both. The mode only says how the audible ones are packed.
    fn audio_section(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        let enc = self.encoders.lock().unwrap().clone();
        subsection(ui, "Audio", |ui| {
            if self.s.tracks.is_empty() {
                ui.label(RichText::new("This file has no audio.").color(Color32::from_gray(130)));
            } else {
                field(ui, "Tracks", |ui| {
                    let label = |m: AudioMode| match m {
                        AudioMode::Separate => "Keep separate",
                        AudioMode::Merge => "Mix into one",
                        AudioMode::None => "No audio",
                    };
                    egui::ComboBox::from_id_salt("audio_mode").selected_text(label(self.s.audio_mode)).width(ui.available_width()).show_ui(ui, |ui| {
                        for (m, tip) in [(AudioMode::Separate, "Every audible track stays a separate audio stream"), (AudioMode::Merge, "All audible tracks are mixed into a single stream"), (AudioMode::None, "No audio in the output")] {
                            ui.selectable_value(&mut self.s.audio_mode, m, label(m)).on_hover_text(tip);
                        }
                    });
                });

                // the tracks: untick = removed from the export (the same as × in the track view)
                let mut toggle: Option<usize> = None;
                let heard: Vec<bool> = {
                    let h = audible(&self.s.tracks);
                    self.s.tracks.iter().map(|t| h.iter().any(|x| std::ptr::eq(*x, t))).collect()
                };
                for (i, t) in self.s.tracks.iter().enumerate() {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(40, 42, 48))
                        .stroke(egui::Stroke::new(1.0, Color32::from_gray(54)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let mut on = !t.deleted;
                                if ui.add(egui::Checkbox::new(&mut on, "")).on_hover_text("Include this track").changed() {
                                    toggle = Some(i);
                                }
                                let ext = matches!(t.source, TrackSource::External { .. });
                                let dim = t.deleted || !heard[i];
                                let col = if dim { Color32::from_gray(115) } else { Color32::from_gray(225) };
                                ui.label(RichText::new(format!("{}", i + 1)).size(12.0).color(Color32::from_gray(120)));
                                ui.add(egui::Label::new(RichText::new(format!("{}{}", if ext { "+ " } else { "" }, t.label)).size(12.5).color(col)).truncate()).on_hover_text(&t.label);
                                let badge = if t.deleted { "" } else if t.solo { "solo" } else if t.muted { "muted" } else if !heard[i] { "silenced by solo" } else { "" };
                                if !badge.is_empty() {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(badge).size(11.5).color(theme::WARN));
                                    });
                                }
                            });
                        });
                }
                if let Some(i) = toggle {
                    let t = &mut self.s.tracks[i];
                    t.deleted = !t.deleted;
                }
            }
            let (w, h) = (ui.available_width(), ui.spacing().interact_size.y);
            if ui.add_sized([w, h], egui::Button::new("+ Add audio track…")).on_hover_text("Use the sound of another audio or video file").clicked() {
                self.add_audio_track();
            }

            let n = audible(&self.s.tracks).len();
            if self.s.audio_mode != AudioMode::None && n > 0 {
                field(ui, "Codec", |ui| {
                    let w = ui.available_width();
                    codecs::audio_codec_combo(ui, "acodec", &mut self.s.audio_codec, &enc, w);
                });
                if let AudioCodec::Encoder(c) = &self.s.audio_codec {
                    if !codecs::is_lossless_audio(c) {
                        field(ui, "Bitrate", |ui| {
                            ui.add(DragValue::new(&mut self.s.audio_bitrate_kbps).range(32..=1536).speed(2.0).suffix(" kbps"));
                        });
                    }
                }
                if self.s.audio_mode == AudioMode::Merge && n > 1 {
                    ui.colored_label(theme::WARN, "Mixing re-encodes the audio.");
                } else if self.tracks_need_filters() {
                    ui.colored_label(theme::WARN, "Timeline edits are active: audio is re-encoded.");
                }
            }
        });
    }

    /// Last block of the sidebar: container, the final command and the Export button.
    pub(super) fn export_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Export", true, |ui| {
            let mut audio_note: Option<String> = None;
            if !self.audio_mode() {
                let before = self.s.audio_only;
                ui.checkbox(&mut self.s.audio_only, "Audio only (no video)").on_hover_text("Export just the sound of this video");
                if before && !self.s.audio_only && !CONTAINERS.contains(&self.container.as_str()) {
                    self.set_container("mp4");
                }
                if !before && self.s.audio_only && !self.cmd_edited {
                    // a plain video container is rarely what one wants for sound only
                    let c = if self.s.tracks.iter().filter(|t| !t.deleted).count() > 1 { "mka" } else { "m4a" };
                    self.set_container(c);
                }
                if self.s.audio_only {
                    let n = audible(&self.s.tracks).len();
                    let ext = self.container.clone();
                    if n > 1 && self.s.audio_mode == AudioMode::Separate && !builder::multi_stream_container(&ext) {
                        audio_note = Some(format!("{n} tracks → {n} files (name_1.{ext}, name_2.{ext}…). Pick .mka / .mkv / .mp4 to keep them in one file."));
                    } else if n > 1 && self.s.audio_mode == AudioMode::Separate {
                        audio_note = Some(format!("{n} tracks stay separate streams of one .{ext} file, without video."));
                    }
                }
            }
            if self.audio_mode() {
                self.audio_file_codec_section(ui);
            } else {
                if !self.s.audio_only {
                    self.video_section(ui);
                }
                self.audio_section(ui);
            }
            field(ui, "Container", |ui| {
                let cur = self.container.clone();
                let r = egui::ComboBox::from_id_salt("container").selected_text(format!(".{cur}")).width(ui.available_width()).show_ui(ui, |ui| {
                    for c in self.containers().iter().copied() {
                        if ui.selectable_label(cur == c, format!(".{c}")).clicked() {
                            self.set_container(c);
                        }
                    }
                });
                let hint = if self.audio_mode() { "Copy keeps the original codec when the container can hold it; otherwise the audio is re-encoded." } else { "Not every codec fits every container (e.g. HEVC in .webm). ffmpeg will tell you if it does not." };
                r.response.on_hover_text(match &audio_note {
                    Some(n) => format!("{hint}\n\n{n}"),
                    None => hint.to_string(),
                });
            });
            ui.add_space(4.0);
            self.command_block(ui);
        });
    }
}

impl App {
    /// Audio-file mode: only what concerns sound. One track, so its effects live here instead of a window.
    fn audio_file_sidebar(&mut self, ui: &mut egui::Ui) {
        self.info_section(ui);
        self.trim_section(ui);
        section(ui, "Volume", true, |ui| {
            let Some(t) = self.s.tracks.first_mut() else { return };
            field(ui, "Level", |ui| {
                let mut pct = t.volume * 100.0;
                if ui.add(DragValue::new(&mut pct).range(0.0..=300.0).speed(1.0).suffix("%").fixed_decimals(0)).on_hover_text("Double-click the waveform to add a volume keyframe; drag it to change the level over time.").changed() {
                    t.volume = pct / 100.0;
                }
                if !t.envelope.is_empty() && ui.small_button("Clear keyframes").clicked() {
                    t.envelope.clear();
                }
            });
        });
        section(ui, "Audio effects", true, |ui| {
            self.fx_presets_row(ui, 0);
            ui.add_space(4.0);
            self.fx_sidebar(ui, 0);
        });
        self.export_section(ui);
    }

    /// Codec and bitrate of an audio file's export (lives inside the Export group).
    fn audio_file_codec_section(&mut self, ui: &mut egui::Ui) {
        let enc = self.encoders.lock().unwrap().clone();
        subsection(ui, "Audio", |ui| {
            field(ui, "Codec", |ui| {
                let w = ui.available_width();
                codecs::audio_codec_combo(ui, "acodec", &mut self.s.audio_codec, &enc, w);
            });
            if let AudioCodec::Encoder(c) = &self.s.audio_codec {
                if !codecs::is_lossless_audio(c) {
                    field(ui, "Bitrate", |ui| {
                        ui.add(DragValue::new(&mut self.s.audio_bitrate_kbps).range(32..=1536).speed(2.0).suffix(" kbps"));
                    });
                }
            }
        });
    }

    /// Effects of one track chosen from a list (a video file can have several audio tracks).
    fn audio_fx_section(&mut self, ui: &mut egui::Ui) {
        let usable: Vec<usize> = (0..self.s.tracks.len()).filter(|&i| !self.s.tracks[i].deleted).collect();
        if usable.is_empty() {
            return;
        }
        let cur = self.fx.window.filter(|i| usable.contains(i)).unwrap_or(usable[0]);
        self.fx.window = Some(cur);
        section(ui, "Audio effects", true, |ui| {
            field(ui, "Track", |ui| {
                egui::ComboBox::from_id_salt("fx_track").width(ui.available_width()).selected_text(format!("{}. {}", cur + 1, self.s.tracks[cur].label)).show_ui(ui, |ui| {
                    for &i in &usable {
                        let fx = if self.s.tracks[i].has_fx() { "  (FX)" } else { "" };
                        if ui.selectable_label(cur == i, format!("{}. {}{fx}", i + 1, self.s.tracks[i].label)).clicked() {
                            self.fx.window = Some(i);
                        }
                    }
                });
            });
            self.fx_presets_row(ui, cur);
            ui.add_space(4.0);
            self.fx_sidebar(ui, cur);
        });
    }

    /// Named effect stacks, like the Presets menu of the menu bar: apply one to the chosen track, save the track's
    /// effects as a new one, or open the editor.
    fn fx_presets_row(&mut self, ui: &mut egui::Ui, track: usize) {
        field(ui, "Presets", |ui| {
            egui::ComboBox::from_id_salt("audio_presets").selected_text("Choose…").width(ui.available_width()).show_ui(ui, |ui| {
                if ui.add_enabled(!self.s.tracks[track].effects.is_empty(), egui::Button::selectable(false, "Save preset…")).on_hover_text("Saves the effects of the chosen track").clicked() {
                    self.fx_preset_name.clear();
                    self.fx_save_track = Some(track);
                }
                if ui.selectable_label(false, "Edit presets…").clicked() {
                    self.presets_tab = 1;
                    self.show_presets = true;
                }
                ui.separator();
                if self.fx_presets.is_empty() {
                    ui.weak("No audio presets yet");
                }
                let names: Vec<String> = self.fx_presets.keys().cloned().collect();
                for name in names {
                    if ui.selectable_label(false, &name).on_hover_text("Replaces the effects of the chosen track").clicked() {
                        if let Some(fx) = self.fx_presets.get(&name) {
                            self.s.tracks[track].effects = fx.clone();
                        }
                    }
                }
            });
        });
    }

    /// Writes the audio presets to disk.
    pub fn save_fx_presets(&mut self) {
        if let Err(e) = ffcut_core::presets::save_fx(&self.fx_presets_path, &self.fx_presets) {
            self.status = Some(crate::app::Status::Error(format!("Cannot save audio presets: {e}")));
        }
    }
}
