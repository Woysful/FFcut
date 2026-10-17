//! The multi-track preview graph against a real libmpv: mpv mixes the tracks into a WAV, ffmpeg measures it.
use ffcut_app::audiomix::{ensure_external, set_mix, single_track};
use libmpv2::{events::Event, Mpv};
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ffcut-mix-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn tone(file: &Path, hz: u32) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(file)
        .args(["-af", &format!("bandpass=f={hz}:width_type=h:w=60,volumedetect"), "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().find_map(|l| l.split("mean_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0)
}

fn sine(name: &str, hz: u32, rate: u32) -> PathBuf {
    let p = tmp(name);
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("sine=frequency={hz}:duration=3:sample_rate={rate}"), "-ac", "1"])
        .arg(&p)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    p
}

/// video with two audio tracks: #0 = 1000 Hz, #1 = 2000 Hz
fn video() -> PathBuf {
    let p = tmp("v.mp4");
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=3",
               "-f", "lavfi", "-i", "sine=frequency=1000:duration=3", "-f", "lavfi", "-i", "sine=frequency=2000:duration=3",
               "-map", "0:v", "-map", "1:a", "-map", "2:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
        .arg(&p)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    p
}

/// Plays the video through mpv with `setup` applied after loading, exactly like the app does at runtime.
fn render(v: &Path, name: &str, setup: impl Fn(&Mpv)) -> PathBuf {
    let wav = tmp(name);
    let _ = std::fs::remove_file(&wav);
    {
        let mpv = Mpv::with_initializer(|i| {
            i.set_property("vo", "null")?;
            i.set_property("ao", "pcm")?;
            i.set_property("ao-pcm-file", wav.to_string_lossy().as_ref())?;
            i.set_property("pause", true)?;
            i.set_property("idle", "yes")?;
            Ok(())
        })
        .unwrap();
        let client = mpv.create_client(None).unwrap();
        mpv.command("loadfile", &[v.to_str().unwrap(), "replace"]).unwrap();
        let mut loaded = false;
        for _ in 0..200 {
            if let Some(Ok(Event::FileLoaded)) = client.wait_event(0.1) {
                loaded = true;
                break;
            }
        }
        assert!(loaded, "file did not load");
        setup(&mpv);
        mpv.set_property("pause", false).unwrap();
        let mut ended = false;
        for _ in 0..600 {
            if let Some(Ok(Event::EndFile(_))) = client.wait_event(0.1) {
                ended = true;
                break;
            }
        }
        assert!(ended, "playback never finished");
    } // mpv dropped: the wav is complete
    wav
}

const PRESENT: f64 = -32.0;
const ABSENT: f64 = -40.0;

#[test]
fn all_tracks_play_together() {
    let v = video();
    let wav = render(&v, "both.wav", |m| set_mix(m, &[(1, 1.0), (2, 1.0)]).unwrap());
    assert!(tone(&wav, 1000) > PRESENT, "track 1 missing: {}", tone(&wav, 1000));
    assert!(tone(&wav, 2000) > PRESENT, "track 2 missing: {}", tone(&wav, 2000));
}

#[test]
fn volume_zero_and_single_track() {
    let v = video();
    let wav = render(&v, "vol0.wav", |m| set_mix(m, &[(1, 1.0), (2, 0.0)]).unwrap());
    assert!(tone(&wav, 1000) > PRESENT && tone(&wav, 2000) < ABSENT, "{} / {}", tone(&wav, 1000), tone(&wav, 2000));
    let wav = render(&v, "only2.wav", |m| set_mix(m, &[(2, 1.0)]).unwrap());
    assert!(tone(&wav, 2000) > PRESENT && tone(&wav, 1000) < ABSENT);
}

#[test]
fn an_added_file_with_another_sample_rate_joins_the_mix() {
    let v = video();
    let music = sine("music.wav", 3000, 44100); // mono, 44.1 kHz: differs from the 48 kHz stereo tracks
    let wav = render(&v, "ext.wav", |m| {
        let id = ensure_external(m, music.to_str().unwrap()).expect("external audio was not added");
        assert!(id > 2, "external id should come after the file's own tracks, got {id}");
        set_mix(m, &[(1, 1.0), (id, 1.0)]).unwrap();
    });
    assert!(tone(&wav, 1000) > PRESENT, "original: {}", tone(&wav, 1000));
    assert!(tone(&wav, 3000) > PRESENT, "added file: {}", tone(&wav, 3000));
    assert!(tone(&wav, 2000) < ABSENT, "track 2 was not in the mix: {}", tone(&wav, 2000));
}

#[test]
fn going_back_to_a_single_track_clears_the_mix() {
    let v = video();
    let wav = render(&v, "back.wav", |m| {
        set_mix(m, &[(1, 1.0), (2, 1.0)]).unwrap();
        single_track(m, 2).unwrap();
    });
    assert!(tone(&wav, 2000) > PRESENT && tone(&wav, 1000) < ABSENT, "{} / {}", tone(&wav, 1000), tone(&wav, 2000));
}

fn window_db(file: &Path, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-ss", &from.to_string(), "-t", &(to - from).to_string(), "-i"])
        .arg(file)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().find_map(|l| l.split("mean_volume:").nth(1)).and_then(|v| v.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0)
}

/// The shift of a track really is heard: the track is rendered by ffmpeg with the export's filters and
/// mpv plays that file next to the video (this is what the app does for edited tracks).
#[test]
fn a_shifted_track_is_heard_late() {
    let v = video(); // 3 s, track #0 = 1000 Hz
    let mut t = ffcut_core::TrackEdit::original(0, "a".into(), 3.0);
    t.shift = 1.5;
    t.trim_end = 1.5; // placed 1.5 .. 3.0
    let rendered = tmp("shifted.flac");
    let args = ffcut_core::audiofx::preview_render_args(&v, 0, &t, &rendered);
    assert!(Command::new("ffmpeg").args(&args).status().unwrap().success());
    let wav = render(&v, "shifted.wav", |m| {
        let id = ensure_external(m, rendered.to_str().unwrap()).expect("rendered track was not added");
        set_mix(m, &[(id, 1.0)]).unwrap();
    });
    assert!(window_db(&wav, 0.1, 1.3) < -60.0, "first part should be silent: {}", window_db(&wav, 0.1, 1.3));
    assert!(window_db(&wav, 1.8, 2.8) > -35.0, "sound after the shift: {}", window_db(&wav, 1.8, 2.8));
}
