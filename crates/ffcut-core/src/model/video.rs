//! Video effects, covered areas, text blocks and caption bars, as the user set them.
//! `videofx` turns them into ffmpeg filters (and re-exports everything here).

use super::filter::FilterFx;
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VideoFx {
    /// Effects, applied in this order inside their group (see the `videofx` module doc).
    #[serde(default)]
    pub effects: Vec<VEffect>,
    #[serde(default)]
    pub top: Caption,
    #[serde(default)]
    pub bottom: Caption,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VEffect {
    /// Off = bypassed (kept in the list, not applied).
    pub on: bool,
    pub kind: VKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VKind {
    Color(ColorAdj),
    Curves(Curves),
    /// 0..1
    Vignette(f32),
    /// Unsharp amount, 0..3
    Sharpen(f32),
    /// 0..1
    Denoise(f32),
    /// 0..1 (weak → strong)
    Deblock(f32),
    /// 0..1
    Deband(f32),
    /// Averaging window in frames, 2..30
    Deflicker(u32),
    Area(Area),
    Text(TextBlock),
    /// Any ffmpeg video filter, by name (applied with the pixel effects).
    Filter(FilterFx),
}

impl VEffect {
    pub fn new(kind: VKind) -> Self {
        VEffect { on: true, kind }
    }
    pub fn name(&self) -> &str {
        match &self.kind {
            VKind::Color(_) => "Color",
            VKind::Curves(_) => "Curves",
            VKind::Vignette(_) => "Vignette",
            VKind::Sharpen(_) => "Sharpen",
            VKind::Denoise(_) => "Denoise",
            VKind::Deblock(_) => "Deblock",
            VKind::Deband(_) => "Deband",
            VKind::Deflicker(_) => "Deflicker",
            VKind::Area(a) => match a.shape {
                AreaShape::Blur => "Blur area",
                AreaShape::Pixelate => "Pixelate area",
                AreaShape::Fill => "Cover area",
            },
            VKind::Text(_) => "Text",
            VKind::Filter(f) => &f.name,
        }
    }
    /// Drawn on the rotated frame (areas, text) rather than on the decoded picture.
    pub fn is_overlay(&self) -> bool {
        matches!(self.kind, VKind::Area(_) | VKind::Text(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorAdj {
    /// -1..1, 0 = unchanged
    pub brightness: f32,
    /// 0..2, 1 = unchanged
    pub contrast: f32,
    /// 0..3, 1 = unchanged
    pub saturation: f32,
    /// 0.1..4, 1 = unchanged
    pub gamma: f32,
}

impl Default for ColorAdj {
    fn default() -> Self {
        ColorAdj { brightness: 0.0, contrast: 1.0, saturation: 1.0, gamma: 1.0 }
    }
}

/// Tone curves: points (input, output) in 0..1 per channel, sorted by input. Two points `(0,0) (1,1)` = identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curves {
    pub master: Vec<(f32, f32)>,
    pub r: Vec<(f32, f32)>,
    pub g: Vec<(f32, f32)>,
    pub b: Vec<(f32, f32)>,
}

pub fn identity_curve() -> Vec<(f32, f32)> {
    vec![(0.0, 0.0), (1.0, 1.0)]
}

impl Default for Curves {
    fn default() -> Self {
        Curves { master: identity_curve(), r: identity_curve(), g: identity_curve(), b: identity_curve() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AreaShape {
    Blur,
    Pixelate,
    Fill,
}

/// A rectangle of the frame that is blurred, pixelated or covered with a colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub shape: AreaShape,
    /// Position and size as fractions of the frame (0..1).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Blur / pixel size, 0..1.
    pub amount: f32,
    pub color: Rgb,
    /// Seconds on the source timeline; `None` = the whole video.
    pub range: Option<(f64, f64)>,
}

impl Area {
    pub fn new(shape: AreaShape) -> Self {
        Area { shape, x: 0.35, y: 0.35, w: 0.3, h: 0.3, amount: 0.6, color: [0, 0, 0], range: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// A block of text placed on the frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// Lines separated by `\n`.
    pub text: String,
    /// Path of a font file; empty = whatever ffmpeg finds by itself.
    pub font: String,
    /// Font size, percent of the frame height.
    pub size: f32,
    pub color: Rgb,
    /// Outline thickness as a percent of the font size (0 = none).
    pub outline: f32,
    pub outline_color: Rgb,
    /// Semi-transparent box behind the text.
    pub boxed: bool,
    pub box_color: Rgb,
    /// Anchor point, fractions of the frame; `h` / `v` say which point of the text sits there.
    pub x: f32,
    pub y: f32,
    pub h: HAlign,
    pub v: VAlign,
    pub range: Option<(f64, f64)>,
}

impl Default for TextBlock {
    fn default() -> Self {
        TextBlock {
            text: "Text".into(),
            font: String::new(),
            size: 7.0,
            color: [255, 255, 255],
            outline: 8.0,
            outline_color: [0, 0, 0],
            boxed: false,
            box_color: [0, 0, 0],
            x: 0.5,
            y: 0.9,
            h: HAlign::Center,
            v: VAlign::Bottom,
            range: None,
        }
    }
}

/// A bar of solid colour with text above or below the picture (the "meme caption").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub on: bool,
    pub text: String,
    pub font: String,
    /// Font size, percent of the picture width.
    pub size: f32,
    pub color: Rgb,
    pub bg: Rgb,
    pub align: HAlign,
}

impl Default for Caption {
    fn default() -> Self {
        Caption { on: false, text: String::new(), font: String::new(), size: 6.0, color: [0, 0, 0], bg: [255, 255, 255], align: HAlign::Center }
    }
}
