//! A thin level meter at the right edge of the timeline.
//!
//! It is read from the waveform data (the same peaks the lanes draw), at the playhead, for the tracks you
//! currently hear: the single preview track, or in the track view every audible track with its volume,
//! mute and solo; the master volume applies too. That makes it independent of mpv and exact while seeking.
//! It rises at once and falls back smoothly; the thin line is the peak hold.

use super::tracks::peak_in;
use crate::app::{App, Wave};
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};
use ffcut_core::{audible, TrackSource};
use std::time::Instant;

pub const METER_W: f32 = 9.0;
const FLOOR_DB: f32 = -60.0;

#[derive(Clone, Copy)]
pub struct MeterState {
    pub db: f32,
    pub hold: f32,
    pub hold_at: Instant,
    pub last: Instant,
}

impl Default for MeterState {
    fn default() -> Self {
        let now = Instant::now();
        MeterState { db: FLOOR_DB, hold: FLOOR_DB, hold_at: now, last: now }
    }
}

fn to_db(l: f32) -> f32 {
    (20.0 * l.max(1e-6).log10()).clamp(FLOOR_DB, 0.0)
}

impl App {
    /// Linear level (0..~1) right now.
    fn level_now(&self, ctx: &egui::Context) -> f32 {
        if self.clip.is_none() || self.player.paused() || self.muted {
            return 0.0;
        }
        let t = self.player.time();
        let parts: Vec<(TrackSource, f32)> = if self.multitrack {
            audible(&self.s.tracks).iter().map(|x| (x.source.clone(), x.volume.max(0.0))).collect()
        } else {
            vec![(TrackSource::Original(self.preview_audio), 1.0)]
        };
        let mut sum = 0.0;
        for (src, vol) in parts {
            self.want_wave(ctx, &src);
            if let Some(Wave::Ready(p)) = self.waves.lock().unwrap().get(&self.wave_key(&src)) {
                sum += peak_in(p, t - 0.03, t + 0.05) * vol;
            }
        }
        sum * (self.volume / 100.0)
    }

    /// Draws the meter in `rect` (a thin vertical bar) and advances its smoothing.
    pub fn level_meter(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let now = Instant::now();
        let target = to_db(self.level_now(ui.ctx()));
        let m = &mut self.meter;
        let dt = now.duration_since(m.last).as_secs_f32().min(0.2);
        m.last = now;
        m.db = if target >= m.db { target } else { (m.db - 30.0 * dt).max(target) };
        if m.db >= m.hold {
            m.hold = m.db;
            m.hold_at = now;
        } else if now.duration_since(m.hold_at).as_secs_f32() > 1.0 {
            m.hold = (m.hold - 14.0 * dt).max(m.db);
        }
        let (db, hold) = (m.db, m.hold);

        let resp = ui.interact(rect, ui.id().with("level_meter"), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, theme::TRACK);
        let h = rect.height();
        let y_of = |d: f32| rect.bottom() - h * (d - FLOOR_DB) / (0.0 - FLOOR_DB);
        // segments of 3 px with a 1 px gap, coloured by level
        let step = 4.0;
        let mut y = rect.bottom() - 1.0;
        while y - 3.0 > rect.top() {
            let d_here = FLOOR_DB + (rect.bottom() - y) / h * (-FLOOR_DB);
            if d_here <= db {
                let col = if d_here > -3.0 { theme::ERR } else if d_here > -12.0 { theme::WARN } else { theme::OK };
                p.rect_filled(Rect::from_min_max(pos2(rect.left() + 1.5, y - 3.0), pos2(rect.right() - 1.5, y)), 1.0, col);
            }
            y -= step;
        }
        if hold > FLOOR_DB + 1.0 {
            let yy = y_of(hold).clamp(rect.top() + 1.0, rect.bottom() - 1.0);
            p.line_segment([pos2(rect.left() + 1.0, yy), pos2(rect.right() - 1.0, yy)], Stroke::new(1.5, Color32::from_white_alpha(200)));
        }
        resp.on_hover_text(format!("Level at the playhead: {}\n(from the waveform of the tracks you hear)", if db <= FLOOR_DB + 0.5 { "silent".to_string() } else { format!("{db:.1} dB") }));
        if !self.player.paused() || self.meter.db > FLOOR_DB + 0.5 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        let _ = vec2(0.0, 0.0);
    }
}
