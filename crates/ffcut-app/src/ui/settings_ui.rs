//! The Settings window: a page list on the left, cards on the right.

use super::{card, os_window_raw};
use crate::app::App;
use crate::theme;
use eframe::egui::{self, vec2, Align, Color32, Layout, RichText};

const PAGES: [&str; 4] = ["General", "Interface", "Updates", "About"];

fn dim(t: impl Into<String>) -> RichText {
    RichText::new(t.into()).size(11.5).color(Color32::from_gray(140))
}

impl App {
    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let open = os_window_raw(ctx, "settings", "Settings", [820.0, 600.0], |ui| {
            egui::Panel::left("settings_nav").exact_size(190.0).resizable(false).frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::symmetric(12, 16))).show(ui, |ui| {
                ui.label(RichText::new("Settings").size(18.0).strong());
                ui.add_space(12.0);
                ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                    for (i, name) in PAGES.iter().enumerate() {
                        let sel = self.settings_tab == i;
                        let b = egui::Button::new(RichText::new(*name).size(13.5).color(if sel { Color32::WHITE } else { Color32::from_gray(185) }))
                            .fill(if sel { theme::ACCENT.gamma_multiply(0.55) } else { Color32::TRANSPARENT })
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(8)
                            .min_size(vec2(0.0, 34.0));
                        if ui.add(b).clicked() {
                            self.settings_tab = i;
                        }
                    }
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::symmetric(22, 18))).show(ui, |ui| {
                ui.label(RichText::new(PAGES[self.settings_tab.min(PAGES.len() - 1)]).size(20.0).strong());
                ui.add_space(10.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.settings_tab {
                    0 => self.page_general(ui),
                    1 => self.page_interface(ui),
                    2 => self.page_updates(ui),
                    _ => self.page_about(ui),
                });
            });
        });
        if !open {
            self.show_settings = false;
        }
    }

    fn page_general(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        card(ui, "FFmpeg", Some("Leave a path empty to find it automatically: next to FFcut first, then in PATH."), |ui| {
            let mut changed = false;
            egui::Grid::new("tools").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
                ui.label("ffmpeg");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.ffmpeg_path).desired_width(f32::INFINITY).hint_text("auto")).lost_focus();
                ui.end_row();
                ui.label("ffprobe");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.ffprobe_path).desired_width(f32::INFINITY).hint_text("auto")).lost_focus();
                ui.end_row();
            });
            if changed {
                self.locate_tools(&ctx);
                self.settings.save();
            }
            ui.add_space(8.0);
            match (&self.ffmpeg, &self.ffprobe) {
                (Some(f), Some(p)) => {
                    ui.colored_label(theme::OK, format!("✔ {}", self.ffmpeg_version));
                    ui.label(dim(format!("{}\n{}", f.display(), p.display())));
                }
                _ => {
                    ui.colored_label(theme::ERR, "ffmpeg and/or ffprobe not found");
                }
            }
        });

        #[cfg(all(unix, not(target_os = "macos")))]
        card(ui, "Display server", Some("Dragging files into the window does not work on native Wayland (the windowing library has no support for it yet). Choose X11 to get it through XWayland."), |ui| {
            let mut cur = match self.settings.display_server.as_str() {
                v @ ("wayland" | "x11") => v.to_string(),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                for (v, label) in [("", "Auto"), ("wayland", "Wayland"), ("x11", "X11 (XWayland)")] {
                    ui.selectable_value(&mut cur, v.to_string(), label);
                }
            });
            if cur != self.settings.display_server {
                self.settings.display_server = cur;
                self.settings.save();
            }
            ui.add_space(4.0);
            ui.label(dim(format!("Running on: {}. A change takes effect after a restart.", self.backend.name())));
            if crate::display::resolve(&self.settings.display_server) != self.backend && ui.button("Restart now").clicked() {
                let open = self.clip.as_ref().map(|c| c.path.clone());
                if crate::display::restart(open.as_deref()) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
        ui.label(dim(format!("Settings file: {}", crate::settings::config_dir().join("settings.json").display())));
    }

    fn page_interface(&mut self, ui: &mut egui::Ui) {
        card(ui, "Window title", Some("Build the title from variables. Click a variable to add it. Empty = default."), |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.settings.title_template).desired_width(f32::INFINITY).hint_text(crate::layout::DEFAULT_TITLE));
            let mut insert: Option<&str> = None;
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (name, what) in crate::layout::TITLE_VARS {
                    if ui.add(egui::Button::new(RichText::new(format!("{{{name}}}")).size(11.5)).corner_radius(12).min_size(vec2(0.0, 22.0))).on_hover_text(*what).clicked() {
                        insert = Some(name);
                    }
                }
            });
            if let Some(n) = insert {
                self.settings.title_template.push_str(&format!("{{{n}}}"));
                self.settings.save();
            }
            if r.lost_focus() {
                self.settings.save();
            }
            let preview = crate::layout::render_title(&self.settings.title_template, self.clip.as_ref().map(|c| (c.path.as_path(), &c.info)), &self.settings.path_sep);
            ui.add_space(6.0);
            ui.label(dim("Preview"));
            ui.label(RichText::new(preview).size(13.0).color(Color32::from_gray(215)));
        });

        card(ui, "Path display", Some("How folder separators look in {path}, {dir} and the file info line."), |ui| {
            let mut sep = self.settings.path_sep.clone();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut sep, String::new(), "As is  ( / )");
                ui.selectable_value(&mut sep, " > ".to_string(), "Arrows  ( > )");
            });
            ui.horizontal(|ui| {
                ui.label(dim("Custom"));
                ui.add(egui::TextEdit::singleline(&mut sep).desired_width(80.0).hint_text(" > "));
            });
            if sep != self.settings.path_sep {
                self.settings.path_sep = sep;
                self.settings.save();
            }
            let sample = std::path::Path::new("/home/me/Videos/clip.mp4");
            ui.add_space(4.0);
            ui.label(dim(format!("Example: {}", crate::layout::fmt_path(sample, &self.settings.path_sep))));
        });

        card(ui, "File info in the menu bar", Some("Tick what to show; move rows with the arrows to set the order."), |ui| {
            let mut mv: Option<(usize, isize)> = None;
            let mut changed = false;
            let n = self.settings.info_items.len();
            for (i, it) in self.settings.info_items.iter_mut().enumerate() {
                egui::Frame::NONE.fill(if i % 2 == 0 { Color32::from_white_alpha(5) } else { Color32::TRANSPARENT }).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut it.on, crate::layout::info_label(&it.key)).changed();
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).on_hover_text("Move down").clicked() {
                                mv = Some((i, 1));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("▲").small()).on_hover_text("Move up").clicked() {
                                mv = Some((i, -1));
                            }
                        });
                    });
                });
            }
            if let Some((i, d)) = mv {
                self.settings.info_items.swap(i, (i as isize + d) as usize);
                changed = true;
            }
            if changed {
                self.settings.save();
            }
            ui.add_space(6.0);
            if ui.button("Reset title and info to default").clicked() {
                self.settings.info_items = crate::layout::default_info_items();
                self.settings.title_template.clear();
                self.settings.save();
            }
        });

        card(ui, "Timer", Some("The timer above the timeline can show time or frame numbers. You can also click it to switch."), |ui| {
            let mut frames = self.settings.show_frames;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut frames, false, "Time");
                ui.selectable_value(&mut frames, true, "Frames");
            });
            if frames != self.settings.show_frames {
                self.settings.show_frames = frames;
                self.settings.save();
            }
        });
    }

    fn page_updates(&mut self, ui: &mut egui::Ui) {
        card(ui, "Automatic check", Some("One small request to GitHub when FFcut starts. Nothing is downloaded or installed automatically."), |ui| {
            ui.label(dim(format!("Current version: {}", crate::update::VERSION)));
            ui.add_space(6.0);
            let mut check = !self.settings.no_update_check;
            if ui.checkbox(&mut check, "Look for a new version at startup").changed() {
                self.settings.no_update_check = !check;
                self.settings.save();
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.update_rx.is_none(), egui::Button::new("Check now")).clicked() {
                    self.start_update_check(ui.ctx(), true);
                }
                if let Some(m) = &self.update_msg {
                    ui.label(RichText::new(m).color(Color32::from_gray(180)));
                }
            });
            if self.update_found.is_some() && ui.button("Show the new version…").clicked() {
                self.show_update = true;
            }
        });
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        card(ui, &format!("FFcut {}", crate::update::VERSION), Some("A small native ffmpeg front end: trim, crop, rotate, re-encode, audio tracks."), |ui| {
            if crate::update::configured() {
                ui.hyperlink_to(crate::update::repo_url(), crate::update::repo_url());
            }
            ui.add_space(4.0);
            ui.label(dim("MIT license. Fonts: JetBrains Mono, Manrope (OFL)."));
        });
    }
}
