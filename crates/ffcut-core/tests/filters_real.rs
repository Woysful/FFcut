//! Real ffmpeg: the filter catalog reads what this ffmpeg has, and a picked filter with typed options runs.
use ffcut_core::filters::*;
use ffcut_core::videofx::{VEffect, VKind, VideoFx};
use ffcut_core::*;
use std::path::PathBuf;
use std::process::Command;

fn ffmpeg() -> Option<PathBuf> {
    let f = ffcut_core::tools::find_tool("ffmpeg", None);
    if f.is_none() {
        eprintln!("skipped: ffmpeg not on PATH");
    }
    f
}

#[test]
fn every_offered_filter_has_readable_options() {
    let Some(ff) = ffmpeg() else { return };
    let list = list_filters(&ff).unwrap();
    let count = |m| list.iter().filter(|f| f.media == m).count();
    assert!(count(Media::Audio) > 40 && count(Media::Video) > 80, "{} audio, {} video", count(Media::Audio), count(Media::Video));
    assert!(list.iter().any(|f| f.name == "aecho") && list.iter().any(|f| f.name == "colorbalance"));
    assert!(!list.iter().any(|f| f.name == "frei0r" || f.name == "anull" || f.name.ends_with("_vaapi")));
    let chunks: Vec<Vec<FilterInfo>> = list.chunks(list.len().div_ceil(8)).map(|c| c.to_vec()).collect();
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|c| {
            let ff = ff.clone();
            std::thread::spawn(move || {
                for f in c {
                    let h = filter_help(&ff, &f.name).unwrap_or_else(|e| panic!("{}: {e}", f.name));
                    assert_eq!(h.name, f.name);
                    for o in &h.options {
                        assert!(valid_key(&o.name), "{}: option {:?}", f.name, o.name);
                        if let (Some(a), Some(b)) = (o.min, o.max) {
                            assert!(a <= b, "{}.{}: {a} > {b}", f.name, o.name);
                        }
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let h = filter_help(&ff, "acompressor").unwrap();
    let mode = h.options.iter().find(|o| o.name == "mode").unwrap();
    assert!(mode.choices.iter().any(|c| c.name == "upward"));
    assert!(filter_help(&ff, "no_such_filter_here").is_err());
}

#[test]
fn a_video_filter_with_awkward_text_runs() {
    let Some(ff) = ffmpeg() else { return };
    let mut f = FilterFx::new("drawtext");
    f.set("text", Some("it's 50%: a, b; [c] \\ d".into()));
    f.set("expansion", Some("none".into()));
    f.set("fontsize", Some("20".into()));
    let fx = VideoFx { effects: vec![VEffect::new(VKind::Filter(f)), VEffect::new(VKind::Filter(FilterFx::new("negate")))], ..Default::default() };
    let chain = videofx::preview_chain(&fx, 0, false, false, 160, 120).unwrap();
    assert!(chain.graph.ends_with(",negate"), "{}", chain.graph);
    let r = Command::new(&ff)
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=size=160x120:duration=1", "-vf", &chain.graph, "-frames:v", "1", "-f", "null", "-"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{}\n{}", chain.graph, String::from_utf8_lossy(&r.stderr));
}

#[test]
fn an_audio_filter_with_a_list_value_runs() {
    let Some(ff) = ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ffcut-filters-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("tone.wav");
    assert!(Command::new(&ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"]).arg(&src).status().unwrap().success());
    let mut t = TrackEdit::original(0, "t".into(), 2.0);
    let mut f = FilterFx::new("aecho");
    f.set("delays", Some("100|180".into()));
    f.set("decays", Some("0.5|0.3".into()));
    t.effects = vec![Effect::new(EffectKind::Filter(f))];
    let out = dir.join("echo.flac");
    let args = audiofx::preview_render_args(&src, 0, &t, &out);
    assert!(args.iter().any(|a| a.contains("aecho=delays=100|180:decays=0.5|0.3")), "{args:?}");
    let r = Command::new(&ff).args(&args).output().unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert!(out.metadata().unwrap().len() > 1000);
    let _ = std::fs::remove_dir_all(&dir);
}
