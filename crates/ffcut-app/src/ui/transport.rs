use crate::app::{fmt_time, App, TlDrag};
use super::icons::{icon_button, Icon};
use super::meter::METER_W;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder};

/// Width of the left column of the multi-track view (track name and controls).
pub const HEADER_W: f32 = 180.0;

impl App {
    pub fn transport_panel(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        let frame = egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8));
        if self.multitrack {
            // taller on demand: with many audio tracks the user drags the top edge up
            // no taller than needed to show every track (the video keeps the rest)
            let screen = ui.ctx().content_rect().height();
            let lanes = self.s.tracks.len().max(1) as f32;
            let fit = 16.0 + 36.0 + 54.0 + lanes * (super::tracks::LANE_H + 5.0) + 40.0;
            let max = fit.min(screen * 0.8).max(210.0);
            egui::Panel::bottom("transport_tracks")
                .frame(frame)
                .resizable(true)
                .default_size(fit.min(330.0))
                .size_range(210.0..=max)
                .show(ui, |ui| {
                    ui.set_min_height(ui.available_height()); // fill the panel, so the size is what the user dragged
                    self.controls(ui);
                    ui.add_space(6.0);
                    let avail = ui.available_rect_before_wrap();
                    let mrect = Rect::from_min_max(pos2(avail.right() - METER_W, avail.top() + 22.0), pos2(avail.right(), avail.bottom() - 40.0));
                    let body = Rect::from_min_max(avail.min, pos2(avail.right() - METER_W - 8.0, avail.bottom()));
                    ui.scope_builder(UiBuilder::new().max_rect(body), |ui| self.tracks_view(ui));
                    self.level_meter(ui, mrect);
                });
        } else {
            egui::Panel::bottom("transport").frame(frame).show(ui, |ui| {
                self.controls(ui);
                ui.add_space(6.0);
                let avail = ui.available_rect_before_wrap();
                let mrect = Rect::from_min_max(pos2(avail.right() - METER_W, avail.top() + 22.0), pos2(avail.right(), avail.top() + 62.0 - 6.0));
                let body = Rect::from_min_max(avail.min, pos2(avail.right() - METER_W - 8.0, avail.bottom()));
                ui.scope_builder(UiBuilder::new().max_rect(body), |ui| self.timeline(ui, 62.0));
                self.level_meter(ui, mrect);
            });
        }
    }

    /// [loop, time]   [crop, ⏮ ◀ ▶ ▶| ⏭, tracks]   [audio track, volume]
    fn controls(&mut self, ui: &mut egui::Ui) {
        let dur = self.dur();
        let t = self.player.time();
        let h = 30.0;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        let centre = Rect::from_center_size(rect.center(), vec2(320.0, h));
        let left = Rect::from_min_max(rect.min, pos2((centre.left() - 10.0).max(rect.left()), rect.max.y));
        let right = Rect::from_min_max(pos2((centre.right() + 10.0).min(rect.right()), rect.min.y), rect.max);
        let lay = |l: Layout, r: Rect| UiBuilder::new().max_rect(r).layout(l);

        // ---- left: loop + time (click the time to switch to frame numbers)
        ui.scope_builder(lay(Layout::left_to_right(Align::Center), left), |ui| {
            ui.set_clip_rect(left);
            if icon_button(ui, vec2(34.0, 28.0), Icon::Loop, self.loop_on, None).on_hover_text("Loop the selection  (A)").clicked() {
                self.loop_on = !self.loop_on;
                self.sync_loop();
            }
            ui.add_space(6.0);
            let fps = self.clip.as_ref().map_or(0.0, |c| c.info.fps);
            let frames = |x: f64| (x * fps).round() as i64;
            let as_time = format!("{} / {}", fmt_time(t), fmt_time(dur));
            let as_frames = format!("{} / {}", frames(t), frames(dur));
            let (shown, other, unit) = if self.settings.show_frames && fps > 0.0 { (as_frames, as_time, "frames") } else { (as_time, as_frames, "time") };
            let tip = if fps > 0.0 { format!("Showing {unit}. Click to switch.\n{other}") } else { "No frame rate known: showing time".to_string() };
            let r = ui.add(egui::Label::new(RichText::new(shown).monospace().size(13.0)).sense(Sense::click())).on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() && fps > 0.0 {
                self.settings.show_frames = !self.settings.show_frames;
                self.settings.save();
            }
        });

        // ---- centre: crop | media buttons | tracks
        ui.scope_builder(lay(Layout::left_to_right(Align::Center), centre), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if icon_button(ui, vec2(34.0, 28.0), Icon::Crop, self.crop_on, None).on_hover_text("Crop  (C)").clicked() {
                self.crop_on = !self.crop_on;
                if self.crop_on {
                    let c = self.crop_rect();
                    if c.w < 4 || c.h < 4 {
                        self.reset_crop();
                    }
                }
            }
            ui.add_space(8.0);
            let sz = vec2(34.0, 28.0);
            if icon_button(ui, sz, Icon::ToStart, false, None).on_hover_text("Go to trim start").clicked() {
                self.player.seek(self.trim_start);
            }
            if icon_button(ui, sz, Icon::StepBack, false, None).on_hover_text("Previous frame  (←)").clicked() {
                self.player.step_frame(false);
            }
            let icon = if self.player.paused() { Icon::Play } else { Icon::Pause };
            if icon_button(ui, vec2(46.0, 28.0), icon, false, Some(theme::ACCENT.gamma_multiply(0.75))).on_hover_text("Play / pause  (Space)").clicked() {
                self.play_pause();
            }
            if icon_button(ui, sz, Icon::StepForward, false, None).on_hover_text("Next frame  (→)").clicked() {
                self.player.step_frame(true);
            }
            if icon_button(ui, sz, Icon::ToEnd, false, None).on_hover_text("Go to trim end").clicked() {
                self.player.seek(self.trim_end);
            }
            ui.add_space(8.0);
            if icon_button(ui, vec2(34.0, 28.0), Icon::Tracks, self.multitrack, None).on_hover_text("Switch the timeline: one bar / one lane per audio track").clicked() {
                self.multitrack = !self.multitrack;
            }
        });

        // ---- right: preview audio track (hidden in track view) + volume
        ui.scope_builder(lay(Layout::right_to_left(Align::Center), right), |ui| {
            ui.set_clip_rect(right);
            ui.spacing_mut().slider_width = 92.0;
            let mut vol = self.volume;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=130.0).show_value(false).trailing_fill(true)).changed() {
                self.volume = vol;
                self.player.set_volume(vol as f64);
            }
            let icon = if self.muted { "🔇" } else { "🔊" };
            if ui.add(egui::Button::new(icon).selected(self.muted)).clicked() {
                self.muted = !self.muted;
                self.player.set_mute(self.muted);
            }
            let tracks = self.clip.as_ref().map(|c| c.info.audio.clone()).unwrap_or_default();
            if !self.multitrack && tracks.len() > 1 {
                ui.add_space(12.0);
                let cur = format!("Audio #{}", self.preview_audio + 1);
                egui::ComboBox::from_id_salt("preview_audio").selected_text(cur).width(96.0).show_ui(ui, |ui| {
                    for a in &tracks {
                        // applied by `sync_preview_audio`
                        ui.selectable_value(&mut self.preview_audio, a.index, super::audio_label(a));
                    }
                });
            }
        });
    }

    /// Track with trim handles. Click/drag = seek; drag a handle = move trim start/end.
    pub(super) fn timeline(&mut self, ui: &mut egui::Ui, height: f32) {
        let dur = self.dur().max(0.001);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        let track = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 22.0), pos2(rect.right() - 10.0, rect.bottom() - 6.0));
        let t2x = |t: f64| track.left() + (t / dur) as f32 * track.width();
        let x2t = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64) * dur;
        let (xs, xe) = (t2x(self.trim_start), t2x(self.trim_end));

        // ---- interaction
        if resp.drag_started() {
            // decide by where the mouse went DOWN, not where it is after the drag threshold
            if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                self.tl_drag = Some(if !self.lock_trim && (p.x - xs).abs() <= 10.0 && (p.x - xs).abs() <= (p.x - xe).abs() {
                    TlDrag::Start
                } else if !self.lock_trim && (p.x - xe).abs() <= 10.0 {
                    TlDrag::End
                } else {
                    TlDrag::Seek
                });
            }
        }
        if resp.dragged() {
            if let (Some(p), Some(mode)) = (resp.interact_pointer_pos(), self.tl_drag) {
                let t = x2t(p.x);
                match mode {
                    TlDrag::Seek => self.player.seek_fast(t),
                    TlDrag::Start => {
                        self.trim_start = t.min(self.trim_end - 0.05).max(0.0);
                        self.player.seek_fast(self.trim_start);
                    }
                    TlDrag::End => {
                        self.trim_end = t.max(self.trim_start + 0.05).min(dur);
                        self.player.seek_fast(self.trim_end);
                    }
                }
            }
        }
        if resp.drag_stopped() {
            match self.tl_drag.take() {
                Some(TlDrag::Seek) => {
                    if let Some(p) = resp.interact_pointer_pos() {
                        self.player.seek(x2t(p.x));
                    }
                }
                Some(TlDrag::Start) => {
                    self.player.seek(self.trim_start);
                    self.sync_loop();
                }
                Some(TlDrag::End) => {
                    self.player.seek(self.trim_end);
                    self.sync_loop();
                }
                None => {}
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.player.seek(x2t(p.x));
            }
        }
        // cursor feedback
        if let Some(p) = resp.hover_pos() {
            if !self.lock_trim && ((p.x - xs).abs() <= 10.0 || (p.x - xe).abs() <= 10.0) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }

        // ---- painting
        let p = ui.painter_at(rect);
        // ticks
        let step = [0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0]
            .into_iter()
            .find(|s| (*s / dur) as f32 * track.width() >= 70.0)
            .unwrap_or(3600.0);
        let mut tk = 0.0;
        while tk <= dur {
            let x = t2x(tk);
            p.line_segment([pos2(x, rect.top() + 14.0), pos2(x, rect.top() + 20.0)], Stroke::new(1.0, Color32::from_gray(110)));
            if x < track.right() - 34.0 {
            p.text(pos2(x + 3.0, rect.top() + 1.0), Align2::LEFT_TOP, short_time(tk), FontId::proportional(10.5), Color32::from_gray(140));
            }
            tk += step;
        }
        p.rect_filled(track, 5.0, theme::TRACK);
        // selected range
        let sel = Rect::from_min_max(pos2(xs, track.top()), pos2(xe, track.bottom()));
        p.rect_filled(sel, 4.0, theme::ACCENT.gamma_multiply(0.38));
        p.rect_stroke(sel, 4.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
        // handles
        for (x, is_start) in [(xs, true), (xe, false)] {
            let hr = Rect::from_center_size(pos2(x, track.center().y), vec2(10.0, track.height() + 6.0));
            let col = if self.lock_trim { Color32::from_gray(120) } else { theme::ACCENT };
            p.rect_filled(hr, 3.0, col);
            let cx = hr.center().x;
            for dx in [-1.5_f32, 1.5] {
                p.line_segment([pos2(cx + dx, hr.center().y - 6.0), pos2(cx + dx, hr.center().y + 6.0)], Stroke::new(1.0, Color32::from_black_alpha(130)));
            }
            let _ = is_start;
        }
        // playhead
        let px = t2x(self.player.time().clamp(0.0, dur));
        p.line_segment([pos2(px, track.top() - 4.0), pos2(px, track.bottom() + 2.0)], Stroke::new(2.0, Color32::WHITE));
        p.add(egui::Shape::convex_polygon(
            vec![pos2(px - 5.0, track.top() - 8.0), pos2(px + 5.0, track.top() - 8.0), pos2(px, track.top() - 1.0)],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
}

fn short_time(t: f64) -> String {
    let s = t.round() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
