//! The Settings window: a page list on the left, cards on the right.

use super::{card, os_window_raw, widgets};
use crate::app::App;
use crate::theme::{self, radius, size};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, Layout, Rect, RichText, Sense, Stroke, StrokeKind};

const PAGES: [&str; 7] = ["General", "Appearance", "Interface", "Plugins", "Hotkeys", "Updates", "About"];

fn dim(t: impl Into<String>) -> RichText {
    RichText::new(t.into()).size(size::HINT).color(theme::text_dim())
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
                .frame(egui::Frame::NONE.fill(theme::bg()).inner_margin(egui::Margin::symmetric(12, 16)))
                .show(ui, |ui| {
                    ui.label(theme::heading("Settings", size::PAGE));
                    ui.add_space(12.0);
                    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                        for (i, name) in PAGES.iter().enumerate() {
                            let sel = self.settings_tab == i;
                            // the new themes number their pages, like the sections of the sidebar
                            let name = if super::deco::on() { format!("{:02}  {}", i + 1, theme::title(name)) } else { name.to_string() };
                            let b = egui::Button::new(RichText::new(name).size(size::TITLE).color(if sel { Color32::WHITE } else { theme::text_label() }))
                                .fill(if sel { theme::accent().gamma_multiply(0.55) } else { Color32::TRANSPARENT })
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(radius::card())
                                .min_size(vec2(0.0, 34.0));
                            if ui.add(b).clicked() {
                                self.settings_tab = i;
                            }
                        }
                    });
                });
            let margin = egui::Margin { left: 22, right: 22 - widgets::SCROLL_GUTTER, top: 18, bottom: 18 };
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::panel()).inner_margin(margin)).show(ui, |ui| {
                ui.label(theme::heading(PAGES[self.settings_tab.min(PAGES.len() - 1)], 20.0));
                ui.add_space(10.0);
                widgets::scroll_page(ui, |ui| match self.settings_tab {
                    0 => self.page_general(ui),
                    1 => self.page_appearance(ui),
                    2 => self.page_interface(ui),
                    3 => self.page_plugins(ui),
                    4 => self.page_hotkeys(ui),
                    5 => self.page_updates(ui),
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
                    ui.colored_label(theme::ok(), format!("✔ {}", self.ffmpeg_version));
                    ui.label(dim(format!("{}\n{}", f.display(), p.display())));
                }
                _ => {
                    ui.colored_label(theme::err(), "ffmpeg and/or ffprobe not found");
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

    fn page_appearance(&mut self, ui: &mut egui::Ui) {
        use theme::Kind;
        let before =
            (self.settings.theme.clone(), self.settings.accents.clone(), self.settings.window_opacity, self.settings.popup_opacity, self.settings.window_blur);
        card(ui, "Theme", Some("The look of the whole interface. It changes at once."), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let cur = self.settings.theme();
                for k in Kind::ALL {
                    if theme_tile(ui, k, k == cur).on_hover_text(k.blurb()).clicked() {
                        self.settings.theme = k.id().to_string();
                    }
                }
            });
        });

        let kind = self.settings.theme();
        card(ui, "Accent colour", Some("The trim range, the main buttons and the selection. Each theme keeps its own."), |ui| {
            let own = kind.default_accent();
            let cur = self.settings.accent().unwrap_or(own);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let list = std::iter::once(own).chain(ACCENTS.into_iter().filter(|c| *c != own));
                for (i, c) in list.enumerate() {
                    let tip = if i == 0 { "The theme's own colour".to_string() } else { format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()) };
                    if swatch(ui, c, c == cur, i == 0).on_hover_text(tip).clicked() {
                        if i == 0 {
                            self.settings.accents.remove(kind.id());
                        } else {
                            self.settings.accents.insert(kind.id().into(), [c.r(), c.g(), c.b()]);
                        }
                    }
                }
                ui.add_space(10.0);
                let mut rgb = [cur.r(), cur.g(), cur.b()];
                if ui.color_edit_button_srgb(&mut rgb).on_hover_text("Any colour").changed() {
                    self.settings.accents.insert(kind.id().into(), rgb);
                }
                ui.label(dim("Custom"));
            });
        });

        card(
            ui,
            "Transparency",
            Some("Lets the desktop show through the window, or the interface through menus, tooltips and dialogs. The video itself always stays opaque."),
            |ui| {
                let pct = |a: f32| (a * 100.0).round();
                let back = |v: f32| (v < 99.5).then_some(v / 100.0);
                egui::Grid::new("opacity").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
                    ui.label("Window");
                    let mut w = pct(self.settings.window_alpha());
                    if ui.add(egui::Slider::new(&mut w, 30.0..=100.0).suffix(" %").integer().trailing_fill(true)).changed() {
                        self.settings.window_opacity = back(w);
                    }
                    ui.end_row();
                    ui.label("Menus and dialogs");
                    let mut m = pct(self.settings.popup_alpha());
                    if ui.add(egui::Slider::new(&mut m, 50.0..=100.0).suffix(" %").integer().trailing_fill(true)).changed() {
                        self.settings.popup_opacity = back(m);
                    }
                    ui.end_row();
                });
                ui.add_space(8.0);
                let see_through = self.settings.window_alpha() < 1.0;
                ui.add_enabled(see_through, egui::Checkbox::new(&mut self.settings.window_blur, "Blur what is behind the window"))
                    .on_hover_text("FFcut only asks for it: the desktop decides how strong the blur is")
                    .on_disabled_hover_text("Make the window see-through first");
                if self.settings.window_blur && see_through {
                    let what = match self.blur.via() {
                        Some(via) if cfg!(windows) => {
                            format!("Windows draws the blur ({via}). It needs Transparency effects on (Windows Settings → Personalization → Colors).")
                        }
                        Some(via) => format!("Your desktop takes the request ({via}). How strong the blur is, is set in its own settings."),
                        None if cfg!(all(unix, not(target_os = "macos"))) => {
                            "This desktop does not take blur requests from apps. Some blur see-through windows \
                                                                               by their own rules (picom, Hyprland, SwayFX); GNOME does not blur."
                                .to_string()
                        }
                        None => "Blur is not available on this system.".to_string(),
                    };
                    ui.label(dim(what));
                    ui.label(dim("Menus, lists and tooltips stay opaque while the blur is on: the desktop blurs what is behind the window, not the interface under a menu."));
                    if self.blur.via().is_some() && !self.blur.reaches_dialogs() {
                        ui.label(dim(
                            "On Wayland the dialogs are not blurred: FFcut cannot reach their windows there. On X11 (General → Display server) they are.",
                        ));
                    }
                }
                if !self.transparent && self.settings.window_alpha() < 1.0 {
                    ui.add_space(6.0);
                    ui.label(dim("The window can only become see-through when FFcut starts."));
                    if ui.button("Restart now").clicked() {
                        let open = self.clip.as_ref().map(|c| c.path.clone());
                        if crate::display::restart(open.as_deref()) {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    ui.add_space(4.0);
                    ui.label(dim(
                        "On X11 a see-through window needs a compositor (KDE, GNOME and most desktops have one); without it the window stays opaque.",
                    ));
                }
            },
        );

        if (self.settings.theme.clone(), self.settings.accents.clone(), self.settings.window_opacity, self.settings.popup_opacity, self.settings.window_blur)
            != before
        {
            self.settings.apply_look(ui.ctx());
            self.sync_blur(ui.ctx());
            self.appearance_dirty = true;
        }
        // not on every step of a drag: once the pointer is let go
        if self.appearance_dirty && !ui.input(|i| i.pointer.any_down()) {
            self.appearance_dirty = false;
            self.settings.save();
        }
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
            ui.label(RichText::new(preview).size(size::TITLE).color(theme::text()));
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
                    .corner_radius(radius::widget())
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
                            let b =
                                egui::Button::new(RichText::new(text).monospace().size(12.5).color(if recording { theme::accent() } else { theme::text() }))
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
                    widgets::warn(ui, self.hk_note.clone());
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
                        ui.label(RichText::new(d).color(if missing { theme::warn() } else { theme::text_strong() }));
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
                    ui.colored_label(theme::err(), e);
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
                        Some(true) => ui.colored_label(theme::warn(), "your ffmpeg was built without it"),
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
                                .color(theme::text_label()),
                        )
                        .on_hover_text(p.file.display().to_string());
                    }
                });
            }
            if !scan.errors.is_empty() {
                egui::CollapsingHeader::new(format!("Could not be read ({})", scan.errors.len())).id_salt("plugins-errors").show(ui, |ui| {
                    for (f, e) in &scan.errors {
                        ui.label(RichText::new(format!("{}: {e}", f.display())).size(size::HINT).color(theme::warn()));
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
                            .color(theme::text_label()),
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
            ui.label(dim("MIT license. Fonts: JetBrains Mono, Manrope, Archivo Black, Michroma (OFL)."));
        });
    }
}

/// Accent colours offered next to the theme's own.
const ACCENTS: [Color32; 9] = [
    Color32::from_rgb(255, 138, 61),
    Color32::from_rgb(255, 72, 72),
    Color32::from_rgb(255, 105, 200),
    Color32::from_rgb(170, 120, 255),
    Color32::from_rgb(143, 168, 255),
    Color32::from_rgb(64, 216, 255),
    Color32::from_rgb(90, 230, 150),
    Color32::from_rgb(204, 255, 0),
    Color32::from_rgb(240, 240, 240),
];

/// A small picture of a theme: its surfaces, its accent, its title face.
fn theme_tile(ui: &mut egui::Ui, k: theme::Kind, selected: bool) -> egui::Response {
    use theme::Kind;
    let (rect, resp) = ui.allocate_exact_size(vec2(172.0, 112.0), Sense::click());
    let p = ui.painter();
    let [bg, panel, card, hover, text] = k.swatches();
    let acc = k.default_accent();
    let rad = match k {
        Kind::Standard => 8.0,
        Kind::Brutalist => 0.0,
        Kind::Y2k => 12.0,
    };
    let pic = Rect::from_min_size(rect.min, vec2(rect.width(), 84.0));
    p.rect_filled(pic, rad, bg);
    let side = Rect::from_min_max(pos2(pic.right() - 58.0, pic.top()), pic.max);
    p.rect_filled(side, egui::CornerRadius { ne: rad as u8, se: rad as u8, ..Default::default() }, panel);
    for (i, c) in [card, hover, card].into_iter().enumerate() {
        p.rect_filled(Rect::from_min_size(pos2(side.left() + 8.0, side.top() + 10.0 + i as f32 * 20.0), vec2(42.0, 14.0)), rad / 3.0, c);
    }
    let tl = Rect::from_min_size(pos2(pic.left() + 10.0, pic.bottom() - 20.0), vec2(side.left() - pic.left() - 20.0, 8.0));
    p.rect_filled(tl, rad / 3.0, card);
    p.rect_filled(Rect::from_min_size(tl.min + vec2(tl.width() * 0.2, 0.0), vec2(tl.width() * 0.45, tl.height())), rad / 3.0, acc);
    let name = if k == Kind::Standard { "FFcut" } else { "FFCUT" };
    p.text(pic.left_top() + vec2(10.0, 12.0), Align2::LEFT_TOP, name, theme::display_of(k, 17.0), text);
    match k {
        Kind::Standard => {}
        Kind::Brutalist => {
            p.rect_filled(Rect::from_min_size(pic.min, vec2(5.0, 5.0)), 0.0, text);
            super::deco::cross(p, pos2(side.left() - 12.0, pic.top() + 12.0), 4.0, Stroke::new(1.0, text));
        }
        Kind::Y2k => {
            super::deco::star(p, pos2(side.left() - 14.0, pic.top() + 16.0), 8.0, text);
            p.rect_stroke(tl.expand(4.0), 8.0, Stroke::new(1.0, theme::line()), StrokeKind::Outside);
            super::deco::dots(p, pos2(pic.left() + 12.0, pic.top() + 42.0), 5, 2, 2.5, text);
        }
    }
    let stroke = if selected {
        Stroke::new(2.0, theme::accent())
    } else if resp.hovered() {
        Stroke::new(1.0, theme::text_label())
    } else {
        Stroke::new(1.0, theme::line())
    };
    p.rect_stroke(pic, rad, stroke, StrokeKind::Outside);
    let col = if selected { theme::text_strong() } else { theme::text_label() };
    p.text(pos2(rect.left() + 2.0, pic.bottom() + 9.0), Align2::LEFT_TOP, k.label(), egui::FontId::proportional(size::BODY), col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One accent colour to pick; `own` = the theme's own colour (marked with a dot).
fn swatch(ui: &mut egui::Ui, c: Color32, selected: bool, own: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let p = ui.painter();
    let r = theme::radius::of(5.0);
    p.rect_filled(rect.shrink(4.0), r, c);
    if own {
        p.circle_filled(rect.center(), 2.5, theme::on(c));
    }
    if selected {
        p.rect_stroke(rect.shrink(1.0), r + 2.0, Stroke::new(2.0, theme::text_strong()), StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_stroke(rect.shrink(1.0), r + 2.0, Stroke::new(1.0, theme::text_label()), StrokeKind::Inside);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
