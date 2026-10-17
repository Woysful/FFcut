use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(255, 138, 61); // trim / primary actions
pub const CROP: Color32 = Color32::from_rgb(86, 200, 250); // crop overlay
pub const OK: Color32 = Color32::from_rgb(110, 207, 140);
pub const WARN: Color32 = Color32::from_rgb(255, 196, 77);
pub const ERR: Color32 = Color32::from_rgb(255, 105, 97);
pub const BG: Color32 = Color32::from_rgb(22, 23, 26);
pub const PANEL: Color32 = Color32::from_rgb(30, 31, 35);
pub const TRACK: Color32 = Color32::from_rgb(44, 46, 52);

/// JetBrains Mono for the whole interface and the command field (Manrope only as a fallback).
/// egui's built-in fonts stay behind them as a fallback for symbols they do not have.
fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};
    use std::sync::Arc;
    let mut f = FontDefinitions::default();
    f.font_data.insert("Manrope".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Manrope-Medium.ttf"))));
    f.font_data.insert("JetBrainsMono".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"))));
    let prop = f.families.entry(FontFamily::Proportional).or_default();
    // JetBrains Mono is the interface font (like the timer); Manrope stays behind it as a fallback for symbols
    prop.insert(0, "JetBrainsMono".into());
    prop.insert(1, "Manrope".into());
    f.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".into());
    ctx.set_fonts(f);
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(36, 37, 42);
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(34, 35, 39);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = CROP;
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(52, 54, 61);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(66, 69, 78);
    v.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.6);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        // JetBrains Mono is wide: slightly smaller sizes keep the layout as it was
        use egui::{FontId, TextStyle::*};
        for (st, size) in [(Body, 12.5), (Button, 12.5), (Small, 10.5), (Monospace, 12.5), (Heading, 16.0)] {
            let fam = if st == Monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
            s.text_styles.insert(st, FontId::new(size, fam));
        }
    });
}
