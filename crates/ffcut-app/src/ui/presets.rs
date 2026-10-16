//! "Edit presets" window: rename, edit every field, apply, export one / all, import, delete.

use super::codecs;
use crate::app::{App, CONTAINERS};
use crate::theme;
use eframe::egui::{self, Color32, DragValue, RichText};
use ffcut_core::presets::{self, Preset};
use ffcut_core::{AudioCodec, Quality, Scale, VideoCodec};

const SPEEDS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
const PIX_FMTS: [&str; 7] = ["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "nv12", "p010le"];

fn summary(p: &Preset) -> String {
    let v = match &p.video_codec {
        VideoCodec::Copy => "copy".to_string(),
        VideoCodec::Auto => "auto".to_string(),
        VideoCodec::Encoder(e) => e.clone(),
    };
    let q = match p.quality {
        Quality::Crf(c) => format!("CRF {c}"),
        Quality::Bitrate(k) => format!("{k} kbps"),
    };
    let a = match &p.audio_codec {
        AudioCodec::Copy => "copy".to_string(),
        AudioCodec::Encoder(e) => e.clone(),
    };
    let scale = match (p.scale.w, p.scale.h) {
        (None, None) => String::new(),
        (w, h) => format!(" · {}×{}", w.map_or("auto".into(), |v| v.to_string()), h.map_or("auto".into(), |v| v.to_string())),
    };
    format!(".{} · {v}{} · audio {a}{scale}", p.container, if v == "copy" { String::new() } else { format!(" ({q})") })
}

impl App {
    fn save_presets(&mut self) {
        if let Err(e) = presets::save(&self.presets_path, &self.presets) {
            self.preset_msg = Some(format!("Cannot save presets: {e}"));
        }
    }

    pub fn presets_window(&mut self, ctx: &egui::Context) {
        if !self.show_presets {
            return;
        }
        let enc = self.encoders.lock().unwrap().clone();
        let mut open = true;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut export_one: Option<String> = None;
        let mut rename: Option<(String, String)> = None;
        let mut changed = false;
        let (mut export_all, mut import) = (false, false);

        egui::Window::new("Presets").open(&mut open).collapsible(false).default_size([620.0, 520.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Import…").on_hover_text("Add presets from a .json file (also reads presets exported from the old FFcut)").clicked() {
                    import = true;
                }
                if ui.add_enabled(!self.presets.is_empty(), egui::Button::new("Export all…")).clicked() {
                    export_all = true;
                }
                ui.weak(format!("{} preset(s)", self.presets.len()));
            });
            if let Some(m) = &self.preset_msg {
                ui.add(egui::Label::new(RichText::new(m).size(12.5).color(theme::WARN)).wrap());
            }
            ui.separator();

            if self.presets.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.weak("No presets yet.");
                    ui.weak("Set up codec, quality and audio in the sidebar, then Presets → Save preset…");
                });
                return;
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let names: Vec<String> = self.presets.keys().cloned().collect();
                for name in names {
                    egui::Frame::NONE.fill(theme::BG).corner_radius(8).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let renaming = self.preset_rename.as_ref().map_or(false, |(old, _)| *old == name);
                        ui.horizontal(|ui| {
                            if renaming {
                                let new = &mut self.preset_rename.as_mut().unwrap().1;
                                let r = ui.add(egui::TextEdit::singleline(new).desired_width(200.0));
                                r.request_focus();
                                let ok = ui.small_button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if ok {
                                    rename = Some((name.clone(), new.trim().to_string()));
                                }
                                if ui.small_button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    self.preset_rename = None;
                                }
                            } else {
                                ui.label(RichText::new(&name).strong().size(14.0));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Delete").on_hover_text("Delete").clicked() {
                                    delete = Some(name.clone());
                                }
                                if ui.small_button("Export").on_hover_text("Save this preset to a file").clicked() {
                                    export_one = Some(name.clone());
                                }
                                if ui.small_button("Rename").clicked() {
                                    self.preset_rename = Some((name.clone(), name.clone()));
                                }
                                if ui.add_enabled(self.clip.is_some(), egui::Button::new("Apply").small()).on_hover_text("Use this preset for the open video").clicked() {
                                    apply = Some(name.clone());
                                }
                            });
                        });
                        ui.add(egui::Label::new(RichText::new(summary(&self.presets[&name])).size(12.0).color(Color32::from_gray(140))).truncate());

                        egui::CollapsingHeader::new(RichText::new("Edit").size(12.5)).id_salt(("edit", &name)).show(ui, |ui| {
                            let p = self.presets.get_mut(&name).unwrap();
                            egui::Grid::new(("grid", &name)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                                ui.label("Container");
                                egui::ComboBox::from_id_salt(("pc", &name)).selected_text(format!(".{}", p.container)).show_ui(ui, |ui| {
                                    for c in CONTAINERS {
                                        changed |= ui.selectable_value(&mut p.container, c.to_string(), format!(".{c}")).changed();
                                    }
                                });
                                ui.end_row();

                                ui.label("Video codec");
                                let before = p.video_codec.clone();
                                codecs::video_codec_combo(ui, ("pv", &name), &mut p.video_codec, &enc, 230.0);
                                changed |= before != p.video_codec;
                                ui.end_row();

                                if !matches!(p.video_codec, VideoCodec::Copy) {
                                    ui.label("Quality");
                                    ui.horizontal(|ui| {
                                        let mut crf = matches!(p.quality, Quality::Crf(_));
                                        if ui.radio_value(&mut crf, true, "CRF").clicked() && !matches!(p.quality, Quality::Crf(_)) {
                                            p.quality = Quality::Crf(23);
                                            changed = true;
                                        }
                                        if ui.radio_value(&mut crf, false, "Bitrate").clicked() && !matches!(p.quality, Quality::Bitrate(_)) {
                                            p.quality = Quality::Bitrate(8000);
                                            changed = true;
                                        }
                                        match &mut p.quality {
                                            Quality::Crf(v) => changed |= ui.add(DragValue::new(v).range(0..=51)).changed(),
                                            Quality::Bitrate(k) => changed |= ui.add(DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps")).changed(),
                                        }
                                    });
                                    ui.end_row();

                                    ui.label("Speed preset");
                                    egui::ComboBox::from_id_salt(("ps", &name)).selected_text(&p.preset).show_ui(ui, |ui| {
                                        for s in SPEEDS {
                                            changed |= ui.selectable_value(&mut p.preset, s.to_string(), s).changed();
                                        }
                                    });
                                    ui.end_row();

                                    ui.label("Pixel format");
                                    egui::ComboBox::from_id_salt(("pp", &name)).selected_text(p.pix_fmt.clone().unwrap_or_else(|| "auto".into())).show_ui(ui, |ui| {
                                        changed |= ui.selectable_value(&mut p.pix_fmt, None, "auto").changed();
                                        for f in PIX_FMTS {
                                            changed |= ui.selectable_value(&mut p.pix_fmt, Some(f.to_string()), f).changed();
                                        }
                                    });
                                    ui.end_row();
                                }

                                ui.label("Audio codec");
                                let before = p.audio_codec.clone();
                                codecs::audio_codec_combo(ui, ("pa", &name), &mut p.audio_codec, &enc, 230.0);
                                changed |= before != p.audio_codec;
                                ui.end_row();

                                if let AudioCodec::Encoder(c) = &p.audio_codec {
                                    if !codecs::is_lossless_audio(c) {
                                        ui.label("Audio bitrate");
                                        changed |= ui.add(DragValue::new(&mut p.audio_bitrate_kbps).range(32..=1536).speed(2.0).suffix(" kbps")).changed();
                                        ui.end_row();
                                    }
                                }

                                ui.label("Resize");
                                ui.horizontal(|ui| {
                                    let (mut w, mut h) = (p.scale.w.unwrap_or(0), p.scale.h.unwrap_or(0));
                                    let fmt = |n: f64, _: std::ops::RangeInclusive<usize>| if n == 0.0 { "auto".to_string() } else { format!("{n:.0}") };
                                    let a = ui.add(DragValue::new(&mut w).range(0..=16384).speed(2.0).custom_formatter(fmt));
                                    ui.label("×");
                                    let b = ui.add(DragValue::new(&mut h).range(0..=16384).speed(2.0).custom_formatter(fmt));
                                    if a.changed() || b.changed() {
                                        p.scale = Scale { w: (w > 0).then_some(w), h: (h > 0).then_some(h) };
                                        changed = true;
                                    }
                                });
                                ui.end_row();
                            });
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        });
        if !open {
            self.show_presets = false;
            self.preset_rename = None;
            self.preset_msg = None;
        }

        // ---- actions (outside the window closure so we may touch everything)
        if let Some((old, new)) = rename {
            if new.is_empty() {
                self.preset_msg = Some("A preset needs a name.".into());
            } else if new != old && self.presets.contains_key(&new) {
                self.preset_msg = Some(format!("A preset named “{new}” already exists."));
            } else {
                if let Some(p) = self.presets.remove(&old) {
                    self.presets.insert(new, p);
                }
                self.preset_rename = None;
                self.preset_msg = None;
                changed = true;
            }
        }
        if let Some(n) = delete {
            self.presets.remove(&n);
            changed = true;
        }
        if let Some(n) = apply {
            if let Some(p) = self.presets.get(&n).cloned() {
                let container = p.apply(&mut self.s);
                self.set_container(&container);
                self.preset_msg = Some(format!("Applied “{n}”."));
            }
        }
        if changed {
            self.save_presets();
        }
        if let Some(n) = export_one {
            if let Some(p) = self.presets.get(&n) {
                let one: presets::Presets = [(n.clone(), p.clone())].into_iter().collect();
                self.write_presets_file(&format!("{n}.json"), &one);
            }
        }
        if export_all {
            let all = self.presets.clone();
            self.write_presets_file("ffcut-presets.json", &all);
        }
        if import {
            self.import_presets();
        }
    }

    fn write_presets_file(&mut self, default_name: &str, set: &presets::Presets) {
        let safe: String = default_name.chars().map(|c| if r#"/\:*?"<>|"#.contains(c) { '_' } else { c }).collect();
        let Some(path) = rfd::FileDialog::new().set_title("Export presets").add_filter("Presets (JSON)", &["json"]).set_file_name(safe).save_file() else { return };
        self.preset_msg = Some(match std::fs::write(&path, presets::export_json(set)) {
            Ok(()) => format!("Exported {} preset(s) to {}", set.len(), path.display()),
            Err(e) => format!("Cannot write {}: {e}", path.display()),
        });
    }

    fn import_presets(&mut self) {
        let Some(files) = rfd::FileDialog::new().set_title("Import presets").add_filter("Presets (JSON)", &["json"]).add_filter("All files", &["*"]).pick_files() else { return };
        let (mut added, mut skipped, mut errors) = (0, 0, Vec::new());
        for f in files {
            match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| presets::parse_import(&t)) {
                Ok((list, bad)) => {
                    skipped += bad;
                    for (name, p) in list {
                        // never overwrite silently: "Name", "Name (2)", ...
                        let mut n = name.clone();
                        let mut i = 2;
                        while self.presets.contains_key(&n) {
                            n = format!("{name} ({i})");
                            i += 1;
                        }
                        self.presets.insert(n, p);
                        added += 1;
                    }
                }
                Err(e) => errors.push(format!("{}: {e}", f.file_name().unwrap_or_default().to_string_lossy())),
            }
        }
        if added > 0 {
            self.save_presets();
        }
        let mut msg = format!("Imported {added} preset(s).");
        if skipped > 0 {
            msg.push_str(&format!(" {skipped} entr{} could not be read.", if skipped == 1 { "y" } else { "ies" }));
        }
        if !errors.is_empty() {
            msg.push_str(&format!(" {}", errors.join("; ")));
        }
        self.preset_msg = Some(msg);
    }
}
