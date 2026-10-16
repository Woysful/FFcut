use super::{audio_label, codecs};
use crate::app::{fmt_time, parse_time, App, CONTAINERS};
use crate::theme;
use eframe::egui::{self, Color32, DragValue, RichText};
use ffcut_core::*;

const PRESETS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
const PIX_FMTS: [&str; 7] = ["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "nv12", "p010le"];

fn section<R>(ui: &mut egui::Ui, title: &str, default_open: bool, f: impl FnOnce(&mut egui::Ui) -> R) {
    egui::CollapsingHeader::new(RichText::new(title).strong().size(13.5)).default_open(default_open).show(ui, |ui| {
        ui.add_space(2.0);
        f(ui);
        ui.add_space(6.0);
    });
}

fn time_field(ui: &mut egui::Ui, v: &mut f64, min: f64, max: f64) -> bool {
    ui.add(
        DragValue::new(v)
            .range(min..=max)
            .speed(0.02)
            .custom_formatter(|n, _| fmt_time(n))
            .custom_parser(parse_time),
    )
    .changed()
}

impl App {
    pub fn side_panel(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        egui::Panel::right("settings").default_size(340.0).size_range(300.0..=560.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(4.0);
                self.info_section(ui);
                self.trim_section(ui);
                self.transform_section(ui);
                self.video_section(ui);
                self.audio_section(ui);
                self.export_section(ui);
            });
        });
    }

    fn info_section(&mut self, ui: &mut egui::Ui) {
        let Some(c) = &self.clip else { return };
        let i = &c.info;
        section(ui, "File", false, |ui| {
            ui.label(RichText::new(c.path.file_name().unwrap_or_default().to_string_lossy()).strong());
            let mut rows = vec![
                ("Video", format!("{} · {}×{} · {:.2} fps", i.video_codec, i.width, i.height, i.fps)),
                ("Pixel format", i.pix_fmt.clone()),
                ("Duration", fmt_time(i.duration)),
                ("Size", format!("{:.1} MB", i.file_size as f64 / 1_048_576.0)),
                ("Container", i.format.clone()),
            ];
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

    fn trim_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Trim", true, |ui| {
            let dur = self.dur();
            ui.checkbox(&mut self.lock_trim, "Lock trim handles").on_hover_text("Shift+A");
            ui.add_enabled_ui(!self.lock_trim, |ui| {
                egui::Grid::new("trim").num_columns(3).show(ui, |ui| {
                    ui.label("Start");
                    let mut v = self.trim_start;
                    if time_field(ui, &mut v, 0.0, (self.trim_end - 0.05).max(0.0)) {
                        self.trim_start = v;
                        self.player.seek(v);
                        self.sync_loop();
                    }
                    if ui.small_button("Set").on_hover_text("Set to playhead  (Q)").clicked() {
                        self.trim_start = self.player.time().min(self.trim_end - 0.05).max(0.0);
                        self.sync_loop();
                    }
                    ui.end_row();
                    ui.label("End");
                    let mut v = self.trim_end;
                    if time_field(ui, &mut v, self.trim_start + 0.05, dur) {
                        self.trim_end = v;
                        self.player.seek(v);
                        self.sync_loop();
                    }
                    if ui.small_button("Set").on_hover_text("Set to playhead  (E)").clicked() {
                        self.trim_end = self.player.time().max(self.trim_start + 0.05).min(dur);
                        self.sync_loop();
                    }
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.weak(format!("Length {}", fmt_time(self.trim_end - self.trim_start)));
                    if ui.small_button("Reset  (R)").clicked() {
                        self.trim_start = 0.0;
                        self.trim_end = dur;
                        self.sync_loop();
                    }
                });
            });
        });
    }

    fn transform_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Transform", true, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⟲ 90°").on_hover_text("Rotate counter-clockwise").clicked() {
                    self.rotate_by(-1);
                }
                if ui.button("⟳ 90°").on_hover_text("Rotate clockwise").clicked() {
                    self.rotate_by(1);
                }
                let mut changed = false;
                changed |= ui.toggle_value(&mut self.s.flip_h, "↔ Flip").on_hover_text("Mirror horizontally").changed();
                changed |= ui.toggle_value(&mut self.s.flip_v, "↕ Flip").on_hover_text("Mirror vertically").changed();
                if changed {
                    self.apply_preview_transform();
                }
            });
            ui.add_space(4.0);

            // ---- crop
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.crop_on, "Crop").on_hover_text("Toggle  (C)").changed() && self.crop_on {
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
                egui::Grid::new("crop").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.weak("X");
                    edited |= ui.add(DragValue::new(&mut x).range(0.0..=(fw - 2) as f32).speed(2.0).fixed_decimals(0)).changed();
                    ui.weak("Y");
                    edited |= ui.add(DragValue::new(&mut y).range(0.0..=(fh - 2) as f32).speed(2.0).fixed_decimals(0)).changed();
                    ui.end_row();
                    ui.weak("W");
                    edited |= ui.add(DragValue::new(&mut w).range(2.0..=fw as f32).speed(2.0).fixed_decimals(0)).changed();
                    ui.weak("H");
                    edited |= ui.add(DragValue::new(&mut h).range(2.0..=fh as f32).speed(2.0).fixed_decimals(0)).changed();
                    ui.end_row();
                });
                if edited {
                    x = x.min(fw as f32 - 2.0);
                    y = y.min(fh as f32 - 2.0);
                    self.crop_edges = [x, y, (x + w).min(fw as f32), (y + h).min(fh as f32)];
                }
                ui.weak("Values snap to even numbers (required by most encoders).");
            }
            ui.add_space(4.0);

            // ---- scale
            ui.horizontal(|ui| {
                ui.label("Resize");
                let mut w = self.s.scale.w.unwrap_or(0);
                let mut h = self.s.scale.h.unwrap_or(0);
                let rw = ui.add(DragValue::new(&mut w).range(0..=16384).speed(2.0).custom_formatter(|n, _| if n == 0.0 { "auto".into() } else { format!("{n:.0}") }));
                ui.label("×");
                let rh = ui.add(DragValue::new(&mut h).range(0..=16384).speed(2.0).custom_formatter(|n, _| if n == 0.0 { "auto".into() } else { format!("{n:.0}") }));
                if rw.changed() || rh.changed() {
                    self.s.scale = Scale { w: (w > 0).then_some(w), h: (h > 0).then_some(h) };
                }
                if self.s.scale != Scale::default() && ui.small_button("✕").clicked() {
                    self.s.scale = Scale::default();
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (label, hh) in [("1080p", 1080u32), ("720p", 720), ("480p", 480)] {
                    if ui.small_button(label).clicked() {
                        self.s.scale = Scale { w: None, h: Some(hh) };
                    }
                }
            });
            if self.s.rotation != Rotation::None || self.s.flip_h || self.s.flip_v || self.crop_on || self.s.scale != Scale::default() {
                ui.add_space(2.0);
                ui.colored_label(theme::WARN, "Filters force re-encoding (stream copy is not possible).");
            }
        });
    }

    fn video_section(&mut self, ui: &mut egui::Ui) {
        let enc = self.encoders.lock().unwrap().clone();
        section(ui, "Video", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Codec");
                codecs::video_codec_combo(ui, "vcodec", &mut self.s.video_codec, &enc, 210.0);
            });

            let encoding = !matches!(self.s.video_codec, VideoCodec::Copy);
            if !encoding {
                ui.weak("Fastest, no quality loss. Cuts snap to the nearest keyframe.");
                return;
            }
            let name = match &self.s.video_codec {
                VideoCodec::Encoder(e) => e.clone(),
                _ => builder::auto_encoder(&self.clip.as_ref().map(|c| c.info.video_codec.clone()).unwrap_or_default()).to_string(),
            };

            ui.horizontal(|ui| {
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
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(v, 0..=51).text("CRF"));
                    });
                    ui.weak("Lower = better quality, bigger file. 18–23 is a good range.");
                }
                Quality::Bitrate(k) => {
                    ui.horizontal(|ui| {
                        ui.add(DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps"));
                    });
                }
            }
            if matches!(name.as_str(), "libx264" | "libx265") {
                ui.horizontal(|ui| {
                    ui.label("Speed preset");
                    egui::ComboBox::from_id_salt("preset").selected_text(&self.s.preset).show_ui(ui, |ui| {
                        for p in PRESETS {
                            ui.selectable_value(&mut self.s.preset, p.to_string(), p);
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                ui.label("Pixel format");
                let cur = self.s.pix_fmt.clone().unwrap_or_else(|| "auto".into());
                egui::ComboBox::from_id_salt("pixfmt").selected_text(cur).show_ui(ui, |ui| {
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
        section(ui, "Audio", true, |ui| {
            if self.s.tracks.is_empty() {
                ui.weak("This file has no audio.");
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut self.s.audio_mode, AudioMode::Separate, "Keep tracks").on_hover_text("Every audible track stays a separate audio stream");
                    ui.radio_value(&mut self.s.audio_mode, AudioMode::Merge, "Mix into one").on_hover_text("All audible tracks are mixed into a single stream");
                    ui.radio_value(&mut self.s.audio_mode, AudioMode::None, "None").on_hover_text("No audio in the output");
                });

                // the tracks: untick = removed from the export (the same as × in the track view)
                let mut toggle: Option<usize> = None;
                let heard: Vec<bool> = {
                    let h = audible(&self.s.tracks);
                    self.s.tracks.iter().map(|t| h.iter().any(|x| std::ptr::eq(*x, t))).collect()
                };
                for (i, t) in self.s.tracks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut on = !t.deleted;
                        if ui.add(egui::Checkbox::new(&mut on, "")).on_hover_text("Include this track").changed() {
                            toggle = Some(i);
                        }
                        let ext = matches!(t.source, TrackSource::External { .. });
                        let mut text = RichText::new(format!("{}{}", if ext { "+ " } else { "" }, t.label)).size(12.5);
                        if t.deleted || !heard[i] {
                            text = text.color(Color32::from_gray(115));
                        }
                        let badge = if t.deleted { "" } else if t.solo { "  solo" } else if t.muted { "  muted" } else if !heard[i] { "  silenced by solo" } else { "" };
                        ui.add(egui::Label::new(text).truncate()).on_hover_text(&t.label);
                        if !badge.is_empty() {
                            ui.weak(badge);
                        }
                    });
                }
                if let Some(i) = toggle {
                    let t = &mut self.s.tracks[i];
                    t.deleted = !t.deleted;
                }
            }
            if ui.button("+ Add audio track…").on_hover_text("Use the sound of another audio or video file").clicked() {
                self.add_audio_track();
            }

            let n = audible(&self.s.tracks).len();
            ui.add_space(2.0);
            let summary = match (self.s.audio_mode, n) {
                (AudioMode::None, _) => "The output will have no audio.".to_string(),
                (_, 0) if self.s.tracks.is_empty() => String::new(),
                (_, 0) => "Nothing is audible: the output will have no audio.".to_string(),
                (AudioMode::Merge, n) if n > 1 => format!("{n} tracks are mixed into one."),
                (_, 1) => "1 track in the output.".to_string(),
                (_, n) => format!("{n} tracks in the output."),
            };
            if !summary.is_empty() {
                ui.label(RichText::new(summary).size(12.0).color(Color32::from_gray(150)));
            }

            if self.s.audio_mode != AudioMode::None && n > 0 {
                ui.horizontal(|ui| {
                    ui.label("Codec");
                    codecs::audio_codec_combo(ui, "acodec", &mut self.s.audio_codec, &enc, 210.0);
                });
                if let AudioCodec::Encoder(c) = &self.s.audio_codec {
                    if !codecs::is_lossless_audio(c) {
                        ui.horizontal(|ui| {
                            ui.label("Bitrate");
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
    fn export_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Export", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Container");
                let cur = self.container.clone();
                egui::ComboBox::from_id_salt("container").selected_text(format!(".{cur}")).show_ui(ui, |ui| {
                    for c in CONTAINERS {
                        if ui.selectable_label(cur == c, format!(".{c}")).clicked() {
                            self.set_container(c);
                        }
                    }
                });
            });
            ui.add(egui::Label::new(RichText::new("Not every codec fits every container (e.g. HEVC in .webm). ffmpeg will tell you if it does not.").size(11.5).color(Color32::from_gray(130))).wrap());
            ui.add_space(6.0);
            self.command_block(ui);
        });
    }
}
