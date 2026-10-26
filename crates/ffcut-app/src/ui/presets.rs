//! "Edit presets" window: rename, edit every field, apply, export one / all, import, delete.

use super::codecs;
use crate::app::{App, CONTAINERS};
use crate::theme;
use eframe::egui::{self, Color32, DragValue, RichText};
use ffcut_core::presets::{self, Part, Preset};
use ffcut_core::{AudioCodec, AudioMode, Quality, Scale, VideoCodec};

fn audio_mode_label(m: AudioMode) -> &'static str {
    match m {
        AudioMode::None => "No audio",
        AudioMode::Merge => "Merge into one track",
        AudioMode::Separate => "Keep tracks separate",
    }
}

const SPEEDS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
const PIX_FMTS: [&str; 7] = ["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "nv12", "p010le"];

fn summary(p: &Preset) -> String {
    let mut v: Vec<String> = Vec::new();
    if let Some(c) = &p.container {
        v.push(format!(".{c}"));
    }
    match (&p.video_codec, p.quality) {
        (Some(VideoCodec::Copy), _) => v.push("video copy".into()),
        (vc, q) => {
            let name = match vc {
                Some(VideoCodec::Auto) => Some("auto".to_string()),
                Some(VideoCodec::Encoder(e)) => Some(e.clone()),
                _ => None,
            };
            let q = q.map(|q| match q {
                Quality::Crf(c) => format!("CRF {c}"),
                Quality::Bitrate(k) => format!("{k} kbps"),
            });
            match (name, q) {
                (Some(n), Some(q)) => v.push(format!("{n} ({q})")),
                (Some(n), None) => v.push(n),
                (None, Some(q)) => v.push(q),
                _ => {}
            }
        }
    }
    match &p.audio_codec {
        Some(AudioCodec::Copy) => v.push("audio copy".into()),
        Some(AudioCodec::Encoder(e)) => v.push(format!("audio {e}")),
        None => {}
    }
    if let Some(sc) = p.scale {
        if sc.w.is_some() || sc.h.is_some() {
            v.push(format!("{}×{}", sc.w.map_or("auto".into(), |x| x.to_string()), sc.h.map_or("auto".into(), |x| x.to_string())));
        }
    }
    if let Some(fx) = &p.video_effects {
        v.push(format!("{} video effect(s)", fx.len()));
    }
    if let Some((t, b)) = &p.captions {
        v.push(format!("captions{}", if t.on || b.on { "" } else { " (off)" }));
    }
    if v.is_empty() { "empty: applying changes nothing".into() } else { v.join(" · ") }
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
        let (cur_container, cur) = (self.container.clone(), self.s.clone());
        let open;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut export_one: Option<String> = None;
        let mut rename: Option<(String, String)> = None;
        let mut changed = false;
        let (mut export_all, mut import) = (false, false);

        open = super::os_window_raw(ctx, "presets", "Presets", [700.0, 600.0], |ui| {
            egui::Panel::top("presets_head").frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::symmetric(18, 12))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Presets").size(18.0).strong());
                    ui.add_space(8.0);
                    for (tab, label, n) in [(0u8, "Export", self.presets.len()), (1, "Audio effects", self.fx_presets.len())] {
                        if ui.selectable_label(self.presets_tab == tab, format!("{label}  {n}")).clicked() {
                            self.presets_tab = tab;
                            self.preset_rename = None;
                            self.fx_rename = None;
                            self.preset_msg = None;
                        }
                    }
                    if self.presets_tab == 0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(!self.presets.is_empty(), egui::Button::new("Export all…")).clicked() {
                                export_all = true;
                            }
                            if ui.button("Import…").on_hover_text("Add presets from a .json file (also reads presets exported from the old FFcut)").clicked() {
                                import = true;
                            }
                        });
                    }
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::symmetric(18, 14))).show(ui, |ui| {
            if let Some(m) = &self.preset_msg {
                ui.add(egui::Label::new(RichText::new(m).size(12.5).color(theme::WARN)).wrap());
                ui.add_space(6.0);
            }

            if self.presets_tab == 1 {
                self.fx_presets_tab(ui);
                return;
            }
            if self.presets.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No presets yet").size(15.0).color(Color32::from_gray(190)));
                    ui.add_space(4.0);
                    ui.weak("Set up codec, quality and audio in the sidebar, then Presets → Save preset…");
                });
                return;
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let names: Vec<String> = self.presets.keys().cloned().collect();
                for name in names {
                    egui::Frame::NONE.fill(Color32::from_rgb(40, 42, 48)).stroke(egui::Stroke::new(1.0, Color32::from_gray(54))).corner_radius(10).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
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
                                if ui.add_enabled(self.clip.is_some(), egui::Button::new(RichText::new("Apply").color(Color32::BLACK)).fill(theme::ACCENT).small()).on_hover_text("Use this preset for the open video").clicked() {
                                    apply = Some(name.clone());
                                }
                            });
                        });
                        ui.add(egui::Label::new(RichText::new(summary(&self.presets[&name])).size(12.0).color(Color32::from_gray(140))).truncate());

                        egui::CollapsingHeader::new(RichText::new("Edit").size(12.5)).id_salt(("edit", &name)).show(ui, |ui| {
                            let p = self.presets.get_mut(&name).unwrap();
                            ui.add(egui::Label::new(RichText::new("Tick what the preset should set (audio effects have their own presets in the sidebar). Anything unticked is left as it is when the preset is applied.").size(11.5).color(Color32::from_gray(130))).wrap());
                            ui.add_space(4.0);
                            egui::Grid::new(("grid", &name)).num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                                for part in Part::ALL {
                                    let mut on = p.has(part);
                                    if ui.checkbox(&mut on, "").changed() {
                                        if on {
                                            p.fill(part, &cur_container, &cur);
                                        } else {
                                            p.remove(part);
                                        }
                                        changed = true;
                                    }
                                    ui.label(part.label());
                                    if !on {
                                        ui.weak("not included");
                                        ui.end_row();
                                        continue;
                                    }
                                    match part {
                                        Part::Container => {
                                            let c = p.container.as_mut().unwrap();
                                            egui::ComboBox::from_id_salt(("pc", &name)).selected_text(format!(".{c}")).show_ui(ui, |ui| {
                                                for k in CONTAINERS {
                                                    changed |= ui.selectable_value(c, k.to_string(), format!(".{k}")).changed();
                                                }
                                            });
                                        }
                                        Part::VideoCodec => {
                                            let v = p.video_codec.as_mut().unwrap();
                                            let before = v.clone();
                                            codecs::video_codec_combo(ui, ("pv", &name), v, &enc, 230.0);
                                            changed |= before != *v;
                                        }
                                        Part::Quality => {
                                            let q = p.quality.as_mut().unwrap();
                                            ui.horizontal(|ui| {
                                                let mut crf = matches!(q, Quality::Crf(_));
                                                if ui.radio_value(&mut crf, true, "CRF").clicked() && !matches!(q, Quality::Crf(_)) {
                                                    *q = Quality::Crf(23);
                                                    changed = true;
                                                }
                                                if ui.radio_value(&mut crf, false, "Bitrate").clicked() && !matches!(q, Quality::Bitrate(_)) {
                                                    *q = Quality::Bitrate(8000);
                                                    changed = true;
                                                }
                                                match q {
                                                    Quality::Crf(v) => changed |= ui.add(DragValue::new(v).range(0..=51)).changed(),
                                                    Quality::Bitrate(k) => changed |= ui.add(DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps")).changed(),
                                                }
                                            });
                                        }
                                        Part::Speed => {
                                            let sp = p.preset.as_mut().unwrap();
                                            egui::ComboBox::from_id_salt(("ps", &name)).selected_text(sp.clone()).show_ui(ui, |ui| {
                                                for k in SPEEDS {
                                                    changed |= ui.selectable_value(sp, k.to_string(), k).changed();
                                                }
                                            });
                                        }
                                        Part::PixFmt => {
                                            let pf = p.pix_fmt.as_mut().unwrap();
                                            egui::ComboBox::from_id_salt(("pp", &name)).selected_text(pf.clone()).show_ui(ui, |ui| {
                                                changed |= ui.selectable_value(pf, "auto".to_string(), "auto").changed();
                                                for k in PIX_FMTS {
                                                    changed |= ui.selectable_value(pf, k.to_string(), k).changed();
                                                }
                                            });
                                        }
                                        Part::Resize => {
                                            let sc = p.scale.as_mut().unwrap();
                                            ui.horizontal(|ui| {
                                                let (mut w, mut h) = (sc.w.unwrap_or(0), sc.h.unwrap_or(0));
                                                let fmt = |n: f64, _: std::ops::RangeInclusive<usize>| if n == 0.0 { "auto".to_string() } else { format!("{n:.0}") };
                                                let a = ui.add(DragValue::new(&mut w).range(0..=16384).speed(2.0).custom_formatter(fmt));
                                                ui.label("×");
                                                let b = ui.add(DragValue::new(&mut h).range(0..=16384).speed(2.0).custom_formatter(fmt));
                                                if a.changed() || b.changed() {
                                                    *sc = Scale { w: (w > 0).then_some(w), h: (h > 0).then_some(h) };
                                                    changed = true;
                                                }
                                            });
                                        }
                                        Part::AudioCodec => {
                                            let a = p.audio_codec.as_mut().unwrap();
                                            let before = a.clone();
                                            codecs::audio_codec_combo(ui, ("pa", &name), a, &enc, 230.0);
                                            changed |= before != *a;
                                        }
                                        Part::AudioBitrate => {
                                            changed |= ui.add(DragValue::new(p.audio_bitrate_kbps.as_mut().unwrap()).range(32..=1536).speed(2.0).suffix(" kbps")).changed();
                                        }
                                        Part::AudioMode => {
                                            let m = p.audio_mode.as_mut().unwrap();
                                            egui::ComboBox::from_id_salt(("pm", &name)).selected_text(audio_mode_label(*m)).show_ui(ui, |ui| {
                                                for k in [AudioMode::None, AudioMode::Merge, AudioMode::Separate] {
                                                    changed |= ui.selectable_value(m, k, audio_mode_label(k)).changed();
                                                }
                                            });
                                        }
                                        Part::VideoEffects | Part::Captions => {
                                            let text = match part {
                                                Part::VideoEffects => {
                                                    let names: Vec<&str> = p.video_effects.as_ref().unwrap().iter().map(|e| e.name()).collect();
                                                    if names.is_empty() { "none".to_string() } else { names.join(", ") }
                                                }
                                                _ => {
                                                    let (t, b) = p.captions.as_ref().unwrap();
                                                    format!("top {} · bottom {}", if t.on { "on" } else { "off" }, if b.on { "on" } else { "off" })
                                                }
                                            };
                                            ui.horizontal(|ui| {
                                                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(Color32::from_gray(160))).truncate());
                                                if ui.small_button("Update from current").on_hover_text("Replace with what is set up in the editor now").clicked() {
                                                    p.fill(part, &cur_container, &cur);
                                                    changed = true;
                                                }
                                            });
                                        }
                                    }
                                    ui.end_row();
                                }
                            });

                        });
                    });
                    ui.add_space(8.0);
                }
            });
            });
        });
        if !open {
            self.show_presets = false;
            self.preset_rename = None;
            self.fx_rename = None;
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
                self.apply_preset(&p);
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

    /// The "Audio effects" tab: named effect stacks, applied to one track at a time.
    fn fx_presets_tab(&mut self, ui: &mut egui::Ui) {
        if self.fx_presets.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No audio presets yet").size(15.0).color(Color32::from_gray(190)));
                ui.add_space(4.0);
                ui.weak("Set up effects on a track, then Audio effects → Audio presets → Save preset…");
            });
            return;
        }
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut rename: Option<(String, String)> = None;
        let mut changed = false;
        let track = self.fx.window.filter(|i| *i < self.s.tracks.len()).or(if self.s.tracks.is_empty() { None } else { Some(0) });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let names: Vec<String> = self.fx_presets.keys().cloned().collect();
            for name in names {
                egui::Frame::NONE.fill(Color32::from_rgb(40, 42, 48)).stroke(egui::Stroke::new(1.0, Color32::from_gray(54))).corner_radius(10).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let renaming = self.fx_rename.as_ref().map_or(false, |(old, _)| *old == name);
                    ui.horizontal(|ui| {
                        if renaming {
                            let new = &mut self.fx_rename.as_mut().unwrap().1;
                            let r = ui.add(egui::TextEdit::singleline(new).desired_width(200.0));
                            r.request_focus();
                            if ui.small_button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                rename = Some((name.clone(), new.trim().to_string()));
                            }
                            if ui.small_button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                self.fx_rename = None;
                            }
                        } else {
                            ui.label(RichText::new(&name).strong().size(14.0));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Delete").clicked() {
                                delete = Some(name.clone());
                            }
                            if ui.small_button("Rename").clicked() {
                                self.fx_rename = Some((name.clone(), name.clone()));
                            }
                            let tip = track.map_or("Open a video or audio file first".to_string(), |i| format!("Replace the effects of “{}”", self.s.tracks[i].label));
                            if ui.add_enabled(track.is_some(), egui::Button::new(RichText::new("Apply to track").color(Color32::BLACK)).fill(theme::ACCENT).small()).on_hover_text(tip).clicked() {
                                apply = Some(name.clone());
                            }
                        });
                    });
                    let fx = self.fx_presets.get_mut(&name).unwrap();
                    let names: Vec<&str> = fx.iter().map(|e| e.name()).collect();
                    ui.add(egui::Label::new(RichText::new(if names.is_empty() { "no effects".to_string() } else { names.join(" · ") }).size(12.0).color(Color32::from_gray(140))).truncate());
                    egui::CollapsingHeader::new(RichText::new("Edit").size(12.5)).id_salt(("fxedit", &name)).show(ui, |ui| {
                        changed |= super::fx_ui::preset_effects(ui, fx);
                    });
                });
                ui.add_space(8.0);
            }
        });
        if let Some((old, new)) = rename {
            if new.is_empty() {
                self.preset_msg = Some("A preset needs a name.".into());
            } else if new != old && self.fx_presets.contains_key(&new) {
                self.preset_msg = Some(format!("A preset named “{new}” already exists."));
            } else {
                if let Some(p) = self.fx_presets.remove(&old) {
                    self.fx_presets.insert(new, p);
                }
                self.fx_rename = None;
                self.preset_msg = None;
                changed = true;
            }
        }
        if let Some(n) = delete {
            self.fx_presets.remove(&n);
            changed = true;
        }
        if let (Some(n), Some(i)) = (apply, track) {
            if let Some(fx) = self.fx_presets.get(&n) {
                self.s.tracks[i].effects = fx.clone();
                self.preset_msg = Some(format!("Applied “{n}” to {}.", self.s.tracks[i].label));
            }
        }
        if changed {
            self.save_fx_presets();
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
