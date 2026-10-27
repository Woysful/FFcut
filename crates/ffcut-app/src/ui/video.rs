use super::widgets::{self, Grip};
use crate::app::{App, FsMode};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Rect, RichText, Sense, Stroke, StrokeKind};

impl App {
    pub fn video_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::BG)).show(ui, |ui| {
            if self.clip.is_none() {
                self.empty_state(ui);
                return;
            }
            if self.audio_mode() {
                self.audio_panel(ui);
                return;
            }
            let avail = ui.available_rect_before_wrap().shrink(if self.fs == FsMode::Video { 0.0 } else { 10.0 });
            let (fw, fh) = self.frame_dims();
            // caption bars make the picture taller than the frame the crop and the markers work in
            let (vw, vh) = self.view_dims();
            let scale = (avail.width() / vw as f32).min(avail.height() / vh as f32).max(0.01);
            let rect = Rect::from_center_size(avail.center(), vec2(vw as f32 * scale, vh as f32 * scale));
            let rect = Rect::from_min_size(rect.min.round(), rect.size().floor());
            let inner = Rect::from_min_size(pos2(rect.left(), rect.top() + self.vfx.bars.0 as f32 * scale), vec2(rect.width(), fh as f32 * scale));

            self.player.paint(ui, rect);

            let click = ui.interact(rect, ui.id().with("video"), Sense::click());
            if click.clicked() && !self.crop.on {
                self.play_pause();
            }
            if click.double_clicked() && !self.crop.on {
                let ctx = ui.ctx().clone();
                self.set_fs(&ctx, if self.fs == FsMode::Video { FsMode::Off } else { FsMode::Video });
            }
            if self.crop.on && self.fs != FsMode::Video {
                self.crop_overlay(ui, rect, inner, fw as f32, fh as f32);
            } else if self.fs != FsMode::Video {
                self.vfx_overlay(ui, inner);
            }
        });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let r = ui.max_rect();
        ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_center_size(r.center(), vec2(420.0, 190.0))), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("FFcut").size(40.0).strong().color(theme::ACCENT));
                ui.label(RichText::new("ffmpeg, with the command in your hands").size(15.0).weak());
                ui.add_space(18.0);
                if !self.tools_ok() {
                    ui.colored_label(theme::ERR, "ffmpeg / ffprobe not found.");
                    if ui.button("Open settings").clicked() {
                        self.dialogs.settings = true;
                    }
                } else {
                    if ui
                        .add(egui::Button::new(RichText::new("Open video…").size(16.0)).fill(theme::ACCENT.gamma_multiply(0.75)).min_size(vec2(180.0, 38.0)))
                        .clicked()
                    {
                        self.open_dialog();
                    }
                    ui.add_space(6.0);
                    if self.backend == crate::display::Backend::Wayland {
                        // winit cannot receive dropped files on Wayland: say so instead of showing a dead cursor
                        ui.weak(format!("{} to open a file", self.keys(crate::hotkeys::Action::Open)));
                        ui.add(
                            egui::Label::new(
                                RichText::new("Drag & drop does not work on Wayland. Switch to X11 in File → Settings.").size(12.0).color(theme::WARN),
                            )
                            .wrap(),
                        );
                    } else {
                        ui.weak(format!("or drop a file here  ·  {}", self.keys(crate::hotkeys::Action::Open)));
                    }
                }
            });
        });
    }

    fn crop_overlay(&mut self, ui: &mut egui::Ui, full: Rect, rect: Rect, fw: f32, fh: f32) {
        let (sx, sy) = (rect.width() / fw, rect.height() / fh);
        let to_screen = |x: f32, y: f32| pos2(rect.left() + x * sx, rect.top() + y * sy);
        let e = self.crop.edges;
        let cr = Rect::from_min_max(to_screen(e[0], e[1]), to_screen(e[2], e[3]));

        // dim everything outside the crop
        let dim = Color32::from_black_alpha(150);
        let p = ui.painter();
        p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.max.x, cr.top())), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(rect.min.x, cr.bottom()), rect.max), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(rect.min.x, cr.top()), pos2(cr.left(), cr.bottom())), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(cr.right(), cr.top()), pos2(rect.max.x, cr.bottom())), 0.0, dim);
        // rule-of-thirds guides
        let guide = Stroke::new(1.0, Color32::from_white_alpha(45));
        for i in 1..3 {
            let f = i as f32 / 3.0;
            p.line_segment([pos2(cr.left() + cr.width() * f, cr.top()), pos2(cr.left() + cr.width() * f, cr.bottom())], guide);
            p.line_segment([pos2(cr.left(), cr.top() + cr.height() * f), pos2(cr.right(), cr.top() + cr.height() * f)], guide);
        }
        p.rect_stroke(cr, 0.0, Stroke::new(1.5, theme::CROP), StrokeKind::Inside);

        let zones = widgets::grips(cr, 14.0);
        if let Some((g, d)) = widgets::drag_grips(ui, ui.id().with("crop"), &zones) {
            let e = &mut self.crop.edges;
            widgets::move_edges(e, g, vec2(d.x / sx, d.y / sy), vec2(fw, fh), 16.0);
            let [x0, y0, x1, y1] = *e;
            // resizing: a loupe at the moving corner / edge
            if let Grip::Edge { l, t, r, b } = g {
                let ptr = ui.input(|i| i.pointer.interact_pos()).unwrap_or(rect.center());
                let px = if l {
                    x0
                } else if r {
                    x1
                } else {
                    ((ptr.x - rect.left()) / sx).clamp(x0, x1)
                };
                let py = if t {
                    y0
                } else if b {
                    y1
                } else {
                    ((ptr.y - rect.top()) / sy).clamp(y0, y1)
                };
                let crop_screen = Rect::from_min_max(to_screen(x0, y0), to_screen(x1, y1));
                self.crop_loupe(ui, full, to_screen(px, py), crop_screen);
            }
        } else if self.loupe.dirty {
            self.loupe.dirty = false;
            self.settings.save();
        }
        widgets::paint_grips(ui.painter(), &zones, theme::CROP, 9.0);

        // size label
        let cropped = self.crop_rect();
        let galley = ui.painter().layout_no_wrap(format!("{} × {}", cropped.w, cropped.h), egui::FontId::proportional(12.0), Color32::WHITE);
        let pos = pos2(cr.left() + 6.0, (cr.top() + 6.0).min(rect.bottom() - 20.0));
        let bg = Rect::from_min_size(pos, galley.size()).expand2(vec2(5.0, 2.0));
        ui.painter().rect_filled(bg, 4.0, Color32::from_black_alpha(190));
        ui.painter().galley(pos, galley, Color32::WHITE);
    }
}
impl App {
    /// A magnifier next to the crop edge being dragged, for placing it to the pixel.
    /// Wheel: magnification. Ctrl + wheel: size of the window.
    fn crop_loupe(&mut self, ui: &mut egui::Ui, full: Rect, at: egui::Pos2, crop: Rect) {
        let ctx = ui.ctx().clone();
        let (mut size, mut zoom) = (self.settings.loupe_size.unwrap_or(150.0), self.settings.loupe_zoom.unwrap_or(4.0));
        for (d, m) in Self::wheel_events(&ctx) {
            if m.ctrl || m.command {
                size = (size * (d.y * 0.003).exp()).clamp(90.0, 320.0);
            } else {
                zoom = (zoom * (d.y * 0.004).exp()).clamp(2.0, 16.0);
            }
            self.loupe.dirty = true;
        }
        self.settings.loupe_size = Some(size);
        self.settings.loupe_zoom = Some(zoom);

        // the piece of the picture around the point (kept inside the picture)
        let side = size / zoom;
        let mut c = at;
        c.x = c.x.clamp(full.left() + side / 2.0, full.right() - side / 2.0);
        c.y = c.y.clamp(full.top() + side / 2.0, full.bottom() - side / 2.0);
        let region = Rect::from_center_size(c, vec2(side, side));
        self.player.request_loupe([
            (region.left() - full.left()) / full.width(),
            (region.top() - full.top()) / full.height(),
            (region.right() - full.left()) / full.width(),
            (region.bottom() - full.top()) / full.height(),
        ]);
        if let Some(img) = self.player.take_loupe() {
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
            match &mut self.loupe.tex {
                Some(t) => t.set(ci, egui::TextureOptions::NEAREST),
                None => self.loupe.tex = Some(ctx.load_texture("crop-loupe", ci, egui::TextureOptions::NEAREST)),
            }
            self.loupe.region = img.region;
        }
        ctx.request_repaint();
        let Some(tex) = self.loupe.tex.clone() else { return };

        // the window sits diagonally from the point, on the side where there is room
        let gap = 26.0;
        let x = if at.x + gap + size <= full.right() { at.x + gap } else { at.x - gap - size };
        let y = if at.y - gap - size >= full.top() { at.y - gap - size } else { at.y + gap };
        let win = Rect::from_min_size(pos2(x, y), vec2(size, size));
        let radius = 14.0;
        let p = ui.painter();
        p.rect_filled(win.expand(3.0), radius + 3.0, Color32::from_black_alpha(160));
        ui.put(win, egui::Image::new((tex.id(), win.size())).corner_radius(radius));
        // the crop edges and the point, in the picture the loupe shows
        let used = Rect::from_min_max(
            pos2(full.left() + self.loupe.region[0] * full.width(), full.top() + self.loupe.region[1] * full.height()),
            pos2(full.left() + self.loupe.region[2] * full.width(), full.top() + self.loupe.region[3] * full.height()),
        );
        let k = size / used.width().max(1.0);
        let to_win = |p: egui::Pos2| pos2(win.left() + (p.x - used.left()) * k, win.top() + (p.y - used.top()) * k);
        let lp = ui.painter().with_clip_rect(win.shrink(1.0));
        let line = Stroke::new(1.5, theme::CROP);
        for xx in [crop.left(), crop.right()] {
            let w = to_win(pos2(xx, 0.0)).x;
            lp.line_segment([pos2(w, win.top()), pos2(w, win.bottom())], line);
        }
        for yy in [crop.top(), crop.bottom()] {
            let w = to_win(pos2(0.0, yy)).y;
            lp.line_segment([pos2(win.left(), w), pos2(win.right(), w)], line);
        }
        let dot = to_win(at);
        lp.circle_stroke(dot, 3.5, Stroke::new(1.0, Color32::WHITE));
        ui.painter().rect_stroke(win, radius, Stroke::new(2.0, Color32::from_white_alpha(210)), StrokeKind::Inside);
        let label = format!("×{zoom:.1}");
        let g = ui.painter().layout_no_wrap(label, egui::FontId::monospace(11.0), Color32::WHITE);
        let at_l = pos2(win.left() + 8.0, win.bottom() - 8.0 - g.size().y);
        ui.painter().rect_filled(Rect::from_min_size(at_l, g.size()).expand2(vec2(4.0, 1.0)), 4.0, Color32::from_black_alpha(170));
        ui.painter().galley(at_l, g, Color32::WHITE);
    }
}
