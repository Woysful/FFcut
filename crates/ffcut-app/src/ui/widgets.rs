//! Small building blocks shared by the sidebar, the dialogs and the effect editors, so that the same kind of
//! control looks and behaves the same everywhere.

use crate::theme::{self, radius, size};
use eframe::egui::{self, Color32, RichText};

/// Width of the label column of every form row, so fields line up from one group to the next.
pub const LABEL_W: f32 = 104.0;
/// Width of every numeric field.
pub const NUM_W: f32 = 84.0;

/// One form row: a label in the fixed column, then the controls.
pub fn field<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(egui::vec2(LABEL_W, h));
            ui.add(egui::Label::new(RichText::new(label).color(theme::TEXT_LABEL)).truncate());
        });
        add(ui)
    })
    .inner
}

/// Adds a right-click menu with "Reset" to a value field; resetting sets `v` to `default` and counts as a change.
pub fn resettable<T: PartialEq>(mut resp: egui::Response, v: &mut T, default: T) -> egui::Response {
    let differs = *v != default;
    let mut reset = false;
    egui::Popup::context_menu(&resp).style(theme::menu_style).show(|ui| {
        if ui.add_enabled(differs, egui::Button::new("Reset")).clicked() {
            reset = true;
            ui.close();
        }
    });
    if reset {
        *v = default;
        resp.mark_changed();
    }
    resp
}

/// Adds a value control (drag value, slider) with the field look of [`theme::field_visuals`].
pub fn value(ui: &mut egui::Ui, w: impl egui::Widget) -> egui::Response {
    ui.scope(|ui| {
        theme::field_visuals(ui.visuals_mut());
        ui.add(w)
    })
    .inner
}

/// [`value`] with a fixed size.
pub fn value_sized(ui: &mut egui::Ui, size: impl Into<egui::Vec2>, w: impl egui::Widget) -> egui::Response {
    ui.scope(|ui| {
        theme::field_visuals(ui.visuals_mut());
        ui.add_sized(size, w)
    })
    .inner
}

/// A numeric field of the standard width.
pub fn num(ui: &mut egui::Ui, dv: egui::DragValue<'_>) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    value_sized(ui, [NUM_W, h], dv)
}

/// A menu button whose menu has the padding of [`theme::menu_style`] (outside the menu bar too).
pub fn menu_button<R>(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    let config = egui::containers::menu::MenuConfig::new().style(theme::menu_style);
    egui::containers::menu::MenuButton::new(label.into()).config(config).ui(ui, add).0
}

/// A two-column grid of "label | control" rows with the same label column as [`field`].
pub fn grid(id: impl std::hash::Hash + std::fmt::Debug) -> egui::Grid {
    egui::Grid::new(id).num_columns(2).min_col_width(LABEL_W).spacing([8.0, 6.0])
}

/// Makes the sliders below fill the rest of the row (call once at the top of a box, after its width is set).
pub fn fit_sliders(ui: &mut egui::Ui) {
    ui.spacing_mut().slider_width = (ui.available_width() - LABEL_W - 110.0).max(70.0);
}

/// Dim explanatory text that wraps.
pub fn hint(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size::HINT).color(theme::TEXT_DIM)).wrap())
}

/// A warning line that wraps.
pub fn warn(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size::HINT).color(theme::WARN)).wrap())
}

/// An error line that wraps.
pub fn error(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size::HINT).color(theme::ERR)).wrap())
}

/// A small upper-case label that introduces a list ("EFFECTS", "TRACKS").
pub fn caption(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(RichText::new(text.to_uppercase()).size(size::SMALL).strong().color(theme::TEXT_DIM))
}

/// A short coloured word next to a title ("auto", "edited", "3 on").
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.label(RichText::new(text).size(size::SMALL).color(color))
}

/// The main action of a place: accent fill, dark text.
pub fn primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).strong().color(Color32::BLACK)).fill(theme::ACCENT)
}

/// The frame of a box inside a panel.
pub fn box_frame() -> egui::Frame {
    egui::Frame::NONE.fill(theme::CARD).stroke(egui::Stroke::new(1.0, theme::LINE)).corner_radius(radius::CARD).inner_margin(egui::Margin::same(10))
}

/// A titled box: the building block of the settings pages. `hint` is shown under the title.
pub fn card(ui: &mut egui::Ui, title: &str, hint_text: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    box_frame().corner_radius(radius::PANEL).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(size::TITLE).strong().color(theme::TEXT_STRONG));
        if let Some(h) = hint_text {
            hint(ui, h);
        }
        ui.add_space(8.0);
        add(ui);
    });
    ui.add_space(12.0);
}

/// A compact titled box for the sidebar: the explanation is a tooltip on the title, not text.
pub fn side_box(ui: &mut egui::Ui, title: &str, tip: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    box_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let t = ui.label(RichText::new(title).size(size::BODY).strong().color(theme::TEXT_STRONG));
        if let Some(h) = tip {
            t.on_hover_text(h);
        }
        ui.add_space(4.0);
        fit_sliders(ui);
        add(ui);
    });
    ui.add_space(6.0);
}

/// A centred "nothing here yet" message with a hint below it.
pub fn empty_state(ui: &mut egui::Ui, title: &str, hint_text: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(15.0).color(theme::TEXT));
        ui.add_space(4.0);
        hint(ui, hint_text);
    });
}

/// Non-button content of a menu (labels, checkboxes, value rows), indented so that its text lines up with
/// the text of the menu buttons, which sits `button_padding.x` inside their highlight.
pub fn menu_indent<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let pad = ui.spacing().button_padding.x;
    ui.horizontal(|ui| {
        ui.add_space(pad);
        add(ui)
    })
    .inner
}

/// A context menu that stays open when its fields are clicked (so values can be typed); buttons close it with `ui.close()`.
pub fn ctx_menu(resp: &egui::Response, add: impl FnOnce(&mut egui::Ui)) {
    egui::Popup::context_menu(resp).style(theme::menu_style).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(add);
}

// ------------------------------------------------------------------ effect stacks

/// What the remove / move buttons of an effect stack asked for.
#[derive(Clone, Copy)]
pub enum StackAct {
    Remove(usize),
    Up(usize),
    Down(usize),
}

impl StackAct {
    pub fn apply<T>(self, list: &mut Vec<T>) {
        match self {
            StackAct::Remove(k) => {
                list.remove(k);
            }
            StackAct::Up(k) => list.swap(k, k - 1),
            StackAct::Down(k) => list.swap(k, k + 1),
        }
    }
}

/// How one entry of an effect stack is shown.
pub struct StackEntry<'a> {
    /// Position in the stack, and the stack's length (for the move buttons).
    pub k: usize,
    pub count: usize,
    pub name: &'a str,
    pub tip: Option<&'a str>,
    /// An accent frame: the entry whose marker is on the video.
    pub highlight: bool,
}

/// One entry of an effect stack (audio or video): bypass checkbox, name, ▲ ▼ ✕, then `body`.
pub fn stack_card(ui: &mut egui::Ui, e: StackEntry, on: &mut bool, act: &mut Option<StackAct>, body: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let StackEntry { k, count, name, tip, highlight } = e;
    let stroke = if highlight { egui::Stroke::new(1.5, theme::ACCENT) } else { egui::Stroke::new(1.0, theme::LINE) };
    let r = box_frame().stroke(stroke).show(ui, |ui| {
        ui.set_width(ui.available_width());
        fit_sliders(ui);
        ui.horizontal(|ui| {
            ui.checkbox(on, "").on_hover_text("Bypass: keep it in the list but do not apply it");
            let n = ui.label(RichText::new(name).size(size::BODY).strong().color(if *on { theme::TEXT_STRONG } else { theme::TEXT_OFF }));
            if let Some(t) = tip {
                n.on_hover_text(t);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").on_hover_text("Remove").clicked() {
                    *act = Some(StackAct::Remove(k));
                }
                if ui.add_enabled(k + 1 < count, egui::Button::new("▼").small()).on_hover_text("Apply later").clicked() {
                    *act = Some(StackAct::Down(k));
                }
                if ui.add_enabled(k > 0, egui::Button::new("▲").small()).on_hover_text("Apply earlier").clicked() {
                    *act = Some(StackAct::Up(k));
                }
            });
        });
        ui.add_space(4.0);
        let enabled = *on;
        ui.add_enabled_ui(enabled, body);
    });
    ui.add_space(6.0);
    r.response
}

// ------------------------------------------------------------------ resizable frames on the picture

/// What part of a frame on the picture is being dragged.
#[derive(Clone, Copy)]
pub enum Grip {
    Body,
    Edge { l: bool, t: bool, r: bool, b: bool },
}

impl Grip {
    const fn edge(l: bool, t: bool, r: bool, b: bool) -> Grip {
        Grip::Edge { l, t, r, b }
    }
}

/// The nine grab zones of a frame on screen (body first, so the edges and corners win where they overlap).
pub fn grips(cr: egui::Rect, m: f32) -> [(egui::Rect, Grip, egui::CursorIcon); 9] {
    use egui::{pos2, CursorIcon as C};
    let (l, r, t, b, c) = (cr.left(), cr.right(), cr.top(), cr.bottom(), cr.center());
    let h = |p: egui::Pos2| egui::Rect::from_center_size(p, egui::vec2(m * 1.4, m * 1.4));
    [
        (cr.shrink(m * 0.6), Grip::Body, C::Grab),
        (h(pos2(c.x, t)), Grip::edge(false, true, false, false), C::ResizeVertical),
        (h(pos2(c.x, b)), Grip::edge(false, false, false, true), C::ResizeVertical),
        (h(pos2(l, c.y)), Grip::edge(true, false, false, false), C::ResizeHorizontal),
        (h(pos2(r, c.y)), Grip::edge(false, false, true, false), C::ResizeHorizontal),
        (h(pos2(l, t)), Grip::edge(true, true, false, false), C::ResizeNwSe),
        (h(pos2(r, t)), Grip::edge(false, true, true, false), C::ResizeNeSw),
        (h(pos2(l, b)), Grip::edge(true, false, false, true), C::ResizeNeSw),
        (h(pos2(r, b)), Grip::edge(false, false, true, true), C::ResizeNwSe),
    ]
}

/// Lets the user drag the grab zones; returns the grip being dragged and the drag in screen points.
pub fn drag_grips(ui: &mut egui::Ui, id: egui::Id, zones: &[(egui::Rect, Grip, egui::CursorIcon)]) -> Option<(Grip, egui::Vec2)> {
    let mut active = None;
    for (i, (rect, grip, cursor)) in zones.iter().enumerate().rev() {
        let resp = ui.interact(*rect, id.with(i), egui::Sense::click_and_drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(*cursor);
        }
        if resp.dragged() && active.is_none() {
            active = Some((*grip, resp.drag_delta()));
        }
    }
    active
}

/// Moves the edges `[x0, y0, x1, y1]` (in any unit) by `d` for `grip`, inside `0..=max`, keeping at least `min` size.
pub fn move_edges(e: &mut [f32; 4], grip: Grip, d: egui::Vec2, max: egui::Vec2, min: f32) {
    let [x0, y0, x1, y1] = e;
    match grip {
        Grip::Body => {
            let (w, h) = (*x1 - *x0, *y1 - *y0);
            *x0 = (*x0 + d.x).clamp(0.0, max.x - w);
            *y0 = (*y0 + d.y).clamp(0.0, max.y - h);
            *x1 = *x0 + w;
            *y1 = *y0 + h;
        }
        Grip::Edge { l, t, r, b } => {
            if l {
                *x0 = (*x0 + d.x).clamp(0.0, *x1 - min);
            }
            if r {
                *x1 = (*x1 + d.x).clamp(*x0 + min, max.x);
            }
            if t {
                *y0 = (*y0 + d.y).clamp(0.0, *y1 - min);
            }
            if b {
                *y1 = (*y1 + d.y).clamp(*y0 + min, max.y);
            }
        }
    }
}

/// Draws the small square handles on the edges and corners.
pub fn paint_grips(p: &egui::Painter, zones: &[(egui::Rect, Grip, egui::CursorIcon)], color: Color32, side: f32) {
    for (rect, grip, _) in zones {
        if matches!(grip, Grip::Edge { .. }) {
            let hr = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
            p.rect_filled(hr, 2.0, color);
            p.rect_stroke(hr, 2.0, egui::Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
        }
    }
}
