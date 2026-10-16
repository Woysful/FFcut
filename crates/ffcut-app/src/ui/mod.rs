//! All widgets. Each file adds an `impl App` block for one area of the window.

mod codecs;
mod command;
mod presets;
mod side;
mod tracks;
mod transport;
mod video;

use crate::app::{open_in_file_manager, App, Status};
use crate::theme;
use eframe::egui::{self, vec2, Align, Layout, Rect, RichText};
use ffcut_core::presets::Preset;
use ffcut_core::AudioStream;

pub fn audio_label(a: &AudioStream) -> String {
    // "#2 Commentary · aac 2ch" — the title helps most, the language only when it says something
    let mut s = format!("#{}", a.index + 1);
    if let Some(t) = &a.title {
        s.push_str(&format!(" {t} ·"));
    }
    s.push_str(&format!(" {} {}ch", a.codec, a.channels));
    if let Some(l) = a.language.as_ref().filter(|l| l.as_str() != "und") {
        s.push_str(&format!(" · {l}"));
    }
    s
}

/// One line for the menu bar: codec · size · fps · pixel format · audio · bitrate · file size.
fn file_info_line(i: &ffcut_core::SourceInfo) -> String {
    let mut parts = vec![
        i.video_codec.to_uppercase(),
        format!("{}×{}", i.width, i.height),
        format!("{:.2} fps", i.fps),
    ];
    if !i.pix_fmt.is_empty() {
        parts.push(i.pix_fmt.clone());
    }
    match i.audio.first() {
        Some(a) => parts.push(format!(
            "{} {}ch{}",
            a.codec.to_uppercase(),
            a.channels,
            if i.audio.len() > 1 { format!(" ({} tracks)", i.audio.len()) } else { String::new() }
        )),
        None => parts.push("no audio".into()),
    }
    if i.bit_rate > 0 {
        parts.push(format!("{:.2} Mbps", i.bit_rate as f64 / 1_000_000.0));
    }
    parts.push(format!("{:.1} MB", i.file_size as f64 / 1_048_576.0));
    if !i.format.is_empty() {
        parts.push(i.format.clone());
    }
    parts.join("  ·  ")
}

impl App {
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            let bar = ui.max_rect();
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…   Ctrl+O").clicked() {
                        ui.close();
                        self.open_dialog();
                    }
                    let can_export = self.clip.is_some() && self.job.is_none();
                    if ui.add_enabled(can_export, egui::Button::new("Export…   Ctrl+E")).clicked() {
                        ui.close();
                        self.export_clicked();
                    }
                    ui.separator();
                    if ui.button("Settings…").clicked() {
                        ui.close();
                        self.show_settings = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Help", |ui| {
                    ui.label("Space  play/pause");
                    ui.label("← →  or  , .   step one frame");
                    ui.label("Shift+← →   seek 5 s");
                    ui.label("Q / E   set trim start / end");
                    ui.label("R   reset trim");
                    ui.label("C   toggle crop");
                    ui.label("A   loop selection,  Shift+A  lock trim");
                    ui.label("F11   interface fullscreen");
                    ui.label("Ctrl+F   video fullscreen  (Esc to leave)");
                    ui.label("Ctrl+O   open,   Ctrl+E   export");
                    ui.separator();
                    if ui.button("Check for updates").clicked() {
                        ui.close();
                        self.show_about = true;
                        self.start_update_check(ui.ctx(), true);
                    }
                    if ui.button(format!("About FFcut {}", crate::update::VERSION)).clicked() {
                        ui.close();
                        self.show_about = true;
                    }
                });

                // right: presets
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.presets_menu(ui);
                    // the "lamp": a newer release exists
                    if let Some(r) = &self.update_found {
                        let b = egui::Button::new(RichText::new(format!("● Update {}", r.version)).size(12.5).color(theme::ACCENT)).frame(false);
                        if ui.add(b).on_hover_text("A new version of FFcut is available. Click for details.").clicked() {
                            self.show_update = true;
                        }
                    }
                });

                // centre: what is open (painted over the free middle part, never pushes the menus)
                if let Some(c) = &self.clip {
                    let text = file_info_line(&c.info);
                    let w = (bar.width() - 2.0 * 190.0).max(100.0);
                    let r = Rect::from_center_size(bar.center(), vec2(w, bar.height()));
                    ui.put(r, egui::Label::new(RichText::new(text).size(12.5).color(egui::Color32::from_gray(170))).truncate());
                }
            });
        });
    }

    fn presets_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Presets", |ui| {
            if ui.add_enabled(self.clip.is_some(), egui::Button::new("Save preset…")).clicked() {
                ui.close();
                self.show_save_preset = true;
                self.preset_name.clear();
            }
            if ui.button("Edit presets…").clicked() {
                ui.close();
                self.show_presets = true;
            }
            ui.separator();
            if self.presets.is_empty() {
                ui.weak("No presets yet");
            }
            let names: Vec<String> = self.presets.keys().cloned().collect();
            for name in names {
                if ui.add_enabled(self.clip.is_some(), egui::Button::new(&name)).clicked() {
                    if let Some(p) = self.presets.get(&name).cloned() {
                        let container = p.apply(&mut self.s);
                        self.set_container(&container);
                    }
                    ui.close();
                }
            }
        });
    }

    /// Windows: settings, save preset, presets editor, errors.
    pub fn windows(&mut self, ctx: &egui::Context) {
        self.presets_window(ctx);
        if let Some((title, body)) = self.error_window.clone() {
            let mut open = true;
            egui::Window::new(RichText::new(&title).color(theme::ERR)).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&body).monospace()).selectable(true));
                });
                ui.add_space(6.0);
                if ui.button("Copy message").clicked() {
                    ctx.copy_text(body.clone());
                }
            });
            if !open {
                self.error_window = None;
            }
        }

        if self.show_save_preset {
            egui::Window::new("Save preset").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Saves codec, quality, audio, scale and container.\n(Trim, crop and rotation belong to the clip and are not saved.)");
                let r = ui.text_edit_singleline(&mut self.preset_name);
                r.request_focus();
                let name = self.preset_name.trim().to_string();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if (ui.add_enabled(!name.is_empty(), egui::Button::new("Save")).clicked() || (enter && !name.is_empty())) && !name.is_empty() {
                        self.presets.insert(name, Preset::capture(&self.container, &self.s));
                        if let Err(e) = ffcut_core::presets::save(&self.presets_path, &self.presets) {
                            self.status = Some(Status::Error(format!("Cannot save presets: {e}")));
                        }
                        self.show_save_preset = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_save_preset = false;
                    }
                });
            });
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("About FFcut").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(RichText::new(format!("FFcut {}", crate::update::VERSION)).size(18.0).strong());
                ui.label("A small native ffmpeg front end: trim, crop, rotate, re-encode, audio tracks.");
                ui.add_space(4.0);
                if crate::update::configured() {
                    ui.hyperlink_to(crate::update::repo_url(), crate::update::repo_url());
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.update_rx.is_none(), egui::Button::new("Check for updates")).clicked() {
                        self.start_update_check(ui.ctx(), true);
                    }
                    if let Some(m) = &self.update_msg {
                        ui.label(RichText::new(m).color(egui::Color32::from_gray(170)));
                    }
                });
                if self.update_found.is_some() && ui.button("Show the new version…").clicked() {
                    self.show_update = true;
                }
            });
            if !open {
                self.show_about = false;
            }
        }

        if self.show_update {
            let mut open = true;
            let rel = self.update_found.clone();
            egui::Window::new("Update available").open(&mut open).collapsible(false).default_width(460.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                let Some(r) = &rel else {
                    ui.label("You have the latest version.");
                    return;
                };
                ui.label(RichText::new(format!("FFcut {} is out", r.version)).size(16.0).strong());
                ui.weak(format!("You have {}.", crate::update::VERSION));
                if !r.notes.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(&r.notes).size(12.5)).wrap());
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let b = egui::Button::new(RichText::new("Open the release page").color(egui::Color32::BLACK)).fill(theme::ACCENT);
                    if ui.add(b).clicked() {
                        let _ = open::that(&r.page);
                    }
                    if ui.button("Skip this version").on_hover_text("Hide the lamp until an even newer version appears").clicked() {
                        self.settings.skipped_version = r.version.clone();
                        self.settings.save();
                        self.update_found = None;
                        self.show_update = false;
                    }
                });
                ui.add_space(4.0);
                ui.weak("Windows: download the zip from the release page and replace the old folder.\nLinux: download the new AppImage (or let GearLever / AppImageUpdate do it).");
            });
            if !open {
                self.show_update = false;
            }
        }

        if self.show_settings {
            let mut open = true;
            egui::Window::new("Settings").open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
                ui.label("Leave a path empty to auto-detect (next to FFcut, then PATH).");
                ui.add_space(4.0);
                let mut changed = false;
                egui::Grid::new("tools").num_columns(2).show(ui, |ui| {
                    ui.label("ffmpeg");
                    changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.ffmpeg_path).desired_width(360.0).hint_text("auto")).lost_focus();
                    ui.end_row();
                    ui.label("ffprobe");
                    changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.ffprobe_path).desired_width(360.0).hint_text("auto")).lost_focus();
                    ui.end_row();
                });
                if changed {
                    self.locate_tools(ctx);
                    self.settings.save();
                }
                ui.add_space(6.0);
                match (&self.ffmpeg, &self.ffprobe) {
                    (Some(f), Some(p)) => {
                        ui.colored_label(theme::OK, format!("✔ {}", self.ffmpeg_version));
                        ui.weak(format!("{}\n{}", f.display(), p.display()));
                    }
                    _ => {
                        ui.colored_label(theme::ERR, "ffmpeg and/or ffprobe not found");
                    }
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(RichText::new("Display server").strong());
                    let mut cur = match self.settings.display_server.as_str() {
                        v @ ("wayland" | "x11") => v.to_string(),
                        _ => String::new(),
                    };
                    ui.horizontal(|ui| {
                        for (v, label) in [("", "Auto"), ("wayland", "Wayland"), ("x11", "X11 (XWayland)")] {
                            ui.radio_value(&mut cur, v.to_string(), label);
                        }
                    });
                    if cur != self.settings.display_server {
                        self.settings.display_server = cur;
                        self.settings.save();
                    }
                    ui.label(format!("Running on: {}. A change takes effect after a restart.", self.backend.name()));
                    ui.add(egui::Label::new(RichText::new("Dragging files into the window does not work on native Wayland (the windowing library has no support for it yet). Choose X11 to get it through XWayland.").size(11.5).color(egui::Color32::from_gray(135))).wrap());
                    if crate::display::resolve(&self.settings.display_server) != self.backend && ui.button("Restart now").clicked() {
                        let open = self.clip.as_ref().map(|c| c.path.clone());
                        if crate::display::restart(open.as_deref()) {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new(format!("Updates  ·  FFcut {}", crate::update::VERSION)).strong());
                let mut check = !self.settings.no_update_check;
                if ui.checkbox(&mut check, "Look for a new version at startup").on_hover_text("One small request to GitHub when FFcut starts. Nothing is downloaded or installed.").changed() {
                    self.settings.no_update_check = !check;
                    self.settings.save();
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.update_rx.is_none(), egui::Button::new("Check now")).clicked() {
                        self.start_update_check(ui.ctx(), true);
                    }
                    if let Some(m) = &self.update_msg {
                        ui.label(RichText::new(m).color(egui::Color32::from_gray(170)));
                    }
                });
                ui.add_space(8.0);
                ui.weak(format!("Settings file: {}", crate::settings::config_dir().join("settings.json").display()));
            });
            if !open {
                self.show_settings = false;
            }
        }
    }

    pub fn show_in_folder(&self, p: &std::path::Path) {
        open_in_file_manager(p);
    }
}
