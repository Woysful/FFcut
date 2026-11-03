//! Colours, sizes and the egui style of the whole interface. Widgets take their colours from here, so the
//! look can be changed in one place: the theme, the accent colour and the transparency of Settings →
//! Appearance are set with [`configure`], and the functions below always give the current values.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Shadow, Stroke, Visuals};
use std::sync::RwLock;

/// The look of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Standard,
    /// Black and white, square corners, hard lines, heavy grotesk titles.
    Brutalist,
    /// Night blue and periwinkle, cut and round corners, wide titles, stars and brackets.
    Y2k,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Standard, Kind::Brutalist, Kind::Y2k];

    /// The name kept in settings.json.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Standard => "standard",
            Kind::Brutalist => "brutalist",
            Kind::Y2k => "y2k",
        }
    }
    pub fn from_id(s: &str) -> Kind {
        Kind::ALL.into_iter().find(|k| k.id() == s).unwrap_or_default()
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Standard => "Standard",
            Kind::Brutalist => "Technical brutalist",
            Kind::Y2k => "Cyber Y2K",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Kind::Standard => "Calm dark grey, soft corners.",
            Kind::Brutalist => "Black and white, hard lines, square corners.",
            Kind::Y2k => "Night blue, wide type, stars and brackets.",
        }
    }
    pub fn default_accent(self) -> Color32 {
        self.palette().accent
    }
    fn palette(self) -> &'static Palette {
        match self {
            Kind::Standard => &STANDARD,
            Kind::Brutalist => &BRUTALIST,
            Kind::Y2k => &Y2K,
        }
    }
    /// Surfaces of the theme, darkest to lightest, and its text colour (for previews of it).
    pub fn swatches(self) -> [Color32; 5] {
        let p = self.palette();
        [p.bg, p.panel, p.card, p.button_hover, p.text]
    }
}

#[derive(Clone, Copy)]
struct Palette {
    accent: Color32,
    crop: Color32,
    ok: Color32,
    warn: Color32,
    err: Color32,
    bg: Color32,
    panel: Color32,
    window: Color32,
    card: Color32,
    track: Color32,
    button: Color32,
    button_hover: Color32,
    field: Color32,
    field_line: Color32,
    field_line_hover: Color32,
    band: Color32,
    line: Color32,
    line_faint: Color32,
    faint_bg: Color32,
    text_strong: Color32,
    text: Color32,
    text_label: Color32,
    text_dim: Color32,
    text_off: Color32,
    /// Corner radii: widgets, cards, panels.
    radius: [u8; 3],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
const fn gray(v: u8) -> Color32 {
    Color32::from_gray(v)
}

const STANDARD: Palette = Palette {
    accent: rgb(255, 138, 61), // trim / primary actions
    crop: rgb(86, 200, 250),   // crop overlay, links, added tracks
    ok: rgb(110, 207, 140),
    warn: rgb(255, 196, 77),
    err: rgb(255, 105, 97),
    bg: rgb(22, 23, 26),
    panel: rgb(30, 31, 35),
    window: rgb(36, 37, 42),
    card: rgb(40, 42, 48),
    track: rgb(44, 46, 52),
    button: rgb(52, 54, 61),
    button_hover: rgb(66, 69, 78),
    // value fields are recessed (darker than the panel, with an outline) so they never look like buttons
    field: rgb(23, 24, 28),
    field_line: gray(62),
    field_line_hover: gray(92),
    band: rgb(62, 65, 75),
    line: gray(54),
    line_faint: gray(44),
    faint_bg: rgb(34, 35, 39),
    text_strong: gray(235),
    text: gray(215),
    text_label: gray(170),
    text_dim: gray(140),
    text_off: gray(120),
    radius: [6, 8, 10],
};

const BRUTALIST: Palette = Palette {
    accent: rgb(204, 255, 0),
    crop: rgb(64, 208, 255),
    ok: rgb(80, 230, 120),
    warn: rgb(255, 200, 0),
    err: rgb(255, 72, 56),
    bg: gray(6),
    panel: gray(13),
    window: gray(16),
    card: gray(18),
    track: gray(30),
    button: gray(22),
    button_hover: gray(44),
    field: gray(0),
    field_line: gray(86),
    field_line_hover: gray(210),
    band: gray(64),
    line: gray(78),
    line_faint: gray(42),
    faint_bg: gray(20),
    text_strong: gray(255),
    text: gray(232),
    text_label: gray(170),
    text_dim: gray(128),
    text_off: gray(98),
    radius: [0, 0, 0],
};

const Y2K: Palette = Palette {
    accent: rgb(143, 168, 255),
    crop: rgb(255, 128, 222),
    ok: rgb(110, 240, 196),
    warn: rgb(255, 214, 120),
    err: rgb(255, 104, 140),
    bg: rgb(5, 8, 19),
    panel: rgb(10, 15, 33),
    window: rgb(14, 21, 44),
    card: rgb(16, 24, 50),
    track: rgb(22, 32, 64),
    button: rgb(21, 31, 63),
    button_hover: rgb(34, 49, 96),
    field: rgb(5, 9, 22),
    field_line: rgb(46, 63, 116),
    field_line_hover: rgb(124, 148, 226),
    band: rgb(44, 62, 120),
    line: rgb(40, 57, 106),
    line_faint: rgb(27, 38, 74),
    faint_bg: rgb(13, 19, 41),
    text_strong: rgb(244, 247, 255),
    text: rgb(214, 222, 255),
    text_label: rgb(158, 172, 222),
    text_dim: rgb(120, 134, 190),
    text_off: rgb(90, 102, 152),
    radius: [9, 12, 16],
};

#[derive(Clone, Copy)]
struct Look {
    kind: Kind,
    p: Palette,
    /// Opacity of the surfaces of the main window, and of menus, tooltips and dialogs.
    window_alpha: f32,
    popup_alpha: f32,
    /// The one of the two that applies to what is being drawn now (see [`dialog_scope`]).
    surface: f32,
}

static LOOK: RwLock<Look> = RwLock::new(Look { kind: Kind::Standard, p: STANDARD, window_alpha: 1.0, popup_alpha: 1.0, surface: 1.0 });

fn look() -> Look {
    *LOOK.read().unwrap_or_else(|e| e.into_inner())
}

/// Chooses the look; `accent` = `None` keeps the theme's own. Call [`apply`] after it.
pub fn configure(kind: Kind, accent: Option<Color32>, window_alpha: f32, popup_alpha: f32) {
    let mut p = *kind.palette();
    if let Some(a) = accent {
        p.accent = a;
    }
    let (window_alpha, popup_alpha) = (window_alpha.clamp(0.2, 1.0), popup_alpha.clamp(0.2, 1.0));
    *LOOK.write().unwrap_or_else(|e| e.into_inner()) = Look { kind, p, window_alpha, popup_alpha, surface: window_alpha };
}

/// Runs `f` with the surfaces of a dialog window (they follow the opacity of menus and dialogs).
pub fn dialog_scope<R>(f: impl FnOnce() -> R) -> R {
    let set = |a: fn(&Look) -> f32| {
        let mut l = LOOK.write().unwrap_or_else(|e| e.into_inner());
        l.surface = a(&l);
    };
    set(|l| l.popup_alpha);
    let r = f();
    set(|l| l.window_alpha);
    r
}

pub fn kind() -> Kind {
    look().kind
}
pub fn popup_alpha() -> f32 {
    look().popup_alpha
}

macro_rules! colours {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {
        $( $(#[$m])* pub fn $name() -> Color32 { look().p.$name } )*
    };
}
colours!(
    /// Trim, primary actions, selection.
    accent,
    /// The crop overlay, links, added tracks.
    crop,
    ok,
    warn,
    err,
    track,
    field_line,
    field_line_hover,
    band,
    line,
    line_faint,
    text_strong,
    text,
    text_label,
    text_dim,
    text_off,
);

/// The darkest surface (behind the video). Translucent when the window is.
pub fn bg() -> Color32 {
    let l = look();
    l.p.bg.gamma_multiply(l.surface)
}
/// The surface of the panels. Translucent when the window is.
pub fn panel() -> Color32 {
    let l = look();
    l.p.panel.gamma_multiply(l.surface)
}
/// Boxes on a panel: half as see-through as the panel, so they stay readable.
pub fn card() -> Color32 {
    let l = look();
    l.p.card.gamma_multiply((1.0 + l.surface) / 2.0)
}
/// Value fields: recessed, darker than the panel.
pub fn field() -> Color32 {
    let l = look();
    l.p.field.gamma_multiply((1.0 + l.surface) / 2.0)
}
/// Text and icons on an accent fill: dark on a light accent, white on a dark one.
pub fn on_accent() -> Color32 {
    on(accent())
}
/// Dark or light, whichever reads better on `c`.
pub fn on(c: Color32) -> Color32 {
    let l = 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
    if l > 150.0 {
        Color32::from_gray(12)
    } else {
        Color32::WHITE
    }
}

/// Font sizes (points).
pub mod size {
    pub const SMALL: f32 = 11.0;
    pub const HINT: f32 = 11.5;
    pub const BODY: f32 = 12.5;
    pub const TITLE: f32 = 13.5;
    pub const HEADING: f32 = 16.0;
    pub const PAGE: f32 = 18.0;
}

/// Corner radii of the theme.
pub mod radius {
    pub fn widget() -> u8 {
        super::look().p.radius[0]
    }
    pub fn card() -> u8 {
        super::look().p.radius[1]
    }
    pub fn panel() -> u8 {
        super::look().p.radius[2]
    }
    /// A radius drawn by hand, in the proportion of the theme (none in the square one).
    pub fn of(r: f32) -> f32 {
        r * super::look().p.radius[0] as f32 / 6.0
    }
}

/// The font of titles: the theme's display face (JetBrains Mono in the standard one). Wide faces come
/// out a little smaller, so titles take about the same room.
pub fn display(size: f32) -> FontId {
    display_of(kind(), size)
}
/// [`display`] of another theme (for previews of it).
pub fn display_of(kind: Kind, size: f32) -> FontId {
    match kind {
        Kind::Standard => FontId::proportional(size),
        Kind::Brutalist => FontId::new(size * 0.96, FontFamily::Name(BRUTAL_FONT.into())),
        Kind::Y2k => FontId::new(size * 0.8, FontFamily::Name(Y2K_FONT.into())),
    }
}

/// A title as the theme writes it: upper case in the new themes.
pub fn title(s: &str) -> String {
    match kind() {
        Kind::Standard => s.to_string(),
        _ => s.to_uppercase(),
    }
}

/// A title in the display face; `strong` titles in the standard theme are bold-ish (brighter).
pub fn heading(s: &str, size: f32) -> RichText {
    let t = RichText::new(title(s)).font(display(size)).color(text_strong());
    if kind() == Kind::Standard {
        t.strong()
    } else {
        t
    }
}

const BRUTAL_FONT: &str = "brutal";
const Y2K_FONT: &str = "y2k";

/// JetBrains Mono for the whole interface and the command field (Manrope only as a fallback); Archivo
/// Black and Michroma for the titles of the brutalist and the Y2K theme. egui's built-in fonts stay behind
/// them as a fallback for symbols they do not have.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions};
    use std::sync::Arc;
    let mut f = FontDefinitions::default();
    let fonts: [(&str, &'static [u8]); 4] = [
        ("Manrope", include_bytes!("../assets/fonts/Manrope-Medium.ttf")),
        ("JetBrainsMono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        ("ArchivoBlack", include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf")),
        ("Michroma", include_bytes!("../assets/fonts/Michroma-Regular.ttf")),
    ];
    for (name, bytes) in fonts {
        f.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let prop = f.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "JetBrainsMono".into());
    prop.insert(1, "Manrope".into());
    let prop = prop.clone();
    f.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".into());
    for (family, face) in [(BRUTAL_FONT, "ArchivoBlack"), (Y2K_FONT, "Michroma")] {
        let mut list = prop.clone();
        list.insert(0, face.into());
        f.families.insert(FontFamily::Name(family.into()), list);
    }
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
    w.inactive.weak_bg_fill = field();
    w.inactive.bg_stroke = Stroke::new(1.0, field_line());
    w.hovered.weak_bg_fill = field();
    w.hovered.bg_stroke = Stroke::new(1.0, field_line_hover());
    w.active.weak_bg_fill = field();
    w.active.bg_stroke = Stroke::new(1.0, accent());
    w.hovered.expansion = 0.0;
    w.active.expansion = 0.0;
}

/// Sets the egui style of the current look (call after [`configure`]).
pub fn apply(ctx: &egui::Context) {
    let l = look();
    let p = &l.p;
    let mut v = Visuals::dark();
    v.panel_fill = panel();
    v.window_fill = p.window.gamma_multiply(l.popup_alpha);
    v.extreme_bg_color = field();
    v.faint_bg_color = p.faint_bg;
    v.selection.bg_fill = p.accent.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.crop;
    let r = CornerRadius::same(p.radius[0]);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(p.radius[2]);
    v.menu_corner_radius = CornerRadius::same(p.radius[1]);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.noninteractive.fg_stroke.color = p.text;
    v.widgets.inactive.weak_bg_fill = p.button;
    v.widgets.hovered.weak_bg_fill = p.button_hover;
    v.widgets.active.weak_bg_fill = p.accent.gamma_multiply(0.6);
    // egui offsets it down and to the right; dropdowns and menus get it evenly around them
    v.popup_shadow.offset = [0, 0];
    match l.kind {
        Kind::Standard => {}
        Kind::Brutalist => {
            // every control outlined; the outline lights up under the pointer
            v.widgets.inactive.bg_fill = p.button;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, gray(96));
            v.widgets.hovered.bg_stroke = Stroke::new(1.5, gray(240));
            v.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);
            v.widgets.open.bg_stroke = Stroke::new(1.5, p.accent);
            v.widgets.hovered.expansion = 0.0;
            v.widgets.active.expansion = 0.0;
            v.window_stroke = Stroke::new(1.5, gray(220));
            v.popup_shadow = Shadow::NONE;
            v.window_shadow = Shadow::NONE;
            v.selection.bg_fill = p.accent.gamma_multiply(0.45);
        }
        Kind::Y2k => {
            v.widgets.inactive.bg_fill = p.button;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, rgb(52, 70, 128));
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, rgb(176, 194, 255));
            v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
            v.widgets.open.bg_stroke = Stroke::new(1.0, p.accent);
            v.window_stroke = Stroke::new(1.0, rgb(70, 92, 160));
            // a soft glow of the accent instead of a shadow
            v.popup_shadow = Shadow { offset: [0, 0], blur: 18, spread: 0, color: p.accent.gamma_multiply(0.22) };
            v.window_shadow = v.popup_shadow;
        }
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        // tooltips appear only when the pointer rests on a control, not while it travels over the menus
        s.interaction.tooltip_delay = 0.9;
        s.interaction.show_tooltips_only_when_still = true;
        // JetBrains Mono is wide: slightly smaller sizes keep the layout compact
        use egui::TextStyle::*;
        for (st, sz) in [(Body, size::BODY), (Button, size::BODY), (Small, 10.5), (Monospace, size::BODY)] {
            let fam = if st == Monospace { FontFamily::Monospace } else { FontFamily::Proportional };
            s.text_styles.insert(st, FontId::new(sz, fam));
        }
        s.text_styles.insert(Heading, display(size::HEADING));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_round_trip() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_id(k.id()), k);
        }
        assert_eq!(Kind::from_id(""), Kind::Standard);
        assert_eq!(Kind::from_id("no such theme"), Kind::Standard);
    }

    #[test]
    fn text_on_a_colour_reads() {
        assert_eq!(on(Color32::WHITE), gray(12));
        assert_eq!(on(BRUTALIST.accent), gray(12));
        assert_eq!(on(rgb(20, 30, 120)), Color32::WHITE);
    }
}
