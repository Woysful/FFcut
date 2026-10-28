//! The Settings window: a page list on the left, cards on the right.

use super::{card, os_window_raw};
use crate::app::App;
use crate::theme::{self, radius, size};
use eframe::egui::{self, vec2, Align, Color32, Layout, RichText};

const PAGES: [&str; 6] = ["General", "Interface", "Plugins", "Hotkeys", "Updates", "About"];

fn dim(t: impl Into<String>) -> RichText {
    RichText::new(t.into()).size(size::HINT).color(theme::TEXT_DIM)
}

impl App {
    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.dialogs.settings {
            return;
        }
        let open = os_window_raw(ctx, "settings", "Settings", [820.0, 600.0], |ui| {
            egui::Panel::left("settings_nav")
                .exact_size(190.0)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::symmetric(12, 16)))
                .show(ui, |ui| {
                    ui.label(RichText::new("Settings").size(size::PAGE).strong());
                    ui.add_space(12.0);
                    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                        for (i, name) in PAGES.iter().enumerate() {
                            let sel = self.settings_tab == i;
                            let b = egui::Button::new(RichText::new(*name).size(size::TITLE).color(if sel { Color32::WHITE } else { theme::TEXT_LABEL }))
                                .fill(if sel { theme::ACCENT.gamma_multiply(0.55) } else { Color32::TRANSPARENT })
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(radius::CARD)
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
                    2 => self.page_plugins(ui),
                    3 => self.page_hotkeys(ui),
                    4 => self.page_updates(ui),
                    _ => self.page_about(ui),
                });
            });
        });
        if !open {
            self.dialogs.settings = false;
            self.hk_capture = None;
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
                    if ui
                        .add(egui::Button::new(RichText::new(format!("{{{name}}}")).size(11.5)).corner_radius(12).min_size(vec2(0.0, 22.0)))
                        .on_hover_text(*what)
                        .clicked()
                    {
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
            let preview =
                crate::layout::render_title(&self.settings.title_template, self.clip.as_ref().map(|c| (c.path.as_path(), &c.info)), &self.settings.path_sep);
            ui.add_space(6.0);
            ui.label(dim("Preview"));
            ui.label(RichText::new(preview).size(size::TITLE).color(theme::TEXT));
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
                egui::Frame::NONE
                    .fill(if i % 2 == 0 { Color32::from_white_alpha(5) } else { Color32::TRANSPARENT })
                    .corner_radius(radius::WIDGET)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
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

        card(
            ui,
            "Waveform: zoom and scroll",
            Some("Which key goes with the mouse wheel in the audio-file waveform. A key can do only one of the two."),
            |ui| {
                use crate::settings::ModKey;
                let (mut zoom, mut scroll) = (self.settings.zoom_key(), self.settings.scroll_key());
                let (z0, s0) = (zoom, scroll);
                ui.horizontal(|ui| {
                    ui.label("Zoom");
                    for k in ModKey::ALL {
                        ui.selectable_value(&mut zoom, k, k.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Scroll");
                    for k in ModKey::ALL {
                        ui.selectable_value(&mut scroll, k, k.label());
                    }
                });
                // both on one key is not possible: the one that was not just touched gives way
                if zoom == scroll {
                    if zoom != z0 {
                        scroll = z0
                    } else {
                        zoom = s0
                    }
                }
                if (zoom, scroll) != (z0, s0) {
                    self.settings.zoom_mod = Some(zoom);
                    self.settings.scroll_mod = Some(scroll);
                    self.settings.save();
                }
            },
        );

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

    fn page_hotkeys(&mut self, ui: &mut egui::Ui) {
        use crate::hotkeys::{assign, chords, Action, Chord};
        // recording: the next key (with its modifiers) goes to the slot
        if let Some((action, slot)) = self.hk_capture {
            let events = ui.ctx().input(|i| i.events.clone());
            for e in events {
                if let egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } = e {
                    if crate::hotkeys::is_modifier(key) {
                        continue; // wait for the key that goes with it
                    }
                    match key {
                        egui::Key::Escape => self.hk_capture = None,
                        egui::Key::Backspace | egui::Key::Delete => {
                            assign(&mut self.settings.hotkeys, action, slot, None);
                            self.hk_note.clear();
                            self.hk_capture = None;
                            self.settings.save();
                        }
                        _ => {
                            let taken = assign(&mut self.settings.hotkeys, action, slot, Some(Chord::new(key, modifiers)));
                            self.hk_note = taken.map(|t| format!("That key was used for “{}”: it no longer has one.", t.label())).unwrap_or_default();
                            self.hk_capture = None;
                            self.settings.save();
                        }
                    }
                    break;
                }
            }
            ui.ctx().request_repaint();
        }
        card(
            ui,
            "Keyboard shortcuts",
            Some("Click a field, then press the new keys. Backspace clears it, Esc cancels. Each action can have two keys."),
            |ui| {
                egui::Grid::new("hotkeys").num_columns(4).spacing([10.0, 8.0]).show(ui, |ui| {
                    for a in Action::ALL {
                        ui.label(a.label());
                        let list = chords(&self.settings.hotkeys, a);
                        for slot in 0..2 {
                            let recording = self.hk_capture == Some((a, slot));
                            let text =
                                if recording { "Press keys…".to_string() } else { list.get(slot).map(|c| c.display()).unwrap_or_else(|| "—".into()) };
                            let b = egui::Button::new(RichText::new(text).monospace().size(12.5).color(if recording { theme::ACCENT } else { theme::TEXT }))
                                .min_size(vec2(104.0, 24.0))
                                .selected(recording);
                            if ui.add(b).clicked() {
                                self.hk_capture = if recording { None } else { Some((a, slot)) };
                                self.hk_note.clear();
                            }
                        }
                        let changed = self.settings.hotkeys.contains_key(a.id());
                        if ui.add_enabled(changed, egui::Button::new("Reset").small()).on_hover_text("Back to the default keys").clicked() {
                            self.settings.hotkeys.remove(a.id());
                            self.hk_note.clear();
                            self.settings.save();
                        }
                        ui.end_row();
                    }
                });
                if !self.hk_note.is_empty() {
                    ui.add_space(6.0);
                    super::widgets::warn(ui, self.hk_note.clone());
                }
                ui.add_space(8.0);
                if ui.add_enabled(!self.settings.hotkeys.is_empty(), egui::Button::new("Reset all to defaults")).clicked() {
                    self.settings.hotkeys.clear();
                    self.hk_note.clear();
                    self.settings.save();
                }
                ui.add_space(4.0);
                ui.label(dim("Esc always leaves the video fullscreen. Letters and arrows are ignored while a text field is being edited; Open, Export and the fullscreen keys always work."));
            },
        );
    }

    fn plugin_folders_changed(&mut self, ctx: &egui::Context) {
        self.settings.save();
        let dirs = self.settings.plugin_dirs();
        dirs.apply_to_children();
        self.catalog = crate::catalog::Catalog::new(self.ffmpeg.clone(), dirs, ctx);
    }

    fn page_plugins(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        card(
            ui,
            "Plugin folders",
            Some("LADSPA and LV2 plugins (audio) and frei0r plugins (video) are found in the usual folders of this system. Add a folder to search it first, with its subfolders."),
            |ui| {
                let mut remove = None;
                for (i, d) in self.settings.plugin_folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✕").on_hover_text("Stop searching this folder").clicked() {
                            remove = Some(i);
                        }
                        let missing = !std::path::Path::new(d).is_dir();
                        ui.label(RichText::new(d).color(if missing { theme::WARN } else { theme::TEXT_STRONG }));
                        if missing {
                            ui.label(dim("(not found)"));
                        }
                    });
                }
                if let Some(i) = remove {
                    self.settings.plugin_folders.remove(i);
                    self.plugin_folders_changed(&ctx);
                }
                let id = egui::Id::new("plugin-folder-input");
                let mut typed: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                let mut add = None;
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut typed).hint_text("a folder").desired_width(ui.available_width() - 150.0));
                    if (ui.add_enabled(!typed.trim().is_empty(), egui::Button::new("Add")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))) && !typed.trim().is_empty() {
                        add = Some(std::mem::take(&mut typed).trim().to_string());
                    }
                    if ui.button("Browse…").clicked() {
                        add = rfd::FileDialog::new().pick_folder().map(|p| p.to_string_lossy().into_owned());
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, typed));
                if let Some(a) = add.filter(|a| !self.settings.plugin_folders.contains(a)) {
                    self.settings.plugin_folders.push(a);
                    self.plugin_folders_changed(&ctx);
                }
                // the player reads the frei0r and LV2 folders of the environment FFcut started with
                let stale = self.settings.plugin_dirs().env().iter().any(|(k, v)| std::env::var_os(k).as_ref() != Some(v));
                if stale {
                    ui.add_space(6.0);
                    ui.label(dim("The export uses the new folders now; the video preview after a restart."));
                    if ui.button("Restart now").clicked() {
                        let open = self.clip.as_ref().map(|c| c.path.clone());
                        if crate::display::restart(open.as_deref()) {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            },
        );

        card(ui, "Found", None, |ui| {
            let scan = match self.catalog.plugins() {
                crate::catalog::Load::Ready(s) => s,
                crate::catalog::Load::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(dim("Looking for plugins…"));
                    });
                    return;
                }
                crate::catalog::Load::Failed(e) => {
                    ui.colored_label(theme::ERR, e);
                    return;
                }
            };
            use ffcut_core::plugins::PluginKind;
            egui::Grid::new("plugin-counts").num_columns(3).spacing([18.0, 6.0]).show(ui, |ui| {
                for kind in [PluginKind::Ladspa, PluginKind::Lv2, PluginKind::Frei0r] {
                    let all = scan.plugins.iter().filter(|p| p.kind == kind);
                    let (n, usable) = (all.clone().count(), all.filter(|p| p.usable()).count());
                    ui.label(RichText::new(kind.label()).strong());
                    ui.label(format!("{usable} usable{}", if n > usable { format!(" (of {n})") } else { String::new() }));
                    match self.catalog.lacks_host(kind) {
                        Some(true) => ui.colored_label(theme::WARN, "your ffmpeg was built without it"),
                        _ => ui.label(dim(if kind == PluginKind::Frei0r { "video effects" } else { "audio effects" })),
                    };
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            if ui.button("Search again").on_hover_text("After installing or removing plugins").clicked() {
                self.catalog.rescan_plugins();
            }
            let not_usable: Vec<_> = scan.plugins.iter().filter(|p| !p.usable()).collect();
            if !not_usable.is_empty() {
                egui::CollapsingHeader::new(format!("Not usable as effects ({})", not_usable.len())).id_salt("plugins-unusable").show(ui, |ui| {
                    for p in not_usable {
                        ui.label(
                            RichText::new(format!("{} ({}): {}", p.name, p.kind.label(), p.problem.as_deref().unwrap_or("")))
                                .size(size::HINT)
                                .color(theme::TEXT_LABEL),
                        )
                        .on_hover_text(p.file.display().to_string());
                    }
                });
            }
            if !scan.errors.is_empty() {
                egui::CollapsingHeader::new(format!("Could not be read ({})", scan.errors.len())).id_salt("plugins-errors").show(ui, |ui| {
                    for (f, e) in &scan.errors {
                        ui.label(RichText::new(format!("{}: {e}", f.display())).size(size::HINT).color(theme::WARN));
                    }
                });
            }
            egui::CollapsingHeader::new("Folders searched").id_salt("plugins-dirs").show(ui, |ui| {
                let d = self.settings.plugin_dirs();
                for (label, list) in [("LADSPA", &d.ladspa), ("LV2", &d.lv2), ("frei0r", &d.frei0r)] {
                    let found: Vec<String> = list.iter().filter(|p| p.is_dir()).map(|p| p.display().to_string()).collect();
                    ui.label(
                        RichText::new(format!("{label}: {}", if found.is_empty() { "none".into() } else { found.join(", ") }))
                            .size(size::HINT)
                            .color(theme::TEXT_LABEL),
                    );
                }
            });
        });

        card(
            ui,
            "Other formats",
            Some("VST, VST3, CLAP, AU and OFX plugins cannot be used: ffmpeg has no host for them. Many effects exist as LV2 too (LSP, x42, Calf, ZAM)."),
            |_| {},
        );
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
            self.update_check_row(ui);
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
