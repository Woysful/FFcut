use super::widgets::{self, Grip};
use crate::app::{App, FsMode};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke, StrokeKind};

impl App {
    pub fn video_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::bg())).show(ui, |ui| {
            if self.clip.is_none() {
                self.empty_state(ui);
                return;
            }
            if self.audio_mode() {
                self.audio_panel(ui);
                return;
            }
            // the themes with ornaments leave room for them around the picture
            let margin = match self.fs {
                FsMode::Video => 0.0,
                _ if super::deco::on() => 20.0,
                _ => 10.0,
            };
            let avail = ui.available_rect_before_wrap().shrink(margin);
            let (fw, fh) = self.frame_dims();
            // caption bars make the picture taller than the frame the crop and the markers work in
            let (vw, vh) = self.view_dims();
            let scale = (avail.width() / vw as f32).min(avail.height() / vh as f32).max(0.01);
            let rect = Rect::from_center_size(avail.center(), vec2(vw as f32 * scale, vh as f32 * scale));
            let rect = Rect::from_min_size(rect.min.round(), rect.size().floor());
            let inner = Rect::from_min_size(pos2(rect.left(), rect.top() + self.vfx.bars.0 as f32 * scale), vec2(rect.width(), fh as f32 * scale));

            self.player.paint(ui, rect);
            if super::deco::on() && self.fs != FsMode::Video {
                self.video_deco(ui.painter(), rect);
            }

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
        if super::deco::on() {
            self.empty_deco(ui, r);
        }
        let new_look = super::deco::on();
        let h = if new_look { 250.0 } else { 190.0 };
        ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_center_size(r.center(), vec2(420.0, h))), |ui| {
            ui.vertical_centered(|ui| {
                if new_look {
                    ui.add_space(18.0);
                    let size = if theme::kind() == theme::Kind::Brutalist { 64.0 } else { 52.0 };
                    ui.label(RichText::new("FFCUT").font(theme::display(size)).color(theme::text_strong()));
                    ui.label(RichText::new("FFMPEG, WITH THE COMMAND IN YOUR HANDS").monospace().size(11.0).color(theme::text_label()));
                } else {
                    ui.label(RichText::new("FFcut").size(40.0).strong().color(theme::accent()));
                    ui.label(RichText::new("ffmpeg, with the command in your hands").size(15.0).weak());
                }
                ui.add_space(18.0);
                if !self.tools_ok() {
                    ui.colored_label(theme::err(), "ffmpeg / ffprobe not found.");
                    if ui.button("Open settings").clicked() {
                        self.dialogs.settings = true;
                    }
                } else {
                    let label = if new_look { "OPEN VIDEO…" } else { "Open video…" };
                    let b = egui::Button::new(RichText::new(label).size(16.0).color(theme::on_accent())).fill(theme::accent()).min_size(vec2(180.0, 38.0));
                    let b = if new_look {
                        b
                    } else {
                        egui::Button::new(RichText::new(label).size(16.0)).fill(theme::accent().gamma_multiply(0.75)).min_size(vec2(180.0, 38.0))
                    };
                    if ui.add(b).clicked() {
                        self.open_dialog();
                    }
                    ui.add_space(6.0);
                    if self.backend == crate::display::Backend::Wayland {
                        // winit cannot receive dropped files on Wayland: say so instead of showing a dead cursor
                        ui.weak(format!("{} to open a file", self.keys(crate::hotkeys::Action::Open)));
                        ui.add(
                            egui::Label::new(
                                RichText::new("Drag & drop does not work on Wayland. Switch to X11 in File → Settings.").size(12.0).color(theme::warn()),
                            )
                            .wrap(),
                        );
                    } else {
                        let or = if new_look { "OR DROP A FILE HERE" } else { "or drop a file here" };
                        ui.weak(format!("{or}  ·  {}", self.keys(crate::hotkeys::Action::Open)));
                    }
                }
            });
        });
    }

    /// The ornaments around the video in the brutalist and the Y2K theme.
    fn video_deco(&self, p: &egui::Painter, rect: Rect) {
        use super::deco;
        let (vw, vh) = self.view_dims();
        if theme::kind() == theme::Kind::Brutalist {
            let o = rect.expand(5.0);
            for c in [o.left_top(), o.right_top(), o.left_bottom(), o.right_bottom()] {
                deco::cross(p, c, 4.0, Stroke::new(1.0, theme::text_label()));
            }
            // dimension lines, as on a drawing: the width above the picture, the height on its left
            let st = Stroke::new(1.0, theme::text_dim());
            let ink = theme::text_label();
            let y = rect.top() - 10.0;
            let r = deco::caption(p, pos2(rect.center().x, y), Align2::CENTER_CENTER, &format!("{vw} PX"), ink);
            p.hline(rect.left()..=r.left() - 6.0, y, st);
            p.hline(r.right() + 6.0..=rect.right(), y, st);
            for x in [rect.left(), rect.right()] {
                p.vline(x, y - 4.0..=y + 4.0, st);
            }
            let x = rect.left() - 10.0;
            let r = deco::caption_up(p, pos2(x, rect.center().y), &format!("{vh} PX"), ink);
            p.vline(x, rect.top()..=r.top() - 6.0, st);
            p.vline(x, r.bottom() + 6.0..=rect.bottom(), st);
            for y in [rect.top(), rect.bottom()] {
                p.hline(x - 4.0..=x + 4.0, y, st);
            }
        } else {
            let o = rect.expand(7.0);
            let line = Stroke::new(1.0, theme::line());
            let dim = theme::text_label();
            // the state of the player cut into the top line, the position as a row of dots into the bottom one
            let playing = !self.player.paused();
            let cap = deco::caption(p, pos2(o.left() + 34.0, o.top()), Align2::LEFT_CENTER, if playing { "PLAYING" } else { "PAUSED" }, dim);
            let dot = pos2(cap.left() - 8.0, o.top());
            if playing {
                p.circle_filled(dot, 2.5, theme::text_strong());
            } else {
                p.circle_stroke(dot, 2.5, Stroke::new(1.0, dim));
            }
            let n = 8;
            let frac = (self.player.time() / self.dur().max(0.001)).clamp(0.0, 1.0);
            let x0 = o.right() - 40.0 - (n as f32 - 1.0) * 7.6;
            let x1 = deco::dots(p, pos2(x0, o.bottom()), n, (frac * n as f64).round() as usize, 2.5, dim);
            deco::gapped_frame(p, o, 14.0, (dot.x - 7.0, cap.right() + 6.0), (x0 - 8.0, x1 + 6.0), line);
            deco::star(p, o.right_top() + vec2(-3.0, 3.0), 10.0, theme::text_strong());
            deco::rail(p, pos2(o.left() - 6.0, o.top() + 40.0), pos2(o.left() - 6.0, (o.top() + 40.0 + o.height() * 0.4).min(o.bottom() - 20.0)), line);
        }
    }

    /// The ornaments around the start screen of the brutalist and the Y2K theme.
    fn empty_deco(&self, ui: &egui::Ui, area: Rect) {
        use super::deco;
        let p = ui.painter();
        let ver = format!("V{}", crate::update::VERSION);
        if theme::kind() == theme::Kind::Brutalist {
            let b = Rect::from_center_size(area.center(), vec2(520.0, 300.0));
            // drawing paper: a dot grid and rulers along the edges
            let grid = theme::line_faint();
            let mut y = area.top() + 24.0;
            while y < area.bottom() {
                let mut x = area.left() + 24.0;
                while x < area.right() {
                    p.rect_filled(Rect::from_center_size(pos2(x, y), vec2(1.5, 1.5)), 0.0, grid);
                    x += 24.0;
                }
                y += 24.0;
            }
            let tick = Stroke::new(1.0, theme::text_off());
            let mut i = 0;
            while (i as f32) * 10.0 < area.width() {
                let x = area.left() + i as f32 * 10.0;
                let len = if i % 10 == 0 {
                    9.0
                } else if i % 5 == 0 {
                    6.0
                } else {
                    3.0
                };
                p.vline(x, area.top()..=area.top() + len, tick);
                if i % 10 == 0 && i > 0 {
                    deco::caption(p, pos2(x + 3.0, area.top() + 2.0), Align2::LEFT_TOP, &(i * 10).to_string(), theme::text_off());
                }
                i += 1;
            }
            let mut i = 0;
            while (i as f32) * 10.0 < area.height() {
                let y = area.top() + i as f32 * 10.0;
                let len = if i % 10 == 0 {
                    9.0
                } else if i % 5 == 0 {
                    6.0
                } else {
                    3.0
                };
                p.hline(area.left()..=area.left() + len, y, tick);
                i += 1;
            }

            let white = theme::text_strong();
            p.rect_filled(b, 0.0, theme::bg());
            p.rect_stroke(b, 0.0, Stroke::new(1.5, white), StrokeKind::Inside);
            // the label strip of a technical drawing
            let strip = Rect::from_min_size(b.min, vec2(b.width(), 20.0));
            p.rect_filled(strip, 0.0, white);
            let ink = theme::on(white);
            deco::caption(p, pos2(strip.left() + 8.0, strip.center().y), Align2::LEFT_CENTER, "FFCUT / VIDEO CUTTER", ink);
            deco::caption(p, pos2(strip.right() - 8.0, strip.center().y), Align2::RIGHT_CENTER, &format!("{ver} / READY"), ink);
            for c in [b.left_top(), b.right_top(), b.left_bottom(), b.right_bottom()] {
                deco::cross(p, c + (c - b.center()).normalized() * 18.0, 7.0, Stroke::new(1.0, theme::text_label()));
            }
            let dim = theme::text_dim();
            deco::caption(p, pos2(b.left() + 8.0, b.bottom() - 8.0), Align2::LEFT_BOTTOM, "[01] OPEN    [02] DROP    [03] CUT", dim);
            p.rect_filled(Rect::from_min_size(pos2(b.right() - 58.0, b.bottom() - 18.0), vec2(50.0, 10.0)), 0.0, theme::accent());
            deco::hatch(p, Rect::from_min_size(pos2(b.right() - 120.0, b.bottom() - 18.0), vec2(54.0, 10.0)), 4.0, Stroke::new(1.0, theme::text_label()));
            p.hline(b.left()..=b.right(), b.bottom() - 26.0, Stroke::new(1.0, theme::line()));
            deco::barcode(p, Rect::from_min_size(pos2(b.left() + 10.0, b.top() + 30.0), vec2(60.0, 12.0)), theme::text_label());
            deco::caption(p, pos2(b.right() - 10.0, b.top() + 30.0), Align2::RIGHT_TOP, "SHEET 01/01", dim);
            deco::caption(p, pos2(b.right() + 30.0, b.top() + 40.0), Align2::LEFT_TOP, "REV.A", dim);
            deco::caption_up(p, pos2(b.left() - 30.0, b.center().y), "FFMPEG FRONT END", dim);
        } else {
            let b = Rect::from_center_size(area.center(), vec2(680.0, 360.0));
            let white = theme::text_strong();
            let lab = theme::text_label();
            let dim = theme::text_dim();
            let line = Stroke::new(1.0, theme::line());
            let thin = Stroke::new(1.0, white);
            // the main panel: notched and stepped, framed by rails
            let pts = deco::tab_shape(b, 24.0, 12.0, 250.0, 200.0);
            p.add(deco::fill(&pts, theme::panel()));
            p.add(egui::Shape::closed_line(pts, Stroke::new(1.2, theme::text_label())));
            deco::rail(p, pos2(b.left() - 16.0, b.top() + 46.0), pos2(b.left() - 16.0, b.bottom() - 46.0), line);
            deco::rail(p, pos2(b.left() + 40.0, b.bottom() + 16.0), pos2(b.right() - 250.0, b.bottom() + 16.0), line);
            deco::rail(p, pos2(b.left() + 300.0, b.top() - 10.0), pos2(b.right() - 40.0, b.top() - 10.0), line);
            deco::caption(p, pos2(b.left() + 26.0, b.top() + 18.0), Align2::LEFT_CENTER, "SYS://FFCUT", lab);
            deco::caption(p, pos2(b.right() - 30.0, b.top() + 28.0), Align2::RIGHT_CENTER, &ver, dim);

            // left: a module with a globe, the loading line under it, a barcode
            let m = Rect::from_min_size(pos2(b.left() + 22.0, b.top() + 44.0), vec2(84.0, 84.0));
            p.rect_stroke(m, 14.0, line, StrokeKind::Inside);
            deco::globe(p, m.center(), 28.0, thin);
            deco::caption(p, pos2(m.left() + 2.0, m.bottom() + 14.0), Align2::LEFT_CENTER, "NOW LOADING…", lab);
            deco::squares(p, pos2(m.left() + 2.0, m.bottom() + 30.0), 7, 3, 7.0, lab);
            let q = Rect::from_min_size(pos2(m.left(), m.bottom() + 46.0), vec2(84.0, 70.0));
            p.rect_stroke(q, 14.0, line, StrokeKind::Inside);
            for (i, row) in ["CUT  OK", "CROP OK", "MIX  OK"].iter().enumerate() {
                deco::caption(p, pos2(q.left() + 12.0, q.top() + 16.0 + i as f32 * 18.0), Align2::LEFT_CENTER, row, dim);
            }
            deco::barcode(p, Rect::from_min_size(pos2(b.left() + 26.0, b.bottom() - 34.0), vec2(84.0, 14.0)), lab);

            // right: glyphs and a vertical caption
            let rx = b.right() - 52.0;
            deco::heart(p, pos2(rx, b.top() + 70.0), 26.0, thin);
            deco::moon(p, pos2(rx, b.top() + 126.0), 13.0, thin);
            deco::cross(p, pos2(rx, b.top() + 176.0), 10.0, line);
            p.circle_stroke(pos2(rx, b.top() + 176.0), 6.0, line);
            deco::caption_up(p, pos2(b.right() + 18.0, b.center().y), "FFMPEG // FRONT END // 2000", dim);
            deco::dots(p, pos2(b.right() - 180.0, b.bottom() - 4.0), 6, 1, 3.5, lab);

            // stars
            deco::star(p, b.right_top() + vec2(-6.0, 18.0), 18.0, white);
            deco::star(p, pos2(b.left() - 16.0, b.top() + 22.0), 8.0, white);
            deco::star(p, pos2(b.right() - 230.0, b.bottom() + 16.0), 6.0, lab);
            deco::star(p, pos2(b.center().x + 250.0, b.bottom() - 40.0), 9.0, white);
        }
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
        p.rect_stroke(cr, 0.0, Stroke::new(1.5, theme::crop()), StrokeKind::Inside);

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
        widgets::paint_grips(ui.painter(), &zones, theme::crop(), 9.0);

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
        let line = Stroke::new(1.5, theme::crop());
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
