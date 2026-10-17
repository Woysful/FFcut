//! The audio effects window of one track: fades and volume keyframes, equalizer, compressor, normalization.

use super::{card, os_window};
use crate::app::App;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, RichText, Sense, Stroke};
use ffcut_core::audiofx;
use ffcut_core::{Compressor, Effect, EffectKind, EqBand, EqKind, NormMode, Normalize};

const MAX_BANDS: usize = 8;

fn kind_name(k: EqKind) -> &'static str {
    match k {
        EqKind::Peak => "Peak",
        EqKind::LowShelf => "Low shelf",
        EqKind::HighShelf => "High shelf",
        EqKind::HighPass => "High-pass",
        EqKind::LowPass => "Low-pass",
    }
}

/// Magnitude (dB) of one band at `f` Hz: the usual biquad formulas (RBJ cookbook), 48 kHz.
fn band_db(b: &EqBand, f: f32) -> f32 {
    use std::f32::consts::PI;
    let fs = 48000.0;
    let w0 = 2.0 * PI * b.freq.clamp(10.0, 23000.0) / fs;
    let (s, c) = w0.sin_cos();
    let q = b.q.max(0.05);
    let alpha = s / (2.0 * q);
    let a = 10f32.powf(b.gain / 40.0);
    let sa = 2.0 * a.sqrt() * alpha;
    let (b0, b1, b2, a0, a1, a2) = match b.kind {
        EqKind::Peak => (1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a),
        EqKind::LowShelf => (
            a * ((a + 1.0) - (a - 1.0) * c + sa),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sa),
            (a + 1.0) + (a - 1.0) * c + sa,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sa,
        ),
        EqKind::HighShelf => (
            a * ((a + 1.0) + (a - 1.0) * c + sa),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sa),
            (a + 1.0) - (a - 1.0) * c + sa,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sa,
        ),
        EqKind::HighPass => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
        EqKind::LowPass => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
    };
    let w = 2.0 * PI * f / fs;
    let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
    let num = ((b0 + b1 * c1 + b2 * c2).powi(2) + (b1 * s1 + b2 * s2).powi(2)).sqrt();
    let den = ((a0 + a1 * c1 + a2 * c2).powi(2) + (a1 * s1 + a2 * s2).powi(2)).sqrt();
    20.0 * (num / den.max(1e-9)).max(1e-9).log10()
}

fn eq_plot(ui: &mut egui::Ui, bands: &mut Vec<EqBand>, sel: &mut Option<usize>) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 140.0), Sense::click_and_drag());
    let (fmin, fmax, range) = (20f32.ln(), 20000f32.ln(), 18.0f32);
    let x_of = |f: f32| rect.left() + (f.clamp(20.0, 20000.0).ln() - fmin) / (fmax - fmin) * rect.width();
    let y_of = |db: f32| rect.center().y - db.clamp(-range, range) / range * (rect.height() / 2.0 - 6.0);
    let f_of = |x: f32| (fmin + ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) * (fmax - fmin)).exp();
    let db_of = |y: f32| (((rect.center().y - y) / (rect.height() / 2.0 - 6.0)) * range).clamp(-range, range);
    let has_gain = |k: EqKind| matches!(k, EqKind::Peak | EqKind::LowShelf | EqKind::HighShelf);
    let point = |b: &EqBand| pos2(x_of(b.freq), y_of(if has_gain(b.kind) { b.gain } else { 0.0 }));
    let hit = |bands: &[EqBand], p: Pos2| bands.iter().position(|b| b.on && point(b).distance(p) <= 11.0);

    // ---- interaction: drag a point (frequency + gain), wheel = width, double-click = new band, right-click = remove
    if resp.drag_started() {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            *sel = hit(bands, p).or(*sel);
        }
    }
    if resp.dragged() {
        if let (Some(k), Some(p)) = (*sel, resp.interact_pointer_pos()) {
            if let Some(b) = bands.get_mut(k) {
                b.freq = f_of(p.x).round().clamp(20.0, 20000.0);
                if has_gain(b.kind) {
                    b.gain = (db_of(p.y) * 10.0).round() / 10.0;
                }
            }
        }
    }
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            *sel = hit(bands, p);
        }
    }
    if resp.double_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            if hit(bands, p).is_none() && bands.len() < MAX_BANDS {
                bands.push(EqBand { kind: EqKind::Peak, freq: f_of(p.x).round(), gain: (db_of(p.y) * 10.0).round() / 10.0, q: 1.0, on: true });
                *sel = Some(bands.len() - 1);
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(k) = resp.interact_pointer_pos().and_then(|p| hit(bands, p)) {
            bands.remove(k);
            *sel = None;
        }
    }
    if let Some(p) = resp.hover_pos() {
        let target = hit(bands, p).or(*sel);
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if let (Some(k), true) = (target, scroll != 0.0) {
            if let Some(b) = bands.get_mut(k) {
                b.q = (b.q * (scroll * 0.004).exp()).clamp(0.1, 10.0);
            }
        }
        if hit(bands, p).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    if sel.map_or(false, |k| k >= bands.len()) {
        *sel = None;
    }

    // ---- drawing
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, theme::BG);
    for db in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
        let col = if db == 0.0 { Color32::from_gray(80) } else { Color32::from_gray(44) };
        p.line_segment([pos2(rect.left(), y_of(db)), pos2(rect.right(), y_of(db))], Stroke::new(1.0, col));
    }
    for f in [100.0f32, 1000.0, 10000.0] {
        p.line_segment([pos2(x_of(f), rect.top()), pos2(x_of(f), rect.bottom())], Stroke::new(1.0, Color32::from_gray(44)));
        p.text(pos2(x_of(f) + 3.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") }, egui::FontId::proportional(10.0), Color32::from_gray(110));
    }
    let n = 240;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let f = (fmin + (fmax - fmin) * k as f32 / n as f32).exp();
            let db: f32 = bands.iter().filter(|b| b.on).map(|b| band_db(b, f)).sum();
            pos2(rect.left() + rect.width() * k as f32 / n as f32, y_of(db))
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(2.0, theme::ACCENT)));
    for (k, b) in bands.iter().enumerate().filter(|(_, b)| b.on) {
        let selected = *sel == Some(k);
        p.circle(point(b), if selected { 6.0 } else { 4.5 }, if selected { theme::ACCENT } else { Color32::WHITE }, Stroke::new(1.5, Color32::BLACK));
    }
    if bands.is_empty() {
        p.text(rect.center(), egui::Align2::CENTER_CENTER, "double-click to add a band", egui::FontId::proportional(11.5), Color32::from_gray(110));
    }
}

fn db_fmt(v: f64) -> String {
    format!("{v:.1}")
}

/// What the stack's remove / move buttons asked for.
enum StackAct {
    Remove(usize),
    Up(usize),
    Down(usize),
}

impl App {
    pub fn fx_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.fx.window else { return };
        let Some(t0) = self.s.tracks.get(i).cloned() else {
            self.fx.window = None;
            return;
        };
        let rendering = self.preview_rendering(&t0);
        let render_err = self.preview_error(&t0);
        // per effect: (measurement error, still measuring)
        let meas: Vec<(Option<String>, bool)> = (0..t0.effects.len())
            .map(|k| {
                let e = self.measure_error(&t0, k);
                let busy = t0.effects[k].on && matches!(t0.effects[k].kind, EffectKind::Norm(_)) && audiofx::valid_measure(&t0, k).is_none() && e.is_none();
                (e, busy)
            })
            .collect();
        let mut t = t0.clone();
        let mut sel = self.fx.eq_sel.filter(|s| matches!(t0.effects.get(s.0).map(|e| &e.kind), Some(EffectKind::Eq(_)))).map(|s| s);
        let len = (t.trim_end - t.trim_start).max(0.1);

        let open = os_window(ctx, "fx", &format!("Audio effects — {}", t.label), [540.0, 760.0], |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);

            card(ui, "Fades", Some("Also adjustable on the track: drag the white dots at the top corners of the clip. Volume keyframes: right-click the track."), |ui| {
                egui::Grid::new("fades").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Fade in");
                    ui.add(egui::DragValue::new(&mut t.fade_in).range(0.0..=(len - t.fade_out).max(0.0)).speed(0.05).suffix(" s").fixed_decimals(2));
                    ui.label("Fade out");
                    ui.add(egui::DragValue::new(&mut t.fade_out).range(0.0..=(len - t.fade_in).max(0.0)).speed(0.05).suffix(" s").fixed_decimals(2));
                    ui.end_row();
                });
            });

            ui.label(RichText::new("EFFECTS").size(11.0).strong().color(Color32::from_gray(150)));
            ui.add(egui::Label::new(RichText::new("Applied from top to bottom. Change the order with the arrows, bypass with the checkbox.").size(11.5).color(Color32::from_gray(120))).wrap());
            let mut act: Option<StackAct> = None;
            let count = t.effects.len();
            for (k, e) in t.effects.iter_mut().enumerate() {
                egui::Frame::NONE
                    .fill(Color32::from_rgb(40, 42, 48))
                    .stroke(Stroke::new(1.0, Color32::from_gray(54)))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut e.on, "").on_hover_text("Bypass: keep the effect in the list but do not apply it");
                            ui.label(RichText::new(e.name()).size(14.0).strong().color(if e.on { Color32::from_gray(235) } else { Color32::from_gray(120) }));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").on_hover_text("Remove this effect").clicked() {
                                    act = Some(StackAct::Remove(k));
                                }
                                if ui.add_enabled(k + 1 < count, egui::Button::new("▼").small()).on_hover_text("Apply later").clicked() {
                                    act = Some(StackAct::Down(k));
                                }
                                if ui.add_enabled(k > 0, egui::Button::new("▲").small()).on_hover_text("Apply earlier").clicked() {
                                    act = Some(StackAct::Up(k));
                                }
                            });
                        });
                        ui.add_space(4.0);
                        ui.add_enabled_ui(e.on, |ui| match &mut e.kind {
                            EffectKind::Eq(bands) => {
                                let mut band_sel = sel.filter(|s| s.0 == k).map(|s| s.1);
                                eq_plot(ui, bands, &mut band_sel);
                                if band_sel.is_some() {
                                    sel = Some((k, band_sel.unwrap()));
                                } else if sel.map_or(false, |s| s.0 == k) {
                                    sel = None;
                                }
                                ui.add(egui::Label::new(RichText::new("Drag a point: frequency and gain. Wheel: width. Double-click: new band. Right-click a point: remove.").size(11.0).color(Color32::from_gray(120))).wrap());
                                ui.add_space(4.0);
                                let mut remove = None;
                                for (b_i, b) in bands.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut b.on, "");
                                        egui::ComboBox::from_id_salt(("eqkind", k, b_i)).selected_text(kind_name(b.kind)).width(86.0).show_ui(ui, |ui| {
                                            for kd in [EqKind::Peak, EqKind::LowShelf, EqKind::HighShelf, EqKind::HighPass, EqKind::LowPass] {
                                                ui.selectable_value(&mut b.kind, kd, kind_name(kd));
                                            }
                                        });
                                        ui.add(egui::DragValue::new(&mut b.freq).range(20.0..=20000.0).speed(5.0).suffix(" Hz").fixed_decimals(0)).on_hover_text("Frequency");
                                        let gains = matches!(b.kind, EqKind::Peak | EqKind::LowShelf | EqKind::HighShelf);
                                        ui.add_enabled(gains, egui::DragValue::new(&mut b.gain).range(-18.0..=18.0).speed(0.1).suffix(" dB").fixed_decimals(1)).on_hover_text("Gain");
                                        ui.add(egui::DragValue::new(&mut b.q).range(0.1..=10.0).speed(0.02).prefix("Q ").fixed_decimals(2)).on_hover_text("Width (higher = narrower)");
                                        if ui.small_button("✕").on_hover_text("Remove this band").clicked() {
                                            remove = Some(b_i);
                                        }
                                    });
                                }
                                if let Some(b_i) = remove {
                                    bands.remove(b_i);
                                    sel = None;
                                }
                                ui.horizontal(|ui| {
                                    if ui.add_enabled(bands.len() < MAX_BANDS, egui::Button::new("+ Band")).clicked() {
                                        bands.push(EqBand { kind: EqKind::Peak, freq: 1000.0, gain: 0.0, q: 1.0, on: true });
                                    }
                                    if ui.add_enabled(bands.len() < MAX_BANDS, egui::Button::new("+ High-pass 80 Hz")).on_hover_text("Cuts rumble below the voice").clicked() {
                                        bands.push(EqBand { kind: EqKind::HighPass, freq: 80.0, gain: 0.0, q: 0.71, on: true });
                                    }
                                });
                            }
                            EffectKind::Comp(c) => {
                                ui.add(egui::Label::new(RichText::new("Evens out loud and quiet parts. Raise the make-up gain to bring the level back.").size(11.5).color(Color32::from_gray(140))).wrap());
                                egui::Grid::new(("comp", k)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                                    ui.label("Threshold");
                                    ui.add(egui::Slider::new(&mut c.threshold_db, -60.0..=0.0).suffix(" dB"));
                                    ui.end_row();
                                    ui.label("Ratio");
                                    ui.add(egui::Slider::new(&mut c.ratio, 1.0..=20.0).suffix(" : 1"));
                                    ui.end_row();
                                    ui.label("Attack");
                                    ui.add(egui::Slider::new(&mut c.attack_ms, 1.0..=200.0).logarithmic(true).suffix(" ms"));
                                    ui.end_row();
                                    ui.label("Release");
                                    ui.add(egui::Slider::new(&mut c.release_ms, 10.0..=2000.0).logarithmic(true).suffix(" ms"));
                                    ui.end_row();
                                    ui.label("Make-up gain");
                                    ui.add(egui::Slider::new(&mut c.makeup_db, 0.0..=24.0).suffix(" dB"));
                                    ui.end_row();
                                });
                            }
                            EffectKind::Norm(n) => {
                                ui.add(egui::Label::new(RichText::new("Measures the clip as it reaches this point of the stack and sets the level for you. The track volume stays on top.").size(11.5).color(Color32::from_gray(140))).wrap());
                                ui.horizontal(|ui| {
                                    let cur = if n.mode == NormMode::Peak { "Peak level" } else { "Loudness (LUFS)" };
                                    egui::ComboBox::from_id_salt(("normmode", k)).selected_text(cur).width(150.0).show_ui(ui, |ui| {
                                        if ui.selectable_label(n.mode == NormMode::Peak, "Peak level").clicked() && n.mode != NormMode::Peak {
                                            *n = Normalize::new(NormMode::Peak);
                                        }
                                        if ui.selectable_label(n.mode == NormMode::Loudness, "Loudness (LUFS)").clicked() && n.mode != NormMode::Loudness {
                                            *n = Normalize::new(NormMode::Loudness);
                                        }
                                    });
                                    let (range, unit) = if n.mode == NormMode::Peak { (-24.0..=0.0, " dB") } else { (-40.0..=-5.0, " LUFS") };
                                    ui.add(egui::DragValue::new(&mut n.target).range(range).speed(0.1).suffix(unit).fixed_decimals(1)).on_hover_text(if n.mode == NormMode::Peak { "The loudest peak of the clip will reach this level" } else { "Target integrated loudness: -16 for the web / YouTube, -23 for broadcast" });
                                });
                                let (err, busy) = &meas[k];
                                if let Some(e) = err {
                                    ui.colored_label(theme::ERR, format!("Could not measure: {e}"));
                                } else if *busy {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(RichText::new("Measuring the track…").color(Color32::from_gray(150)));
                                    });
                                } else if let Some(m) = &n.measured {
                                    ui.label(RichText::new(format!("Measured: {} LUFS, peak {} dB", db_fmt(m.i), db_fmt(m.tp))).size(12.0).color(theme::OK));
                                }
                            }
                        });
                    });
            }
            match act {
                Some(StackAct::Remove(k)) => {
                    t.effects.remove(k);
                    sel = None;
                }
                Some(StackAct::Up(k)) => {
                    t.effects.swap(k, k - 1);
                    sel = None;
                }
                Some(StackAct::Down(k)) => {
                    t.effects.swap(k, k + 1);
                    sel = None;
                }
                None => {}
            }
            if t.effects.is_empty() {
                ui.label(RichText::new("No effects yet.").color(Color32::from_gray(130)));
            }

            ui.horizontal(|ui| {
                ui.menu_button("+ Add effect", |ui| {
                    if ui.button("Equalizer").clicked() {
                        t.effects.push(Effect::new(EffectKind::Eq(vec![EqBand { kind: EqKind::Peak, freq: 1000.0, gain: 0.0, q: 1.0, on: true }])));
                        ui.close();
                    }
                    if ui.button("Compressor").clicked() {
                        t.effects.push(Effect::new(EffectKind::Comp(Compressor::default())));
                        ui.close();
                    }
                    if ui.button("Normalize — loudness (LUFS)").clicked() {
                        t.effects.push(Effect::new(EffectKind::Norm(Normalize::new(NormMode::Loudness))));
                        ui.close();
                    }
                    if ui.button("Normalize — peak level").clicked() {
                        t.effects.push(Effect::new(EffectKind::Norm(Normalize::new(NormMode::Peak))));
                        ui.close();
                    }
                });
                if ui.add_enabled(t.has_fx() || !t.effects.is_empty(), egui::Button::new("Reset all")).on_hover_text("Remove every effect, fade and keyframe of this track").clicked() {
                    t.fade_in = 0.0;
                    t.fade_out = 0.0;
                    t.envelope.clear();
                    t.effects.clear();
                    sel = None;
                }
                if rendering {
                    ui.spinner();
                    ui.label(RichText::new("Preparing the preview…").size(12.0).color(Color32::from_gray(150)));
                } else if let Some(e) = &render_err {
                    ui.colored_label(theme::ERR, format!("Preview failed: {e}"));
                }
            });
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new("What you hear is the track rendered with these effects by ffmpeg, the same filters the export uses. The preview refreshes a moment after you stop editing.").size(11.5).color(Color32::from_gray(120))).wrap());
        });

        if t != t0 {
            self.s.tracks[i] = t;
        }
        self.fx.eq_sel = sel;
        if !open {
            self.fx.window = None;
        }
    }
}
