//! The "Video effects & text" sidebar section (colour, curves, style, restoration, covered areas, text,
//! captions) and the markers it draws on the video.

use super::filter_ui;
use super::widgets::{self, ComboExt, StackAct, StackEntry};
use crate::app::App;
use crate::catalog::Catalog;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, CursorIcon, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use ffcut_core::filters::Media;
use ffcut_core::fonts::Font;
use ffcut_core::videofx::*;
use ffcut_core::FilterFx;

/// Curves editor channels: name and the colour of its line.
const CHANNELS: [(&str, Color32); 4] = [
    ("Master", Color32::from_gray(225)),
    ("Red", Color32::from_rgb(235, 90, 90)),
    ("Green", Color32::from_rgb(100, 210, 110)),
    ("Blue", Color32::from_rgb(100, 150, 245)),
];

fn vfx_tip(k: &VKind) -> Option<&'static str> {
    Some(match k {
        VKind::Curves(_) => "Drag the points. Double-click to add one, right-click to remove it.",
        VKind::Denoise(_) => "Smooths grain and noise (hqdn3d). Strong values make skin look waxy.",
        VKind::Deblock(_) => "Softens the square blocks of heavily compressed video.",
        VKind::Deband(_) => "Hides the visible steps in skies and gradients.",
        VKind::Deflicker(_) => "Evens out brightness that pulses from frame to frame. A longer window is smoother.",
        VKind::Area(_) => "Hides a part of the picture. Drag the frame on the video to place it.",
        VKind::Text(_) => "Place it with the buttons, the numbers, or by dragging the marker on the video.",
        _ => return None,
    })
}

fn pct_slider(ui: &mut egui::Ui, v: &mut f32, label: &str) {
    ui.label(label);
    widgets::value(
        ui,
        egui::Slider::new(v, 0.0..=1.0)
            .custom_formatter(|n, _| format!("{:.0} %", n * 100.0))
            .custom_parser(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|n| n / 100.0)),
    );
    ui.end_row();
}

// ------------------------------------------------------------------ fonts

fn font_label(fonts: &[Font], path: &str) -> String {
    if path.is_empty() {
        return "System default".into();
    }
    fonts
        .iter()
        .find(|f| f.path.to_string_lossy() == path)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| std::path::Path::new(path).file_stem().map_or(path.to_string(), |s| s.to_string_lossy().into_owned()))
}

fn font_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, path: &mut String, fonts: Option<&[Font]>) {
    let Some(fonts) = fonts else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Looking for fonts…").color(theme::TEXT_DIM));
        });
        return;
    };
    egui::ComboBox::from_id_salt(id).selected_text(font_label(fonts, path)).width(ui.available_width()).height(360.0).show_list(ui, |ui| {
        for f in fonts {
            let p = f.path.to_string_lossy();
            if ui.selectable_label(*path == p, &f.name).clicked() {
                *path = p.into_owned();
            }
        }
    });
}

fn align_row(ui: &mut egui::Ui, h: &mut HAlign) {
    for (a, label, tip) in [(HAlign::Left, "⬅", "Left"), (HAlign::Center, "⬌", "Centre"), (HAlign::Right, "➡", "Right")] {
        if ui.selectable_label(*h == a, label).on_hover_text(tip).clicked() {
            *h = a;
        }
    }
}

// ------------------------------------------------------------------ curves

/// Natural cubic spline through the points, sampled at `n` places: ffmpeg's `curves` draws a smooth curve, so
/// the plot does too.
fn spline(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    let k = pts.len();
    if k < 2 {
        return pts.to_vec();
    }
    if k == 2 {
        return (0..=n)
            .map(|i| {
                let x = i as f32 / n as f32;
                (x, pts[0].1 + (pts[1].1 - pts[0].1) * (x - pts[0].0) / (pts[1].0 - pts[0].0).max(1e-6))
            })
            .collect();
    }
    // second derivatives (natural boundary), tridiagonal solve
    let h: Vec<f32> = (0..k - 1).map(|i| (pts[i + 1].0 - pts[i].0).max(1e-6)).collect();
    let mut m = vec![0.0f32; k];
    let mut c = vec![0.0f32; k];
    let mut d = vec![0.0f32; k];
    for i in 1..k - 1 {
        let (a, b, cc) = (h[i - 1], 2.0 * (h[i - 1] + h[i]), h[i]);
        let r = 6.0 * ((pts[i + 1].1 - pts[i].1) / h[i] - (pts[i].1 - pts[i - 1].1) / h[i - 1]);
        let w = b - a * c[i - 1];
        c[i] = cc / w;
        d[i] = (r - a * d[i - 1]) / w;
    }
    for i in (1..k - 1).rev() {
        m[i] = d[i] - c[i] * m[i + 1];
    }
    (0..=n)
        .map(|j| {
            let x = j as f32 / n as f32;
            let i = (0..k - 1).find(|&i| x <= pts[i + 1].0).unwrap_or(k - 2);
            let (x0, x1) = (pts[i].0, pts[i + 1].0);
            let (a, b) = ((x1 - x) / h[i], (x - x0) / h[i]);
            let y = a * pts[i].1 + b * pts[i + 1].1 + ((a * a * a - a) * m[i] + (b * b * b - b) * m[i + 1]) * h[i] * h[i] / 6.0;
            (x, y.clamp(0.0, 1.0))
        })
        .collect()
}

fn curve_plot(ui: &mut egui::Ui, pts: &mut Vec<(f32, f32)>, sel: &mut Option<usize>, color: Color32) {
    let side = ui.available_width().min(260.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let r = rect.shrink(6.0);
    let to = |p: (f32, f32)| pos2(r.left() + p.0 * r.width(), r.bottom() - p.1 * r.height());
    let from = |p: Pos2| (((p.x - r.left()) / r.width()).clamp(0.0, 1.0), ((r.bottom() - p.y) / r.height()).clamp(0.0, 1.0));
    let hit = |pts: &[(f32, f32)], p: Pos2| pts.iter().position(|q| to(*q).distance(p) <= 11.0);
    let last = |pts: &[(f32, f32)]| pts.len().saturating_sub(1);

    if resp.drag_started() {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            *sel = hit(pts, p).or(*sel);
        }
    }
    if resp.dragged() {
        if let (Some(k), Some(p)) = (*sel, resp.interact_pointer_pos()) {
            let (x, y) = from(p);
            let n = pts.len();
            if k < n {
                let (lo, hi) = (if k == 0 { 0.0 } else { pts[k - 1].0 + 0.01 }, if k + 1 == n { 1.0 } else { pts[k + 1].0 - 0.01 });
                // the end points keep their place on the left / right edge and only move up and down
                let x = if k == 0 {
                    0.0
                } else if k == last(pts) {
                    1.0
                } else {
                    x.clamp(lo, hi.max(lo))
                };
                pts[k] = (x, y);
            }
        }
    }
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            *sel = hit(pts, p);
        }
    }
    if resp.double_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            if hit(pts, p).is_none() && pts.len() < 10 {
                let (x, y) = from(p);
                let at = pts.iter().position(|q| q.0 > x).unwrap_or(pts.len());
                if at > 0 && at < pts.len() {
                    pts.insert(at, (x, y));
                    *sel = Some(at);
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(k) = resp.interact_pointer_pos().and_then(|p| hit(pts, p)) {
            if k > 0 && k < last(pts) {
                pts.remove(k);
                *sel = None;
            }
        }
    }
    if resp.hover_pos().is_some_and(|p| hit(pts, p).is_some()) {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    if sel.is_some_and(|k| k >= pts.len()) {
        *sel = None;
    }

    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_gray(24));
    for i in 0..=4 {
        let f = i as f32 / 4.0;
        let c = Color32::from_gray(if i == 2 { 52 } else { 38 });
        p.line_segment([to((f, 0.0)), to((f, 1.0))], Stroke::new(1.0, c));
        p.line_segment([to((0.0, f)), to((1.0, f))], Stroke::new(1.0, c));
    }
    p.line_segment([to((0.0, 0.0)), to((1.0, 1.0))], Stroke::new(1.0, Color32::from_gray(70)));
    let line: Vec<Pos2> = spline(pts, 64).into_iter().map(to).collect();
    p.add(egui::Shape::line(line, Stroke::new(2.0, color)));
    for (k, q) in pts.iter().enumerate() {
        let c = to(*q);
        let on = *sel == Some(k);
        p.circle_filled(c, if on { 6.0 } else { 4.5 }, if on { Color32::WHITE } else { color });
        p.circle_stroke(c, if on { 6.0 } else { 4.5 }, Stroke::new(1.0, Color32::BLACK));
    }
}

// ------------------------------------------------------------------ time range

fn range_ui(ui: &mut egui::Ui, range: &mut Option<(f64, f64)>, now: f64, dur: f64) {
    ui.horizontal_wrapped(|ui| {
        let mut on = range.is_some();
        if ui.checkbox(&mut on, "Only between").on_hover_text("Show it only during part of the video (seconds in the source)").changed() {
            *range = if on { Some((now, (now + 3.0).min(dur).max(now + 0.1))) } else { None };
        }
        if let Some((a, b)) = range {
            super::widgets::value(ui, egui::DragValue::new(a).range(0.0..=dur).speed(0.05).suffix(" s").fixed_decimals(2));
            if ui.small_button("◂ playhead").on_hover_text("Start at the playhead").clicked() {
                *a = now.min(*b);
            }
            ui.label("to");
            super::widgets::value(ui, egui::DragValue::new(b).range(0.0..=dur).speed(0.05).suffix(" s").fixed_decimals(2));
            if ui.small_button("playhead ▸").on_hover_text("End at the playhead").clicked() {
                *b = now.max(*a);
            }
            if *b < *a {
                *b = *a;
            }
        }
    });
}

// ------------------------------------------------------------------ one effect

struct Ctx<'a> {
    fonts: Option<&'a [Font]>,
    now: f64,
    dur: f64,
    curve_ch: &'a mut usize,
    curve_sel: &'a mut Option<(usize, usize)>,
    cat: &'a Catalog,
}

fn effect_body(ui: &mut egui::Ui, k: usize, e: &mut VEffect, cx: &mut Ctx) {
    match &mut e.kind {
        VKind::Filter(f) => {
            filter_ui::body(ui, ("vfilter", k), f, cx.cat);
            widgets::hint(ui, "Applied with the colour effects. A filter that resizes or retimes the picture moves the crop, areas and text.");
        }
        VKind::Color(c) => {
            widgets::grid(("color", k)).show(ui, |ui| {
                ui.label("Brightness");
                super::widgets::value(ui, egui::Slider::new(&mut c.brightness, -1.0..=1.0).fixed_decimals(2));
                ui.end_row();
                ui.label("Contrast");
                super::widgets::value(ui, egui::Slider::new(&mut c.contrast, 0.0..=2.0).fixed_decimals(2));
                ui.end_row();
                ui.label("Saturation");
                super::widgets::value(ui, egui::Slider::new(&mut c.saturation, 0.0..=3.0).fixed_decimals(2));
                ui.end_row();
                ui.label("Gamma");
                super::widgets::value(ui, egui::Slider::new(&mut c.gamma, 0.1..=4.0).logarithmic(true).fixed_decimals(2))
                    .on_hover_text("Above 1 lightens the mid-tones, below 1 darkens them");
                ui.end_row();
            });
            if *c != ColorAdj::default() && ui.small_button("Reset").clicked() {
                *c = ColorAdj::default();
            }
        }
        VKind::Curves(c) => {
            ui.horizontal(|ui| {
                for (i, (name, col)) in CHANNELS.into_iter().enumerate() {
                    if ui.selectable_label(*cx.curve_ch == i, RichText::new(name).color(col)).clicked() {
                        *cx.curve_ch = i;
                        *cx.curve_sel = None;
                    }
                }
            });
            let ch = (*cx.curve_ch).min(3);
            let color = CHANNELS[ch].1;
            let pts = match ch {
                0 => &mut c.master,
                1 => &mut c.r,
                2 => &mut c.g,
                _ => &mut c.b,
            };
            let mut sel = cx.curve_sel.filter(|s| s.0 == k).map(|s| s.1);
            curve_plot(ui, pts, &mut sel, color);
            *cx.curve_sel = sel.map(|s| (k, s));
            if !is_identity_pts(pts) && ui.small_button("Reset this curve").clicked() {
                *pts = identity_curve();
                *cx.curve_sel = None;
            }
        }
        VKind::Vignette(s) => {
            widgets::grid(("vig", k)).show(ui, |ui| pct_slider(ui, s, "Strength"));
        }
        VKind::Sharpen(a) => {
            widgets::grid(("sharp", k)).show(ui, |ui| {
                ui.label("Amount");
                super::widgets::value(ui, egui::Slider::new(a, 0.0..=3.0).fixed_decimals(2));
                ui.end_row();
            });
        }
        VKind::Denoise(s) => {
            widgets::grid(("dn", k)).show(ui, |ui| pct_slider(ui, s, "Strength"));
        }
        VKind::Deblock(s) => {
            widgets::grid(("db", k)).show(ui, |ui| pct_slider(ui, s, "Strength"));
        }
        VKind::Deband(s) => {
            widgets::grid(("dband", k)).show(ui, |ui| pct_slider(ui, s, "Strength"));
        }
        VKind::Deflicker(n) => {
            ui.horizontal(|ui| {
                ui.label("Window");
                super::widgets::value(ui, egui::DragValue::new(n).range(2..=30).speed(0.2).suffix(" frames"));
            });
        }
        VKind::Area(a) => {
            ui.horizontal(|ui| {
                for (s, label) in [(AreaShape::Blur, "Blur"), (AreaShape::Pixelate, "Pixelate"), (AreaShape::Fill, "Cover")] {
                    if ui.selectable_label(a.shape == s, label).clicked() {
                        a.shape = s;
                    }
                }
            });
            if a.shape == AreaShape::Fill {
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    ui.color_edit_button_srgb(&mut a.color);
                });
            } else {
                widgets::grid(("area", k)).show(ui, |ui| pct_slider(ui, &mut a.amount, if a.shape == AreaShape::Blur { "Blur" } else { "Block size" }));
            }
            for (title, pair) in [("Position", [("X", 0usize), ("Y", 1)]), ("Size", [("Width", 2), ("Height", 3)])] {
                widgets::field(ui, title, |ui| {
                    for (label, i) in pair {
                        let v = match i {
                            0 => &mut a.x,
                            1 => &mut a.y,
                            2 => &mut a.w,
                            _ => &mut a.h,
                        };
                        let mut p = *v * 100.0;
                        if widgets::num(ui, egui::DragValue::new(&mut p).range(0.0..=100.0).speed(0.3).suffix(" %").fixed_decimals(1))
                            .on_hover_text(label)
                            .changed()
                        {
                            *v = p / 100.0;
                        }
                    }
                });
            }
            a.w = a.w.clamp(0.02, 1.0);
            a.h = a.h.clamp(0.02, 1.0);
            a.x = a.x.clamp(0.0, 1.0 - a.w);
            a.y = a.y.clamp(0.0, 1.0 - a.h);
            range_ui(ui, &mut a.range, cx.now, cx.dur);
        }
        VKind::Text(t) => {
            ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(2).desired_width(f32::INFINITY).hint_text("Text"));
            widgets::grid(("text", k)).show(ui, |ui| {
                ui.label("Font");
                font_combo(ui, ("font", k), &mut t.font, cx.fonts);
                ui.end_row();
                ui.label("Size");
                super::widgets::value(ui, egui::Slider::new(&mut t.size, 1.0..=30.0).suffix(" %").fixed_decimals(1))
                    .on_hover_text("Percent of the picture height");
                ui.end_row();
                ui.label("Colour");
                ui.color_edit_button_srgb(&mut t.color);
                ui.end_row();
                ui.label("Outline");
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut t.outline_color);
                    widgets::num(ui, egui::DragValue::new(&mut t.outline).range(0.0..=30.0).speed(0.2).suffix(" %").fixed_decimals(0))
                        .on_hover_text("Thickness, percent of the font size (0 = none)");
                });
                ui.end_row();
                ui.label("Background");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut t.boxed, "");
                    ui.add_enabled_ui(t.boxed, |ui| ui.color_edit_button_srgb(&mut t.box_color));
                });
                ui.end_row();
                ui.label("Align");
                ui.horizontal(|ui| {
                    for (label, x, h) in [("⬅", 0.05, HAlign::Left), ("⬌", 0.5, HAlign::Center), ("➡", 0.95, HAlign::Right)] {
                        if ui.selectable_label(t.h == h, label).on_hover_text("Align to the side").clicked() {
                            t.x = x;
                            t.h = h;
                        }
                    }
                });
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    for (label, y, v) in [("⬆", 0.06, VAlign::Top), ("⬍", 0.5, VAlign::Middle), ("⬇", 0.94, VAlign::Bottom)] {
                        if ui.selectable_label(t.v == v, label).on_hover_text("Align to the edge").clicked() {
                            t.y = y;
                            t.v = v;
                        }
                    }
                });
                ui.end_row();
            });
            widgets::field(ui, "Position", |ui| {
                for (label, v) in [("X", &mut t.x), ("Y", &mut t.y)] {
                    let mut p = *v * 100.0;
                    if widgets::num(ui, egui::DragValue::new(&mut p).range(0.0..=100.0).speed(0.3).suffix(" %").fixed_decimals(1))
                        .on_hover_text(label)
                        .changed()
                    {
                        *v = p / 100.0;
                    }
                }
            });
            range_ui(ui, &mut t.range, cx.now, cx.dur);
        }
    }
}

fn is_identity_pts(p: &[(f32, f32)]) -> bool {
    p.len() == 2 && p[0] == (0.0, 0.0) && p[1] == (1.0, 1.0)
}

fn caption_ui(ui: &mut egui::Ui, id: &str, c: &mut Caption, fonts: Option<&[Font]>) {
    ui.checkbox(&mut c.on, RichText::new(id).strong());
    if !c.on {
        return;
    }
    ui.add(egui::TextEdit::multiline(&mut c.text).id_salt((id, "text")).desired_rows(2).desired_width(f32::INFINITY).hint_text("Caption text"));
    widgets::grid(("cap", id)).show(ui, |ui| {
        ui.label("Font");
        font_combo(ui, ("capfont", id), &mut c.font, fonts);
        ui.end_row();
        ui.label("Size");
        super::widgets::value(ui, egui::Slider::new(&mut c.size, 1.0..=20.0).suffix(" %").fixed_decimals(1)).on_hover_text("Percent of the picture width");
        ui.end_row();
        ui.label("Colours");
        ui.horizontal(|ui| {
            ui.color_edit_button_srgb(&mut c.color);
            ui.label("on");
            ui.color_edit_button_srgb(&mut c.bg);
        });
        ui.end_row();
        ui.label("Align");
        ui.horizontal(|ui| align_row(ui, &mut c.align));
        ui.end_row();
    });
    ui.add_space(4.0);
}

// ------------------------------------------------------------------ window

fn add_menu(ui: &mut egui::Ui, fx: &mut VideoFx, default_font: &str, cat: &Catalog) -> bool {
    let mut added = false;
    let mut push = |ui: &mut egui::Ui, kind: VKind| {
        fx.effects.push(VEffect::new(kind));
        ui.close();
        added = true;
    };
    widgets::caption(ui, "COLOUR");
    if ui.button("Brightness / contrast / saturation / gamma").clicked() {
        push(ui, VKind::Color(ColorAdj::default()));
    }
    if ui.button("Curves").clicked() {
        push(ui, VKind::Curves(Curves::default()));
    }
    ui.separator();
    widgets::caption(ui, "STYLE");
    if ui.button("Vignette").clicked() {
        push(ui, VKind::Vignette(0.5));
    }
    if ui.button("Sharpen").clicked() {
        push(ui, VKind::Sharpen(1.0));
    }
    ui.separator();
    widgets::caption(ui, "RESTORATION");
    if ui.button("Denoise").clicked() {
        push(ui, VKind::Denoise(0.5));
    }
    if ui.button("Deblock").clicked() {
        push(ui, VKind::Deblock(0.5));
    }
    if ui.button("Deband").clicked() {
        push(ui, VKind::Deband(0.5));
    }
    if ui.button("Deflicker").clicked() {
        push(ui, VKind::Deflicker(5));
    }
    ui.separator();
    widgets::caption(ui, "HIDE A PART OF THE PICTURE");
    if ui.button("Blur area").clicked() {
        push(ui, VKind::Area(Area::new(AreaShape::Blur)));
    }
    if ui.button("Pixelate area").clicked() {
        push(ui, VKind::Area(Area::new(AreaShape::Pixelate)));
    }
    if ui.button("Cover area with a colour").clicked() {
        push(ui, VKind::Area(Area::new(AreaShape::Fill)));
    }
    ui.separator();
    if ui.button("Text").clicked() {
        push(ui, VKind::Text(TextBlock { font: default_font.to_string(), ..Default::default() }));
    }
    ui.separator();
    widgets::form_submenu(ui, "FFmpeg filter", |ui| {
        if let Some(name) = filter_ui::picker(ui, cat, Media::Video) {
            push(ui, VKind::Filter(FilterFx::new(name)));
        }
    })
    .on_hover_text("Any video filter of your ffmpeg, with its settings");
    added
}

impl App {
    /// Body of the "Video effects & text" sidebar section. While it is open the video shows the markers.
    pub fn vfx_body(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.load_fonts(&ctx);
        let fonts = self.vfx.fonts.lock().unwrap().clone();
        let default_font = fonts.as_deref().and_then(ffcut_core::fonts::pick_default).map(|f| f.path.to_string_lossy().into_owned()).unwrap_or_default();

        let mut fx = self.s.vfx.clone();
        // texts made before the fonts were known get the default font
        if fonts.is_some() {
            let texts = fx.effects.iter_mut().filter_map(|e| match &mut e.kind {
                VKind::Text(t) => Some(&mut t.font),
                _ => None,
            });
            for f in [&mut fx.top.font, &mut fx.bottom.font].into_iter().chain(texts) {
                if f.is_empty() {
                    *f = default_font.clone();
                }
            }
        }
        let (now, dur) = (self.player.time(), self.dur());
        let mut sel = self.vfx.sel;
        let mut curve_ch = self.vfx.curve_ch;
        let mut curve_sel = self.vfx.curve_sel;
        let cat = self.catalog.clone();

        widgets::side_box(ui, "Captions", Some("A bar of solid colour above or below the video with text on it, like in video memes."), |ui| {
            caption_ui(ui, "Top", &mut fx.top, fonts.as_deref());
            ui.add_space(6.0);
            caption_ui(ui, "Bottom", &mut fx.bottom, fonts.as_deref());
        });

        ui.add_space(2.0);
        widgets::caption(ui, "Effects").on_hover_text(
            "Colour and restoration are applied to the picture first, then areas and text are drawn on top, each group top to bottom. \
             Click an area or text to show its frame on the video.\nThe preview runs the same ffmpeg filters as the export and refreshes a moment after you stop editing.",
        );
        let mut act: Option<StackAct> = None;
        let count = fx.effects.len();
        for (k, e) in fx.effects.iter_mut().enumerate() {
            let desc = match &e.kind {
                VKind::Filter(f) => cat.desc(&f.name),
                _ => None,
            };
            let name = e.name().to_string();
            let entry = StackEntry { k, count, name: &name, tip: desc.as_deref().or(vfx_tip(&e.kind)), highlight: sel == Some(k) && e.is_overlay() };
            let mut on = e.on;
            let mut cx = Ctx { fonts: fonts.as_deref(), now, dur, curve_ch: &mut curve_ch, curve_sel: &mut curve_sel, cat: &cat };
            let resp = widgets::stack_card(ui, entry, &mut on, &mut act, |ui| effect_body(ui, k, e, &mut cx));
            e.on = on;
            // pressing anywhere on a card selects it (its frame shows on the video)
            if ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| resp.rect.contains(p))) {
                sel = Some(k);
            }
        }
        if let Some(a) = act {
            a.apply(&mut fx.effects);
            sel = None;
            curve_sel = None;
        }
        if fx.effects.is_empty() {
            widgets::hint(ui, "No effects yet.");
        }
        ui.horizontal(|ui| {
            widgets::menu_button(ui, "+ Add effect", |ui| {
                if add_menu(ui, &mut fx, &default_font, &cat) {
                    sel = Some(fx.effects.len() - 1);
                }
            });
            if ui
                .add_enabled(!fx.effects.is_empty() || fx.top.on || fx.bottom.on, egui::Button::new("Reset all"))
                .on_hover_text("Remove every effect and both captions")
                .clicked()
            {
                fx = VideoFx::default();
                sel = None;
                curve_sel = None;
            }
        });
        if let Some(e) = &self.vfx.error {
            widgets::error(ui, format!("The preview could not apply this: {e}"));
            let missing = ["No such filter", "Could not find module", "not found"].iter().any(|m| e.contains(m));
            if missing && fx.effects.iter().any(|e| e.on && matches!(e.kind, VKind::Filter(_))) {
                widgets::hint(ui, "The preview uses the player's own ffmpeg, which may lack a filter that yours has: the export can still work.");
            }
        }

        if fx != self.s.vfx {
            self.s.vfx = fx;
        }
        self.vfx.sel = sel.filter(|k| *k < self.s.vfx.effects.len());
        self.vfx.curve_ch = curve_ch;
        self.vfx.curve_sel = curve_sel;
    }

    /// Frame of the selected area / marker of the selected text on the video. `rect` = the picture (without bars).
    pub fn vfx_overlay(&mut self, ui: &mut egui::Ui, rect: Rect) {
        if !self.vfx.window {
            return;
        }
        let Some(k) = self.vfx.sel else { return };
        let Some(e) = self.s.vfx.effects.get_mut(k) else { return };
        if !e.on {
            return;
        }
        let id = ui.id().with(("vfx-overlay", k));
        match &mut e.kind {
            VKind::Area(a) => {
                let mut r = [a.x, a.y, a.x + a.w, a.y + a.h];
                area_frame(ui, id, rect, &mut r);
                [a.x, a.y, a.w, a.h] = [r[0], r[1], r[2] - r[0], r[3] - r[1]];
            }
            VKind::Text(t) => {
                let c = pos2(rect.left() + t.x * rect.width(), rect.top() + t.y * rect.height());
                let resp = ui.interact(Rect::from_center_size(c, vec2(26.0, 26.0)), id, Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
                if resp.dragged() {
                    let d = resp.drag_delta();
                    t.x = (t.x + d.x / rect.width()).clamp(0.0, 1.0);
                    t.y = (t.y + d.y / rect.height()).clamp(0.0, 1.0);
                }
                let p = ui.painter();
                let s = Stroke::new(1.5, theme::ACCENT);
                p.circle_stroke(c, 9.0, s);
                p.line_segment([c - vec2(14.0, 0.0), c + vec2(14.0, 0.0)], s);
                p.line_segment([c - vec2(0.0, 14.0), c + vec2(0.0, 14.0)], s);
            }
            _ => {}
        }
    }
}

/// A draggable, resizable frame over `rect`; `e` = edges (x0, y0, x1, y1) as fractions of it.
fn area_frame(ui: &mut egui::Ui, id: egui::Id, rect: Rect, e: &mut [f32; 4]) {
    let to = |x: f32, y: f32| pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
    let cr = Rect::from_min_max(to(e[0], e[1]), to(e[2], e[3]));
    let p = ui.painter();
    p.rect_filled(cr, 0.0, theme::ACCENT.gamma_multiply(0.18));
    p.rect_stroke(cr, 0.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
    let zones = widgets::grips(cr, 12.0);
    if let Some((g, d)) = widgets::drag_grips(ui, id, &zones) {
        widgets::move_edges(e, g, vec2(d.x / rect.width(), d.y / rect.height()), vec2(1.0, 1.0), 0.02);
    }
    widgets::paint_grips(ui.painter(), &zones, theme::ACCENT, 8.0);
}
