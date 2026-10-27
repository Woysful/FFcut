//! Colours, sizes and the egui style of the whole interface. Widgets take their colours from here, so the
//! look can be changed in one place.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

// ---- accents
pub const ACCENT: Color32 = Color32::from_rgb(255, 138, 61); // trim / primary actions
pub const CROP: Color32 = Color32::from_rgb(86, 200, 250); // crop overlay, links, added tracks
pub const OK: Color32 = Color32::from_rgb(110, 207, 140);
pub const WARN: Color32 = Color32::from_rgb(255, 196, 77);
pub const ERR: Color32 = Color32::from_rgb(255, 105, 97);

// ---- surfaces, darkest to lightest
pub const BG: Color32 = Color32::from_rgb(22, 23, 26);
pub const PANEL: Color32 = Color32::from_rgb(30, 31, 35);
pub const WINDOW: Color32 = Color32::from_rgb(36, 37, 42);
pub const CARD: Color32 = Color32::from_rgb(40, 42, 48);
pub const TRACK: Color32 = Color32::from_rgb(44, 46, 52);
pub const BUTTON: Color32 = Color32::from_rgb(52, 54, 61);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(66, 69, 78);
/// Value fields are recessed (darker than the panel, with an outline) so they never look like buttons.
pub const FIELD: Color32 = Color32::from_rgb(23, 24, 28);
pub const FIELD_LINE: Color32 = Color32::from_gray(62);
pub const FIELD_LINE_HOVER: Color32 = Color32::from_gray(92);
pub const BAND: Color32 = Color32::from_rgb(62, 65, 75);
pub const LINE: Color32 = Color32::from_gray(54);
pub const LINE_FAINT: Color32 = Color32::from_gray(44);

// ---- text, brightest to dimmest
pub const TEXT_STRONG: Color32 = Color32::from_gray(235);
pub const TEXT: Color32 = Color32::from_gray(215);
pub const TEXT_LABEL: Color32 = Color32::from_gray(170);
pub const TEXT_DIM: Color32 = Color32::from_gray(140);
pub const TEXT_OFF: Color32 = Color32::from_gray(120);

/// Font sizes (points).
pub mod size {
    pub const SMALL: f32 = 11.0;
    pub const HINT: f32 = 11.5;
    pub const BODY: f32 = 12.5;
    pub const TITLE: f32 = 13.5;
    pub const HEADING: f32 = 16.0;
    pub const PAGE: f32 = 18.0;
}

/// Corner radii.
pub mod radius {
    pub const WIDGET: u8 = 6;
    pub const CARD: u8 = 8;
    pub const PANEL: u8 = 10;
}

/// JetBrains Mono for the whole interface and the command field (Manrope only as a fallback).
/// egui's built-in fonts stay behind them as a fallback for symbols they do not have.
fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};
    use std::sync::Arc;
    let mut f = FontDefinitions::default();
    f.font_data.insert("Manrope".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Manrope-Medium.ttf"))));
    f.font_data.insert("JetBrainsMono".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"))));
    let prop = f.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "JetBrainsMono".into());
    prop.insert(1, "Manrope".into());
    f.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".into());
    ctx.set_fonts(f);
}

/// egui's menu style (no frames on inactive entries) with room on the sides of the text; egui's own
/// padding is 2 px, which makes the hover highlight hug the text.
pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
}

/// The look of a value field (drag value, the number of a slider): recessed, outlined, accent while dragged.
pub fn field_visuals(v: &mut Visuals) {
    let w = &mut v.widgets;
    w.inactive.weak_bg_fill = FIELD;
    w.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
    w.hovered.weak_bg_fill = FIELD;
    w.hovered.bg_stroke = Stroke::new(1.0, FIELD_LINE_HOVER);
    w.active.weak_bg_fill = FIELD;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.expansion = 0.0;
    w.active.expansion = 0.0;
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = WINDOW;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(34, 35, 39);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = CROP;
    let r = CornerRadius::same(radius::WIDGET);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.open.corner_radius = r;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.weak_bg_fill = BUTTON;
    v.widgets.hovered.weak_bg_fill = BUTTON_HOVER;
    v.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.6);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        // tooltips appear only when the pointer rests on a control, not while it travels over the menus
        s.interaction.tooltip_delay = 0.9;
        s.interaction.show_tooltips_only_when_still = true;
        // JetBrains Mono is wide: slightly smaller sizes keep the layout compact
        use egui::{FontId, TextStyle::*};
        for (st, sz) in [(Body, size::BODY), (Button, size::BODY), (Small, 10.5), (Monospace, size::BODY), (Heading, size::HEADING)] {
            let fam = if st == Monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
            s.text_styles.insert(st, FontId::new(sz, fam));
        }
    });
}
