//! Multi-track timeline: the trim bar plus one lane per audio track.
//! Per track: trim (drag the clip edges), shift (drag the clip), volume, delete / restore,
//! replace the sound with another file, and add new tracks. Lanes show a waveform.

use super::transport::HEADER_W;
use crate::app::fmt_time;
use crate::app::{App, TrkDrag, Wave};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, DragValue, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder};
use ffcut_core::waveform::PEAKS_PER_SEC;
use ffcut_core::{audible, TrackEdit, TrackSource};

pub const LANE_H: f32 = 64.0;
/// A removed track is only a thin strip.
pub const DEL_H: f32 = 32.0;
const EDGE: f32 = 8.0;
const ENV_MAX: f32 = 12.0;
const ENV_MIN: f32 = -48.0;

enum Act {
    Delete(usize),
    Restore(usize),
    Replace(usize),
    Reset(usize),
    Fx(usize),
}

pub(super) fn peak_in(peaks: &[f32], t0: f64, t1: f64) -> f32 {
    let a = (t0 * PEAKS_PER_SEC as f64).floor().max(0.0) as usize;
    if a >= peaks.len() {
        return 0.0;
    }
    let b = ((t1 * PEAKS_PER_SEC as f64).ceil() as usize).max(a + 1).min(peaks.len());
    peaks[a..b].iter().cloned().fold(0.0, f32::max)
}

impl App {
    pub fn tracks_view(&mut self, ui: &mut egui::Ui) {
        // row 0: the trim range, aligned with the lanes below
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(vec2(HEADER_W, 50.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(HEADER_W);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(8.0);
                // what will be exported: in / out and the resulting length
                let end = if self.trim_end > 0.0 { self.trim_end } else { self.dur() };
                let dim = theme::TEXT_DIM;
                ui.label(RichText::new(format!("{} → {}", fmt_time(self.trim_start), fmt_time(end))).monospace().size(11.5).color(theme::TEXT));
                ui.label(RichText::new(format!("length {}", fmt_time((end - self.trim_start).max(0.0)))).monospace().size(11.5).color(dim));
            });
            self.timeline(ui, 50.0);
        });

        let mut act: Option<Act> = None;
        let ctx = ui.ctx().clone();
        let list_h = (ui.available_height() - 36.0).max(LANE_H);
        egui::ScrollArea::vertical().max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            let n = self.s.tracks.len();
            // who is audible right now (mute / solo): silent lanes are drawn dimmed
            let heard: Vec<bool> = {
                let h = audible(&self.s.tracks);
                self.s.tracks.iter().map(|t| h.iter().any(|x| std::ptr::eq(*x, t))).collect()
            };
            for (i, &heard) in heard.iter().enumerate().take(n) {
                ui.horizontal(|ui| {
                    self.track_header(ui, i, heard, &mut act);
                    self.track_lane(ui, &ctx, i, heard, &mut act);
                });
            }
            if n == 0 {
                ui.weak("This file has no audio tracks. Add one with the button below.");
            }
        });

        ui.horizontal(|ui| {
            if ui.button("+ Add audio track…").on_hover_text("Use the sound of another audio or video file").clicked() {
                self.add_audio_track();
            }
            if self.tracks_edited() && ui.button("Reset all tracks").clicked() {
                self.reset_tracks();
            }
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "The preview plays all audible tracks together, with their trim, shift and effects (FX). Drag the panel edge up for more room.",
                    )
                    .size(11.5)
                    .color(theme::TEXT_OFF),
                )
                .truncate(),
            );
        });

        match act {
            Some(Act::Delete(i)) => {
                self.fx.env_edit = None;
                if self.side.fx_track == Some(i) {
                    self.side.fx_track = None;
                }
                if matches!(self.s.tracks[i].source, TrackSource::External { .. }) {
                    self.s.tracks.remove(i); // an added file has nothing to restore
                } else {
                    self.s.tracks[i].deleted = true;
                }
            }
            Some(Act::Fx(i)) => self.show_track_fx(i),
            Some(Act::Restore(i)) => self.s.tracks[i].deleted = false,
            Some(Act::Replace(i)) => self.replace_track(i),
            Some(Act::Reset(i)) => {
                let d = self.dur();
                let t = &mut self.s.tracks[i];
                t.trim_start = 0.0;
                t.trim_end = t.source_duration(d);
                t.shift = 0.0;
                t.volume = 1.0;
                t.muted = false;
                t.solo = false;
                t.fade_in = 0.0;
                t.fade_out = 0.0;
                t.envelope.clear();
                t.effects.clear();
            }
            None => {}
        }
    }

    /// The menu of a track (right-click on its lane or its buttons). `point`: the keyframe it was opened on.
    fn lane_menu(&mut self, ui: &mut egui::Ui, i: usize, point: Option<usize>, act: &mut Option<Act>) {
        let Some(t) = self.s.tracks.get_mut(i) else { return };
        if let Some(k) = point.filter(|k| *k < t.envelope.len()) {
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
            ui.separator();
        }
        if t.deleted {
            if ui.button("Restore track").clicked() {
                *act = Some(Act::Restore(i));
                ui.close();
            }
            return;
        }
        if ui.button("Audio effects…").clicked() {
            self.show_track_fx(i);
            ui.close();
            return;
        }
        let mut editing = self.fx.env_edit == Some(i);
        if super::widgets::menu_indent(ui, |ui| ui.checkbox(&mut editing, "Edit volume keyframes"))
            .on_hover_text("Click the track to add a point, drag to move it, right-click a point for its menu")
            .changed()
        {
            self.fx.env_edit = if editing { Some(i) } else { None };
            ui.close();
            return;
        }
        ui.separator();
        super::widgets::menu_indent(ui, |ui| {
            ui.label("Volume");
            let mut pct = t.volume * 100.0;
            if super::widgets::value(ui, DragValue::new(&mut pct).range(0.0..=300.0).speed(1.0).suffix("%").fixed_decimals(0)).changed() {
                t.volume = pct / 100.0;
            }
        });
        if ui.button("Replace file…").clicked() {
            *act = Some(Act::Replace(i));
            ui.close();
        }
        if ui
            .add_enabled(t.shift.abs() > 0.0005, egui::Button::new("Reset position"))
            .on_hover_text("Move the track back to where it starts in the file")
            .clicked()
        {
            t.shift = 0.0;
            ui.close();
        }
        if ui.button("Reset whole track").on_hover_text("Trim, position, volume, mute, solo and effects").clicked() {
            *act = Some(Act::Reset(i));
            ui.close();
        }
        ui.separator();
        if ui.button("Remove track").clicked() {
            *act = Some(Act::Delete(i));
            ui.close();
        }
    }

    /// Left column of a lane: two rows of buttons (the file name is drawn on the track itself).
    fn track_header(&mut self, ui: &mut egui::Ui, i: usize, _heard: bool, act: &mut Option<Act>) {
        let deleted = self.s.tracks[i].deleted;
        let h = if deleted { DEL_H } else { LANE_H };
        let (rect, head) = ui.allocate_exact_size(vec2(HEADER_W, h), Sense::click());
        super::ctx_menu(&head, |ui| self.lane_menu(ui, i, None, act));
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        ui.spacing_mut().button_padding = vec2(2.0, 1.0);

        let fx_open = self.side.fx_track == Some(i) && self.is_open(super::Section::Audio);
        let t = &mut self.s.tracks[i];
        let has_fx = t.has_fx();
        let icon = |glyph: &str| RichText::new(glyph).size(14.0);
        let letter = |l: &str| RichText::new(l).size(12.5).strong();
        if t.deleted {
            // centred in the thin strip (a plain horizontal row is taller than the strip and got cut off)
            ui.allocate_ui_with_layout(vec2(HEADER_W, DEL_H), Layout::left_to_right(Align::Center), |ui| {
                if ui.add_sized(vec2(41.0, 20.0), egui::Button::new(icon("↩"))).on_hover_text("Put this track back into the export").clicked() {
                    *act = Some(Act::Restore(i));
                }
                ui.label(RichText::new("removed").size(11.5).color(theme::TEXT_OFF));
            });
            return;
        }
        ui.add_space(2.0);
        let sz = vec2(41.0, 24.0);
        ui.horizontal(|ui| {
            if ui.add_sized(sz, egui::Button::new(icon("🗑"))).on_hover_text("Remove this track from the export").clicked() {
                *act = Some(Act::Delete(i));
            }
            if ui.add_sized(sz, egui::Button::new(icon("⟳"))).on_hover_text("Replace the sound of this track with another file").clicked() {
                *act = Some(Act::Replace(i));
            }
            let moved = t.shift.abs() > 0.0005;
            let r = ui.add_enabled(moved, egui::Button::new("").min_size(sz)).on_hover_text("Reset position: move the track back to its original place");
            {
                // |← drawn by hand: the font has no glyph for it
                let c = r.rect.center();
                let col = if moved { theme::TEXT } else { Color32::from_gray(90) };
                let st = Stroke::new(1.6, col);
                let p = ui.painter();
                p.line_segment([pos2(c.x - 8.0, c.y - 5.0), pos2(c.x - 8.0, c.y + 5.0)], st);
                p.line_segment([pos2(c.x - 8.0, c.y), pos2(c.x + 8.0, c.y)], st);
                p.line_segment([pos2(c.x - 3.0, c.y - 4.5), pos2(c.x - 8.0, c.y)], st);
                p.line_segment([pos2(c.x - 3.0, c.y + 4.5), pos2(c.x - 8.0, c.y)], st);
            }
            if r.clicked() {
                t.shift = 0.0;
            }
            let fx = egui::Button::new(RichText::new("FX").size(12.0).strong().color(if has_fx { Color32::BLACK } else { theme::TEXT })).fill(if has_fx {
                theme::ACCENT
            } else if fx_open {
                Color32::from_gray(70)
            } else {
                ui.visuals().widgets.inactive.weak_bg_fill
            });
            if ui.add_sized(sz, fx).on_hover_text("Audio effects").clicked() {
                *act = Some(Act::Fx(i));
            }
        });
        ui.horizontal(|ui| {
            let mut pct = t.volume * 100.0;
            if super::widgets::value_sized(ui, vec2(86.0, 24.0), DragValue::new(&mut pct).range(0.0..=300.0).speed(1.0).suffix("%").fixed_decimals(0))
                .on_hover_text("Track volume (drag or type)")
                .changed()
            {
                t.volume = pct / 100.0;
            }
            // solo / mute are exclusive on one track
            let solo = egui::Button::new(letter("S")).fill(if t.solo { theme::WARN.gamma_multiply(0.7) } else { ui.visuals().widgets.inactive.weak_bg_fill });
            if ui.add_sized(sz, solo).on_hover_text("Solo: hear and export only the soloed tracks").clicked() {
                t.solo = !t.solo;
                if t.solo {
                    t.muted = false;
                }
            }
            let mute = egui::Button::new(letter("M")).fill(if t.muted { theme::ERR.gamma_multiply(0.7) } else { ui.visuals().widgets.inactive.weak_bg_fill });
            if ui.add_sized(sz, mute).on_hover_text("Mute: leave this track out of the preview and the export").clicked() {
                t.muted = !t.muted;
                if t.muted {
                    t.solo = false;
                }
            }
        });
    }

    fn track_lane(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, i: usize, heard: bool, act: &mut Option<Act>) {
        let dur = self.dur().max(0.001);
        let t: TrackEdit = self.s.tracks[i].clone();
        self.want_wave(ctx, &t.source);

        if t.deleted {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), DEL_H), Sense::click());
            super::ctx_menu(&resp, |ui| self.lane_menu(ui, i, None, act));
            let strip = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 2.0), pos2(rect.right() - 10.0, rect.bottom() - 2.0));
            let p = ui.painter_at(rect);
            p.rect_filled(strip, 5.0, Color32::from_rgb(34, 35, 40));
            p.text(
                strip.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{} — removed from the export (right-click: restore)", t.label),
                FontId::proportional(11.0),
                theme::TEXT_OFF,
            );
            return;
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), LANE_H), Sense::click_and_drag());
        let track = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 2.0), pos2(rect.right() - 10.0, rect.bottom() - 2.0));
        let t2x = |s: f64| track.left() + (s / dur) as f32 * track.width();
        let x2t = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64) * dur;
        let src_dur = t.source_duration(dur);
        let (pa, pb) = t.placed();
        let (xs, xe) = (t2x(pa), t2x(pb));
        let px_per_s = track.width() / dur as f32;
        let env_mode = (self.fx.env_edit == Some(i) || ui.input(|i| i.modifiers.alt)) && !t.deleted; // Alt held = keyframe mode
                                                                                                     // keyframe height: +12 dB at the top, -48 dB at the bottom
        let y_of_db = |db: f32| track.top() + (ENV_MAX - db.clamp(ENV_MIN, ENV_MAX)) / (ENV_MAX - ENV_MIN) * track.height();
        let db_of_y = |y: f32| {
            let v = ENV_MAX - ((y - track.top()) / track.height()).clamp(0.0, 1.0) * (ENV_MAX - ENV_MIN);
            if v.abs() < 1.0 {
                0.0
            } else {
                v
            }
        };
        let env_xy = |pt: &ffcut_core::EnvPoint| pos2(t2x(pt.t + t.shift), y_of_db(pt.db));
        let near_env = |p: egui::Pos2, r: f32| t.envelope.iter().position(|pt| env_xy(pt).distance(p) <= r);
        let (fi_x, fo_x) = (xs + t.fade_in as f32 * px_per_s, xe - t.fade_out as f32 * px_per_s);
        let near_top = |p: egui::Pos2| (p.y - (track.top() + 7.0)).abs() <= 9.0;

        // ---- interaction
        if !t.deleted {
            if resp.drag_started() {
                if let Some(p) = ui.input(|inp| inp.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                    let mode = if !env_mode && near_top(p) && (p.x - fi_x).abs() <= 8.0 && (p.x - fi_x).abs() <= (p.x - fo_x).abs() {
                        TrkDrag::FadeIn
                    } else if !env_mode && near_top(p) && (p.x - fo_x).abs() <= 8.0 {
                        TrkDrag::FadeOut
                    } else if env_mode {
                        near_env(p, 9.0).map_or(TrkDrag::Seek, TrkDrag::EnvPt)
                    } else if (p.x - xs).abs() <= EDGE && (p.x - xs).abs() <= (p.x - xe).abs() {
                        TrkDrag::TrimStart
                    } else if (p.x - xe).abs() <= EDGE {
                        TrkDrag::TrimEnd
                    } else if p.x > xs && p.x < xe {
                        TrkDrag::Move
                    } else {
                        TrkDrag::Seek
                    };
                    self.trk_drag = Some((i, mode));
                }
            }
            if resp.dragged() {
                if let Some((di, mode)) = self.trk_drag {
                    if di == i {
                        let dx = (resp.drag_delta().x / track.width()) as f64 * dur;
                        let tr = &mut self.s.tracks[i];
                        match mode {
                            TrkDrag::TrimStart => tr.trim_start = (tr.trim_start + dx).clamp(0.0, tr.trim_end - 0.05),
                            TrkDrag::TrimEnd => tr.trim_end = (tr.trim_end + dx).clamp(tr.trim_start + 0.05, src_dur),
                            TrkDrag::Move => {
                                // The clip may slide partly past either end of the video (the overhang is cut
                                // off on export); only keep a sliver on screen so it can be grabbed again.
                                let (lo, hi) = (-(tr.trim_end - 0.1), dur - tr.trim_start - 0.1);
                                tr.shift = (tr.shift + dx).clamp(lo.min(hi), hi.max(lo));
                            }
                            TrkDrag::FadeIn => {
                                if let Some(p) = resp.interact_pointer_pos() {
                                    let room = (tr.trim_end - tr.trim_start - tr.fade_out).max(0.0);
                                    let f = (((p.x - xs) / px_per_s) as f64).clamp(0.0, room);
                                    tr.fade_in = if f < 0.05 { 0.0 } else { f };
                                }
                            }
                            TrkDrag::FadeOut => {
                                if let Some(p) = resp.interact_pointer_pos() {
                                    let room = (tr.trim_end - tr.trim_start - tr.fade_in).max(0.0);
                                    let f = (((xe - p.x) / px_per_s) as f64).clamp(0.0, room);
                                    tr.fade_out = if f < 0.05 { 0.0 } else { f };
                                }
                            }
                            TrkDrag::EnvPt(k) => {
                                if let (Some(p), true) = (resp.interact_pointer_pos(), k < tr.envelope.len()) {
                                    // stays between its neighbours, so the list stays sorted
                                    let lo = if k > 0 { tr.envelope[k - 1].t + 0.02 } else { tr.trim_start };
                                    let hi = if k + 1 < tr.envelope.len() { tr.envelope[k + 1].t - 0.02 } else { tr.trim_end };
                                    tr.envelope[k].t = (x2t(p.x) - tr.shift).clamp(lo, hi.max(lo));
                                    tr.envelope[k].db = db_of_y(p.y);
                                }
                            }
                            TrkDrag::Seek => {
                                if let Some(p) = resp.interact_pointer_pos() {
                                    self.player.seek_fast(x2t(p.x));
                                }
                            }
                        }
                    }
                }
            }
            if resp.drag_stopped() {
                if let Some((di, TrkDrag::Seek)) = self.trk_drag {
                    if di == i {
                        if let Some(p) = resp.interact_pointer_pos() {
                            self.player.seek(x2t(p.x));
                        }
                    }
                }
                if matches!(self.trk_drag, Some((di, _)) if di == i) {
                    self.trk_drag = None;
                }
            }
            if let Some(p) = resp.hover_pos() {
                let cur = if env_mode {
                    Some(if near_env(p, 9.0).is_some() { egui::CursorIcon::Grab } else { egui::CursorIcon::Crosshair })
                } else if (near_top(p) && ((p.x - fi_x).abs() <= 8.0 || (p.x - fo_x).abs() <= 8.0)) || (p.x - xs).abs() <= EDGE || (p.x - xe).abs() <= EDGE {
                    Some(egui::CursorIcon::ResizeHorizontal)
                } else if p.x > xs && p.x < xe {
                    Some(egui::CursorIcon::Grab)
                } else {
                    None
                };
                if let Some(c) = cur {
                    ui.ctx().set_cursor_icon(c);
                }
            }
        }
        if resp.secondary_clicked() {
            self.fx.ctx_point = resp.interact_pointer_pos().and_then(|p| near_env(p, 10.0));
        }
        super::ctx_menu(&resp, |ui| {
            let pt = self.fx.ctx_point;
            self.lane_menu(ui, i, pt, act);
        });
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let src_t = x2t(p.x) - t.shift;
                if env_mode && src_t >= t.trim_start && src_t <= t.trim_end && near_env(p, 9.0).is_none() {
                    // a new keyframe, kept in time order
                    let tr = &mut self.s.tracks[i];
                    let at = tr.envelope.iter().position(|e| e.t > src_t).unwrap_or(tr.envelope.len());
                    tr.envelope.insert(at, ffcut_core::EnvPoint { t: src_t, db: db_of_y(p.y) });
                } else if !env_mode || near_env(p, 9.0).is_none() {
                    self.player.seek(x2t(p.x));
                }
            }
        }

        // ---- painting
        let p = ui.painter_at(rect);
        p.rect_filled(track, 5.0, theme::TRACK);
        let mid = track.center().y;
        let half = track.height() * 0.5 - 4.0;
        let accent = if matches!(t.source, TrackSource::External { .. }) { theme::CROP } else { theme::OK };
        let (on, off) = (accent, Color32::from_gray(85));
        let (on, off) = if t.deleted {
            (Color32::from_gray(70), theme::LINE)
        } else if !heard {
            (theme::TEXT_OFF, Color32::from_gray(70))
        } else {
            (on, off)
        };

        // With effects the lane shows the rendered preview of the track (so the picture follows the effects);
        // until it is ready, and outside the kept part, the file itself.
        let baked = if t.has_fx() { self.baked_path(&t) } else { None };
        let baked_src = baked.as_ref().map(|p| TrackSource::External { path: p.to_string_lossy().into_owned(), duration: dur });
        if let Some(b) = &baked_src {
            self.want_wave(ctx, b);
        }
        let baked_key = baked_src.as_ref().map(|b| self.wave_key(b));
        let wave = self.waves.lock().unwrap();
        match wave.get(&self.wave_key(&t.source)) {
            Some(Wave::Ready(peaks)) => {
                let baked_peaks = match baked_key.as_ref().and_then(|k| wave.get(k)) {
                    Some(Wave::Ready(b)) => Some(b),
                    _ => None,
                };
                // one column per pixel, over the part of the source that is on screen
                let x0 = t2x(t.shift).max(track.left()).floor();
                let x1 = t2x(t.shift + src_dur).min(track.right()).ceil();
                let mut x = x0;
                while x < x1 {
                    let (tl0, tl1) = (x2t(x), x2t(x + 1.0));
                    let (s0, s1) = (tl0 - t.shift, tl1 - t.shift);
                    if s1 >= 0.0 && s0 <= src_dur {
                        let inside = s0 >= t.trim_start - 1e-6 && s1 <= t.trim_end + 1e-6;
                        let a = match baked_peaks {
                            // the rendered file lives on the timeline: effects, fades and keyframes are in it
                            Some(b) if inside => peak_in(b, tl0, tl1) * t.volume.clamp(0.02, 3.0).sqrt(),
                            _ => {
                                let g = if t.has_fx() { ffcut_core::audiofx::gain_at(&t, (s0 + s1) * 0.5) } else { 1.0 };
                                peak_in(peaks, s0.max(0.0), s1.max(0.0)) * (t.volume * g).clamp(0.02, 3.0).sqrt()
                            }
                        };
                        let h = (a * half).clamp(0.5, half);
                        p.line_segment([pos2(x + 0.5, mid - h), pos2(x + 0.5, mid + h)], Stroke::new(1.0, if inside { on } else { off }));
                    }
                    x += 1.0;
                }
            }
            Some(Wave::Loading) | None => {
                p.text(track.center(), Align2::CENTER_CENTER, "reading waveform…", FontId::proportional(11.5), theme::TEXT_OFF);
            }
            Some(Wave::Failed) => {
                p.text(track.center(), Align2::CENTER_CENTER, "no waveform", FontId::proportional(11.5), theme::TEXT_OFF);
            }
        }
        drop(wave);

        if t.deleted {
            p.text(track.center(), Align2::CENTER_CENTER, "removed from export", FontId::proportional(12.0), theme::TEXT_LABEL);
        } else {
            let clip = Rect::from_min_max(pos2(xs.max(track.left()), track.top()), pos2(xe.min(track.right()), track.bottom()));
            if clip.width() > 0.0 {
                p.rect_stroke(clip, 4.0, Stroke::new(1.5, accent), StrokeKind::Inside);
                for x in [xs, xe] {
                    if x >= track.left() - 1.0 && x <= track.right() + 1.0 {
                        p.rect_filled(Rect::from_center_size(pos2(x, mid), vec2(5.0, track.height() - 6.0)), 2.0, accent);
                    }
                }
            }
            // fade ramps and their handles
            let top = track.top();
            if t.fade_in > 0.001 {
                let x1 = fi_x.min(track.right());
                p.add(egui::Shape::convex_polygon(
                    vec![pos2(xs.max(track.left()), top), pos2(x1, top), pos2(xs.max(track.left()), track.bottom())],
                    Color32::from_black_alpha(90),
                    Stroke::NONE,
                ));
                p.line_segment([pos2(xs, track.bottom()), pos2(fi_x, top)], Stroke::new(1.5, Color32::from_white_alpha(170)));
            }
            if t.fade_out > 0.001 {
                let x0 = fo_x.max(track.left());
                p.add(egui::Shape::convex_polygon(
                    vec![pos2(xe.min(track.right()), top), pos2(x0, top), pos2(xe.min(track.right()), track.bottom())],
                    Color32::from_black_alpha(90),
                    Stroke::NONE,
                ));
                p.line_segment([pos2(fo_x, top), pos2(xe, track.bottom())], Stroke::new(1.5, Color32::from_white_alpha(170)));
            }
            if !env_mode {
                for (x, on) in [(fi_x, t.fade_in > 0.001), (fo_x, t.fade_out > 0.001)] {
                    if x >= track.left() - 1.0 && x <= track.right() + 1.0 {
                        p.circle(
                            pos2(x, top + 7.0),
                            3.5,
                            if on { Color32::WHITE } else { Color32::from_white_alpha(60) },
                            Stroke::new(1.0, Color32::from_black_alpha(160)),
                        );
                    }
                }
            }
            // volume keyframes
            if env_mode || !t.envelope.is_empty() {
                if env_mode {
                    let y0 = y_of_db(0.0);
                    p.line_segment([pos2(track.left(), y0), pos2(track.right(), y0)], Stroke::new(1.0, Color32::from_white_alpha(40)));
                    p.text(pos2(track.right() - 4.0, y0 + 1.0), Align2::RIGHT_TOP, "0 dB", FontId::proportional(10.0), Color32::from_white_alpha(110));
                }
                if !t.envelope.is_empty() {
                    let mut pts: Vec<egui::Pos2> = t.envelope.iter().map(env_xy).collect();
                    if pts[0].x > xs {
                        pts.insert(0, pos2(xs, pts[0].y));
                    }
                    let last = pts[pts.len() - 1];
                    if last.x < xe {
                        pts.push(pos2(xe, last.y));
                    }
                    p.add(egui::Shape::line(pts, Stroke::new(1.8, theme::WARN)));
                    for pt in &t.envelope {
                        p.circle(env_xy(pt), 4.0, theme::WARN, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
            // the file name in the corner, like in video editors
            let label_x = xs.max(track.left()) + 14.0;
            let name = format!("{}{}", if matches!(t.source, TrackSource::External { .. }) { "+ " } else { "" }, t.label);
            let gal = p.layout_no_wrap(name, FontId::proportional(10.5), if heard { theme::TEXT_STRONG } else { theme::TEXT_DIM });
            let at = pos2(label_x, track.top() + 3.0);
            p.rect_filled(Rect::from_min_size(at, gal.size()).expand2(vec2(4.0, 1.0)), 3.0, Color32::from_black_alpha(150));
            p.galley(at, gal, Color32::WHITE);
            if t.shift.abs() > 0.001 {
                let gal = p.layout_no_wrap(format!("{:+.2}s", t.shift), FontId::proportional(10.5), theme::TEXT);
                let at = pos2(label_x, track.bottom() - 3.0 - gal.size().y);
                p.rect_filled(Rect::from_min_size(at, gal.size()).expand2(vec2(4.0, 1.0)), 3.0, Color32::from_black_alpha(150));
                p.galley(at, gal, Color32::WHITE);
            }
            if env_mode {
                p.text(
                    pos2(track.right() - 8.0, track.top() + 3.0),
                    Align2::RIGHT_TOP,
                    "keyframes: click adds, right-click = menu",
                    FontId::proportional(10.5),
                    theme::WARN,
                );
            }
        }

        // playhead
        let px = t2x(self.player.time().clamp(0.0, dur));
        p.line_segment([pos2(px, track.top()), pos2(px, track.bottom())], Stroke::new(1.5, Color32::WHITE));
    }
}
