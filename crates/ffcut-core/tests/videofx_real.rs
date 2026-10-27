//! Real ffmpeg: the filters `videofx` writes must be accepted, and must do what they promise.
use ffcut_core::videofx::*;
use ffcut_core::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// These tests need the real tools; without them they pass vacuously (and say so).
fn tools_missing() -> bool {
    let missing = ffcut_core::tools::find_tool("ffmpeg", None).is_none() || ffcut_core::tools::find_tool("ffprobe", None).is_none();
    if missing {
        eprintln!("skipped: ffmpeg / ffprobe not on PATH");
    }
    missing
}

const W: u32 = 320;
const H: u32 = 240;

fn dir() -> &'static PathBuf {
    static D: OnceLock<PathBuf> = OnceLock::new();
    D.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("ffcut-vfx-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    })
}

fn input() -> &'static PathBuf {
    static I: OnceLock<PathBuf> = OnceLock::new();
    I.get_or_init(|| {
        let p = dir().join("src.mp4");
        let st = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=size={W}x{H}:rate=10:duration=3"),
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                "libx264",
                "-crf",
                "0",
            ])
            .arg(&p)
            .status()
            .unwrap();
        assert!(st.success());
        p
    })
}

fn src_info() -> SourceInfo {
    SourceInfo { duration: 3.0, width: W, height: H, video_codec: "h264".into(), pix_fmt: "yuv420p".into(), fps: 10.0, ..Default::default() }
}

fn settings(vfx: VideoFx) -> ExportSettings {
    ExportSettings { vfx, video_codec: VideoCodec::Encoder("libx264".into()), quality: Quality::Crf(0), audio_mode: AudioMode::None, ..Default::default() }
}

/// Export with the real builder, return the output file.
fn export(name: &str, s: &ExportSettings) -> PathBuf {
    let out = dir().join(format!("{name}.mkv"));
    let args = build_args(&src_info(), s, input(), &out);
    let r = Command::new("ffmpeg").args(&args[1..]).output().unwrap();
    assert!(r.status.success(), "ffmpeg failed for {name}:\n{}\n{}", args.join(" "), String::from_utf8_lossy(&r.stderr));
    out
}

/// One frame at `t` as 8-bit gray, row by row.
fn gray(file: &Path, t: f64, w: u32, h: u32) -> Vec<u8> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &t.to_string(), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-pix_fmt", "gray", "-f", "rawvideo", "-"])
        .output()
        .unwrap();
    assert_eq!(o.stdout.len(), (w * h) as usize, "frame size");
    o.stdout
}

fn px(f: &[u8], w: u32, x: u32, y: u32) -> u8 {
    f[(y * w + x) as usize]
}

fn mean(f: &[u8], w: u32, x: u32, y: u32, rw: u32, rh: u32) -> f64 {
    let mut s = 0u64;
    for yy in y..y + rh {
        for xx in x..x + rw {
            s += px(f, w, xx, yy) as u64;
        }
    }
    s as f64 / (rw * rh) as f64
}

fn variance(f: &[u8], w: u32, x: u32, y: u32, rw: u32, rh: u32) -> f64 {
    let m = mean(f, w, x, y, rw, rh);
    let mut s = 0.0;
    for yy in y..y + rh {
        for xx in x..x + rw {
            s += (px(f, w, xx, yy) as f64 - m).powi(2);
        }
    }
    s / (rw * rh) as f64
}

fn with(kinds: Vec<VKind>) -> VideoFx {
    VideoFx { effects: kinds.into_iter().map(VEffect::new).collect(), ..Default::default() }
}

fn reference() -> PathBuf {
    static R: OnceLock<PathBuf> = OnceLock::new();
    R.get_or_init(|| export("ref", &settings(VideoFx::default()))).clone()
}

fn font() -> String {
    let fonts = ffcut_core::fonts::scan();
    ffcut_core::fonts::pick_default(&fonts).map(|f| f.path.to_string_lossy().into_owned()).unwrap_or_default()
}

#[test]
fn cover_area_is_solid_and_the_rest_is_untouched() {
    if tools_missing() {
        return;
    }
    let a = Area { shape: AreaShape::Fill, x: 0.25, y: 0.25, w: 0.5, h: 0.5, color: [0, 0, 0], ..Area::new(AreaShape::Fill) };
    let out = export("cover", &settings(with(vec![VKind::Area(a)])));
    let (r, o) = (gray(&reference(), 1.0, W, H), gray(&out, 1.0, W, H));
    assert!(mean(&o, W, 90, 70, 140, 100) < 3.0, "covered");
    assert!(mean(&o, W, 0, 0, 60, 50) > 5.0);
    assert_eq!(mean(&r, W, 0, 0, 60, 50), mean(&o, W, 0, 0, 60, 50), "outside is identical");
}

#[test]
fn area_time_range_follows_the_cut() {
    if tools_missing() {
        return;
    }
    // source seconds 1.5..2.5, export starts at 1.0 -> covered from output second 0.5 to 1.5
    let a = Area { shape: AreaShape::Fill, range: Some((1.5, 2.5)), ..Area::new(AreaShape::Fill) };
    let mut s = settings(with(vec![VKind::Area(a)]));
    s.trim_start = 1.0;
    let out = export("range", &s);
    let covered = |t: f64| mean(&gray(&out, t, W, H), W, 120, 90, 60, 40) < 3.0;
    assert!(!covered(0.2), "before the range");
    assert!(covered(1.0), "inside the range");
    assert!(!covered(1.8), "after the range");
}

#[test]
fn pixelate_makes_blocks_and_blur_removes_detail() {
    if tools_missing() {
        return;
    }
    let r = gray(&reference(), 1.0, W, H);
    let a = Area { x: 0.1, y: 0.1, w: 0.6, h: 0.6, amount: 0.5, ..Area::new(AreaShape::Pixelate) };
    let p = gray(&export("pix", &settings(with(vec![VKind::Area(a)]))), 1.0, W, H);
    // the area starts at (32, 24); its first block (about 18 px) is a single colour
    let first = (26..38).flat_map(|y| (34..46).map(move |x| (x, y))).map(|(x, y)| px(&p, W, x, y)).collect::<Vec<_>>();
    assert!(first.iter().all(|v| *v == first[0]), "one block: {first:?}");
    let differs = (24..24 + 140).flat_map(|y| (32..32 + 190).map(move |x| (x, y))).filter(|(x, y)| px(&p, W, *x, *y) != px(&r, W, *x, *y)).count();
    assert!(differs > 1000, "the area changed: {differs}");
    assert_eq!(px(&p, W, 300, 230), px(&r, W, 300, 230));

    let a = Area { x: 0.1, y: 0.1, w: 0.6, h: 0.6, amount: 0.8, ..Area::new(AreaShape::Blur) };
    let b = gray(&export("blur", &settings(with(vec![VKind::Area(a)]))), 1.0, W, H);
    assert!(variance(&b, W, 40, 30, 100, 80) < variance(&r, W, 40, 30, 100, 80) * 0.6, "blurred");
    assert_eq!(px(&b, W, 300, 230), px(&r, W, 300, 230));
}

#[test]
fn every_pixel_effect_is_accepted() {
    if tools_missing() {
        return;
    }
    let curves = Curves { master: vec![(0.0, 0.0), (0.4, 0.3), (1.0, 1.0)], r: vec![(0.0, 0.1), (1.0, 1.0)], ..Default::default() };
    let fx = with(vec![
        VKind::Color(ColorAdj { brightness: 0.1, contrast: 1.3, saturation: 1.5, gamma: 1.2 }),
        VKind::Curves(curves),
        VKind::Vignette(0.7),
        VKind::Sharpen(1.5),
        VKind::Denoise(0.5),
        VKind::Deblock(0.8),
        VKind::Deband(0.5),
        VKind::Deflicker(5),
    ]);
    let o = gray(&export("allpix", &settings(fx)), 1.0, W, H);
    let r = gray(&reference(), 1.0, W, H);
    assert_ne!(o, r);
    // vignette: the corner is darker than it is without it
    let v = gray(&export("vig", &settings(with(vec![VKind::Vignette(1.0)]))), 1.0, W, H);
    assert!(mean(&v, W, 0, 0, 20, 20) < mean(&r, W, 0, 0, 20, 20) - 5.0, "vignette darkens the corner");
}

#[test]
fn brightness_and_gamma_move_the_picture() {
    if tools_missing() {
        return;
    }
    let r = gray(&reference(), 1.0, W, H);
    let up = gray(&export("bright", &settings(with(vec![VKind::Color(ColorAdj { brightness: 0.2, ..Default::default() })]))), 1.0, W, H);
    let gam = gray(&export("gamma", &settings(with(vec![VKind::Color(ColorAdj { gamma: 2.0, ..Default::default() })]))), 1.0, W, H);
    let m = |f: &[u8]| mean(f, W, 0, 0, W, H);
    assert!(m(&up) > m(&r) + 10.0);
    assert!(m(&gam) > m(&r) + 10.0, "ffmpeg's gamma > 1 brightens");
}

#[test]
fn text_with_awkward_characters_renders() {
    if tools_missing() {
        return;
    }
    let t =
        TextBlock { text: "it's 50%: a,b;c [x] \\ \"q\"\nline two".into(), font: font(), x: 0.5, y: 0.5, v: VAlign::Middle, size: 12.0, ..Default::default() };
    let out = export("text", &settings(with(vec![VKind::Text(t)])));
    let (r, o) = (gray(&reference(), 1.0, W, H), gray(&out, 1.0, W, H));
    let changed = (60..180).flat_map(|y| (0..W).map(move |x| (x, y))).filter(|(x, y)| px(&o, W, *x, *y) != px(&r, W, *x, *y)).count();
    assert!(changed > 300, "text pixels: {changed}");
    // nothing far from the text changed
    assert_eq!(px(&o, W, 2, 2), px(&r, W, 2, 2));
}

#[test]
fn captions_add_bars_with_text() {
    if tools_missing() {
        return;
    }
    let fx = VideoFx {
        top: Caption { on: true, text: "when the\nexport finishes".into(), font: font(), ..Default::default() },
        bottom: Caption { on: true, text: "bottom".into(), font: font(), bg: [0, 0, 0], color: [255, 255, 255], ..Default::default() },
        ..Default::default()
    };
    let (top, bottom) = bars(&fx, W);
    assert!(top > 0 && bottom > 0);
    let out = export("caps", &settings(fx));
    let h = H + top + bottom;
    let o = gray(&out, 1.0, W, h);
    assert!(px(&o, W, 1, 1) > 240, "white top bar");
    assert!((0..W).any(|x| (0..top).any(|y| px(&o, W, x, y) < 100)), "dark text in the top bar");
    assert!(px(&o, W, 1, h - 2) < 15, "black bottom bar");
    assert!((0..W).any(|x| (h - bottom..h).any(|y| px(&o, W, x, y) > 200)), "light text in the bottom bar");
}

#[test]
fn areas_use_the_rotated_frame_and_survive_a_crop() {
    if tools_missing() {
        return;
    }
    // rotate 90°: the frame is 240x320. Cover its top-left corner, then crop that corner away from nothing.
    let a = Area { shape: AreaShape::Fill, x: 0.0, y: 0.0, w: 0.25, h: 0.25, ..Area::new(AreaShape::Fill) };
    let mut s = settings(with(vec![VKind::Area(a)]));
    s.rotation = Rotation::Cw90;
    let o = gray(&export("rot", &s), 1.0, H, W);
    assert!(mean(&o, H, 0, 0, 55, 75) < 3.0, "corner covered in the rotated frame");
    assert!(mean(&o, H, 120, 160, 60, 60) > 5.0);

    // with a crop the area keeps its place on the picture
    let a = Area { shape: AreaShape::Fill, x: 0.5, y: 0.5, w: 0.25, h: 0.25, ..Area::new(AreaShape::Fill) };
    let mut s = settings(with(vec![VKind::Area(a)]));
    s.crop = Some(Crop { x: 160, y: 120, w: 160, h: 120 });
    let o = gray(&export("cropped", &s), 1.0, 160, 120);
    assert!(mean(&o, 160, 2, 2, 70, 50) < 3.0, "covered area starts at the crop corner");
}

#[test]
fn preview_chain_runs_in_ffmpeg_too() {
    if tools_missing() {
        return;
    }
    // the preview uses the same graph (absolute times): it must be a valid -vf
    let fx = with(vec![VKind::Sharpen(1.0), VKind::Area(Area::new(AreaShape::Blur)), VKind::Text(TextBlock { font: font(), ..Default::default() })]);
    let p = preview_chain(&fx, 90, true, false, H, W).unwrap();
    let st =
        Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(input()).args(["-vf", &p.graph, "-frames:v", "3", "-f", "null", "-"]).output().unwrap();
    assert!(st.status.success(), "{}\n{}", p.graph, String::from_utf8_lossy(&st.stderr));
}

#[test]
fn awkward_text_is_drawn_exactly_as_typed() {
    if tools_missing() {
        return;
    }
    // reference: the same drawtext fed from a text file (no escaping involved)
    let typed = "it's 50%: a,b;c [x] \\ \"q\" {n} é";
    let t = TextBlock { text: typed.into(), font: font(), x: 0.5, y: 0.5, v: VAlign::Middle, size: 9.0, ..Default::default() };
    let built = overlay_filters(&with(vec![VKind::Text(t)]), W, H, 0.0).remove(0);
    let a = built.find(":text=").map(|i| i + 1).unwrap();
    let b = built.find(":fontfile=").unwrap();
    let txt = dir().join("typed.txt");
    std::fs::write(&txt, typed).unwrap();
    let reference = format!("{}textfile={}{}", &built[..a], txt.display(), &built[b..]);
    let render = |vf: &str| {
        let o = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "color=c=0x303030:s=320x240", "-vf", vf, "-frames:v", "1", "-pix_fmt", "gray", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(o.status.success(), "{vf}\n{}", String::from_utf8_lossy(&o.stderr));
        o.stdout
    };
    assert_eq!(render(&built), render(&reference));
}
