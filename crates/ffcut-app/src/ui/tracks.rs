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

pub const LANE_H: f32 = 58.0;
const EDGE: f32 = 8.0;

enum Act {
    Delete(usize),
    Restore(usize),
    Replace(usize),
    Reset(usize),
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
                let dim = Color32::from_gray(140);
                ui.label(RichText::new(format!("{} → {}", fmt_time(self.trim_start), fmt_time(end))).monospace().size(11.5).color(Color32::from_gray(200)));
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
            for i in 0..n {
                ui.horizontal(|ui| {
                    self.track_header(ui, i, heard[i], &mut act);
                    self.track_lane(ui, &ctx, i, heard[i]);
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
            ui.add(egui::Label::new(RichText::new("All audible tracks play together in the preview. Trim and shift apply on export. Drag the panel edge up for more room.").size(11.5).color(Color32::from_gray(120))).truncate());
        });

        match act {
            Some(Act::Delete(i)) => {
                if matches!(self.s.tracks[i].source, TrackSource::External { .. }) {
                    self.s.tracks.remove(i); // an added file has nothing to restore
                } else {
                    self.s.tracks[i].deleted = true;
                }
            }
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
            }
            None => {}
        }
    }

    /// Left column of a lane. It has a fixed size and clips its content: a long file name or an extra
    /// button can no longer push the waveform to the right. Buttons are icons to save room.
    fn track_header(&mut self, ui: &mut egui::Ui, i: usize, heard: bool, act: &mut Option<Act>) {
        let (rect, head) = ui.allocate_exact_size(vec2(HEADER_W, LANE_H), Sense::click());
        head.context_menu(|ui| {
            if ui.button("Reset this track (trim, shift, volume, mute, solo)").clicked() {
                *act = Some(Act::Reset(i));
                ui.close();
            }
        });
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        let clip = rect.intersect(ui.clip_rect());
        ui.set_clip_rect(clip);
        ui.spacing_mut().item_spacing = vec2(4.0, 5.0);
        ui.spacing_mut().button_padding = vec2(2.0, 1.0);

        let t = &mut self.s.tracks[i];
        let ext_path = if let TrackSource::External { path, .. } = &t.source { Some(path.clone()) } else { None };
        let col = if t.deleted || !heard { Color32::from_gray(105) } else { Color32::from_gray(205) };
        let name = format!("{}{}", if ext_path.is_some() { "+ " } else { "" }, t.label);
        let tip = match &ext_path {
            Some(p) => format!("{}\n{p}", t.label),
            None => t.label.clone(),
        };
        ui.add(egui::Label::new(RichText::new(name).size(12.0).color(col)).truncate()).on_hover_text(tip);

        let icon = |glyph: &str| RichText::new(glyph).size(13.0);
        let letter = |l: &str| RichText::new(l).size(12.5).strong();
        let sz = vec2(26.0, 24.0);
        ui.horizontal(|ui| {
            if t.deleted {
                ui.label(RichText::new("removed").size(11.5).color(Color32::from_gray(120)));
                if ui.add_sized(sz, egui::Button::new(icon("↩"))).on_hover_text("Put this track back into the export").clicked() {
                    *act = Some(Act::Restore(i));
                }
                return;
            }
            // volume · replace · delete · solo · mute
            let mut pct = t.volume * 100.0;
            if ui.add_sized(vec2(58.0, 24.0), DragValue::new(&mut pct).range(0.0..=300.0).speed(1.0).suffix("%").fixed_decimals(0)).on_hover_text("Track volume (drag or type)").changed() {
                t.volume = pct / 100.0;
            }
            if ui.add_sized(sz, egui::Button::new(icon("⟳"))).on_hover_text("Replace the sound of this track with another file").clicked() {
                *act = Some(Act::Replace(i));
            }
            if ui.add_sized(sz, egui::Button::new(icon("🗑"))).on_hover_text("Remove this track from the export").clicked() {
                *act = Some(Act::Delete(i));
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

    fn track_lane(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, i: usize, heard: bool) {
        let dur = self.dur().max(0.001);
        let t: TrackEdit = self.s.tracks[i].clone();
        self.want_wave(ctx, &t.source);

        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), LANE_H), Sense::click_and_drag());
        let track = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 2.0), pos2(rect.right() - 10.0, rect.bottom() - 2.0));
        let t2x = |s: f64| track.left() + (s / dur) as f32 * track.width();
        let x2t = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64) * dur;
        let src_dur = t.source_duration(dur);
        let (pa, pb) = t.placed();
        let (xs, xe) = (t2x(pa), t2x(pb));

        // ---- interaction
        if !t.deleted {
            if resp.drag_started() {
                if let Some(p) = ui.input(|inp| inp.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                    let mode = if (p.x - xs).abs() <= EDGE && (p.x - xs).abs() <= (p.x - xe).abs() {
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
                let cur = if (p.x - xs).abs() <= EDGE || (p.x - xe).abs() <= EDGE {
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
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.player.seek(x2t(p.x));
            }
        }

        // ---- painting
        let p = ui.painter_at(rect);
        p.rect_filled(track, 5.0, theme::TRACK);
        let mid = track.center().y;
        let half = track.height() * 0.5 - 4.0;
        let accent = if matches!(t.source, TrackSource::External { .. }) { theme::CROP } else { theme::OK };
        let (on, off) = (accent, Color32::from_gray(85));
        let (on, off) = if t.deleted { (Color32::from_gray(70), Color32::from_gray(55)) } else if !heard { (Color32::from_gray(105), Color32::from_gray(70)) } else { (on, off) };

        let wave = self.waves.lock().unwrap();
        match wave.get(&self.wave_key(&t.source)) {
            Some(Wave::Ready(peaks)) => {
                // one column per pixel, over the part of the source that is on screen
                let x0 = t2x(t.shift).max(track.left()).floor();
                let x1 = t2x(t.shift + src_dur).min(track.right()).ceil();
                let mut x = x0;
                while x < x1 {
                    let (s0, s1) = (x2t(x) - t.shift, x2t(x + 1.0) - t.shift);
                    if s1 >= 0.0 && s0 <= src_dur {
                        let a = peak_in(peaks, s0.max(0.0), s1.max(0.0)) * (t.volume.max(0.05)).min(3.0).sqrt();
                        let h = (a * half).clamp(0.5, half);
                        let inside = s0 >= t.trim_start - 1e-6 && s1 <= t.trim_end + 1e-6;
                        p.line_segment([pos2(x + 0.5, mid - h), pos2(x + 0.5, mid + h)], Stroke::new(1.0, if inside { on } else { off }));
                    }
                    x += 1.0;
                }
            }
            Some(Wave::Loading) | None => {
                p.text(track.center(), Align2::CENTER_CENTER, "reading waveform…", FontId::proportional(11.5), Color32::from_gray(120));
            }
            Some(Wave::Failed) => {
                p.text(track.center(), Align2::CENTER_CENTER, "no waveform", FontId::proportional(11.5), Color32::from_gray(120));
            }
        }
        drop(wave);

        if t.deleted {
            p.text(track.center(), Align2::CENTER_CENTER, "removed from export", FontId::proportional(12.0), Color32::from_gray(150));
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
            if t.shift.abs() > 0.001 {
                p.text(pos2(track.left() + 6.0, track.top() + 3.0), Align2::LEFT_TOP, format!("{:+.2}s", t.shift), FontId::proportional(10.5), Color32::from_gray(190));
            }
        }

        // playhead
        let px = t2x(self.player.time().clamp(0.0, dur));
        p.line_segment([pos2(px, track.top()), pos2(px, track.bottom())], Stroke::new(1.5, Color32::WHITE));
    }
}
