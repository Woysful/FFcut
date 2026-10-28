//! All widgets. Each file adds an `impl App` block for one area of the window; `widgets` holds the small
//! building blocks they share.

mod audio_view;
mod codecs;
mod command;
mod filter_ui;
mod fx_ui;
mod icons;
mod meter;
mod plugin_ui;
mod presets;
mod settings_ui;
mod side;
mod tracks;
mod transport;
mod vfx_ui;
mod video;
pub mod widgets;

pub use meter::MeterState;
pub use side::{Section, SideState};
pub use widgets::{card, ctx_menu, field, grid, num};

use crate::app::{App, Status};
use crate::hotkeys::{chords, Action};
use crate::theme::{self, size};
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

/// What the name prompt of a "save as" dialog asks for.
enum Prompt {
    Save(String),
    Cancel,
}

/// A name field with Save / Cancel (Enter saves). `ready` = everything else needed to save is there.
fn name_prompt(ui: &mut egui::Ui, name: &mut String, ready: bool) -> Option<Prompt> {
    let r = ui.add(egui::TextEdit::singleline(name).hint_text("Name").desired_width(f32::INFINITY));
    r.request_focus();
    let trimmed = name.trim().to_string();
    let ok = ready && !trimmed.is_empty();
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut out = None;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(ok, widgets::primary_button("Save")).clicked() || (enter && ok) {
            out = Some(Prompt::Save(trimmed.clone()));
        }
        if ui.button("Cancel").clicked() {
            out = Some(Prompt::Cancel);
        }
    });
    out
}

impl App {
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            let bar = ui.max_rect();
            let config = egui::containers::menu::MenuConfig::new().style(theme::menu_style);
            egui::MenuBar::new().style(theme::menu_style).config(config).ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button(format!("Open…   {}", self.keys(Action::Open))).clicked() {
                        ui.close();
                        self.open_dialog();
                    }
                    let can_export = self.clip.is_some() && self.job.is_none();
                    if ui.add_enabled(can_export, egui::Button::new(format!("Export…   {}", self.keys(Action::Export)))).clicked() {
                        ui.close();
                        self.export_clicked();
                    }
                    ui.separator();
                    if ui.button("Settings…").clicked() {
                        ui.close();
                        self.dialogs.settings = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let ctx = ui.ctx().clone();
                    if ui.add_enabled(self.clip.is_some() && self.hist.can_undo(), egui::Button::new(format!("Undo   {}", self.keys(Action::Undo)))).clicked() {
                        ui.close();
                        self.undo(&ctx);
                    }
                    if ui.add_enabled(self.clip.is_some() && self.hist.can_redo(), egui::Button::new(format!("Redo   {}", self.keys(Action::Redo)))).clicked() {
                        ui.close();
                        self.redo(&ctx);
                    }
                });

                ui.menu_button("Help", |ui| {
                    widgets::menu_indent(ui, |ui| widgets::hint(ui, "Keyboard shortcuts (change them in Settings → Hotkeys)"));
                    widgets::menu_indent(ui, |ui| {
                        egui::Grid::new("help-keys").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
                            for a in Action::ALL {
                                let keys: Vec<String> = chords(&self.settings.hotkeys, a).iter().map(|c| c.display()).collect();
                                ui.label(RichText::new(if keys.is_empty() { "—".to_string() } else { keys.join("  /  ") }).monospace().size(12.0));
                                ui.label(a.label());
                                ui.end_row();
                            }
                        })
                    });
                    ui.separator();
                    if ui.button("Check for updates").clicked() {
                        ui.close();
                        self.dialogs.about = true;
                        self.start_update_check(ui.ctx(), true);
                    }
                    if ui.button(format!("About FFcut {}", crate::update::VERSION)).clicked() {
                        ui.close();
                        self.dialogs.about = true;
                    }
                });

                // right (next to the sidebar): presets, and the "lamp" when a newer release exists
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("Presets", |ui| self.presets_menu(ui));
                    if let Some(r) = &self.updates.found {
                        let b = egui::Button::new(RichText::new(format!("● Update {}", r.version)).color(theme::ACCENT)).frame(false);
                        if ui.add(b).on_hover_text("A new version of FFcut is available. Click for details.").clicked() {
                            self.dialogs.update = true;
                        }
                    }
                });

                // centre: what is open (painted over the free middle part, never pushes the menus)
                if let Some(c) = &self.clip {
                    let text = crate::layout::info_line(&c.path, &c.info, &self.settings.info_items, &self.settings.path_sep);
                    let w = (bar.width() - 2.0 * 230.0).max(100.0);
                    let r = Rect::from_center_size(bar.center(), vec2(w, bar.height()));
                    ui.put(r, egui::Label::new(RichText::new(text).color(theme::TEXT_LABEL)).truncate());
                }
            });
        });
    }

    fn presets_menu(&mut self, ui: &mut egui::Ui) {
        if ui.add_enabled(self.clip.is_some(), egui::Button::new("Save current settings…")).clicked() {
            ui.close();
            self.presets_ui.save_parts = Preset::default_parts(&self.s);
            self.presets_ui.name.clear();
            self.dialogs.save_preset = true;
        }
        if ui.button("Manage presets…").clicked() {
            ui.close();
            self.presets_ui.tab = crate::state::PresetsTab::Export;
            self.dialogs.presets = true;
        }
        ui.separator();
        if self.presets.export.is_empty() {
            widgets::menu_indent(ui, |ui| ui.weak("No presets yet"));
        }
        let mut chosen = None;
        for name in self.presets.export.keys() {
            if ui.add_enabled(self.clip.is_some(), egui::Button::new(name)).clicked() {
                chosen = Some(name.clone());
                ui.close();
            }
        }
        if let Some(p) = chosen.and_then(|n| self.presets.export.get(&n).cloned()) {
            self.apply_preset(&p);
        }
    }

    /// Windows: settings, save preset, presets editor, errors, about, update.
    pub fn windows(&mut self, ctx: &egui::Context) {
        self.presets_window(ctx);
        self.error_dialog(ctx);
        self.save_preset_dialog(ctx);
        self.save_fx_preset_dialog(ctx);
        self.about_dialog(ctx);
        self.update_dialog(ctx);
        self.settings_window(ctx);
    }

    fn error_dialog(&mut self, ctx: &egui::Context) {
        let Some((title, body)) = &self.dialogs.error else { return };
        let (title, body) = (title.clone(), body.clone());
        let open = os_window(ctx, "error", &title, [620.0, 380.0], |ui| {
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&body).monospace()).selectable(true));
            });
            ui.add_space(6.0);
            if ui.button("Copy message").clicked() {
                ctx.copy_text(body.clone());
            }
        });
        if !open {
            self.dialogs.error = None;
        }
    }

    fn save_preset_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.save_preset {
            return;
        }
        let mut close = false;
        let open = os_window(ctx, "save_preset", "Save preset", [440.0, 440.0], |ui| {
            widgets::hint(ui, "Choose what goes into the preset. Trim, crop and rotation belong to the clip and are never saved.");
            ui.add_space(4.0);
            let parts = &mut self.presets_ui.save_parts;
            for part in ffcut_core::presets::Part::ALL {
                let mut on = parts.contains(&part);
                if ui.checkbox(&mut on, part.label()).changed() {
                    if on {
                        parts.push(part);
                    } else {
                        parts.retain(|p| *p != part);
                    }
                }
            }
            ui.add_space(6.0);
            let ready = !self.presets_ui.save_parts.is_empty();
            match name_prompt(ui, &mut self.presets_ui.name, ready) {
                Some(Prompt::Save(name)) => {
                    let p = Preset::from_parts(&self.container, &self.s, &self.presets_ui.save_parts);
                    self.presets.export.insert(name, p);
                    if let Err(e) = self.presets.save_export() {
                        self.status = Some(Status::Error(e));
                    }
                    close = true;
                }
                Some(Prompt::Cancel) => close = true,
                None => {}
            }
        });
        if !open || close {
            self.dialogs.save_preset = false;
        }
    }

    fn save_fx_preset_dialog(&mut self, ctx: &egui::Context) {
        let Some(track) = self.presets_ui.fx_save_track else { return };
        let mut close = false;
        let open = os_window(ctx, "save_fx_preset", "Save audio preset", [400.0, 170.0], |ui| {
            widgets::hint(ui, "Saves the effects of this track under a name. Apply it to any track later.");
            ui.add_space(4.0);
            match name_prompt(ui, &mut self.presets_ui.name, true) {
                Some(Prompt::Save(name)) => {
                    let mut fx = self.s.tracks.get(track).map(|t| t.effects.clone()).unwrap_or_default();
                    for e in &mut fx {
                        if let ffcut_core::EffectKind::Norm(n) = &mut e.kind {
                            n.measured = None; // a measurement belongs to one clip
                        }
                    }
                    self.presets.fx.insert(name, fx);
                    self.save_fx_presets();
                    close = true;
                }
                Some(Prompt::Cancel) => close = true,
                None => {}
            }
        });
        if !open || close {
            self.presets_ui.fx_save_track = None;
        }
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.about {
            return;
        }
        let open = os_window(ctx, "about", "About FFcut", [460.0, 240.0], |ui| {
            ui.label(RichText::new(format!("FFcut {}", crate::update::VERSION)).size(size::PAGE).strong());
            ui.label("A small native ffmpeg front end: trim, crop, rotate, re-encode, audio tracks.");
            ui.add_space(4.0);
            if crate::update::configured() {
                ui.hyperlink_to(crate::update::repo_url(), crate::update::repo_url());
            }
            ui.add_space(6.0);
            self.update_check_row(ui);
        });
        if !open {
            self.dialogs.about = false;
        }
    }

    /// "Check for updates" with the result of the last check (About and Settings → Updates).
    pub(super) fn update_check_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.updates.rx.is_none(), egui::Button::new("Check for updates")).clicked() {
                self.start_update_check(ui.ctx(), true);
            }
            if let Some(m) = &self.updates.msg {
                ui.label(RichText::new(m).color(theme::TEXT_LABEL));
            }
        });
        if self.updates.found.is_some() && ui.button("Show the new version…").clicked() {
            self.dialogs.update = true;
        }
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.update {
            return;
        }
        let rel = self.updates.found.clone();
        let open = os_window(ctx, "update", "Update available", [500.0, 420.0], |ui| {
            let Some(r) = &rel else {
                ui.label("You have the latest version.");
                return;
            };
            ui.label(RichText::new(format!("FFcut {} is out", r.version)).size(size::HEADING).strong());
            ui.weak(format!("You have {}.", crate::update::VERSION));
            if !r.notes.is_empty() {
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    ui.add(egui::Label::new(&r.notes).wrap());
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(widgets::primary_button("Open the release page")).clicked() {
                    let _ = open::that(&r.page);
                }
                if ui.button("Skip this version").on_hover_text("Hide the lamp until an even newer version appears").clicked() {
                    self.settings.skipped_version = r.version.clone();
                    self.settings.save();
                    self.updates.found = None;
                    self.dialogs.update = false;
                }
            });
            ui.add_space(4.0);
            widgets::hint(ui, "Windows: download the zip from the release page and replace the old folder.\nLinux: download the new AppImage (or let GearLever / AppImageUpdate do it).");
        });
        if !open {
            self.dialogs.update = false;
        }
    }
}

/// A dialog as a real OS window (the system draws its title bar). Where the backend cannot open a second
/// window, egui wraps the same content in an in-window frame, so it never goes missing.
/// Returns `false` once the user has closed it (title-bar button or Esc).
pub fn os_window(ctx: &egui::Context, id: &str, title: &str, size: [f32; 2], mut body: impl FnMut(&mut egui::Ui)) -> bool {
    os_window_raw(ctx, id, title, size, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| body(ui));
        });
    })
}

/// Same, but `body` gets the bare window and lays out its own panels.
pub fn os_window_raw(ctx: &egui::Context, id: &str, title: &str, size: [f32; 2], mut body: impl FnMut(&mut egui::Ui)) -> bool {
    let mut open = true;
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size([size[0].min(480.0), size[1].min(300.0)])
        .with_app_id("ffcut")
        .with_minimize_button(false)
        .with_maximize_button(false);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("ffcut-dialog", id)), builder, |ui, _class| {
        if ui.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        body(ui);
    });
    open
}
