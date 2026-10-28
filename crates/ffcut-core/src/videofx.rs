//! Video effects: colour, stylise, restore, covered areas, text blocks and caption bars -> ffmpeg filters.
//!
//! One description (`VideoFx`) feeds both the export (`builder`) and the live preview (mpv `vf`), so what
//! you see is what ffmpeg renders. Where each part sits in the chain:
//!
//! ```text
//!  source ─► pixel effects ─► rotate / flip ─► areas + text ─► crop ─► caption bars ─► resize
//!            (colour, restore)                 (frame = what the crop editor shows)
//! ```
//!
//! * pixel effects work on the decoded picture (denoise / deblock like the original block grid);
//! * areas and text are placed in the rotated + flipped frame, in fractions of it (0..1), so they do not
//!   depend on the resolution and the crop does not move them;
//! * caption bars are added around the cropped picture; their text size is a share of its width.
//!
//! The data types live in `model::video` and are re-exported here.

use crate::filters::{self, quote_value};
pub use crate::model::video::*;

/// A colour / curve setting this close to neutral changes nothing.
const NEUTRAL_EPS: f32 = 0.001;
/// An effect strength (0..1 scale) at or below this is off.
const MIN_STRENGTH: f64 = 0.01;
/// The smallest area side (fraction of the frame) that is still drawn.
const MIN_AREA: f32 = 0.001;
/// Curve points closer than this to the diagonal leave the channel unchanged.
const CURVE_EPS: f32 = 0.002;

// ------------------------------------------------------------------ what is active

impl VideoFx {
    pub fn has_pixel(&self) -> bool {
        self.effects.iter().any(|e| e.on && !e.is_overlay() && pixel_filter(&e.kind).is_some())
    }
    pub fn has_overlay(&self) -> bool {
        self.effects.iter().any(|e| e.on && overlay_valid(&e.kind))
    }
    pub fn has_captions(&self) -> bool {
        self.top.on || self.bottom.on
    }
    /// Does anything change the picture (so stream copy is impossible)?
    pub fn is_active(&self) -> bool {
        self.has_pixel() || self.has_overlay() || self.has_captions()
    }
    /// How many effects are switched on (for badges).
    pub fn count_on(&self) -> usize {
        self.effects.iter().filter(|e| e.on).count() + self.top.on as usize + self.bottom.on as usize
    }
}

fn overlay_valid(k: &VKind) -> bool {
    match k {
        VKind::Area(a) => a.w > MIN_AREA && a.h > MIN_AREA,
        VKind::Text(t) => !t.text.trim().is_empty(),
        _ => false,
    }
}

// ------------------------------------------------------------------ small helpers

fn n(v: f64) -> String {
    crate::fmtnum::num(v, 4)
}

fn rgb(c: Rgb) -> String {
    format!("0x{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn even(v: f64) -> u32 {
    (((v / 2.0).round() * 2.0).max(2.0)) as u32
}

fn enable(range: Option<(f64, f64)>, shift: f64) -> String {
    match range {
        Some((a, b)) => format!(":enable='between(t,{},{})'", n((a - shift).max(0.0)), n((b - shift).max(0.0))),
        None => String::new(),
    }
}

fn is_identity(c: &[(f32, f32)]) -> bool {
    c.len() < 2 || c.iter().all(|(x, y)| (x - y).abs() < CURVE_EPS)
}

fn curve_arg(c: &[(f32, f32)]) -> String {
    let mut pts = c.to_vec();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.iter().map(|(x, y)| format!("{}/{}", n(*x as f64), n(*y as f64))).collect::<Vec<_>>().join(" ")
}

/// Filters that turn the picture `deg` degrees clockwise (a multiple of 90), then mirror it.
pub(crate) fn orient_filters(deg: i64, flip_h: bool, flip_v: bool) -> Vec<String> {
    let mut f = Vec::new();
    match deg.rem_euclid(360) {
        90 => f.push("transpose=1".into()),
        180 => f.push("hflip,vflip".into()),
        270 => f.push("transpose=2".into()),
        _ => {}
    }
    if flip_h {
        f.push("hflip".into());
    }
    if flip_v {
        f.push("vflip".into());
    }
    f
}

// ------------------------------------------------------------------ pixel effects

/// The ffmpeg filter for a pixel effect, or `None` when it would change nothing.
pub(crate) fn pixel_filter(k: &VKind) -> Option<String> {
    let strength = |s: f32| Some(s.clamp(0.0, 1.0) as f64).filter(|s| *s > MIN_STRENGTH);
    match k {
        VKind::Color(c) => {
            let mut o = Vec::new();
            if c.brightness.abs() > NEUTRAL_EPS {
                o.push(format!("brightness={}", n(c.brightness as f64)));
            }
            if (c.contrast - 1.0).abs() > NEUTRAL_EPS {
                o.push(format!("contrast={}", n(c.contrast as f64)));
            }
            if (c.saturation - 1.0).abs() > NEUTRAL_EPS {
                o.push(format!("saturation={}", n(c.saturation as f64)));
            }
            if (c.gamma - 1.0).abs() > NEUTRAL_EPS {
                o.push(format!("gamma={}", n(c.gamma.clamp(0.1, 10.0) as f64)));
            }
            (!o.is_empty()).then(|| format!("eq={}", o.join(":")))
        }
        VKind::Curves(c) => {
            let mut o = Vec::new();
            for (name, pts) in [("master", &c.master), ("red", &c.r), ("green", &c.g), ("blue", &c.b)] {
                if !is_identity(pts) {
                    o.push(format!("{name}='{}'", curve_arg(pts)));
                }
            }
            (!o.is_empty()).then(|| format!("curves={}", o.join(":")))
        }
        VKind::Vignette(s) => strength(*s).map(|s| {
            // a smaller lens angle darkens more: pi/2 (none) .. 0.2 rad
            let angle = std::f64::consts::FRAC_PI_2 - s * (std::f64::consts::FRAC_PI_2 - 0.2);
            format!("vignette=angle={}", n(angle))
        }),
        VKind::Sharpen(a) => (*a as f64 > MIN_STRENGTH).then(|| format!("unsharp=lx=5:ly=5:la={}:cx=5:cy=5:ca=0", n(a.clamp(0.0, 5.0) as f64))),
        VKind::Denoise(s) => strength(*s).map(|s| format!("hqdn3d={}:{}:{}:{}", n(8.0 * s), n(6.0 * s), n(12.0 * s), n(9.0 * s))),
        VKind::Deblock(s) => strength(*s).map(|s| {
            let a = 0.05 + 0.4 * s;
            format!("deblock=filter={}:block=8:alpha={}:beta={}", if s > 0.5 { "strong" } else { "weak" }, n(a), n(a / 2.0))
        }),
        VKind::Deband(s) => strength(*s).map(|s| {
            let t = 0.01 + 0.07 * s;
            format!("deband=1thr={t}:2thr={t}:3thr={t}:range=16:blur=1", t = n(t))
        }),
        VKind::Deflicker(size) => Some(format!("deflicker=size={}:mode=am", (*size).clamp(2, 30))),
        VKind::Filter(f) => filters::filter_text(f),
        VKind::Plugin(p) => crate::plugins::plugin_text(p),
        VKind::Area(_) | VKind::Text(_) => None,
    }
}

/// Filters that work on the decoded picture, in stack order.
pub(crate) fn pixel_filters(fx: &VideoFx) -> Vec<String> {
    fx.effects.iter().filter(|e| e.on).filter_map(|e| pixel_filter(&e.kind)).collect()
}

// ------------------------------------------------------------------ areas and text

/// Pixel rectangle of an area inside a `w`×`h` frame (even numbers, inside the frame).
pub(crate) fn area_px(a: &Area, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let rw = even((a.w as f64 * w as f64).min(w as f64)).min(w & !1).max(2);
    let rh = even((a.h as f64 * h as f64).min(h as f64)).min(h & !1).max(2);
    let x = (((a.x as f64 * w as f64).round()) as i64).clamp(0, (w as i64 - rw as i64).max(0)) as u32;
    let y = (((a.y as f64 * h as f64).round()) as i64).clamp(0, (h as i64 - rh as i64).max(0)) as u32;
    (x & !1, y & !1, rw, rh)
}

fn area_filter(a: &Area, idx: usize, w: u32, h: u32, shift: f64) -> String {
    let (x, y, rw, rh) = area_px(a, w, h);
    let en = enable(a.range, shift);
    match a.shape {
        AreaShape::Fill => format!("drawbox=x={x}:y={y}:w={rw}:h={rh}:color={}:t=fill{en}", rgb(a.color)),
        AreaShape::Blur => {
            let sigma = 2.0 + 60.0 * a.amount.clamp(0.0, 1.0) as f64 * (rw.min(rh) as f64 / 400.0).clamp(0.2, 1.5);
            format!("split=2[ba{idx}][bb{idx}];[bb{idx}]crop={rw}:{rh}:{x}:{y},gblur=sigma={}[bc{idx}];[ba{idx}][bc{idx}]overlay={x}:{y}{en}", n(sigma))
        }
        AreaShape::Pixelate => {
            let block = (4.0 + a.amount.clamp(0.0, 1.0) as f64 * 0.12 * w.min(h) as f64).round().max(2.0) as u32;
            let (sw, sh) = ((rw / block).max(1), (rh / block).max(1));
            format!(
                "split=2[pa{idx}][pb{idx}];[pb{idx}]crop={rw}:{rh}:{x}:{y},scale={sw}:{sh}:flags=area,scale={rw}:{rh}:flags=neighbor[pc{idx}];[pa{idx}][pc{idx}]overlay={x}:{y}{en}"
            )
        }
    }
}

/// A font path as a drawtext option (`C:` needs its colon escaped).
fn font_opt(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    format!(":fontfile={}", quote_value(&path.replace('\\', "/")))
}

fn lines_of(text: &str) -> Vec<&str> {
    let v: Vec<&str> = text.lines().collect();
    if v.is_empty() {
        vec![""]
    } else {
        v
    }
}

/// One line of text. Always `expansion=none`, so `%` and `{}` are plain characters. `x` is an expression.
fn drawtext(line: &str, font: &str, size: f64, color: Rgb, x: &str, y: f64) -> String {
    format!("drawtext=expansion=none:text={}{}:fontsize={}:fontcolor={}:x={x}:y={}", quote_value(line), font_opt(font), n(size), rgb(color), n(y.round()))
}

fn text_filters(t: &TextBlock, w: u32, h: u32, shift: f64) -> Vec<String> {
    let size = ((t.size.clamp(0.5, 60.0) as f64 / 100.0) * h as f64).round().max(6.0);
    let lh = (size * 1.2).round();
    let lines = lines_of(&t.text);
    let block_h = lh * lines.len() as f64;
    let ax = t.x as f64 * w as f64;
    let top = t.y as f64 * h as f64
        - match t.v {
            VAlign::Top => 0.0,
            VAlign::Middle => block_h / 2.0,
            VAlign::Bottom => block_h,
        };
    let x = match t.h {
        HAlign::Left => n(ax.round()),
        HAlign::Center => format!("{}-text_w/2", n(ax.round())),
        HAlign::Right => format!("{}-text_w", n(ax.round())),
    };
    let en = enable(t.range, shift);
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, line)| {
            let mut f = drawtext(line, &t.font, size, t.color, &x, top + lh * i as f64);
            if t.outline > 0.01 {
                f += &format!(":borderw={}:bordercolor={}", n((size * t.outline as f64 / 100.0).round().max(1.0)), rgb(t.outline_color));
            }
            if t.boxed {
                f += &format!(":box=1:boxcolor={}@0.65:boxborderw={}", rgb(t.box_color), n((size * 0.2).round()));
            }
            f + &en
        })
        .collect()
}

/// Areas and text, in stack order, for a frame of `w`×`h`. `shift` = seconds cut off the start (export
/// seeks before the input, so filter time starts at 0 there; the preview passes 0).
pub fn overlay_filters(fx: &VideoFx, w: u32, h: u32, shift: f64) -> Vec<String> {
    let mut out = Vec::new();
    for (i, e) in fx.effects.iter().enumerate().filter(|(_, e)| e.on) {
        match &e.kind {
            VKind::Area(a) if overlay_valid(&e.kind) => out.push(area_filter(a, i, w, h, shift)),
            VKind::Text(t) if overlay_valid(&e.kind) => out.extend(text_filters(t, w, h, shift)),
            _ => {}
        }
    }
    out
}

// ------------------------------------------------------------------ caption bars

/// Height in pixels of one caption bar for a picture `w` pixels wide (0 when off).
pub(crate) fn bar_height(c: &Caption, w: u32) -> u32 {
    if !c.on {
        return 0;
    }
    let size = caption_size(c, w);
    let lines = lines_of(&c.text).len() as f64;
    even(lines * (size * 1.25).round() + size).max(even(size * 2.0))
}

fn caption_size(c: &Caption, w: u32) -> f64 {
    ((c.size.clamp(1.0, 30.0) as f64 / 100.0) * w as f64).round().max(8.0)
}

/// `(top, bottom)` bar heights for a picture of `w` pixels.
pub fn bars(fx: &VideoFx, w: u32) -> (u32, u32) {
    (bar_height(&fx.top, w), bar_height(&fx.bottom, w))
}

fn caption_text(c: &Caption, w: u32, y0: f64) -> Vec<String> {
    let size = caption_size(c, w);
    let lh = (size * 1.25).round();
    let x = match c.align {
        HAlign::Left => n((size * 0.6).round()),
        HAlign::Center => "(w-text_w)/2".to_string(),
        HAlign::Right => format!("w-text_w-{}", n((size * 0.6).round())),
    };
    lines_of(&c.text)
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, line)| drawtext(line, &c.font, size, c.color, &x, y0 + size * 0.5 + lh * i as f64))
        .collect()
}

/// Caption bars around a picture of `w`×`h`: top bar first, then the bottom one.
pub(crate) fn caption_filters(fx: &VideoFx, w: u32, h: u32) -> Vec<String> {
    let (top, bottom) = bars(fx, w);
    let mut out = Vec::new();
    if top > 0 {
        out.push(format!("pad=w=iw:h=ih+{top}:x=0:y={top}:color={}", rgb(fx.top.bg)));
        out.extend(caption_text(&fx.top, w, 0.0));
    }
    if bottom > 0 {
        out.push(format!("pad=w=iw:h=ih+{bottom}:x=0:y=0:color={}", rgb(fx.bottom.bg)));
        out.extend(caption_text(&fx.bottom, w, (top + h) as f64));
    }
    out
}

// ------------------------------------------------------------------ preview

/// What mpv should run for the preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewChain {
    pub graph: String,
    /// The chain also rotates / flips the picture, so mpv must not do it again.
    pub rotates: bool,
}

/// `rot_deg` = clockwise degrees the picture is turned in total (container + user), `(w, h)` = the frame
/// the user sees (rotated, uncropped). `None` = nothing to show.
pub fn preview_chain(fx: &VideoFx, rot_deg: i64, flip_h: bool, flip_v: bool, w: u32, h: u32) -> Option<PreviewChain> {
    if !fx.is_active() {
        return None;
    }
    let mut f = pixel_filters(fx);
    let rotates = fx.has_overlay() || fx.has_captions();
    if rotates {
        f.extend(orient_filters(rot_deg, flip_h, flip_v));
        f.extend(overlay_filters(fx, w, h, 0.0));
        f.extend(caption_filters(fx, w, h));
    }
    (!f.is_empty()).then(|| PreviewChain { graph: f.join(","), rotates })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(kinds: Vec<VKind>) -> VideoFx {
        VideoFx { effects: kinds.into_iter().map(VEffect::new).collect(), ..Default::default() }
    }

    #[test]
    fn neutral_settings_add_no_filters() {
        let f = fx(vec![VKind::Color(ColorAdj::default()), VKind::Curves(Curves::default()), VKind::Sharpen(0.0), VKind::Denoise(0.0)]);
        assert!(!f.is_active());
        assert!(pixel_filters(&f).is_empty());
        assert!(preview_chain(&f, 0, false, false, 100, 100).is_none());
    }

    #[test]
    fn pixel_effects_in_stack_order() {
        let f = fx(vec![
            VKind::Denoise(0.5),
            VKind::Color(ColorAdj { brightness: 0.1, contrast: 1.2, ..Default::default() }),
            VKind::Curves(Curves { master: vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)], ..Default::default() }),
            VKind::Deflicker(5),
        ]);
        assert_eq!(pixel_filters(&f), ["hqdn3d=4:3:6:4.5", "eq=brightness=0.1:contrast=1.2", "curves=master='0/0 0.5/0.6 1/1'", "deflicker=size=5:mode=am"]);
    }

    #[test]
    fn bypassed_effects_are_skipped() {
        let mut f = fx(vec![VKind::Sharpen(1.0)]);
        f.effects[0].on = false;
        assert!(!f.is_active());
    }

    #[test]
    fn area_fill_and_time_range() {
        let a = Area { shape: AreaShape::Fill, x: 0.25, y: 0.5, w: 0.5, h: 0.25, color: [255, 0, 0], range: Some((12.0, 14.5)), ..Area::new(AreaShape::Fill) };
        let f = fx(vec![VKind::Area(a)]);
        // export cut starts at 10 s: filter time starts there
        assert_eq!(overlay_filters(&f, 640, 360, 10.0), ["drawbox=x=160:y=180:w=320:h=90:color=0xFF0000:t=fill:enable='between(t,2,4.5)'"]);
        // preview uses source time
        assert!(overlay_filters(&f, 640, 360, 0.0)[0].contains("between(t,12,14.5)"));
    }

    #[test]
    fn areas_stay_inside_the_frame() {
        let a = Area { x: 0.9, y: 0.95, w: 0.5, h: 0.5, ..Area::new(AreaShape::Blur) };
        let (x, y, w, h) = area_px(&a, 640, 360);
        assert!(x + w <= 640 && y + h <= 360, "{x} {y} {w} {h}");
        assert!(w % 2 == 0 && h % 2 == 0 && x % 2 == 0 && y % 2 == 0);
    }

    #[test]
    fn text_is_one_drawtext_per_line_and_escaped() {
        let t = TextBlock { text: "it's 50%\nline: two".into(), x: 0.5, y: 0.5, h: HAlign::Center, v: VAlign::Middle, outline: 0.0, ..Default::default() };
        let v = overlay_filters(&fx(vec![VKind::Text(t)]), 1000, 1000, 0.0);
        assert_eq!(v.len(), 2);
        assert!(v[0].contains(r"text='it'\\\''s 50%'"), "{}", v[0]);
        assert!(v[0].contains("x=500-text_w/2"));
        assert!(v[1].contains(r"text='line\: two'"), "{}", v[1]);
    }

    #[test]
    fn text_block_filter_text_is_pinned() {
        let t = TextBlock { text: "Hi".into(), font: "/f/a.ttf".into(), boxed: true, range: Some((1.0, 2.0)), ..Default::default() };
        let v = overlay_filters(&fx(vec![VKind::Text(t)]), 1000, 500, 0.0);
        assert_eq!(
            v,
            ["drawtext=expansion=none:text='Hi':fontfile='/f/a.ttf':fontsize=35:fontcolor=0xFFFFFF:x=500-text_w/2:y=408:borderw=3:bordercolor=0x000000:box=1:boxcolor=0x000000@0.65:boxborderw=7:enable='between(t,1,2)'"]
        );
    }

    #[test]
    fn windows_font_path_is_escaped() {
        assert_eq!(font_opt(r"C:\Windows\Fonts\arial.ttf"), r":fontfile='C\:/Windows/Fonts/arial.ttf'");
    }

    #[test]
    fn captions_pad_and_write_text() {
        let f = VideoFx { top: Caption { on: true, text: "when you".into(), ..Default::default() }, ..Default::default() };
        let (top, bottom) = bars(&f, 1000);
        assert!(top > 0 && top % 2 == 0 && bottom == 0);
        let v = caption_filters(&f, 1000, 500);
        assert_eq!(v[0], format!("pad=w=iw:h=ih+{top}:x=0:y={top}:color=0xFFFFFF"));
        assert!(v[1].starts_with("drawtext=expansion=none:text='when you'") && v[1].contains("fontcolor=0x000000"));
    }

    #[test]
    fn preview_rotates_only_when_it_draws_overlays() {
        let f = fx(vec![VKind::Sharpen(1.0)]);
        let p = preview_chain(&f, 90, false, false, 100, 200).unwrap();
        assert!(!p.rotates && !p.graph.contains("transpose"));
        let f = fx(vec![VKind::Sharpen(1.0), VKind::Area(Area::new(AreaShape::Fill))]);
        let p = preview_chain(&f, 90, true, false, 100, 200).unwrap();
        assert!(p.rotates && p.graph.starts_with("unsharp") && p.graph.contains("transpose=1,hflip,drawbox"), "{}", p.graph);
    }
}
