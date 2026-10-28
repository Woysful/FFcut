//! The audio effects of one track (fades, equalizer, compressor, de-esser, noise reduction, normalization),
//! shown in the sidebar and in the audio presets editor.

use super::widgets::{self, StackAct, StackEntry};
use super::{builtin_ui, field, filter_ui, grid, plugin_ui};
use crate::app::App;
use crate::catalog::Catalog;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, RichText, Sense, Stroke};
use ffcut_core::audiofx;
use ffcut_core::builtin;
use ffcut_core::filters::Media;
use ffcut_core::plugins::PluginKind;
use ffcut_core::{Compressor, DeEsser, Denoise, Effect, EffectKind, EqBand, EqKind, FilterFx, NormMode, Normalize, TrackEdit};

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
    if sel.is_some_and(|k| k >= bands.len()) {
        *sel = None;
    }

    // ---- drawing
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, theme::BG);
    for db in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
        let col = if db == 0.0 { theme::BAND } else { theme::LINE_FAINT };
        p.line_segment([pos2(rect.left(), y_of(db)), pos2(rect.right(), y_of(db))], Stroke::new(1.0, col));
    }
    for f in [100.0f32, 1000.0, 10000.0] {
        p.line_segment([pos2(x_of(f), rect.top()), pos2(x_of(f), rect.bottom())], Stroke::new(1.0, theme::LINE_FAINT));
        p.text(
            pos2(x_of(f) + 3.0, rect.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") },
            egui::FontId::proportional(10.0),
            theme::TEXT_OFF,
        );
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
        p.text(rect.center(), egui::Align2::CENTER_CENTER, "double-click to add a band", egui::FontId::proportional(11.5), theme::TEXT_OFF);
    }
}

fn fx_tip(k: &EffectKind) -> Option<&'static str> {
    Some(match k {
        EffectKind::Eq(_) => "Drag a point: frequency and gain. Wheel: width. Double-click: new band. Right-click a point: remove.",
        EffectKind::Comp(_) => "Evens out loud and quiet parts. Raise the make-up gain to bring the level back.",
        EffectKind::DeEss(_) => "Softens harsh “s” and “sh” sounds in voice recordings. Raise Intensity until they stop stinging.",
        EffectKind::Denoise(_) => "Takes steady background noise (hiss, fan, hum) out of the sound. It follows the noise while you play.",
        EffectKind::Norm(_) => "Measures the clip as it reaches this point of the stack and sets the level for you. The track volume stays on top.",
        EffectKind::Builtin(b) => return builtin_ui::tip(b),
        EffectKind::Filter(_) | EffectKind::Plugin(_) => return None,
    })
}

/// Headings of the audio "+ Add effect" menu, in order (the ready-made effects of `builtin` use the same).
const GROUPS: [&str; 5] = ["Level & dynamics", "Tone", "Repair", "Space", "Character"];

/// The audio effects of the "+ Add effect" menu: FFcut's own, then the ready-made ones, by heading.
fn entries() -> Vec<filter_ui::Entry<'static, EffectKind>> {
    use filter_ui::Entry;
    let mut v: Vec<Entry<EffectKind>> = vec![
        Entry {
            group: GROUPS[0],
            title: "Normalize — loudness (LUFS)",
            desc: "Sets the loudness to a target, like streaming sites do",
            keywords: "loudnorm normalise ebu r128 lufs loudness level",
            make: Box::new(|| EffectKind::Norm(Normalize::new(NormMode::Loudness))),
        },
        Entry {
            group: GROUPS[0],
            title: "Normalize — peak level",
            desc: "Raises the loudest peak to a level",
            keywords: "normalise peak volume level maximum",
            make: Box::new(|| EffectKind::Norm(Normalize::new(NormMode::Peak))),
        },
        Entry {
            group: GROUPS[0],
            title: "Compressor",
            desc: "Evens out loud and quiet parts",
            keywords: "acompressor dynamics squash compression",
            make: Box::new(|| EffectKind::Comp(Compressor::default())),
        },
        Entry {
            group: GROUPS[1],
            title: "Equalizer",
            desc: "Bands you drag on a curve",
            keywords: "equalizer eq parametric lowpass highpass low-pass high-pass lowshelf highshelf shelf band",
            make: Box::new(|| EffectKind::Eq(vec![EqBand { kind: EqKind::Peak, freq: 1000.0, gain: 0.0, q: 1.0, on: true }])),
        },
        Entry {
            group: GROUPS[2],
            title: "Noise reduction",
            desc: "Takes out steady hiss, fan or hum",
            keywords: "afftdn denoise noise hiss background",
            make: Box::new(|| EffectKind::Denoise(Denoise::default())),
        },
        Entry {
            group: GROUPS[2],
            title: "De-esser",
            desc: "Softens harsh “s” sounds of a voice",
            keywords: "deesser sibilance voice s sh",
            make: Box::new(|| EffectKind::DeEss(DeEsser::default())),
        },
    ];
    for d in builtin::defs(Media::Audio) {
        v.push(Entry { group: d.group, title: d.name, desc: d.desc, keywords: d.keywords, make: Box::new(move || EffectKind::Builtin(d.new_fx())) });
    }
    v
}
fn db_fmt(v: f64) -> String {
    format!("{v:.1}")
}

/// Everything the effects body needs besides the track itself.
struct FxCtx {
    rendering: bool,
    render_err: Option<String>,
    /// per effect: (measurement error, still measuring)
    meas: Vec<(Option<String>, bool)>,
    sel: Option<(usize, usize)>,
    /// show the fades card (not for a preset, which has no clip)
    fades: bool,
    cat: Catalog,
}

/// The effects stack of one track (fades, effects, add menu): shared by the sidebar and the presets editor.
fn fx_body(ui: &mut egui::Ui, t: &mut TrackEdit, c: &mut FxCtx) {
    let FxCtx { rendering, render_err, meas, sel, fades, cat } = c;
    let (rendering, render_err) = (*rendering, render_err.clone());
    let len = (t.trim_end - t.trim_start).max(0.1);
    ui.spacing_mut().item_spacing = vec2(8.0, 6.0);

    if *fades {
        let tip = "Also adjustable on the waveform / track: drag the white dots at the top corners.";
        field(ui, "Fade in/out", |ui| {
            let max_in = (len - t.fade_out).max(0.0);
            let r = super::num(ui, egui::DragValue::new(&mut t.fade_in).range(0.0..=max_in).speed(0.05).suffix(" s").fixed_decimals(2));
            widgets::resettable(r, &mut t.fade_in, 0.0).on_hover_text(format!("Fade in. {tip}"));
            let max_out = (len - t.fade_in).max(0.0);
            let r = super::num(ui, egui::DragValue::new(&mut t.fade_out).range(0.0..=max_out).speed(0.05).suffix(" s").fixed_decimals(2));
            widgets::resettable(r, &mut t.fade_out, 0.0).on_hover_text(format!("Fade out. {tip}"));
        });
        ui.add_space(4.0);
    }

    widgets::caption(ui, "Effects").on_hover_text("Applied from top to bottom. Change the order with the arrows, bypass with the checkbox.\nWhat you hear is the track rendered with these effects by ffmpeg, the same filters the export uses; the preview refreshes a moment after you stop editing.");
    let mut act: Option<StackAct> = None;
    let count = t.effects.len();
    for (k, e) in t.effects.iter_mut().enumerate() {
        let desc = match &e.kind {
            EffectKind::Filter(f) => cat.desc(&f.name),
            EffectKind::Plugin(p) => cat.plugin_desc(p),
            _ => None,
        };
        let name = e.name().to_string();
        let entry = StackEntry { k, count, name: &name, tip: desc.as_deref().or(fx_tip(&e.kind)), highlight: false };
        let mut on = e.on;
        widgets::stack_card(ui, entry, &mut on, &mut act, |ui| effect_body(ui, k, &mut e.kind, sel, meas, cat));
        e.on = on;
    }
    if let Some(a) = act {
        a.apply(&mut t.effects);
        *sel = None;
    }
    if t.effects.is_empty() {
        widgets::hint(ui, "No effects yet.");
    }

    ui.horizontal(|ui| {
        widgets::form_menu(ui, "+ Add effect", |ui| {
            let plugins = |ui: &mut egui::Ui| {
                widgets::form_submenu(ui, "Plugin", |ui| {
                    if let Some(p) = plugin_ui::picker(ui, cat, &[PluginKind::Ladspa, PluginKind::Lv2]) {
                        t.effects.push(Effect::new(EffectKind::Plugin(p)));
                        ui.close();
                    }
                })
                .on_hover_text("A LADSPA or LV2 plugin installed on this computer (ffmpeg runs it)");
            };
            let picked = filter_ui::add_menu(ui, &entries(), &GROUPS, cat, Media::Audio, plugins);
            if let Some(p) = picked {
                t.effects.push(Effect::new(match p {
                    filter_ui::Pick::Effect(k) => k,
                    filter_ui::Pick::Filter(name) => EffectKind::Filter(FilterFx::new(name)),
                }));
                ui.close();
            }
        });
        if ui
            .add_enabled(t.has_fx() || !t.effects.is_empty(), egui::Button::new("Reset all"))
            .on_hover_text("Remove every effect, fade and keyframe of this track")
            .clicked()
        {
            t.fade_in = 0.0;
            t.fade_out = 0.0;
            t.envelope.clear();
            t.effects.clear();
            *sel = None;
        }
    });
    if rendering {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Preparing the preview…").size(theme::size::HINT).color(theme::TEXT_LABEL));
        });
    } else if let Some(e) = &render_err {
        widgets::error(ui, format!("Preview failed: {e}"));
    }
}

/// The settings of one effect of the stack.
fn effect_body(ui: &mut egui::Ui, k: usize, kind: &mut EffectKind, sel: &mut Option<(usize, usize)>, meas: &[(Option<String>, bool)], cat: &Catalog) {
    match kind {
        EffectKind::Filter(f) => filter_ui::body(ui, ("afilter", k), f, cat),
        EffectKind::Plugin(p) => plugin_ui::body(ui, ("aplugin", k), p, cat),
        EffectKind::Builtin(b) => builtin_ui::body(ui, ("abuiltin", k), b),
        EffectKind::Eq(bands) => {
            let mut band_sel = (*sel).filter(|s| s.0 == k).map(|s| s.1);
            eq_plot(ui, bands, &mut band_sel);
            if let Some(b) = band_sel {
                *sel = Some((k, b));
            } else if (*sel).is_some_and(|s| s.0 == k) {
                *sel = None;
            }
            ui.add_space(4.0);
            let mut remove = None;
            for (b_i, b) in bands.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut b.on, "");
                    let w = ui.available_width() - 72.0;
                    widgets::Combo::from_id_salt(("eqkind", k, b_i)).selected_text(kind_name(b.kind)).width(w).show_list(ui, |ui| {
                        for kd in [EqKind::Peak, EqKind::LowShelf, EqKind::HighShelf, EqKind::HighPass, EqKind::LowPass] {
                            ui.selectable_value(&mut b.kind, kd, kind_name(kd));
                        }
                    });
                    if ui.small_button("✕").on_hover_text("Remove this band").clicked() {
                        remove = Some(b_i);
                    }
                });
                ui.horizontal(|ui| {
                    let w = super::widgets::NUM_W - 6.0;
                    let h = ui.spacing().interact_size.y;
                    super::widgets::value_sized(ui, [w, h], egui::DragValue::new(&mut b.freq).range(20.0..=20000.0).speed(5.0).suffix(" Hz").fixed_decimals(0))
                        .on_hover_text("Frequency");
                    let gains = matches!(b.kind, EqKind::Peak | EqKind::LowShelf | EqKind::HighShelf);
                    ui.add_enabled_ui(gains, |ui| {
                        super::widgets::value_sized(
                            ui,
                            [w, h],
                            egui::DragValue::new(&mut b.gain).range(-18.0..=18.0).speed(0.1).suffix(" dB").fixed_decimals(1),
                        )
                    })
                    .response
                    .on_hover_text("Gain");
                    super::widgets::value_sized(ui, [w, h], egui::DragValue::new(&mut b.q).range(0.1..=10.0).speed(0.02).prefix("Q ").fixed_decimals(2))
                        .on_hover_text("Width (higher = narrower)");
                });
                ui.add_space(2.0);
            }
            if let Some(b_i) = remove {
                bands.remove(b_i);
                *sel = None;
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
            grid(("comp", k)).show(ui, |ui| {
                ui.label("Threshold");
                super::widgets::value(ui, egui::Slider::new(&mut c.threshold_db, -60.0..=0.0).suffix(" dB"));
                ui.end_row();
                ui.label("Ratio");
                super::widgets::value(ui, egui::Slider::new(&mut c.ratio, 1.0..=20.0).suffix(" : 1"));
                ui.end_row();
                ui.label("Attack");
                super::widgets::value(ui, egui::Slider::new(&mut c.attack_ms, 1.0..=200.0).logarithmic(true).suffix(" ms"));
                ui.end_row();
                ui.label("Release");
                super::widgets::value(ui, egui::Slider::new(&mut c.release_ms, 10.0..=2000.0).logarithmic(true).suffix(" ms"));
                ui.end_row();
                ui.label("Make-up gain");
                super::widgets::value(ui, egui::Slider::new(&mut c.makeup_db, 0.0..=24.0).suffix(" dB"));
                ui.end_row();
            });
        }
        EffectKind::DeEss(d) => {
            grid(("deess", k)).show(ui, |ui| {
                ui.label("Intensity");
                super::widgets::value(ui, egui::Slider::new(&mut d.intensity, 0.0..=1.0).fixed_decimals(2)).on_hover_text("How strongly sibilance is reduced");
                ui.end_row();
                ui.label("Max reduction");
                super::widgets::value(ui, egui::Slider::new(&mut d.max, 0.0..=1.0).fixed_decimals(2)).on_hover_text("Upper limit of the reduction");
                ui.end_row();
                ui.label("Frequency");
                super::widgets::value(ui, egui::Slider::new(&mut d.freq, 0.0..=1.0).fixed_decimals(2))
                    .on_hover_text("Where the “s” sits: low = deeper voices, high = brighter ones");
                ui.end_row();
            });
        }
        EffectKind::Denoise(d) => {
            grid(("denoise", k)).show(ui, |ui| {
                ui.label("Reduction");
                super::widgets::value(ui, egui::Slider::new(&mut d.reduction_db, 1.0..=40.0).suffix(" dB"))
                    .on_hover_text("How much noise is removed. Too much makes the voice “watery”");
                ui.end_row();
                ui.label("Noise floor");
                super::widgets::value(ui, egui::Slider::new(&mut d.floor_db, -80.0..=-20.0).suffix(" dB")).on_hover_text("Roughly how loud the noise is");
                ui.end_row();
            });
        }
        EffectKind::Norm(n) => {
            field(ui, "Mode", |ui| {
                let cur = if n.mode == NormMode::Peak { "Peak level" } else { "Loudness (LUFS)" };
                widgets::Combo::from_id_salt(("normmode", k)).selected_text(cur).width(ui.available_width()).show_list(ui, |ui| {
                    if ui.selectable_label(n.mode == NormMode::Peak, "Peak level").clicked() && n.mode != NormMode::Peak {
                        *n = Normalize::new(NormMode::Peak);
                    }
                    if ui.selectable_label(n.mode == NormMode::Loudness, "Loudness (LUFS)").clicked() && n.mode != NormMode::Loudness {
                        *n = Normalize::new(NormMode::Loudness);
                    }
                });
            });
            field(ui, "Target", |ui| {
                let (range, unit) = if n.mode == NormMode::Peak { (-24.0..=0.0, " dB") } else { (-40.0..=-5.0, " LUFS") };
                super::widgets::value(ui, egui::DragValue::new(&mut n.target).range(range).speed(0.1).suffix(unit).fixed_decimals(1)).on_hover_text(
                    if n.mode == NormMode::Peak {
                        "The loudest peak of the clip will reach this level"
                    } else {
                        "Target integrated loudness: -16 for the web / YouTube, -23 for broadcast"
                    },
                );
            });
            let (err, busy) = &meas[k];
            if let Some(e) = err {
                widgets::error(ui, format!("Could not measure: {e}"));
            } else if *busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Measuring the track…").color(theme::TEXT_LABEL));
                });
            } else if let Some(m) = &n.measured {
                ui.label(RichText::new(format!("Measured: {} LUFS, peak {} dB", db_fmt(m.i), db_fmt(m.tp))).size(12.0).color(theme::OK));
            }
        }
    }
}

impl App {
    fn fx_ctx(&self, t0: &TrackEdit) -> FxCtx {
        let meas = (0..t0.effects.len())
            .map(|k| {
                let e = self.measure_error(t0, k);
                let busy = t0.effects[k].on && matches!(t0.effects[k].kind, EffectKind::Norm(_)) && audiofx::valid_measure(t0, k).is_none() && e.is_none();
                (e, busy)
            })
            .collect();
        let sel = self.fx.eq_sel.filter(|s| matches!(t0.effects.get(s.0).map(|e| &e.kind), Some(EffectKind::Eq(_))));
        FxCtx { rendering: self.preview_rendering(t0), render_err: self.preview_error(t0), meas, sel, fades: true, cat: self.catalog.clone() }
    }

    /// The effects of track `i` in the sidebar.
    pub fn fx_sidebar(&mut self, ui: &mut egui::Ui, i: usize) {
        let Some(t0) = self.s.tracks.get(i).cloned() else { return };
        let mut c = self.fx_ctx(&t0);
        let mut t = t0.clone();
        ui.spacing_mut().slider_width = 110.0;
        fx_body(ui, &mut t, &mut c);
        if t != t0 {
            self.s.tracks[i] = t;
        }
        self.fx.eq_sel = c.sel;
    }
}

/// The effect stack of an audio preset, edited with the same cards as a track (no clip, so no measuring or fades).
pub fn preset_effects(ui: &mut egui::Ui, fx: &mut Vec<ffcut_core::Effect>, cat: &Catalog) -> bool {
    let mut t = TrackEdit::original(0, "preset".into(), 3600.0);
    t.effects = fx.clone();
    let mut c = FxCtx { rendering: false, render_err: None, meas: vec![(None, false); t.effects.len()], sel: None, fades: false, cat: cat.clone() };
    fx_body(ui, &mut t, &mut c);
    let changed = t.effects != *fx;
    if changed {
        *fx = t.effects;
    }
    changed
}
