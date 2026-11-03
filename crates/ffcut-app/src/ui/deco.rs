//! The ornaments of the brutalist and the Y2K theme: corner brackets, rulers, stars, barcodes, notched
//! boxes, small line-art glyphs, technical captions. The standard theme draws none of them.

use crate::theme::{self, Kind};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use std::f32::consts::{PI, TAU};

/// The theme has ornaments.
pub fn on() -> bool {
    theme::kind() != Kind::Standard
}

/// L-shaped marks at the four corners of `r`.
pub fn brackets(p: &egui::Painter, r: Rect, len: f32, stroke: Stroke) {
    for (c, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
        p.add(Shape::line(vec![pos2(c.x + dx * len, c.y), c, pos2(c.x, c.y + dy * len)], stroke));
    }
}

/// A "+" registration mark.
pub fn cross(p: &egui::Painter, c: Pos2, s: f32, stroke: Stroke) {
    p.line_segment([pos2(c.x - s, c.y), pos2(c.x + s, c.y)], stroke);
    p.line_segment([pos2(c.x, c.y - s), pos2(c.x, c.y + s)], stroke);
}

/// A four-pointed sparkle of radius `r` with curved, pinched sides.
pub fn star(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    // a superellipse with an exponent above 2 has concave sides; it is star-shaped around its centre,
    // so a fan from the centre fills it
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, col);
    const N: u32 = 48;
    for i in 0..N {
        let a = i as f32 / N as f32 * TAU;
        let (s, k) = (a.sin(), a.cos());
        m.colored_vertex(pos2(c.x + r * k.signum() * k.abs().powf(3.0), c.y + r * s.signum() * s.abs().powf(3.0)), col);
    }
    for i in 0..N {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % N);
    }
    p.add(Shape::mesh(m));
}

/// A barcode filling `r`.
pub fn barcode(p: &egui::Painter, r: Rect, col: Color32) {
    // bar, gap, bar... in units
    const W: [u8; 27] = [2, 1, 1, 1, 3, 1, 1, 2, 1, 1, 2, 3, 1, 1, 1, 2, 3, 1, 1, 1, 2, 1, 1, 3, 2, 1, 1];
    let unit = r.width() / W.iter().map(|w| *w as f32).sum::<f32>();
    let mut x = r.left();
    for (i, w) in W.iter().enumerate() {
        let ww = *w as f32 * unit;
        if i % 2 == 0 {
            p.rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(ww, r.height())), 0.0, col);
        }
        x += ww;
    }
}

/// A row of `n` small circles from `left` (the centre of the first), the first `filled` of them filled.
/// Returns the right end of the row.
pub fn dots(p: &egui::Painter, left: Pos2, n: usize, filled: usize, r: f32, col: Color32) -> f32 {
    let step = r * 2.0 + r.max(1.5) * 1.2;
    for i in 0..n {
        let c = pos2(left.x + i as f32 * step, left.y);
        if i < filled {
            p.circle_filled(c, r, col);
        } else {
            p.circle_stroke(c, r - 0.5, Stroke::new(1.0, col));
        }
    }
    left.x + (n.max(1) - 1) as f32 * step + r
}

/// A row of `n` small squares of side `s` from `left` (the middle of the left side of the first), the
/// first `filled` of them filled.
pub fn squares(p: &egui::Painter, left: Pos2, n: usize, filled: usize, s: f32, col: Color32) {
    for i in 0..n {
        let r = Rect::from_min_size(pos2(left.x + i as f32 * (s + 3.0), left.y - s / 2.0), vec2(s, s));
        if i < filled {
            p.rect_filled(r, 0.0, col);
        } else {
            p.rect_stroke(r, 0.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
        }
    }
}

/// Diagonal hatching inside `r`.
pub fn hatch(p: &egui::Painter, r: Rect, gap: f32, stroke: Stroke) {
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    let mut x = r.left() - r.height();
    while x < r.right() {
        p.line_segment([pos2(x, r.bottom()), pos2(x + r.height(), r.top())], stroke);
        x += gap;
    }
}

/// A thin line between `a` and `b` that ends in small rings.
pub fn rail(p: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let r = 2.2;
    let u = (b - a).normalized();
    p.line_segment([a + u * r, b - u * r], stroke);
    p.circle_stroke(a, r, stroke);
    p.circle_stroke(b, r, stroke);
}

/// A wireframe globe.
pub fn globe(p: &egui::Painter, c: Pos2, r: f32, stroke: Stroke) {
    p.circle_stroke(c, r, stroke);
    for k in [0.33, 0.7] {
        p.add(Shape::ellipse_stroke(c, vec2(r * k, r), stroke));
    }
    p.line_segment([pos2(c.x, c.y - r), pos2(c.x, c.y + r)], stroke);
    for k in [-0.55_f32, 0.0, 0.55] {
        let w = r * (1.0 - k * k).sqrt();
        p.line_segment([pos2(c.x - w, c.y + k * r), pos2(c.x + w, c.y + k * r)], stroke);
    }
}

/// The outline of a heart about `s` wide.
pub fn heart(p: &egui::Painter, c: Pos2, s: f32, stroke: Stroke) {
    let k = s / 34.0;
    let pts = (0..64)
        .map(|i| {
            let t = i as f32 / 64.0 * TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            pos2(c.x + x * k, c.y - (y + 2.5) * k)
        })
        .collect();
    p.add(Shape::closed_line(pts, stroke));
}

/// The outline of a crescent moon of radius `r`, open to the upper right.
pub fn moon(p: &egui::Painter, c: Pos2, r: f32, stroke: Stroke) {
    // the moon is the disc minus a second, shifted disc; the outline runs along the outer arc between the
    // two crossings, then back along the inner one
    let (cb, rb) = (c + vec2(r * 0.42, -r * 0.32), r * 0.82);
    let d = (cb - c).length();
    let a = (r * r - rb * rb + d * d) / (2.0 * d);
    let h = (r * r - a * a).max(0.0).sqrt();
    let u = (cb - c) / d;
    let mid = c + u * a;
    let (i1, i2) = (mid + u.rot90() * h, mid - u.rot90() * h);
    let mut pts = arc(c, r, i1, i2, -u);
    let mut back = arc(cb, rb, i2, i1, -u);
    pts.append(&mut back);
    p.add(Shape::closed_line(pts, stroke));
}

/// Points of the arc of the circle (`c`, `r`) from `from` to `to` that passes on the side of `via`.
fn arc(c: Pos2, r: f32, from: Pos2, to: Pos2, via: Vec2) -> Vec<Pos2> {
    let (a0, a1) = ((from - c).angle(), (to - c).angle());
    let mut sweep = (a1 - a0).rem_euclid(TAU);
    if Vec2::angled(a0 + sweep / 2.0).dot(via) < 0.0 {
        sweep -= TAU;
    }
    let n = ((sweep.abs() * r / 2.5).ceil() as usize).clamp(4, 64);
    (0..=n).map(|i| c + r * Vec2::angled(a0 + sweep * i as f32 / n as f32)).collect()
}

/// The outline of `r` with round corners and two openings: `top` and `bottom` are the x ranges left
/// open on the top and the bottom side.
pub fn gapped_frame(p: &egui::Painter, r: Rect, rad: f32, top: (f32, f32), bottom: (f32, f32), stroke: Stroke) {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let corner = |c: Pos2, from: Pos2, to: Pos2| arc(c, rad, from, to, from - c + (to - c));
    let mut right = vec![pos2(top.1.max(l + rad), t)];
    right.extend(corner(pos2(rt - rad, t + rad), pos2(rt - rad, t), pos2(rt, t + rad)));
    right.extend(corner(pos2(rt - rad, b - rad), pos2(rt, b - rad), pos2(rt - rad, b)));
    right.push(pos2(bottom.1.min(rt - rad), b));
    let mut left = vec![pos2(bottom.0.max(l + rad), b)];
    left.extend(corner(pos2(l + rad, b - rad), pos2(l + rad, b), pos2(l, b - rad)));
    left.extend(corner(pos2(l + rad, t + rad), pos2(l, t + rad), pos2(l + rad, t)));
    left.push(pos2(top.0.min(rt - rad), t));
    p.add(Shape::line(right, stroke));
    p.add(Shape::line(left, stroke));
}

/// A small technical caption.
pub fn caption(p: &egui::Painter, at: Pos2, align: Align2, text: &str, col: Color32) -> Rect {
    p.text(at, align, text, FontId::monospace(9.5), col)
}

/// A caption that reads from bottom to top, centred on `c`.
pub fn caption_up(p: &egui::Painter, c: Pos2, text: &str, col: Color32) -> Rect {
    let g = p.layout_no_wrap(text.to_string(), FontId::monospace(9.5), col);
    let (w, h) = (g.size().x, g.size().y);
    // turned by -90°, the text runs up from its anchor and its top faces left
    let at = pos2(c.x - h / 2.0, c.y + w / 2.0);
    p.add(egui::epaint::TextShape::new(at, g, col).with_angle(-PI / 2.0));
    Rect::from_center_size(c, vec2(h, w))
}

/// The polygon `pts` with its corners rounded: `r[i]` is the radius at `pts[i]` (smaller where the sides
/// are too short for it).
pub fn rounded(pts: &[Pos2], r: &[f32]) -> Vec<Pos2> {
    let n = pts.len();
    let mut out: Vec<Pos2> = Vec::with_capacity(n * 8);
    for i in 0..n {
        let (a, v, b) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let (da, db) = (a - v, b - v);
        let (la, lb) = (da.length(), db.length());
        let half = if la > 0.01 && lb > 0.01 { (da / la).dot(db / lb).clamp(-1.0, 1.0).acos() / 2.0 } else { 0.0 };
        if r[i] <= 0.0 || half.sin() < 0.02 || half > PI / 2.0 - 0.02 {
            out.push(v);
            continue;
        }
        let (ua, ub) = (da / la, db / lb);
        let t = (r[i] / half.tan()).min(la / 2.0).min(lb / 2.0);
        let rr = t * half.tan();
        let c = v + (ua + ub).normalized() * (rr / half.sin());
        out.extend(arc(c, rr, v + ua * t, v + ub * t, v - c));
    }
    out.dedup_by(|a, b| (*a - *b).length() < 0.05);
    out
}

/// Fills any simple polygon (egui fills only convex ones) by cutting it into triangles.
pub fn fill(pts: &[Pos2], col: Color32) -> Shape {
    let n = pts.len();
    let mut m = egui::Mesh::default();
    if n < 3 || col.a() == 0 {
        return Shape::Noop;
    }
    for p in pts {
        m.colored_vertex(*p, col);
    }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b - a).x * (c - b).y - (b - a).y * (c - b).x;
    let area: f32 = (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum();
    let ccw = area > 0.0;
    let mut idx: Vec<usize> = (0..n).collect();
    let mut i = 0;
    let mut misses = 0;
    while idx.len() > 3 && misses < idx.len() {
        let k = idx.len();
        let (ia, ib, ic) = (idx[(i + k - 1) % k], idx[i % k], idx[(i + 1) % k]);
        let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
        let turn = cross(a, b, c);
        let ear = if turn.abs() < 1e-4 {
            true // a straight run: drop the point, nothing to fill
        } else if (turn > 0.0) == ccw {
            let inside = |q: Pos2| {
                let (d1, d2, d3) = (cross(a, b, q), cross(b, c, q), cross(c, a, q));
                if ccw {
                    d1 > 0.0 && d2 > 0.0 && d3 > 0.0
                } else {
                    d1 < 0.0 && d2 < 0.0 && d3 < 0.0
                }
            };
            !idx.iter().any(|&j| j != ia && j != ib && j != ic && inside(pts[j]))
        } else {
            false
        };
        if ear {
            if turn.abs() >= 1e-4 {
                m.add_triangle(ia as u32, ib as u32, ic as u32);
            }
            idx.remove(i % k);
            misses = 0;
        } else {
            i += 1;
            misses += 1;
        }
        i %= idx.len().max(1);
    }
    if idx.len() == 3 {
        m.add_triangle(idx[0] as u32, idx[1] as u32, idx[2] as u32);
    }
    Shape::mesh(m)
}

/// The Y2K box: rounded, with a raised tab `tw` wide on the left of the top edge and a lowered one `bw`
/// wide on the right of the bottom edge, both `d` deep.
pub fn tab_shape(r: Rect, rad: f32, d: f32, tw: f32, bw: f32) -> Vec<Pos2> {
    let s = d * 0.9;
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    rounded(
        &[pos2(l, t), pos2(l + tw, t), pos2(l + tw + d, t + d), pos2(rt, t + d), pos2(rt, b), pos2(rt - bw, b), pos2(rt - bw - d, b - d), pos2(l, b - d)],
        &[rad, s, s, rad, rad, s, s, rad],
    )
}

/// The depth of the steps of a Y2K box.
pub const STEP: f32 = 6.0;

/// A box in the theme's shape: the frame as it is in the standard theme, with a corner tab and ruler
/// ticks in the brutalist one, notched and stepped in the Y2K one.
pub fn show_box<R>(frame: egui::Frame, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    match theme::kind() {
        Kind::Standard => frame.show(ui, add),
        Kind::Brutalist => {
            let r = frame.show(ui, add);
            let rect = r.response.rect;
            let p = ui.painter();
            p.rect_filled(Rect::from_min_size(rect.min, vec2(5.0, 5.0)), 0.0, theme::text_label());
            let st = Stroke::new(1.0, theme::text_dim());
            // ruler ticks in the bottom-right corner
            for i in 0..4 {
                let x = rect.right() - 5.0 - i as f32 * 4.0;
                p.vline(x, rect.bottom() - 5.0..=rect.bottom() - 1.0, st);
            }
            r
        }
        Kind::Y2k => {
            let (fill, stroke) = (frame.fill, frame.stroke);
            let rad = frame.corner_radius.nw as f32;
            let at = ui.painter().add(Shape::Noop);
            let mut f = frame.fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
            f.inner_margin.top += 2;
            f.inner_margin.bottom += 2;
            let r = f.show(ui, add);
            let rect = r.response.rect.shrink(stroke.width / 2.0);
            let (tw, bw) = ((rect.width() * 0.42).min(150.0), (rect.width() * 0.3).min(110.0));
            let pts = tab_shape(rect, rad, STEP, tw, bw);
            let p = ui.painter();
            p.set(at, Shape::Vec(vec![self::fill(&pts, fill), Shape::closed_line(pts, stroke)]));
            // small marks in the notches the steps leave
            let ink = theme::text_dim();
            let y = rect.top() + STEP / 2.0 - 0.5;
            let x0 = rect.right() - rad - 30.0;
            if x0 > rect.left() + tw + STEP + 10.0 {
                dots(p, pos2(x0, y), 4, 1, 1.6, ink);
            }
            let bar = Rect::from_min_size(pos2(rect.left() + rad + 4.0, rect.bottom() - STEP * 0.75), vec2(34.0, STEP * 0.5));
            if bar.right() < rect.right() - bw - STEP - 6.0 {
                barcode(p, bar, ink);
            }
            r
        }
    }
}
