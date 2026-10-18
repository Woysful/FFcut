use crate::app::{App, FsMode};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, CursorIcon, Rect, RichText, Sense, Stroke, StrokeKind};

#[derive(Clone, Copy)]
enum Grip {
    Body,
    Edge { l: bool, t: bool, r: bool, b: bool },
}

impl App {
    pub fn video_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::BG)).show(ui, |ui| {
            if self.clip.is_none() {
                self.empty_state(ui);
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
            if click.clicked() && !self.crop_on {
                self.play_pause();
            }
            if click.double_clicked() && !self.crop_on {
                let ctx = ui.ctx().clone();
                self.set_fs(&ctx, if self.fs == FsMode::Video { FsMode::Off } else { FsMode::Video });
            }
            if self.crop_on && self.fs != FsMode::Video {
                self.crop_overlay(ui, inner, fw as f32, fh as f32);
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
                        self.show_settings = true;
                    }
                } else {
                    if ui.add(egui::Button::new(RichText::new("Open video…").size(16.0)).fill(theme::ACCENT.gamma_multiply(0.75)).min_size(vec2(180.0, 38.0))).clicked() {
                        self.open_dialog();
                    }
                    ui.add_space(6.0);
                    if self.backend == crate::display::Backend::Wayland {
                        // winit cannot receive dropped files on Wayland: say so instead of showing a dead cursor
                        ui.weak("Ctrl+O to open a file");
                        ui.add(egui::Label::new(RichText::new("Drag & drop does not work on Wayland. Switch to X11 in File → Settings.").size(12.0).color(theme::WARN)).wrap());
                    } else {
                        ui.weak("or drop a file here  ·  Ctrl+O");
                    }
                }
            });
        });
    }

    fn crop_overlay(&mut self, ui: &mut egui::Ui, rect: Rect, fw: f32, fh: f32) {
        let sx = rect.width() / fw;
        let sy = rect.height() / fh;
        let to_screen = |x: f32, y: f32| pos2(rect.left() + x * sx, rect.top() + y * sy);
        let [mut x0, mut y0, mut x1, mut y1] = self.crop_edges;
        let cr = Rect::from_min_max(to_screen(x0, y0), to_screen(x1, y1));

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

        // grips: 4 corners, 4 edges, body
        let m = 14.0_f32;
        let mut grips: Vec<(Rect, Grip, CursorIcon)> = Vec::new();
        let (l, r, t, b) = (cr.left(), cr.right(), cr.top(), cr.bottom());
        let c = cr.center();
        let h = |p: egui::Pos2| Rect::from_center_size(p, vec2(m * 1.4, m * 1.4));
        grips.push((h(pos2(l, t)), Grip::Edge { l: true, t: true, r: false, b: false }, CursorIcon::ResizeNwSe));
        grips.push((h(pos2(r, t)), Grip::Edge { l: false, t: true, r: true, b: false }, CursorIcon::ResizeNeSw));
        grips.push((h(pos2(l, b)), Grip::Edge { l: true, t: false, r: false, b: true }, CursorIcon::ResizeNeSw));
        grips.push((h(pos2(r, b)), Grip::Edge { l: false, t: false, r: true, b: true }, CursorIcon::ResizeNwSe));
        grips.push((h(pos2(c.x, t)), Grip::Edge { l: false, t: true, r: false, b: false }, CursorIcon::ResizeVertical));
        grips.push((h(pos2(c.x, b)), Grip::Edge { l: false, t: false, r: false, b: true }, CursorIcon::ResizeVertical));
        grips.push((h(pos2(l, c.y)), Grip::Edge { l: true, t: false, r: false, b: false }, CursorIcon::ResizeHorizontal));
        grips.push((h(pos2(r, c.y)), Grip::Edge { l: false, t: false, r: true, b: false }, CursorIcon::ResizeHorizontal));
        grips.push((cr.shrink(m * 0.7), Grip::Body, CursorIcon::Grab));

        let base = ui.id().with("crop");
        let mut delta = vec2(0.0, 0.0);
        let mut active: Option<Grip> = None;
        // later grips win overlaps (body is last, so test it first)
        for (i, (grect, grip, cursor)) in grips.iter().enumerate().rev() {
            let resp = ui.interact(*grect, base.with(i), Sense::click_and_drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(*cursor);
            }
            if resp.dragged() && active.is_none() {
                active = Some(*grip);
                delta = resp.drag_delta();
            }
        }
        if let Some(g) = active {
            let (dx, dy) = (delta.x / sx, delta.y / sy);
            match g {
                Grip::Body => {
                    let (w, hh) = (x1 - x0, y1 - y0);
                    x0 = (x0 + dx).clamp(0.0, fw - w);
                    y0 = (y0 + dy).clamp(0.0, fh - hh);
                    x1 = x0 + w;
                    y1 = y0 + hh;
                }
                Grip::Edge { l, t, r, b } => {
                    if l { x0 = (x0 + dx).clamp(0.0, x1 - 16.0); }
                    if r { x1 = (x1 + dx).clamp(x0 + 16.0, fw); }
                    if t { y0 = (y0 + dy).clamp(0.0, y1 - 16.0); }
                    if b { y1 = (y1 + dy).clamp(y0 + 16.0, fh); }
                }
            }
            self.crop_edges = [x0, y0, x1, y1];
        }

        // handles drawn last so they are on top
        let p = ui.painter();
        for (grect, grip, _) in &grips {
            if matches!(grip, Grip::Edge { .. }) {
                let hr = Rect::from_center_size(grect.center(), vec2(9.0, 9.0));
                p.rect_filled(hr, 2.0, theme::CROP);
                p.rect_stroke(hr, 2.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Outside);
            }
        }
        // size label
        let cropped = self.crop_rect();
        let label = format!("{} × {}", cropped.w, cropped.h);
        let font = egui::FontId::proportional(12.0);
        let galley = ui.painter().layout_no_wrap(label, font, Color32::WHITE);
        let pos = pos2(cr.left() + 6.0, (cr.top() + 6.0).min(rect.bottom() - 20.0));
        let bg = Rect::from_min_size(pos, galley.size()).expand2(vec2(5.0, 2.0));
        ui.painter().rect_filled(bg, 4.0, Color32::from_black_alpha(190));
        ui.painter().galley(pos, galley, Color32::WHITE);
    }
}
