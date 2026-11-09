use super::icons::{icon_button, Icon};
use super::meter::METER_W;
use super::widgets;
use crate::app::{fmt_time, App, TlDrag};
use crate::hotkeys::Action;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder};
use ffcut_core::AudioStream;

/// Width of the left column of the multi-track view (track name and controls).
pub const HEADER_W: f32 = 180.0;

impl App {
    pub fn transport_panel(&mut self, ui: &mut egui::Ui) {
        if self.clip.is_none() {
            return;
        }
        let frame = egui::Frame::NONE.fill(theme::panel()).inner_margin(egui::Margin::symmetric(12, 8));
        if self.multitrack {
            // taller on demand: with many audio tracks the user drags the top edge up
            // no taller than needed to show every track (the video keeps the rest)
            let screen = ui.ctx().content_rect().height();
            // removed tracks are thin strips, so they count less
            let lanes: f32 = if self.s.tracks.is_empty() {
                super::tracks::LANE_H + 5.0
            } else {
                self.s.tracks.iter().map(|t| if t.deleted { super::tracks::DEL_H } else { super::tracks::LANE_H } + 5.0).sum()
            };
            let fit = 16.0 + 36.0 + 54.0 + lanes + 40.0 + 6.0; // + a few pixels: the sum is a hair short, and the list would scroll
            let max = fit.min(screen * 0.8).max(210.0);
            egui::Panel::bottom("transport_tracks").frame(frame).resizable(true).default_size(fit.min(330.0)).size_range(210.0..=max).show(ui, |ui| {
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

    /// The size the video preview runs at while effects are on: smaller keeps heavy effects real-time.
    /// A button that shows the chosen size as a pictogram (finer or coarser pixels) and opens the list.
    fn preview_quality(&mut self, ui: &mut egui::Ui) {
        let div = self.settings.preview_div();
        let tip = format!("Preview quality: {}. Click to change.\n\n{QUALITY_TIP}", quality_label(div));
        let r = icon_button(ui, vec2(34.0, 28.0), Icon::Quality(div), false, None).on_hover_text(tip);
        widgets::menu_for(&r, |ui| self.quality_items(ui));
    }

    fn quality_items(&mut self, ui: &mut egui::Ui) {
        let cur = self.settings.preview_div();
        for d in [1, 2, 4] {
            // room on the left for the pictogram of the button
            let r = ui.selectable_label(cur == d, format!("     {}", quality_label(d)));
            let col = if cur == d { ui.visuals().selection.stroke.color } else { ui.style().interact(&r).text_color() };
            super::icons::draw(ui.painter(), Icon::Quality(d), pos2(r.rect.left() + 15.0, r.rect.center().y), 9.0, col);
            if r.clicked() && cur != d {
                self.settings.preview_div = d;
                self.settings.save();
            }
        }
    }

    fn audio_items(&mut self, ui: &mut egui::Ui, tracks: &[AudioStream]) {
        for a in tracks {
            // applied by `sync_preview_audio`
            ui.selectable_value(&mut self.preview_audio, a.index, super::audio_label(a));
        }
    }

    fn volume_slider(&mut self, ui: &mut egui::Ui) {
        let mut vol = self.volume;
        if ui.add(egui::Slider::new(&mut vol, 0.0..=130.0).show_value(false).trailing_fill(true)).changed() {
            self.volume = vol;
            self.player.set_volume(vol as f64);
        }
    }

    /// [loop, time]   [crop, frame, ⏮ ◀ ▶ ▶| ⏭, preview quality, tracks]   [audio track, volume]
    ///
    /// When the bar is narrow the sides give way in steps: the volume slider gets shorter, the time drops
    /// the duration, the audio track moves into a "⋯" menu, then the preview quality, then the slider too.
    /// The centre moves off the middle before anything has to go.
    fn controls(&mut self, ui: &mut egui::Ui) {
        let dur = self.dur();
        let t = self.player.time();
        let h = 30.0;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        let audio = self.audio_mode();
        let tracks = self.clip.as_ref().map(|c| c.info.audio.clone()).unwrap_or_default();
        let show_tracks = !self.multitrack && !audio && tracks.len() > 1;
        let show_quality = !audio;

        let fps = self.clip.as_ref().map_or(0.0, |c| c.info.fps);
        let frames = |x: f64| (x * fps).round() as i64;
        let in_frames = self.settings.show_frames && fps > 0.0;
        let (cur, total) = if in_frames { (frames(t).to_string(), frames(dur).to_string()) } else { (fmt_time(t), fmt_time(dur)) };

        // ---- what fits
        let sp = ui.spacing().item_spacing.x;
        let font = FontId::monospace(13.0);
        let text_w = |s: String, f: FontId| ui.painter().layout_no_wrap(s, f, Color32::WHITE).size().x;
        // measured on the duration: the current time never gets wider, so the layout does not flicker while playing
        let long_w = text_w(format!("{total} / {total}"), font.clone());
        let short_w = text_w(total.clone(), font);
        let mute_w = text_w("🔊".into(), egui::TextStyle::Button.resolve(ui.style())) + 2.0 * ui.spacing().button_padding.x;
        let left_need = |short: bool| 34.0 + sp + 6.0 + if short { short_w } else { long_w } + 4.0;
        let right_need = |fit: &Fit| {
            let mut w = mute_w + sp;
            if !fit.vol_in_more {
                w += fit.vol_w + sp;
            }
            if fit.more {
                w += 12.0 + 34.0 + sp;
            } else if show_tracks {
                w += 12.0 + 96.0 + sp;
            }
            w + 4.0
        };
        // the preview quality sits in the centre, before the timeline switch (and the spacing after it)
        // at the narrowest the groups inside the centre lose the space between them (2 × 8)
        let centre_w = |fit: &Fit| {
            let w = if audio { 210.0 } else { 344.0 + if show_quality && fit.quality_centre { 34.0 + 6.0 } else { 0.0 } };
            if fit.tight && !audio {
                w - 16.0
            } else {
                w
            }
        };
        let gap = 10.0;
        let steps = [
            Fit { short_time: false, vol_w: 92.0, more: false, vol_in_more: false, quality_centre: true, tight: false },
            Fit { short_time: false, vol_w: 60.0, more: false, vol_in_more: false, quality_centre: true, tight: false },
            Fit { short_time: true, vol_w: 60.0, more: false, vol_in_more: false, quality_centre: true, tight: false },
            Fit { short_time: true, vol_w: 60.0, more: show_tracks, vol_in_more: false, quality_centre: true, tight: false },
            Fit { short_time: true, vol_w: 60.0, more: show_tracks || show_quality, vol_in_more: false, quality_centre: false, tight: false },
            Fit { short_time: true, vol_w: 0.0, more: true, vol_in_more: true, quality_centre: false, tight: false },
            Fit { short_time: true, vol_w: 0.0, more: true, vol_in_more: true, quality_centre: false, tight: true },
        ];
        let fits = |f: &Fit| left_need(f.short_time) + centre_w(f) + right_need(f) + 2.0 * gap <= rect.width();
        let fit = steps.iter().find(|f| fits(f)).unwrap_or(&steps[steps.len() - 1]);
        let (lneed, rneed, cw) = (left_need(fit.short_time), right_need(fit), centre_w(fit));
        let quality_centre = show_quality && fit.quality_centre;
        let group_gap = if fit.tight { 0.0 } else { 8.0 };
        let mid = rect.center().x - cw / 2.0;
        let (lo, hi) = (rect.left() + lneed + gap, rect.right() - rneed - gap - cw);
        let cx = if lo <= hi { mid.clamp(lo, hi) } else { mid };
        let centre = Rect::from_min_size(pos2(cx, rect.top()), vec2(cw, h));
        let left = Rect::from_min_max(rect.min, pos2((centre.left() - gap).max(rect.left()), rect.max.y));
        let right = Rect::from_min_max(pos2((centre.right() + gap).min(rect.right()), rect.min.y), rect.max);
        let lay = |l: Layout, r: Rect| UiBuilder::new().max_rect(r).layout(l);

        // ---- left: loop + time (click the time to switch to frame numbers)
        ui.scope_builder(lay(Layout::left_to_right(Align::Center), left), |ui| {
            ui.set_clip_rect(left);
            if icon_button(ui, vec2(34.0, 28.0), Icon::Loop, self.loop_on, None).on_hover_text(self.tip("Loop the selection", Action::Loop)).clicked() {
                self.loop_on = !self.loop_on;
                self.sync_loop();
            }
            ui.add_space(6.0);
            let as_time = format!("{} / {}", fmt_time(t), fmt_time(dur));
            let as_frames = format!("{} / {}", frames(t), frames(dur));
            let shown = if fit.short_time { cur.clone() } else { format!("{cur} / {total}") };
            let unit = if in_frames { "frames" } else { "time" };
            let tip = if fps <= 0.0 {
                format!("No frame rate known: showing time\n{as_time}")
            } else if fit.short_time {
                format!("Showing {unit}. Click to switch.\n{as_time}\nFrame {as_frames}")
            } else {
                format!("Showing {unit}. Click to switch.\n{}", if in_frames { as_time } else { as_frames })
            };
            let r = ui
                .add(egui::Label::new(RichText::new(shown).monospace().size(13.0)).sense(Sense::click()))
                .on_hover_text(tip)
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() && fps > 0.0 {
                self.settings.show_frames = !self.settings.show_frames;
                self.settings.save();
            }
        });
        // ---- centre: crop, frame | media buttons | preview quality, tracks
        let group = ui.scope_builder(lay(Layout::left_to_right(Align::Center), centre), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if !audio {
                if icon_button(ui, vec2(34.0, 28.0), Icon::Crop, self.crop.on, None).on_hover_text(self.tip("Crop", Action::Crop)).clicked() {
                    self.set_crop(!self.crop.on);
                }
                let tip = self.tip("Save this frame as a picture (with crop, rotation and effects)", Action::ExportFrame);
                if icon_button(ui, vec2(34.0, 28.0), Icon::Frame, false, None).on_hover_text(tip).clicked() {
                    self.export_frame(ui.ctx());
                }
                ui.add_space(group_gap);
            }
            let sz = vec2(34.0, 28.0);
            if icon_button(ui, sz, Icon::ToStart, false, None).on_hover_text("Go to trim start").clicked() {
                self.player.seek(self.trim_start);
            }
            if icon_button(ui, sz, Icon::StepBack, false, None)
                .on_hover_text(self.tip(if audio { "Back 1 s" } else { "Previous frame" }, Action::StepBack))
                .clicked()
            {
                if audio {
                    self.player.seek((self.player.time() - 1.0).max(0.0));
                } else {
                    self.player.step_frame(false);
                }
            }
            let icon = if self.player.paused() { Icon::Play } else { Icon::Pause };
            let fill = if super::deco::on() { theme::accent() } else { theme::accent().gamma_multiply(0.75) };
            if icon_button(ui, vec2(46.0, 28.0), icon, false, Some(fill)).on_hover_text(self.tip("Play / pause", Action::PlayPause)).clicked() {
                self.play_pause();
            }
            if icon_button(ui, sz, Icon::StepForward, false, None)
                .on_hover_text(self.tip(if audio { "Forward 1 s" } else { "Next frame" }, Action::StepForward))
                .clicked()
            {
                if audio {
                    self.player.seek(self.player.time() + 1.0);
                } else {
                    self.player.step_frame(true);
                }
            }
            if icon_button(ui, sz, Icon::ToEnd, false, None).on_hover_text("Go to trim end").clicked() {
                self.player.seek(self.trim_end);
            }
            if !audio {
                ui.add_space(group_gap);
                if quality_centre {
                    self.preview_quality(ui);
                }
                if icon_button(ui, vec2(34.0, 28.0), Icon::Tracks, self.multitrack, None)
                    .on_hover_text("Switch the timeline: one bar / one lane per audio track")
                    .clicked()
                {
                    self.multitrack = !self.multitrack;
                }
            }
        });
        // the group is as tall as the row, the buttons in it 28 px: measured from the buttons, the
        // ornaments keep the same distance from them on every side
        let g = group.response.rect;
        let g = Rect::from_center_size(g.center(), vec2(g.width(), 28.0));
        match theme::kind() {
            theme::Kind::Standard => {}
            theme::Kind::Brutalist => super::deco::brackets(ui.painter(), g.expand(4.0), 5.0, Stroke::new(1.0, theme::text_label())),
            theme::Kind::Y2k => {
                // the media buttons sit in one capsule, like the controls of a player module
                let cap = g.expand(3.0);
                ui.painter().rect_stroke(cap, cap.height() / 2.0, Stroke::new(1.0, theme::line_faint()), StrokeKind::Outside);
            }
        }

        // ---- right: preview audio track (hidden in track view), volume
        ui.scope_builder(lay(Layout::right_to_left(Align::Center), right), |ui| {
            ui.set_clip_rect(right);
            if !fit.vol_in_more {
                ui.spacing_mut().slider_width = fit.vol_w;
                self.volume_slider(ui);
            }
            let icon = if self.muted { "🔇" } else { "🔊" };
            if ui.add(egui::Button::new(icon).selected(self.muted)).on_hover_text("Mute").clicked() {
                self.muted = !self.muted;
                self.player.set_mute(self.muted);
            }
            if fit.more {
                ui.add_space(12.0);
                let in_more = show_quality && !quality_centre;
                let what: Vec<&str> = [(show_tracks, "audio track"), (in_more, "preview quality"), (fit.vol_in_more, "volume")]
                    .into_iter()
                    .filter_map(|(on, w)| on.then_some(w))
                    .collect();
                let more = icon_button(ui, vec2(34.0, 28.0), Icon::More, false, None).on_hover_text(format!("More: {}", what.join(", ")));
                widgets::form_menu_for(&more, |ui| {
                    let head = |ui: &mut egui::Ui, s: &str| widgets::menu_indent(ui, |ui| ui.label(RichText::new(s).strong()));
                    let mut first = true;
                    let mut sep = |ui: &mut egui::Ui| {
                        if !std::mem::take(&mut first) {
                            ui.separator();
                        }
                    };
                    if fit.vol_in_more {
                        sep(ui);
                        head(ui, "Volume");
                        widgets::menu_indent(ui, |ui| {
                            ui.spacing_mut().slider_width = 140.0;
                            self.volume_slider(ui);
                        });
                    }
                    if show_tracks {
                        sep(ui);
                        head(ui, "Audio track");
                        self.audio_items(ui, &tracks);
                    }
                    if in_more {
                        sep(ui);
                        head(ui, "Preview quality").on_hover_text(QUALITY_TIP);
                        self.quality_items(ui);
                    }
                });
            } else if show_tracks {
                ui.add_space(12.0);
                let cur = format!("Audio #{}", self.preview_audio + 1);
                widgets::Combo::from_id_salt("preview_audio").selected_text(cur).width(96.0).show_list(ui, |ui| {
                    self.audio_items(ui, &tracks);
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
                self.tl_drag = Some(if (p.x - xs).abs() <= 10.0 && (p.x - xs).abs() <= (p.x - xe).abs() {
                    TlDrag::Start
                } else if (p.x - xe).abs() <= 10.0 {
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
        // right-click on a handle: exact value as a time code or a frame number
        if resp.secondary_clicked() {
            self.trim_menu = resp.interact_pointer_pos().and_then(|p| {
                let (ds, de) = ((p.x - xs).abs(), (p.x - xe).abs());
                (ds.min(de) <= 12.0).then_some(ds <= de)
            });
        }
        super::ctx_menu(&resp, |ui| {
            let Some(is_start) = self.trim_menu else {
                ui.close();
                return;
            };
            let (lo, hi) = if is_start { (0.0, (self.trim_end - 0.05).max(0.0)) } else { (self.trim_start + 0.05, dur) };
            let mut v = if is_start { self.trim_start } else { self.trim_end };
            let fps = self.clip.as_ref().map_or(0.0, |c| c.info.fps);
            super::widgets::menu_indent(ui, |ui| ui.label(RichText::new(if is_start { "Trim start" } else { "Trim end" }).strong()));
            let mut changed = false;
            super::widgets::menu_indent(ui, |ui| {
                super::grid("trim-menu").min_col_width(0.0).show(ui, |ui| {
                    ui.label("Timecode");
                    changed |= super::side::time_field(ui, &mut v, lo, hi, None).changed();
                    ui.end_row();
                    if fps > 0.0 {
                        ui.label("Frame");
                        changed |= super::side::time_field(ui, &mut v, lo, hi, Some(fps)).changed();
                        ui.end_row();
                    }
                })
            });
            if ui.button("Set to playhead").clicked() {
                v = self.player.time().clamp(lo, hi);
                changed = true;
                ui.close();
            }
            if changed {
                v = v.clamp(lo, hi);
                if is_start {
                    self.trim_start = v;
                } else {
                    self.trim_end = v;
                }
                self.player.seek(v);
                self.sync_loop();
            }
        });
        // cursor feedback
        if let Some(p) = resp.hover_pos() {
            if (p.x - xs).abs() <= 10.0 || (p.x - xe).abs() <= 10.0 {
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
        if super::deco::on() {
            // minor ticks between the labelled ones, like a ruler
            let minor = step / 5.0;
            let mut tk = minor;
            while tk <= dur {
                let x = t2x(tk);
                p.line_segment([pos2(x, rect.top() + 17.0), pos2(x, rect.top() + 20.0)], Stroke::new(1.0, theme::line_faint()));
                tk += minor;
            }
        }
        let mut tk = 0.0;
        while tk <= dur {
            let x = t2x(tk);
            p.line_segment([pos2(x, rect.top() + 14.0), pos2(x, rect.top() + 20.0)], Stroke::new(1.0, theme::text_off()));
            if x < track.right() - 34.0 {
                p.text(pos2(x + 3.0, rect.top() + 1.0), Align2::LEFT_TOP, short_time(tk), FontId::proportional(theme::size::SMALL), theme::text_dim());
            }
            tk += step;
        }
        p.rect_filled(track, theme::radius::of(5.0), theme::track());
        // selected range
        let sel = Rect::from_min_max(pos2(xs, track.top()), pos2(xe, track.bottom()));
        p.rect_filled(sel, theme::radius::of(4.0), theme::accent().gamma_multiply(0.38));
        p.rect_stroke(sel, theme::radius::of(4.0), Stroke::new(1.5, theme::accent()), StrokeKind::Inside);
        // handles
        for x in [xs, xe] {
            let hr = Rect::from_center_size(pos2(x, track.center().y), vec2(10.0, track.height() + 6.0));
            p.rect_filled(hr, theme::radius::of(3.0), theme::accent());
            let cx = hr.center().x;
            for dx in [-1.5_f32, 1.5] {
                p.line_segment([pos2(cx + dx, hr.center().y - 6.0), pos2(cx + dx, hr.center().y + 6.0)], Stroke::new(1.0, Color32::from_black_alpha(130)));
            }
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
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

const QUALITY_TIP: &str = "Preview quality: the size the video preview runs at while video effects are on. \
                           A smaller one keeps heavy effects playing smoothly. The export always uses the full size.";

fn quality_label(div: u32) -> &'static str {
    match div {
        2 => "1/2 of the size",
        4 => "1/4 of the size",
        _ => "Full size",
    }
}

/// One step of [`App::controls`] giving way on a narrow bar.
struct Fit {
    short_time: bool,
    vol_w: f32,
    /// The audio track (and the preview quality, when it is not in the centre) sit in the "⋯" menu.
    more: bool,
    vol_in_more: bool,
    /// The preview quality is in the centre group; otherwise in the "⋯" menu.
    quality_centre: bool,
    /// No extra space between the groups of buttons in the centre.
    tight: bool,
}
