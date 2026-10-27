//! Toolbar icons drawn with the painter: they look the same on every system and need no font symbols.

use eframe::egui::{self, pos2, vec2, Color32, Pos2, Response, Shape, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Play,
    Pause,
    ToStart,
    ToEnd,
    StepBack,
    StepForward,
    Loop,
    Tracks,
    Crop,
}

fn tri(c: Pos2, s: f32, right: bool, col: Color32) -> Shape {
    let d = if right { 1.0 } else { -1.0 };
    Shape::convex_polygon(
        vec![pos2(c.x - d * s * 0.45, c.y - s * 0.55), pos2(c.x - d * s * 0.45, c.y + s * 0.55), pos2(c.x + d * s * 0.55, c.y)],
        col,
        Stroke::NONE,
    )
}

fn draw(p: &egui::Painter, icon: Icon, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.7, col);
    let bar = |x: f32, w: f32, h: f32| Shape::rect_filled(egui::Rect::from_center_size(pos2(x, c.y), vec2(w, h)), 1.0, col);
    match icon {
        Icon::Play => p.add(tri(c, s * 1.1, true, col)),
        Icon::Pause => {
            p.add(bar(c.x - s * 0.3, s * 0.32, s * 1.0));
            p.add(bar(c.x + s * 0.3, s * 0.32, s * 1.0))
        }
        Icon::StepBack => p.add(tri(c, s * 0.8, false, col)),
        Icon::StepForward => p.add(tri(c, s * 0.8, true, col)),
        Icon::ToStart => {
            p.add(bar(c.x - s * 0.5, s * 0.2, s * 0.95));
            p.add(tri(pos2(c.x + s * 0.12, c.y), s * 0.8, false, col))
        }
        Icon::ToEnd => {
            p.add(bar(c.x + s * 0.5, s * 0.2, s * 0.95));
            p.add(tri(pos2(c.x - s * 0.12, c.y), s * 0.8, true, col))
        }
        Icon::Tracks => {
            // three lanes of different length, like clips on a timeline
            for (dy, x0, x1) in [(-0.5, -0.6, 0.35), (0.0, -0.25, 0.6), (0.5, -0.6, 0.1)] {
                p.add(Shape::rect_filled(
                    egui::Rect::from_min_max(pos2(c.x + s * x0, c.y + s * dy - s * 0.14), pos2(c.x + s * x1, c.y + s * dy + s * 0.14)),
                    1.5,
                    col,
                ));
            }
            p.add(Shape::Noop)
        }
        Icon::Crop => {
            let pts = |v: [(f32, f32); 3]| Shape::line(v.iter().map(|(x, y)| pos2(c.x + s * x, c.y + s * y)).collect(), st);
            p.add(pts([(-0.25, -0.65), (-0.25, 0.25), (0.65, 0.25)]));
            p.add(pts([(0.25, 0.65), (0.25, -0.25), (-0.65, -0.25)]))
        }
        Icon::Loop => {
            // an arc with an arrow head: repeat
            let r = s * 0.62;
            let (a0, a1) = (-0.9_f32, 4.5_f32); // radians; clockwise on screen
            let n = 28;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let a = a0 + (a1 - a0) * i as f32 / n as f32;
                    pos2(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, st));
            let end = pos2(c.x + r * a1.cos(), c.y + r * a1.sin());
            let t = vec2(-a1.sin(), a1.cos()); // tangent (direction of travel)
            let n_ = vec2(a1.cos(), a1.sin());
            let tip = end + t * s * 0.42;
            p.add(Shape::convex_polygon(vec![end + n_ * s * 0.3, end - n_ * s * 0.3, tip], col, Stroke::NONE))
        }
    };
}

/// A button showing `icon`. `selected` = toggled on; `fill` overrides the background.
pub fn icon_button(ui: &mut egui::Ui, size: Vec2, icon: Icon, selected: bool, fill: Option<Color32>) -> Response {
    let mut b = egui::Button::new("").selected(selected);
    if let Some(f) = fill {
        b = b.fill(f);
    }
    let resp = ui.add_sized(size, b);
    let col = if selected { Color32::WHITE } else { ui.style().interact(&resp).fg_stroke.color };
    draw(ui.painter(), icon, resp.rect.center(), size.y.min(size.x) * 0.36, col);
    resp
}
