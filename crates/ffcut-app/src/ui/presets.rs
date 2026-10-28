//! The presets window: export presets and audio effect presets. Rename, edit, apply, export, import, delete.

use super::side::{audio_mode_label, quality_fields, size_fields, PIX_FMTS, SPEEDS};
use super::widgets::ComboExt;
use super::{codecs, widgets};
use crate::app::{App, CONTAINERS};
use crate::state::PresetsTab;
use crate::theme::{self, radius, size};
use eframe::egui::{self, DragValue, RichText};
use ffcut_core::presets::{self, Part, Preset};
use ffcut_core::{AudioCodec, AudioMode, Quality, VideoCodec};
use std::collections::BTreeMap;

/// What a button on a preset card asked for.
enum CardAct {
    Apply,
    Export,
    Delete,
    StartRename,
    Rename(String),
    CancelRename,
}

/// One preset in the list: its name (or the rename field), the buttons, a one-line summary, and the editor.
fn preset_card(
    ui: &mut egui::Ui,
    name: &str,
    rename: Option<&mut String>,
    summary: &str,
    apply: (&str, Option<String>),
    exportable: bool,
    edit: impl FnOnce(&mut egui::Ui),
) -> Option<CardAct> {
    let mut act = None;
    widgets::box_frame().corner_radius(radius::PANEL).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            match rename {
                Some(new) => {
                    let r = ui.add(egui::TextEdit::singleline(new).desired_width(200.0));
                    r.request_focus();
                    if ui.small_button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        act = Some(CardAct::Rename(new.trim().to_string()));
                    }
                    if ui.small_button("Cancel").clicked() {
                        act = Some(CardAct::CancelRename);
                    }
                }
                None => {
                    ui.label(RichText::new(name).strong().size(size::HEADING - 2.0).color(theme::TEXT_STRONG));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Delete").clicked() {
                    act = Some(CardAct::Delete);
                }
                if exportable && ui.small_button("Export").on_hover_text("Save this preset to a file").clicked() {
                    act = Some(CardAct::Export);
                }
                if ui.small_button("Rename").clicked() {
                    act = Some(CardAct::StartRename);
                }
                let (label, tip) = apply;
                let b = ui.add_enabled(tip.is_some(), widgets::primary_button(label).small());
                if b.on_hover_text(tip.unwrap_or_default()).on_disabled_hover_text("Open a file first").clicked() {
                    act = Some(CardAct::Apply);
                }
            });
        });
        ui.add(egui::Label::new(RichText::new(summary).size(size::HINT).color(theme::TEXT_DIM)).truncate());
        egui::CollapsingHeader::new(RichText::new("Edit").size(size::BODY)).id_salt(("edit", name)).show(ui, edit);
    });
    ui.add_space(8.0);
    act
}

/// Renames `old` to `new` in `map`, refusing an empty name or one that is taken.
fn rename_in<T>(map: &mut BTreeMap<String, T>, old: &str, new: String) -> Result<(), String> {
    if new.is_empty() {
        return Err("A preset needs a name.".into());
    }
    if new != old && map.contains_key(&new) {
        return Err(format!("A preset named “{new}” already exists."));
    }
    if let Some(p) = map.remove(old) {
        map.insert(new, p);
    }
    Ok(())
}

/// The controls for one part of an export preset. True when something changed.
fn part_editor(
    ui: &mut egui::Ui,
    id: &str,
    part: Part,
    p: &mut Preset,
    enc: &Option<ffcut_core::probe::Encoders>,
    cur: (&str, &ffcut_core::ExportSettings),
) -> bool {
    let mut changed = false;
    match part {
        Part::Container => {
            let c = p.container.get_or_insert_with(|| "mp4".into());
            egui::ComboBox::from_id_salt(("pc", id)).selected_text(format!(".{c}")).show_list(ui, |ui| {
                for k in CONTAINERS {
                    changed |= ui.selectable_value(c, k.to_string(), format!(".{k}")).changed();
                }
            });
        }
        Part::VideoCodec => {
            let v = p.video_codec.get_or_insert(VideoCodec::Auto);
            let before = v.clone();
            codecs::video_codec_combo(ui, ("pv", id), v, enc, 230.0);
            changed |= before != *v;
        }
        Part::Quality => {
            let q = p.quality.get_or_insert(Quality::Crf(23));
            ui.horizontal(|ui| {
                changed |= quality_fields(ui, q);
                match q {
                    Quality::Crf(v) => changed |= widgets::num(ui, DragValue::new(v).range(0..=51)).changed(),
                    Quality::Bitrate(k) => changed |= widgets::num(ui, DragValue::new(k).range(100..=400_000).speed(50.0).suffix(" kbps")).changed(),
                }
            });
        }
        Part::Speed => {
            let sp = p.preset.get_or_insert_with(|| "medium".into());
            egui::ComboBox::from_id_salt(("ps", id)).selected_text(sp.clone()).show_list(ui, |ui| {
                for k in SPEEDS {
                    changed |= ui.selectable_value(sp, k.to_string(), k).changed();
                }
            });
        }
        Part::PixFmt => {
            let pf = p.pix_fmt.get_or_insert_with(|| "auto".into());
            egui::ComboBox::from_id_salt(("pp", id)).selected_text(pf.clone()).show_list(ui, |ui| {
                for k in std::iter::once("auto").chain(PIX_FMTS) {
                    changed |= ui.selectable_value(pf, k.to_string(), k).changed();
                }
            });
        }
        Part::Resize => {
            let sc = p.scale.get_or_insert_with(Default::default);
            ui.horizontal(|ui| changed |= size_fields(ui, sc));
        }
        Part::AudioCodec => {
            let a = p.audio_codec.get_or_insert_with(|| AudioCodec::Encoder("aac".into()));
            let before = a.clone();
            codecs::audio_codec_combo(ui, ("pa", id), a, enc, 230.0);
            changed |= before != *a;
        }
        Part::AudioBitrate => {
            changed |= widgets::num(ui, DragValue::new(p.audio_bitrate_kbps.get_or_insert(192)).range(32..=1536).speed(2.0).suffix(" kbps")).changed();
        }
        Part::AudioMode => {
            let m = p.audio_mode.get_or_insert(AudioMode::Separate);
            egui::ComboBox::from_id_salt(("pm", id)).selected_text(audio_mode_label(*m)).show_list(ui, |ui| {
                for k in [AudioMode::Separate, AudioMode::Merge, AudioMode::None] {
                    changed |= ui.selectable_value(m, k, audio_mode_label(k)).changed();
                }
            });
        }
        Part::VideoEffects | Part::Captions => {
            let text = match (part, &p.video_effects, &p.captions) {
                (Part::VideoEffects, Some(fx), _) => {
                    let names: Vec<&str> = fx.iter().map(|e| e.name()).collect();
                    if names.is_empty() {
                        "none".to_string()
                    } else {
                        names.join(", ")
                    }
                }
                (_, _, Some((t, b))) => format!("top {} · bottom {}", if t.on { "on" } else { "off" }, if b.on { "on" } else { "off" }),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(text).size(size::HINT).color(theme::TEXT_LABEL)).truncate());
                if ui.small_button("Update from current").on_hover_text("Replace with what is set up in the editor now").clicked() {
                    p.fill(part, cur.0, cur.1);
                    changed = true;
                }
            });
        }
    }
    changed
}

impl App {
    fn save_presets(&mut self) {
        if let Err(e) = self.presets.save_export() {
            self.presets_ui.msg = Some(e);
        }
    }

    /// Writes the audio presets to disk.
    pub fn save_fx_presets(&mut self) {
        if let Err(e) = self.presets.save_fx() {
            self.presets_ui.msg = Some(e.clone());
            self.status = Some(crate::app::Status::Error(e));
        }
    }

    fn set_presets_tab(&mut self, tab: PresetsTab) {
        self.presets_ui.tab = tab;
        self.presets_ui.rename = None;
        self.presets_ui.msg = None;
    }

    pub fn presets_window(&mut self, ctx: &egui::Context) {
        if !self.dialogs.presets {
            return;
        }
        let (mut export_all, mut import) = (false, false);
        let open = super::os_window_raw(ctx, "presets", "Presets", [700.0, 600.0], |ui| {
            egui::Panel::top("presets_head").frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::symmetric(18, 12))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Presets").size(size::PAGE).strong());
                    ui.add_space(8.0);
                    for (tab, label, n) in
                        [(PresetsTab::Export, "Export", self.presets.export.len()), (PresetsTab::Audio, "Audio effects", self.presets.fx.len())]
                    {
                        if ui.selectable_label(self.presets_ui.tab == tab, format!("{label}  {n}")).clicked() {
                            self.set_presets_tab(tab);
                        }
                    }
                    if self.presets_ui.tab == PresetsTab::Export {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            export_all = ui.add_enabled(!self.presets.export.is_empty(), egui::Button::new("Export all…")).clicked();
                            import =
                                ui.button("Import…").on_hover_text("Add presets from a .json file (also reads presets exported from the old FFcut)").clicked();
                        });
                    }
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::symmetric(18, 14))).show(ui, |ui| {
                if let Some(m) = &self.presets_ui.msg {
                    widgets::warn(ui, m.clone());
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.presets_ui.tab {
                    PresetsTab::Export => self.export_presets_tab(ui),
                    PresetsTab::Audio => self.fx_presets_tab(ui),
                });
            });
        });
        if !open {
            self.dialogs.presets = false;
            self.set_presets_tab(self.presets_ui.tab);
        }
        if export_all {
            let all = self.presets.export.clone();
            self.write_presets_file("ffcut-presets.json", &all);
        }
        if import {
            self.import_presets();
        }
    }

    /// Handles what the card of `name` asked for that is the same on both tabs. Returns the act if it is not.
    fn common_card_act<T>(&mut self, name: &str, act: CardAct, map: impl Fn(&mut Self) -> &mut BTreeMap<String, T>) -> (Option<CardAct>, bool) {
        match act {
            CardAct::StartRename => self.presets_ui.rename = Some((name.to_string(), name.to_string())),
            CardAct::CancelRename => self.presets_ui.rename = None,
            CardAct::Rename(new) => {
                let r = rename_in(map(self), name, new);
                match r {
                    Ok(()) => {
                        self.presets_ui.rename = None;
                        self.presets_ui.msg = None;
                        return (None, true);
                    }
                    Err(e) => self.presets_ui.msg = Some(e),
                }
            }
            CardAct::Delete => {
                map(self).remove(name);
                return (None, true);
            }
            other => return (Some(other), false),
        }
        (None, false)
    }

    fn rename_field(&mut self, name: &str) -> Option<&mut String> {
        self.presets_ui.rename.as_mut().filter(|(old, _)| old == name).map(|(_, new)| new)
    }

    fn export_presets_tab(&mut self, ui: &mut egui::Ui) {
        if self.presets.export.is_empty() {
            widgets::empty_state(ui, "No presets yet", "Set up codec, quality and audio in the sidebar, then Presets → Save current settings…");
            return;
        }
        let enc = self.encoders.lock().unwrap().clone();
        let (cur_container, cur) = (self.container.clone(), self.s.clone());
        let can_apply = self.clip.is_some();
        let mut changed = false;
        let names: Vec<String> = self.presets.export.keys().cloned().collect();
        for name in names {
            let summary = self.presets.export[&name].summary();
            let mut p = self.presets.export[&name].clone();
            let mut edited = false;
            let rename = self.presets_ui.rename.as_ref().filter(|(o, _)| *o == name).map(|(_, n)| n.clone());
            let mut rename_buf = rename.clone().unwrap_or_default();
            let act = preset_card(
                ui,
                &name,
                rename.is_some().then_some(&mut rename_buf),
                &summary,
                ("Apply", can_apply.then(|| "Use this preset for the open file".to_string())),
                true,
                |ui| {
                    widgets::hint(ui, "Tick what the preset should set (audio effects have their own presets). Anything unticked is left as it is when the preset is applied.");
                    ui.add_space(4.0);
                    widgets::grid(("grid", &name)).num_columns(3).show(ui, |ui| {
                        for part in Part::ALL {
                            let mut on = p.has(part);
                            if ui.checkbox(&mut on, "").changed() {
                                if on {
                                    p.fill(part, &cur_container, &cur);
                                } else {
                                    p.remove(part);
                                }
                                edited = true;
                            }
                            ui.label(part.label());
                            if on {
                                edited |= part_editor(ui, &name, part, &mut p, &enc, (&cur_container, &cur));
                            } else {
                                ui.label(RichText::new("not included").color(theme::TEXT_OFF));
                            }
                            ui.end_row();
                        }
                    });
                },
            );
            if let Some(f) = self.rename_field(&name) {
                *f = rename_buf;
            }
            if edited {
                self.presets.export.insert(name.clone(), p);
                changed = true;
            }
            let Some(act) = act else { continue };
            let (rest, ch) = self.common_card_act(&name, act, |a| &mut a.presets.export);
            changed |= ch;
            match rest {
                Some(CardAct::Apply) => {
                    if let Some(p) = self.presets.export.get(&name).cloned() {
                        self.apply_preset(&p);
                        self.presets_ui.msg = Some(format!("Applied “{name}”."));
                    }
                }
                Some(CardAct::Export) => {
                    if let Some(p) = self.presets.export.get(&name) {
                        let one: presets::Presets = [(name.clone(), p.clone())].into_iter().collect();
                        self.write_presets_file(&format!("{name}.json"), &one);
                    }
                }
                _ => {}
            }
        }
        if changed {
            self.save_presets();
        }
    }

    /// The "Audio effects" tab: named effect stacks, applied to one track at a time.
    fn fx_presets_tab(&mut self, ui: &mut egui::Ui) {
        if self.presets.fx.is_empty() {
            widgets::empty_state(ui, "No audio presets yet", "Set up effects on a track in the sidebar's Audio section, then Presets → Save these effects…");
            return;
        }
        let track = self.side.fx_track.filter(|i| *i < self.s.tracks.len()).or((!self.s.tracks.is_empty()).then_some(0));
        let mut changed = false;
        let names: Vec<String> = self.presets.fx.keys().cloned().collect();
        let cat = self.catalog.clone();
        for name in names {
            let mut fx = self.presets.fx[&name].clone();
            let summary = {
                let n: Vec<&str> = fx.iter().map(|e| e.name()).collect();
                if n.is_empty() {
                    "no effects".to_string()
                } else {
                    n.join(" · ")
                }
            };
            let rename = self.presets_ui.rename.as_ref().filter(|(o, _)| *o == name).map(|(_, n)| n.clone());
            let mut rename_buf = rename.clone().unwrap_or_default();
            let tip = track.map(|i| format!("Replace the effects of “{}”", self.s.tracks[i].label));
            let mut edited = false;
            let act = preset_card(ui, &name, rename.is_some().then_some(&mut rename_buf), &summary, ("Apply to track", tip), false, |ui| {
                edited = super::fx_ui::preset_effects(ui, &mut fx, &cat);
            });
            if let Some(f) = self.rename_field(&name) {
                *f = rename_buf;
            }
            if edited {
                self.presets.fx.insert(name.clone(), fx);
                changed = true;
            }
            let Some(act) = act else { continue };
            let (rest, ch) = self.common_card_act(&name, act, |a| &mut a.presets.fx);
            changed |= ch;
            if let (Some(CardAct::Apply), Some(i)) = (rest, track) {
                if let Some(fx) = self.presets.fx.get(&name) {
                    self.s.tracks[i].effects = fx.clone();
                    self.presets_ui.msg = Some(format!("Applied “{name}” to {}.", self.s.tracks[i].label));
                }
            }
        }
        if changed {
            self.save_fx_presets();
        }
    }

    fn write_presets_file(&mut self, default_name: &str, set: &presets::Presets) {
        let safe: String = default_name.chars().map(|c| if r#"/\:*?"<>|"#.contains(c) { '_' } else { c }).collect();
        let Some(path) = rfd::FileDialog::new().set_title("Export presets").add_filter("Presets (JSON)", &["json"]).set_file_name(safe).save_file() else {
            return;
        };
        let written = presets::export_json(set).map_err(|e| e.to_string()).and_then(|j| std::fs::write(&path, j).map_err(|e| e.to_string()));
        self.presets_ui.msg = Some(match written {
            Ok(()) => format!("Exported {} preset(s) to {}", set.len(), path.display()),
            Err(e) => format!("Cannot write {}: {e}", path.display()),
        });
    }

    fn import_presets(&mut self) {
        let Some(files) =
            rfd::FileDialog::new().set_title("Import presets").add_filter("Presets (JSON)", &["json"]).add_filter("All files", &["*"]).pick_files()
        else {
            return;
        };
        let (mut added, mut skipped, mut errors) = (0, 0, Vec::new());
        for f in files {
            match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| presets::parse_import(&t).map_err(|e| e.to_string())) {
                Ok((list, bad)) => {
                    skipped += bad;
                    for (name, p) in list {
                        // never overwrite silently: "Name", "Name (2)", ...
                        let mut n = name.clone();
                        let mut i = 2;
                        while self.presets.export.contains_key(&n) {
                            n = format!("{name} ({i})");
                            i += 1;
                        }
                        self.presets.export.insert(n, p);
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
        self.presets_ui.msg = Some(msg);
    }
}
