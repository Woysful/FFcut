//! Audio-file mode: a big zoomable waveform instead of the picture. Volume keyframes, fades and the
//! trim range are edited right on it (one track, trimmed like the file, so what you see is what is exported).

use super::tracks::peak_in;
use crate::app::{fmt_time, App, TrkDrag};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align2, Color32, DragValue, FontId, Pos2, Rect, RichText, Sense, Stroke};
use ffcut_core::{EnvPoint, TrackSource};

const ENV_MAX: f32 = 12.0;
const ENV_MIN: f32 = -48.0;
const EDGE: f32 = 8.0;
const MIN_SPAN: f64 = 0.2;
const RULER_H: f32 = 20.0;
const BAR_H: f32 = 12.0;

/// A tick step (seconds) that keeps labels at least ~80 px apart.
fn tick_step(px_per_s: f32) -> f64 {
    const STEPS: [f64; 16] = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0];
    STEPS.iter().copied().find(|s| *s as f32 * px_per_s >= 80.0).unwrap_or(3600.0)
}

fn short_time(t: f64, step: f64) -> String {
    let t = t.max(0.0);
    let (m, s) = ((t / 60.0).floor() as u64, t % 60.0);
    if step < 1.0 {
        format!("{m}:{s:04.1}")
    } else {
        format!("{m}:{:02}", s.round() as u64 % 60)
    }
}

impl App {
    /// Current view: (start, span), clamped to the file.
    fn view(&self) -> (f64, f64) {
        let dur = self.dur().max(0.001);
        let span = if self.aview.span <= 0.0 { dur } else { self.aview.span.clamp(MIN_SPAN.min(dur), dur) };
        (self.aview.t0.clamp(0.0, (dur - span).max(0.0)), span)
    }

    fn zoom_to(&mut self, t0: f64, span: f64) {
        let dur = self.dur().max(0.001);
        let span = span.clamp(MIN_SPAN.min(dur), dur);
        self.aview.span = if span >= dur - 1e-6 { 0.0 } else { span };
        self.aview.t0 = t0.clamp(0.0, (dur - span).max(0.0));
    }

    pub fn audio_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let dur = self.dur().max(0.001);
        let Some(src) = self.s.tracks.first().map(|t| t.source.clone()) else {
            ui.centered_and_justified(|ui| ui.weak("This file has no audio."));
            return;
        };
        self.want_wave(&ctx, &src);
        let full = ui.available_rect_before_wrap().shrink(12.0);

        // ---- toolbar
        let bar = Rect::from_min_size(full.min, vec2(full.width(), 26.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            let (t0, span) = self.view();
            if ui.button("−").on_hover_text("Zoom out  (mouse wheel)").clicked() {
                self.zoom_to(t0 - span * 0.5, span * 2.0);
            }
            if ui.button("+").on_hover_text("Zoom in  (mouse wheel)").clicked() {
                self.zoom_to(t0 + span * 0.25, span * 0.5);
            }
            if ui.button("Fit").on_hover_text("Show the whole file").clicked() {
                self.zoom_to(0.0, dur);
            }
            if ui.button("Selection").on_hover_text("Zoom to the trim range").clicked() {
                let (a, b) = (self.trim_start, self.trim_end.max(self.trim_start + 0.1));
                self.zoom_to(a - (b - a) * 0.05, (b - a) * 1.1);
            }
            ui.add_space(8.0);
            let hint = format!(
                "{}: zoom · {} or middle-drag: scroll · Double-click or Alt+click: add a volume keyframe · Right-click: menu",
                self.settings.zoom_key().label(),
                self.settings.scroll_key().label()
            );
            ui.add(egui::Label::new(RichText::new(hint).size(11.5).color(theme::TEXT_OFF)).truncate());
        });

        // ---- layout
        let body = Rect::from_min_max(pos2(full.left(), bar.bottom() + 6.0), full.max);
        let ruler = Rect::from_min_size(body.min, vec2(body.width(), RULER_H));
        let wave_r = Rect::from_min_max(pos2(body.left(), ruler.bottom()), pos2(body.right(), (body.bottom() - BAR_H - 6.0).max(ruler.bottom() + 40.0)));
        let ovw = Rect::from_min_size(pos2(body.left(), wave_r.bottom() + 6.0), vec2(body.width(), BAR_H));

        let (mut t0, mut span) = self.view();
        let resp = ui.interact(wave_r, ui.id().with("awave"), Sense::click_and_drag());
        let x_of = |t: f64, t0: f64, span: f64| wave_r.left() + ((t - t0) / span) as f32 * wave_r.width();
        let t_at = |x: f32, t0: f64, span: f64| t0 + ((x - wave_r.left()) / wave_r.width()) as f64 * span;

        // ---- zoom / scroll (the modifiers are the user's choice, see Settings → Interface)
        if resp.hovered() {
            let (zoom_k, scroll_k) = (self.settings.zoom_key(), self.settings.scroll_key());
            let pos = ui.input(|i| i.pointer.hover_pos());
            for (d, m) in Self::wheel_events(ui.ctx()) {
                let (t0_now, span_now) = self.view();
                if d.x != 0.0 || scroll_k.matches(&m) {
                    let step = if d.x != 0.0 { d.x } else { d.y };
                    self.zoom_to(t0_now - step as f64 / wave_r.width() as f64 * span_now, span_now);
                } else if zoom_k.matches(&m) && d.y != 0.0 {
                    if let Some(p) = pos {
                        let tc = t_at(p.x, t0_now, span_now);
                        let new_span = (span_now * (-d.y as f64 * 0.0025).exp()).clamp(MIN_SPAN.min(dur), dur);
                        let frac = ((p.x - wave_r.left()) / wave_r.width()) as f64;
                        self.zoom_to(tc - frac * new_span, new_span);
                    }
                }
            }
            (t0, span) = self.view();
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            t0 -= resp.drag_delta().x as f64 / wave_r.width() as f64 * span;
            self.zoom_to(t0, span);
            (t0, span) = self.view();
        }
        // the overview strip: drag = scroll
        let ov = ui.interact(ovw, ui.id().with("aov"), Sense::click_and_drag());
        if let (true, Some(p)) = (ov.dragged() || ov.clicked(), ov.interact_pointer_pos()) {
            let t = ((p.x - ovw.left()) / ovw.width()).clamp(0.0, 1.0) as f64 * dur;
            self.zoom_to(t - span * 0.5, span);
            (t0, span) = self.view();
        }
        // follow the playhead while playing
        if !self.player.paused() && self.trk_drag.is_none() && span < dur {
            let now = self.player.time();
            if now > t0 + span || now < t0 {
                self.zoom_to(now - span * 0.1, span);
                (t0, span) = self.view();
            }
        }

        let alt = ui.input(|i| i.modifiers.alt);
        let t = self.s.tracks[0].clone();
        let px_per_s = wave_r.width() / span as f32;
        let y_of_db = |db: f32| wave_r.top() + (ENV_MAX - db.clamp(ENV_MIN, ENV_MAX)) / (ENV_MAX - ENV_MIN) * wave_r.height();
        let db_of_y = |y: f32| {
            let v = ENV_MAX - ((y - wave_r.top()) / wave_r.height()).clamp(0.0, 1.0) * (ENV_MAX - ENV_MIN);
            if v.abs() < 1.0 {
                0.0
            } else {
                v
            }
        };
        let env_xy = |pt: &EnvPoint| pos2(x_of(pt.t, t0, span), y_of_db(pt.db));
        let near_env = |p: Pos2, r: f32| t.envelope.iter().position(|pt| env_xy(pt).distance(p) <= r);
        let (xs, xe) = (x_of(self.trim_start, t0, span), x_of(self.trim_end, t0, span));
        let (fi_x, fo_x) = (xs + t.fade_in as f32 * px_per_s, xe - t.fade_out as f32 * px_per_s);
        let near_top = |p: Pos2| (p.y - (wave_r.top() + 9.0)).abs() <= 10.0;

        // ---- interaction
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                let mode = if let Some(k) = near_env(p, 9.0) {
                    TrkDrag::EnvPt(k)
                } else if near_top(p) && (p.x - fi_x).abs() <= 9.0 && (p.x - fi_x).abs() <= (p.x - fo_x).abs() {
                    TrkDrag::FadeIn
                } else if near_top(p) && (p.x - fo_x).abs() <= 9.0 {
                    TrkDrag::FadeOut
                } else if (p.x - xs).abs() <= EDGE && (p.x - xs).abs() <= (p.x - xe).abs() {
                    TrkDrag::TrimStart
                } else if (p.x - xe).abs() <= EDGE {
                    TrkDrag::TrimEnd
                } else {
                    TrkDrag::Seek
                };
                self.trk_drag = Some((0, mode));
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let (Some((_, mode)), Some(p)) = (self.trk_drag, resp.interact_pointer_pos()) {
                let now = t_at(p.x, t0, span).clamp(0.0, dur);
                let (ts, te) = (self.trim_start, self.trim_end);
                match mode {
                    TrkDrag::TrimStart => {
                        self.trim_start = now.clamp(0.0, te - 0.05);
                        self.sync_loop();
                    }
                    TrkDrag::TrimEnd => {
                        self.trim_end = now.clamp(ts + 0.05, dur);
                        self.sync_loop();
                    }
                    TrkDrag::FadeIn => {
                        let room = (te - ts - t.fade_out).max(0.0);
                        let f = ((p.x - xs) / px_per_s) as f64;
                        let f = f.clamp(0.0, room);
                        self.s.tracks[0].fade_in = if f < 0.05 { 0.0 } else { f };
                    }
                    TrkDrag::FadeOut => {
                        let room = (te - ts - t.fade_in).max(0.0);
                        let f = ((xe - p.x) / px_per_s) as f64;
                        let f = f.clamp(0.0, room);
                        self.s.tracks[0].fade_out = if f < 0.05 { 0.0 } else { f };
                    }
                    TrkDrag::EnvPt(k) => {
                        let tr = &mut self.s.tracks[0];
                        if k < tr.envelope.len() {
                            let lo = if k > 0 { tr.envelope[k - 1].t + 0.02 } else { ts };
                            let hi = if k + 1 < tr.envelope.len() { tr.envelope[k + 1].t - 0.02 } else { te };
                            tr.envelope[k].t = now.clamp(lo, hi.max(lo));
                            tr.envelope[k].db = db_of_y(p.y);
                        }
                    }
                    _ => self.player.seek_fast(now),
                }
            }
        }
        if resp.drag_stopped() {
            if let (Some((_, TrkDrag::Seek)), Some(p)) = (self.trk_drag, resp.interact_pointer_pos()) {
                self.player.seek(t_at(p.x, t0, span).clamp(0.0, dur));
            }
            self.trk_drag = None;
        }
        if let Some(p) = resp.hover_pos() {
            let cur = if near_env(p, 9.0).is_some() {
                Some(egui::CursorIcon::Grab)
            } else if alt {
                Some(egui::CursorIcon::Crosshair)
            } else if (near_top(p) && ((p.x - fi_x).abs() <= 9.0 || (p.x - fo_x).abs() <= 9.0)) || (p.x - xs).abs() <= EDGE || (p.x - xe).abs() <= EDGE {
                Some(egui::CursorIcon::ResizeHorizontal)
            } else {
                None
            };
            if let Some(c) = cur {
                ui.ctx().set_cursor_icon(c);
            }
        }
        if resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if near_env(p, 9.0).is_none() {
                    let at = t_at(p.x, t0, span).clamp(self.trim_start, self.trim_end);
                    self.add_keyframe(at, db_of_y(p.y));
                }
            }
        } else if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if alt && near_env(p, 9.0).is_none() {
                    // Alt held: a click puts a keyframe
                    let at = t_at(p.x, t0, span).clamp(self.trim_start, self.trim_end);
                    self.add_keyframe(at, db_of_y(p.y));
                } else if near_env(p, 9.0).is_none() {
                    self.player.seek(t_at(p.x, t0, span).clamp(0.0, dur));
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.fx.ctx_point = near_env(p, 10.0);
                self.aview.menu_t = t_at(p.x, t0, span).clamp(self.trim_start, self.trim_end);
                self.aview.menu_db = db_of_y(p.y);
            }
        }
        super::ctx_menu(&resp, |ui| self.audio_menu(ui));

        // ---- painting
        let p = ui.painter_at(body.expand(2.0));
        p.rect_filled(wave_r, 6.0, theme::TRACK);

        // ruler
        let step = tick_step(px_per_s);
        let mut k = (t0 / step).floor();
        while k * step <= t0 + span + step {
            let tt = k * step;
            let x = x_of(tt, t0, span);
            if x >= wave_r.left() - 0.5 && x <= wave_r.right() + 0.5 {
                p.line_segment([pos2(x, ruler.bottom() - 6.0), pos2(x, ruler.bottom())], Stroke::new(1.0, theme::TEXT_OFF));
                p.text(pos2(x + 3.0, ruler.center().y - 2.0), Align2::LEFT_CENTER, short_time(tt, step), FontId::monospace(10.5), theme::TEXT_DIM);
                p.line_segment([pos2(x, wave_r.top()), pos2(x, wave_r.bottom())], Stroke::new(1.0, Color32::from_white_alpha(10)));
            }
            k += 1.0;
        }
        // level guides of the keyframe lane
        for db in [12.0f32, 0.0, -12.0, -24.0, -36.0] {
            let y = y_of_db(db);
            p.line_segment([pos2(wave_r.left(), y), pos2(wave_r.right(), y)], Stroke::new(1.0, Color32::from_white_alpha(if db == 0.0 { 34 } else { 14 })));
            p.text(pos2(wave_r.left() + 4.0, y + 1.0), Align2::LEFT_TOP, format!("{db:+.0} dB"), FontId::proportional(9.5), Color32::from_white_alpha(70));
        }

        let mid = wave_r.center().y;
        let half = wave_r.height() * 0.5 - 6.0;
        // With effects the picture comes from the rendered preview (so it shows what the effects do);
        // until that is ready, and outside the trim range, from the file itself.
        let baked = if t.has_fx() { self.baked_path(&t) } else { None };
        let baked_src = baked.as_ref().map(|p| TrackSource::External { path: p.to_string_lossy().into_owned(), duration: dur });
        if let Some(b) = &baked_src {
            self.want_wave(&ctx, b);
        }
        let baked_key = baked_src.as_ref().map(|b| self.wave_key(b));
        let src_key = self.wave_key(&src);
        let baked_ready = baked_key.as_ref().is_some_and(|k| matches!(self.waves.lock().unwrap().get(k), Some(crate::app::Wave::Ready(_))));
        let (hi_key, hi_file, hi_ord) = match (&baked_src, baked_ready) {
            (Some(TrackSource::External { path, .. }), true) => (baked_key.clone().unwrap(), std::path::PathBuf::from(path), 0),
            _ => match &src {
                TrackSource::Original(i) => (src_key.clone(), self.clip.as_ref().map(|c| c.path.clone()).unwrap_or_default(), *i),
                TrackSource::External { path, .. } => (src_key.clone(), std::path::PathBuf::from(path), 0),
            },
        };
        self.want_hires(&ctx, &hi_key, &hi_file, hi_ord, (t0, span), px_per_s as f64);
        let hi = self.hires_for(&hi_key, (t0, span), px_per_s as f64);
        {
            let wave = self.waves.lock().unwrap();
            let low = |k: &str| match wave.get(k) {
                Some(crate::app::Wave::Ready(p)) => Some(p),
                _ => None,
            };
            match low(&src_key) {
                Some(src_peaks) => {
                    let baked_peaks = if baked_ready { baked_key.as_deref().and_then(low) } else { None };
                    let (x0, x1) = (wave_r.left().floor() as i32, wave_r.right().ceil() as i32);
                    for xi in x0..x1 {
                        let x = xi as f32;
                        let (s0, s1) = (t_at(x, t0, span), t_at(x + 1.0, t0, span));
                        if s1 < 0.0 || s0 > dur {
                            continue;
                        }
                        let inside = s1 >= self.trim_start - 1e-6 && s0 <= self.trim_end + 1e-6;
                        let use_baked = baked_peaks.is_some() && inside;
                        let (raw, gain) = if use_baked {
                            // the finer picture (if there is one) is of this same file
                            (hi.as_ref().map_or_else(|| peak_in(baked_peaks.unwrap(), s0.max(0.0), s1.max(0.0)), |h| h.peak(s0, s1)), 1.0)
                        } else {
                            let g = if t.has_fx() { ffcut_core::audiofx::gain_at(&t, (s0 + s1) * 0.5) } else { 1.0 };
                            let raw = match (&hi, baked_peaks.is_some()) {
                                (Some(h), false) => h.peak(s0, s1),
                                _ => peak_in(src_peaks, s0.max(0.0), s1.max(0.0)),
                            };
                            (raw, g)
                        };
                        let a = raw * (t.volume * gain).clamp(0.02, 3.0).sqrt();
                        let h = (a * half).clamp(0.5, half);
                        p.line_segment(
                            [pos2(x + 0.5, mid - h), pos2(x + 0.5, mid + h)],
                            Stroke::new(1.0, if inside { theme::OK } else { Color32::from_gray(80) }),
                        );
                    }
                }
                None => {
                    let failed = matches!(wave.get(&src_key), Some(crate::app::Wave::Failed));
                    p.text(
                        wave_r.center(),
                        Align2::CENTER_CENTER,
                        if failed { "no waveform" } else { "reading waveform…" },
                        FontId::proportional(13.0),
                        theme::TEXT_DIM,
                    );
                }
            }
        }
        p.line_segment([pos2(wave_r.left(), mid), pos2(wave_r.right(), mid)], Stroke::new(1.0, Color32::from_white_alpha(22)));

        // outside the trim range: dimmed, with the handles on the edges
        let (cl, cr) = (xs.clamp(wave_r.left(), wave_r.right()), xe.clamp(wave_r.left(), wave_r.right()));
        let dim = Color32::from_black_alpha(120);
        if cl > wave_r.left() {
            p.rect_filled(Rect::from_min_max(wave_r.min, pos2(cl, wave_r.bottom())), 0.0, dim);
        }
        if cr < wave_r.right() {
            p.rect_filled(Rect::from_min_max(pos2(cr, wave_r.top()), wave_r.max), 0.0, dim);
        }
        for x in [xs, xe] {
            if x >= wave_r.left() - 1.0 && x <= wave_r.right() + 1.0 {
                p.rect_filled(Rect::from_center_size(pos2(x, mid), vec2(4.0, wave_r.height() - 8.0)), 2.0, theme::ACCENT);
            }
        }

        // fades
        let top = wave_r.top();
        if t.fade_in > 0.001 {
            let x1 = fi_x.min(wave_r.right());
            let a = xs.max(wave_r.left());
            if x1 > a {
                p.add(egui::Shape::convex_polygon(vec![pos2(a, top), pos2(x1, top), pos2(a, wave_r.bottom())], Color32::from_black_alpha(90), Stroke::NONE));
            }
            p.line_segment([pos2(xs, wave_r.bottom()), pos2(fi_x, top)], Stroke::new(1.5, Color32::from_white_alpha(170)));
        }
        if t.fade_out > 0.001 {
            let x0 = fo_x.max(wave_r.left());
            let b = xe.min(wave_r.right());
            if b > x0 {
                p.add(egui::Shape::convex_polygon(vec![pos2(b, top), pos2(x0, top), pos2(b, wave_r.bottom())], Color32::from_black_alpha(90), Stroke::NONE));
            }
            p.line_segment([pos2(fo_x, top), pos2(xe, wave_r.bottom())], Stroke::new(1.5, Color32::from_white_alpha(170)));
        }
        for (x, on) in [(fi_x, t.fade_in > 0.001), (fo_x, t.fade_out > 0.001)] {
            if x >= wave_r.left() - 1.0 && x <= wave_r.right() + 1.0 {
                p.circle(
                    pos2(x, top + 9.0),
                    4.0,
                    if on { Color32::WHITE } else { Color32::from_white_alpha(70) },
                    Stroke::new(1.0, Color32::from_black_alpha(160)),
                );
            }
        }

        // keyframes
        if !t.envelope.is_empty() {
            // flat before the first and after the last keyframe, but only inside the trim range
            let mut pts: Vec<Pos2> = t.envelope.iter().map(env_xy).collect();
            if pts[0].x > xs {
                pts.insert(0, pos2(xs, pts[0].y));
            }
            let last = pts[pts.len() - 1];
            if last.x < xe {
                pts.push(pos2(xe, last.y));
            }
            p.add(egui::Shape::line(pts, Stroke::new(1.8, theme::WARN)));
            for (k, pt) in t.envelope.iter().enumerate() {
                let q = env_xy(pt);
                if q.x >= wave_r.left() - 6.0 && q.x <= wave_r.right() + 6.0 {
                    p.circle(q, 4.5, theme::WARN, Stroke::new(1.0, Color32::BLACK));
                    if resp.hover_pos().is_some_and(|h| h.distance(q) <= 10.0) || matches!(self.trk_drag, Some((_, TrkDrag::EnvPt(d))) if d == k) {
                        p.text(
                            q + vec2(8.0, -8.0),
                            Align2::LEFT_BOTTOM,
                            format!("{:+.1} dB · {}", pt.db, fmt_time(pt.t)),
                            FontId::proportional(11.0),
                            Color32::WHITE,
                        );
                    }
                }
            }
        }

        // playhead
        let nowx = x_of(self.player.time().clamp(0.0, dur), t0, span);
        if nowx >= wave_r.left() && nowx <= wave_r.right() {
            p.line_segment([pos2(nowx, ruler.top() + 2.0), pos2(nowx, wave_r.bottom())], Stroke::new(1.5, Color32::WHITE));
        }

        // overview strip
        p.rect_filled(ovw, 4.0, Color32::from_gray(38));
        let ox = |t: f64| ovw.left() + (t / dur) as f32 * ovw.width();
        p.rect_filled(Rect::from_min_max(pos2(ox(self.trim_start), ovw.top()), pos2(ox(self.trim_end), ovw.bottom())), 4.0, theme::ACCENT.gamma_multiply(0.35));
        p.rect_stroke(
            Rect::from_min_max(pos2(ox(t0), ovw.top()), pos2(ox(t0 + span).max(ox(t0) + 6.0), ovw.bottom())),
            4.0,
            Stroke::new(1.5, theme::TEXT),
            egui::StrokeKind::Inside,
        );
        p.line_segment([pos2(ox(self.player.time()), ovw.top()), pos2(ox(self.player.time()), ovw.bottom())], Stroke::new(1.5, Color32::WHITE));
    }

    fn add_keyframe(&mut self, at: f64, db: f32) {
        let tr = &mut self.s.tracks[0];
        let i = tr.envelope.iter().position(|e| e.t > at).unwrap_or(tr.envelope.len());
        tr.envelope.insert(i, EnvPoint { t: at, db });
    }

    fn audio_menu(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.s.tracks.get_mut(0) else { return };
        if let Some(k) = self.fx.ctx_point.filter(|k| *k < t.envelope.len()) {
            super::widgets::menu_indent(ui, |ui| ui.label(RichText::new("Volume keyframe").strong()));
            super::widgets::menu_indent(ui, |ui| {
                ui.label("Level");
                let mut db = t.envelope[k].db;
                if super::widgets::value(ui, DragValue::new(&mut db).range(-60.0..=12.0).speed(0.2).suffix(" dB").fixed_decimals(1))
                    .on_hover_text("Drag, or click and type a value")
                    .changed()
                {
                    t.envelope[k].db = db;
                }
            });
            if ui.button("Delete keyframe").clicked() {
                t.envelope.remove(k);
                ui.close();
            }
        } else {
            if ui.button("Add volume keyframe here").clicked() {
                let (at, db) = (self.aview.menu_t, self.aview.menu_db);
                self.add_keyframe(at, db);
                ui.close();
            }
        }
        ui.separator();
        let t = &mut self.s.tracks[0];
        if ui.add_enabled(!t.envelope.is_empty(), egui::Button::new("Remove all keyframes")).clicked() {
            t.envelope.clear();
            ui.close();
        }
        if ui.add_enabled(t.fade_in > 0.0 || t.fade_out > 0.0, egui::Button::new("Remove fades")).clicked() {
            t.fade_in = 0.0;
            t.fade_out = 0.0;
            ui.close();
        }
        ui.separator();
        let dur = self.dur();
        if ui.button("Zoom to fit").clicked() {
            self.zoom_to(0.0, dur);
            ui.close();
        }
        if ui.button("Zoom to the trim range").clicked() {
            let (a, b) = (self.trim_start, self.trim_end.max(self.trim_start + 0.1));
            self.zoom_to(a - (b - a) * 0.05, (b - a) * 1.1);
            ui.close();
        }
        let _ = TrackSource::Original(0);
    }
}
