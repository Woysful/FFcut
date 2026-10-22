//! The audio effects against a real ffmpeg: render the same chain the export uses and measure the result.
use ffcut_core::audiofx::*;
use ffcut_core::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ffcut-fx-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

/// 10 s video with one audio track: a 1 kHz sine at -6 dB peak.
fn tone() -> PathBuf {
    static ONCE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ONCE.get_or_init(make_tone).clone()
}

fn make_tone() -> PathBuf {
    let p = tmp("tone.mkv");
    assert!(Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=5:duration=10", "-f", "lavfi", "-i", "sine=frequency=1000:duration=10,volume=4", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "pcm_s16le"])
        .arg(&p)
        .status()
        .unwrap()
        .success());
    p
}

fn track() -> TrackEdit {
    TrackEdit::original(0, "t".into(), 10.0)
}

fn render(t: &TrackEdit, input: &Path, name: &str) -> PathBuf {
    let out = tmp(name);
    let args = preview_render_args(input, 0, t, &out);
    let r = Command::new("ffmpeg").args(&args).output().unwrap();
    assert!(r.status.success(), "{}\n{}", args.join(" "), String::from_utf8_lossy(&r.stderr));
    out
}

fn level(file: &Path, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-ss", &from.to_string(), "-t", &(to - from).to_string(), "-i"])
        .arg(file)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().find_map(|l| l.split("mean_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0)
}

#[test]
fn shift_and_trim_are_in_the_rendered_preview() {
    let i = tone();
    let mut t = track();
    t.shift = 2.0;
    t.trim_start = 1.0;
    t.trim_end = 6.0; // placed 3..8
    let o = render(&t, &i, "shift.flac");
    assert!(level(&o, 0.0, 2.8) < -60.0, "silence before the clip");
    assert!(level(&o, 3.2, 7.8) > -16.0, "sound inside");
    assert!(level(&o, 8.3, 9.5) < -60.0 || level(&o, 8.3, 9.0) < -60.0, "nothing after");
    assert!(t.needs_bake(10.0));
}

#[test]
fn fades_and_keyframes_follow_the_clip() {
    let i = tone();
    let mut t = track();
    t.fade_in = 2.0;
    t.fade_out = 2.0;
    let o = render(&t, &i, "fade.flac");
    let (a, b, c) = (level(&o, 0.0, 0.4), level(&o, 4.0, 6.0), level(&o, 9.6, 10.0));
    assert!(a < b - 12.0 && c < b - 12.0, "fade in {a}, middle {b}, fade out {c}");
    // keyframes: -20 dB before 4 s, 0 dB after 6 s, straight line (in dB) between
    let mut t = track();
    t.envelope = vec![EnvPoint { t: 4.0, db: -20.0 }, EnvPoint { t: 6.0, db: 0.0 }];
    let o = render(&t, &i, "env.flac");
    let (lo, hi, mid) = (level(&o, 1.0, 3.0), level(&o, 7.0, 9.0), level(&o, 4.9, 5.1));
    assert!((hi - lo - 20.0).abs() < 1.0, "lo {lo} hi {hi}");
    assert!((hi - mid - 10.0).abs() < 1.5, "mid {mid} hi {hi}");
    // the same through the export builder, with an export range that cuts into the clip
    let src = probe::probe(Path::new("ffprobe"), &i).unwrap();
    let s = ExportSettings { tracks: vec![t], trim_start: 3.0, trim_end: Some(8.0), audio_codec: AudioCodec::Encoder("flac".into()), video_codec: VideoCodec::Encoder("libx264".into()), preset: "ultrafast".into(), ..Default::default() };
    let out = tmp("export.mka");
    let argv = builder::build_args(&src, &s, &i, &out);
    let r = Command::new("ffmpeg").args(&argv[1..]).output().unwrap_or_else(|_| panic!("run"));
    assert!(r.status.success(), "{}\n{}", cmdline::to_text(&argv), String::from_utf8_lossy(&r.stderr));
    // export time 0..1 = source 3..4 (-20 dB), export time 3..5 = source 6..8 (0 dB)
    let (lo, hi) = (level(&out, 0.2, 0.9), level(&out, 3.2, 4.8));
    assert!((hi - lo - 20.0).abs() < 1.0, "export lo {lo} hi {hi}\n{}", cmdline::to_text(&argv));
}

#[test]
fn equalizer_and_compressor_change_the_sound() {
    let i = tone();
    let base = level(&render(&track(), &i, "plain.flac"), 1.0, 9.0);
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Eq(vec![EqBand { kind: EqKind::HighPass, freq: 4000.0, gain: 0.0, q: 0.7, on: true }]))];
    assert!(level(&render(&t, &i, "hp.flac"), 1.0, 9.0) < base - 20.0, "a high-pass at 4 kHz removes 1 kHz");
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Eq(vec![EqBand { kind: EqKind::Peak, freq: 1000.0, gain: 6.0, q: 1.0, on: true }]))];
    assert!((level(&render(&t, &i, "peak.flac"), 1.0, 9.0) - base - 6.0).abs() < 1.0);
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Comp(Compressor { threshold_db: -30.0, ratio: 10.0, attack_ms: 5.0, release_ms: 100.0, makeup_db: 0.0 }))];
    assert!(level(&render(&t, &i, "comp.flac"), 2.0, 9.0) < base - 6.0, "compressor lowers a loud signal");
}

#[test]
fn normalization_reaches_the_target() {
    let i = tone();
    let ff = Path::new("ffmpeg");
    // peak: -6 dB tone -> peak -1 dB
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Norm(Normalize::new(NormMode::Peak)))];
    let m = measure(ff, &i, 0, &t, 0).unwrap();
    assert!((m.tp + 6.0).abs() < 0.5, "true peak {}", m.tp);
    if let EffectKind::Norm(n) = &mut t.effects[0].kind {
        n.measured = Some(m);
    }
    let o = render(&t, &i, "peak_norm.flac");
    let out = Command::new("ffmpeg").args(["-hide_banner", "-nostats", "-i"]).arg(&o).args(["-af", "volumedetect", "-f", "null", "-"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    let max: f64 = err.lines().find_map(|l| l.split("max_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap();
    assert!((max + 1.0).abs() < 0.5, "max {max} chain {}", preview_render_args(&i, 0, &t, &o).join(" "));
    // loudness: -16 LUFS
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Norm(Normalize::new(NormMode::Loudness)))];
    let m = measure(ff, &i, 0, &t, 0).unwrap();
    if let EffectKind::Norm(n) = &mut t.effects[0].kind {
        n.measured = Some(m);
    }
    let o = render(&t, &i, "lufs.flac");
    let mut plain = TrackEdit::original(0, "o".into(), 10.0);
    plain.effects = vec![Effect::new(EffectKind::Norm(Normalize::new(NormMode::Loudness)))];
    let after = measure(ff, &o, 0, &plain, 0).unwrap();
    assert!((after.i + 16.0).abs() < 1.0, "loudness after: {}", after.i);
}

#[test]
fn waveform_of_the_rendered_track_shows_the_effects() {
    // the level meter reads the peaks of the rendered preview: a compressor with make-up gain must show
    let i = tone();
    let peak = |f: &Path| ffcut_core::waveform::peaks(Path::new("ffmpeg"), f, 0).unwrap().into_iter().fold(0.0f32, f32::max);
    let plain = peak(&render(&track(), &i, "m_plain.flac"));
    let mut t = track();
    t.effects = vec![Effect::new(EffectKind::Comp(Compressor { threshold_db: -20.0, ratio: 2.0, attack_ms: 5.0, release_ms: 100.0, makeup_db: 12.0 }))];
    let loud = peak(&render(&t, &i, "m_comp.flac"));
    assert!(loud > plain * 1.5, "plain {plain}, with make-up {loud}");
}

#[test]
fn audio_file_is_probed_and_exported_without_video() {
    let src_path = tmp("only.mp3");
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=6", "-c:a", "libmp3lame"]).arg(&src_path).status().unwrap().success());
    let info = probe::probe(Path::new("ffprobe"), &src_path).unwrap();
    assert!(info.audio_only);
    assert_eq!(info.audio.len(), 1);
    assert!((info.duration - 6.0).abs() < 0.2);

    let mut s = ExportSettings { trim_start: 1.0, trim_end: Some(4.0), ..Default::default() };
    s.tracks = vec![TrackEdit::original(0, "a".into(), info.duration)];
    s.tracks[0].effects = vec![Effect::new(EffectKind::DeEss(DeEsser::default())), Effect::new(EffectKind::Denoise(Denoise::default()))];
    for ext in ["mp3", "flac", "m4a", "wav", "ogg"] {
        let out = tmp(&format!("out.{ext}"));
        let args = build_args(&info, &s, &src_path, &out);
        let r = Command::new("ffmpeg").args(&args[1..]).output().unwrap();
        assert!(r.status.success(), "{ext}: {}\n{}", args.join(" "), String::from_utf8_lossy(&r.stderr));
        let o = probe::probe(Path::new("ffprobe"), &out).unwrap();
        assert!(o.audio_only, "{ext}");
        assert!((o.duration - 3.0).abs() < 0.3, "{ext}: {}", o.duration);
    }
}

#[test]
fn audio_only_export_of_a_video_with_two_tracks() {
    let video = tmp("two.mkv");
    assert!(Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=5:duration=4", "-f", "lavfi", "-i", "sine=frequency=440:duration=4", "-f", "lavfi", "-i", "sine=frequency=880:duration=4", "-map", "0", "-map", "1", "-map", "2", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
        .arg(&video)
        .status()
        .unwrap()
        .success());
    let ff = Path::new("ffprobe");
    let info = probe::probe(ff, &video).unwrap();
    assert!(!info.audio_only);
    let mut s = ExportSettings { audio_only: true, trim_start: 1.0, trim_end: Some(3.0), ..Default::default() };
    s.tracks = info.audio.iter().map(|a| TrackEdit::original(a.index, "t".into(), info.duration)).collect();
    let run = |out: &Path| {
        let args = build_args(&info, &s, &video, out);
        let r = Command::new("ffmpeg").args(&args[1..]).output().unwrap();
        assert!(r.status.success(), "{}\n{}", args.join(" "), String::from_utf8_lossy(&r.stderr));
    };
    // one stream per file
    let mp3 = tmp("sep.mp3");
    run(&mp3);
    for n in 1..=2 {
        let o = probe::probe(ff, &builder::numbered(&mp3, n)).unwrap();
        assert!(o.audio_only && o.audio.len() == 1, "file {n}");
        assert!((o.duration - 2.0).abs() < 0.3, "file {n} is {} s", o.duration);
    }
    // one container, two streams, no picture
    let mkv = tmp("one.mkv");
    run(&mkv);
    let o = probe::probe(ff, &mkv).unwrap();
    assert!(o.audio_only && o.audio.len() == 2);
}
