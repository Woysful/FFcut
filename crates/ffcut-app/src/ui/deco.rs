//! The ornaments of the brutalist and the Y2K theme: corner brackets, cut corners, stars, barcodes,
//! technical captions. The standard theme draws none of them.

use crate::theme::{self, Kind};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke};

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

/// A four-pointed star (a sparkle) of radius `r`.
pub fn star(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    let w = r * 0.26;
    for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
        let (nx, ny) = (-dy, dx);
        p.add(Shape::convex_polygon(
            vec![pos2(c.x + nx * w, c.y + ny * w), pos2(c.x + dx * r, c.y + dy * r), pos2(c.x - nx * w, c.y - ny * w)],
            col,
            Stroke::NONE,
        ));
    }
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
pub fn dots(p: &egui::Painter, left: Pos2, n: usize, filled: usize, r: f32, col: Color32) {
    for i in 0..n {
        let c = pos2(left.x + i as f32 * (r * 2.0 + 4.0), left.y);
        if i < filled {
            p.circle_filled(c, r, col);
        } else {
            p.circle_stroke(c, r - 0.5, Stroke::new(1.0, col));
        }
    }
}

/// A small technical caption.
pub fn caption(p: &egui::Painter, at: Pos2, align: Align2, text: &str, col: Color32) -> Rect {
    p.text(at, align, text, FontId::monospace(9.5), col)
}

/// The outline of `r` with its top-right and bottom-left corners cut off by `cut`.
pub fn chamfer(r: Rect, cut: f32) -> Vec<Pos2> {
    vec![
        r.left_top(),
        pos2(r.right() - cut, r.top()),
        pos2(r.right(), r.top() + cut),
        r.right_bottom(),
        pos2(r.left() + cut, r.bottom()),
        pos2(r.left(), r.bottom() - cut),
    ]
}

/// A box in the theme's shape: the frame as it is, with cut corners in the Y2K theme and a corner tab in
/// the brutalist one.
pub fn show_box<R>(frame: egui::Frame, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    match theme::kind() {
        Kind::Standard => frame.show(ui, add),
        Kind::Brutalist => {
            let r = frame.show(ui, add);
            let tab = Rect::from_min_size(r.response.rect.min, vec2(5.0, 5.0));
            ui.painter().rect_filled(tab, 0.0, theme::text_label());
            r
        }
        Kind::Y2k => {
            let (fill, stroke) = (frame.fill, frame.stroke);
            let at = ui.painter().add(Shape::Noop);
            let r = frame.fill(Color32::TRANSPARENT).stroke(Stroke::NONE).show(ui, add);
            let rect = r.response.rect.shrink(stroke.width / 2.0);
            ui.painter().set(at, Shape::convex_polygon(chamfer(rect, 9.0), fill, stroke));
            r
        }
    }
}
