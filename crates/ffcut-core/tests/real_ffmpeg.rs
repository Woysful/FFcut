//! End-to-end checks against the real ffmpeg/ffprobe on PATH.
use ffcut_core::{builder::build_args, cmdline, probe::probe, run::*, *};
use std::path::{Path, PathBuf};
use std::process::Command;

/// These tests need the real tools; without them they pass vacuously (and say so).
fn tools_missing() -> bool {
    let missing = ffcut_core::tools::find_tool("ffmpeg", None).is_none() || ffcut_core::tools::find_tool("ffprobe", None).is_none();
    if missing {
        eprintln!("skipped: ffmpeg / ffprobe not on PATH");
    }
    missing
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ffcut-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn make_sample() -> PathBuf {
    // unique per call: tests run in parallel and must not rewrite each other's files
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let p = tmp(&format!("sample dir {}.mkv", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst))); // space in the name on purpose
    let st = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x360:rate=25:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=4",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(&p)
        .status()
        .unwrap();
    assert!(st.success());
    p
}

fn run(argv: &[String], total: f64) -> Event {
    let outputs = [PathBuf::from(argv.last().unwrap())];
    let job = Job::spawn(Path::new("ffmpeg"), argv, &outputs, Some(total)).unwrap();
    loop {
        match job.events.recv().unwrap() {
            Event::Progress { .. } => continue,
            e => return e,
        }
    }
}

#[test]
fn probe_build_run_and_user_edit() {
    if tools_missing() {
        return;
    }
    let input = make_sample();
    let src = probe(Path::new("ffprobe"), &input).unwrap();
    assert_eq!((src.width, src.height, src.video_codec.as_str(), src.audio.len()), (640, 360, "hevc", 2));

    // 1. Generated command: trim + rotate + scale + merge audio, re-encoded to h264.
    let out = tmp("out1.mp4");
    let s = ExportSettings {
        trim_start: 1.0,
        trim_end: Some(3.0),
        rotation: Rotation::Cw90,
        scale: Scale { w: None, h: Some(320) },
        video_codec: VideoCodec::Encoder("libx264".into()),
        preset: "ultrafast".into(),
        audio_mode: AudioMode::Merge,
        ..Default::default()
    };
    let argv = build_args(&src, &s, &input, &out);
    assert_eq!(run(&argv, 2.0), Event::Finished, "cmd: {}", cmdline::to_text(&argv));
    let o = probe(Path::new("ffprobe"), &out).unwrap();
    assert_eq!(o.video_codec, "h264");
    assert_eq!(o.height, 320); // scaled
    assert!(o.width < o.height); // rotated to portrait
    assert_eq!(o.audio.len(), 1); // merged
    assert!((o.duration - 2.0).abs() < 0.2, "duration {}", o.duration);

    // 2. The user edits the text box: an extra flag the old app silently dropped.
    let mut args = argv.clone();
    let out2 = tmp("out2.mp4");
    *args.last_mut().unwrap() = out2.to_string_lossy().into_owned();
    let text = cmdline::to_text(&args);
    // user appends flags before the output path in the text box
    let pos = text.rfind(" out2.mp4").or_else(|| text.rfind(" /tmp")).unwrap();
    let edited = format!("{} -metadata title=\"My edit\"{}", &text[..pos], &text[pos..]);
    let argv2 = cmdline::parse(&edited).unwrap();
    assert_eq!(run(&argv2, 2.0), Event::Finished);
    let tags = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags=title", "-of", "csv=p=0"]).arg(&out2).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&tags.stdout).trim(), "My edit");

    // 3. Lossless trim.
    let out3 = tmp("out3.mkv");
    let s = ExportSettings { trim_start: 0.0, trim_end: Some(2.0), ..Default::default() };
    let argv = build_args(&src, &s, &input, &out3);
    assert_eq!(run(&argv, 2.0), Event::Finished);
    assert_eq!(probe(Path::new("ffprobe"), &out3).unwrap().video_codec, "hevc");
}

/// "duration" and "nb_read_frames" style values of one stream (`v:0`, `a:0`...).
fn stream_value(file: &Path, stream: &str, entry: &str) -> f64 {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error", "-select_streams", stream]);
    if entry == "nb_read_frames" {
        cmd.arg("-count_frames");
    }
    let o = cmd.args(["-show_entries", &format!("stream={entry}"), "-of", "csv=p=0"]).arg(file).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse().unwrap_or(-1.0)
}

#[test]
fn speed_changes_the_length_of_picture_and_sound() {
    if tools_missing() {
        return;
    }
    let input = make_sample();
    let src = probe(Path::new("ffprobe"), &input).unwrap();
    for (speed, keep_pitch, trim, want) in
        [(2.0, true, (0.0, None), 2.0), (2.0, false, (1.0, Some(3.0)), 1.0), (0.5, true, (1.0, Some(3.0)), 4.0), (3.0, true, (0.0, None), 4.0 / 3.0)]
    {
        let out = tmp(&format!("speed {speed} {keep_pitch}.mp4"));
        let s = ExportSettings {
            speed,
            keep_pitch,
            trim_start: trim.0,
            trim_end: trim.1,
            video_codec: VideoCodec::Encoder("libx264".into()),
            preset: "ultrafast".into(),
            ..Default::default()
        };
        let argv = build_args(&src, &s, &input, &out);
        assert_eq!(run(&argv, want), Event::Finished, "cmd: {}", cmdline::to_text(&argv));
        let cmd = cmdline::to_text(&argv);
        // the picture keeps the source frame rate: fewer (or more) frames, not faster ones
        let frames = stream_value(&out, "v:0", "nb_read_frames");
        assert!((frames - want * 25.0).abs() <= 2.0, "{speed}×: {frames} frames, want {}: {cmd}", want * 25.0);
        for a in ["a:0", "a:1"] {
            let d = stream_value(&out, a, "duration");
            assert!((d - want).abs() < 0.15, "{speed}× {a}: {d} s, want {want}: {cmd}");
        }
    }
}

#[test]
fn frame_export_writes_exactly_the_frame_at_that_time() {
    if tools_missing() {
        return;
    }
    // every frame its own brightness, long GOP (the seek has to decode up to the frame), lossless
    let input = tmp("numbered frames.mkv");
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "nullsrc=s=64x48:r=25:d=2,geq=lum='16+N*4':cb=128:cr=128", "-c:v", "libx264", "-qp", "0", "-g", "50"])
        .arg(&input)
        .status()
        .unwrap();
    assert!(st.success());
    let src = probe(Path::new("ffprobe"), &input).unwrap();
    let gray = |file: &Path, select: Option<usize>| -> f64 {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-i"]).arg(file);
        if let Some(n) = select {
            cmd.args(["-vf", &format!("select=eq(n\\,{n})"), "-frames:v", "1"]);
        }
        let o = cmd.args(["-pix_fmt", "gray", "-f", "rawvideo", "-"]).output().unwrap();
        assert!(!o.stdout.is_empty(), "{}", String::from_utf8_lossy(&o.stderr));
        o.stdout.iter().map(|v| *v as f64).sum::<f64>() / o.stdout.len() as f64
    };
    for n in [0usize, 1, 7, 26, 49] {
        let out = tmp(&format!("frame {n} 100%.png"));
        let argv = builder::frame_args(&src, &ExportSettings::default(), &input, n as f64 / 25.0, &out);
        let o = Command::new("ffmpeg").args(&argv[1..]).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let (got, want) = (gray(&out, None), gray(&input, Some(n)));
        assert!((got - want).abs() < 1.5, "frame {n}: {got} vs {want}");
    }
    // crop and rotation reach the picture; JPEG and WebP work too
    let s = ExportSettings { rotation: Rotation::Cw90, crop: Some(Crop { x: 0, y: 0, w: 48, h: 32 }), ..Default::default() };
    for ext in ["jpg", "webp"] {
        let out = tmp(&format!("frame.{ext}"));
        let argv = builder::frame_args(&src, &s, &input, 1.0, &out);
        let o = Command::new("ffmpeg").args(&argv[1..]).output().unwrap();
        assert!(o.status.success(), "{ext}: {}", String::from_utf8_lossy(&o.stderr));
        let p = probe(Path::new("ffprobe"), &out).unwrap();
        assert_eq!((p.width, p.height), (48, 32), "{ext}");
    }
}

#[test]
fn failure_reports_ffmpeg_message_and_leaves_no_file() {
    if tools_missing() {
        return;
    }
    let out = tmp("never.mp4");
    let argv: Vec<String> = ["ffmpeg", "-y", "-i", "/definitely/not/here.mp4", out.to_str().unwrap()].map(String::from).to_vec();
    match run(&argv, 1.0) {
        Event::Failed(msg) => assert!(msg.contains("No such file"), "{msg}"),
        e => panic!("{e:?}"),
    }
    assert!(!out.exists());
}

#[test]
fn cancel_kills_process_and_removes_partial_output() {
    if tools_missing() {
        return;
    }
    let out = tmp("long.mp4");
    // endless synthetic source: would run forever unless cancelled
    let argv: Vec<String> =
        ["ffmpeg", "-y", "-f", "lavfi", "-i", "testsrc=size=1280x720:rate=60", "-c:v", "libx264", "-preset", "veryslow", out.to_str().unwrap()]
            .map(String::from)
            .to_vec();
    let job = Job::spawn(Path::new("ffmpeg"), &argv, std::slice::from_ref(&out), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(800));
    job.cancel();
    let last = loop {
        match job.events.recv().unwrap() {
            Event::Progress { .. } => continue,
            e => break e,
        }
    };
    assert_eq!(last, Event::Cancelled);
    assert!(!out.exists());
}

/// Mean volume (dB) of a time window of the first audio stream.
fn level_db(file: &Path, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-ss", &from.to_string(), "-t", &(to - from).to_string(), "-i"])
        .arg(file)
        .args(["-map", "0:a:0", "-af", "volumedetect", "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().find_map(|l| l.split("mean_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0)
}

#[test]
fn timeline_audio_edits_really_happen() {
    if tools_missing() {
        return;
    }
    let input = make_sample(); // 4 s, audio #0 = 440 Hz, audio #1 = 880 Hz
    let src = probe(Path::new("ffprobe"), &input).unwrap();

    // delete track 0, shift track 1 by 1 s, halve it: output = 1 s silence + sound
    let mut tracks: Vec<TrackEdit> = src.audio.iter().map(|a| TrackEdit::original(a.index, format!("#{}", a.index), src.duration)).collect();
    tracks[0].deleted = true;
    tracks[1].shift = 1.0;
    tracks[1].trim_end = 3.0; // placed 1..4
    let s = ExportSettings { tracks, ..Default::default() };
    let out = tmp("tracks.mkv");
    let argv = build_args(&src, &s, &input, &out);
    assert_eq!(run(&argv, 4.0), Event::Finished, "cmd: {}", cmdline::to_text(&argv));
    let o = probe(Path::new("ffprobe"), &out).unwrap();
    assert_eq!(o.audio.len(), 1, "deleted track must be gone");
    assert!(level_db(&out, 0.0, 0.8) < -60.0, "first second should be silent");
    assert!(level_db(&out, 1.5, 3.5) > -30.0, "sound after the shift");

    // external track replaces the sound
    let ext = tmp("ext.wav");
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=300:duration=2"]).arg(&ext).status().unwrap().success());
    let mut tracks: Vec<TrackEdit> = src.audio.iter().map(|a| TrackEdit::original(a.index, format!("#{}", a.index), src.duration)).collect();
    tracks[0].deleted = true;
    tracks[1].deleted = true;
    tracks.push(TrackEdit::external(ext.to_string_lossy().into_owned(), "ext".into(), 2.0));
    // re-encode video: with stream copy the cut snaps to a keyframe and shifts the whole container
    let s = ExportSettings {
        tracks,
        trim_start: 1.0,
        trim_end: Some(3.0),
        video_codec: VideoCodec::Encoder("libx264".into()),
        preset: "ultrafast".into(),
        ..Default::default()
    };
    let out2 = tmp("ext_out.mkv");
    let argv = build_args(&src, &s, &input, &out2);
    assert_eq!(run(&argv, 2.0), Event::Finished, "cmd: {}", cmdline::to_text(&argv));
    assert_eq!(probe(Path::new("ffprobe"), &out2).unwrap().audio.len(), 1);
    // external file occupies timeline 0..2; export is 1..3 -> sound for the first second, then silence
    assert!(level_db(&out2, 0.0, 0.9) > -30.0, "cmd: {} level {}", cmdline::to_text(&argv), level_db(&out2, 0.0, 0.9));
    assert!(level_db(&out2, 1.2, 1.9) < -60.0);
}

#[test]
fn waveform_has_the_right_length() {
    if tools_missing() {
        return;
    }
    let input = make_sample();
    let p = ffcut_core::waveform::peaks(Path::new("ffmpeg"), &input, 0).unwrap();
    assert!((p.len() as i64 - 4 * ffcut_core::waveform::PEAKS_PER_SEC as i64).abs() <= 15, "{}", p.len());
    assert!(p.iter().cloned().fold(0.0, f32::max) > 0.1);
}

/// Mean level (dB) of one frequency band of the first audio stream: tells which tones are present.
fn tone_db(file: &Path, hz: u32) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(file)
        .args(["-map", "0:a:0", "-af", &format!("bandpass=f={hz}:width_type=h:w=60,volumedetect"), "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().find_map(|l| l.split("mean_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0)
}

#[test]
fn mute_solo_and_mixing_end_to_end() {
    if tools_missing() {
        return;
    }
    let input = make_sample(); // audio #0 = 440 Hz, audio #1 = 880 Hz
    let src = probe(Path::new("ffprobe"), &input).unwrap();
    let originals = || -> Vec<TrackEdit> { src.audio.iter().map(|a| TrackEdit::original(a.index, format!("#{}", a.index), src.duration)).collect() };
    let export = |s: &ExportSettings, name: &str| -> PathBuf {
        let out = tmp(name);
        let argv = build_args(&src, s, &input, &out);
        assert_eq!(run(&argv, 4.0), Event::Finished, "cmd: {}", cmdline::to_text(&argv));
        out
    };

    // mute #0: only 880 Hz is left, as a single stream
    let mut t = originals();
    t[0].muted = true;
    let out = export(&ExportSettings { tracks: t, ..Default::default() }, "muted.mkv");
    assert_eq!(probe(Path::new("ffprobe"), &out).unwrap().audio.len(), 1);
    assert!(tone_db(&out, 880) > -30.0 && tone_db(&out, 440) < -35.0);

    // solo #0 (and #1 not muted): only 440 Hz
    let mut t = originals();
    t[0].solo = true;
    let out = export(&ExportSettings { tracks: t, ..Default::default() }, "solo.mkv");
    assert!(tone_db(&out, 440) > -30.0 && tone_db(&out, 880) < -35.0);

    // the scenario that used to conflict: remove #0, add a 44.1 kHz mono music file, mix everything into one
    let music = tmp("music44.wav");
    assert!(Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=1000:duration=3:sample_rate=44100", "-ac", "1"])
        .arg(&music)
        .status()
        .unwrap()
        .success());
    let mut t = originals();
    t[0].deleted = true;
    t.push(TrackEdit::external(music.to_string_lossy().into_owned(), "music".into(), 3.0));
    let out = export(&ExportSettings { tracks: t, audio_mode: AudioMode::Merge, ..Default::default() }, "mixed.mkv");
    assert_eq!(probe(Path::new("ffprobe"), &out).unwrap().audio.len(), 1, "everything mixed into one stream");
    assert!(tone_db(&out, 880) > -35.0, "kept original");
    assert!(tone_db(&out, 1000) > -35.0, "added music");
    assert!(tone_db(&out, 440) < -35.0, "removed original must be gone");
}
