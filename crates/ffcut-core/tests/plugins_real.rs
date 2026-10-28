//! Real plugins (when installed): LV2 bundles, LADSPA and frei0r libraries are read, and their filter text
//! runs in the real ffmpeg.
use ffcut_core::plugins::*;
use ffcut_core::videofx::{VEffect, VKind, VideoFx};
use ffcut_core::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ffmpeg_with(filter: &str) -> Option<PathBuf> {
    let ff = ffcut_core::tools::find_tool("ffmpeg", None)?;
    let out = Command::new(&ff).args(["-hide_banner", "-filters"]).output().ok()?;
    let has = String::from_utf8_lossy(&out.stdout).lines().any(|l| l.split_whitespace().nth(1) == Some(filter));
    if !has {
        eprintln!("skipped: this ffmpeg has no {filter}");
    }
    has.then_some(ff)
}

fn installed(p: &str) -> Option<&Path> {
    let p = Path::new(p);
    if !p.exists() {
        eprintln!("skipped: {} is not installed", p.display());
    }
    p.exists().then_some(p)
}

fn run(ff: &Path, input: &str, af: Option<&str>, vf: Option<&str>) -> Result<(), String> {
    let mut c = Command::new(ff);
    c.args(["-v", "error", "-f", "lavfi", "-i", input]);
    if let Some(a) = af {
        c.args(["-af", a]);
    }
    if let Some(v) = vf {
        c.args(["-vf", v, "-frames:v", "2"]);
    }
    let r = c.args(["-f", "null", "-"]).output().unwrap();
    if r.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&r.stderr).into_owned())
    }
}

/// Encodes to AAC, which takes only named channel layouts.
fn run_aac(ff: &Path, af: &str) -> Result<(), String> {
    let r = Command::new(ff).args(["-v", "error", "-f", "lavfi", "-i", "sine=f=440:d=0.3", "-af", af, "-c:a", "aac", "-f", "adts", "-"]).output().unwrap();
    if r.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&r.stderr).into_owned())
    }
}

#[test]
fn plugin_output_reaches_an_aac_encoder() {
    let scan = lv2_plugins(&Dirs::new(&[]));
    let mono = scan.plugins.iter().find(|p| p.id == "http://plugin.org.uk/swh-plugins/autoPhaser");
    if let (Some(p), Some(ff)) = (mono, ffmpeg_with("lv2")) {
        let text = plugin_text(&p.new_fx()).unwrap();
        run_aac(&ff, &text).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
    let plate = installed("/usr/lib/ladspa/plate_1423.so").and_then(|f| probe_file(f).ok()).and_then(|l| l.into_iter().find(|p| p.outputs == 2));
    if let (Some(p), Some(ff)) = (plate, ffmpeg_with("ladspa")) {
        let text = plugin_text(&p.new_fx()).unwrap();
        run_aac(&ff, &text).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
}

#[test]
fn lv2_bundles_read_and_run() {
    let Some(_) = installed("/usr/lib/lv2/sc4-swh.lv2") else { return };
    let scan = lv2_plugins(&Dirs::new(&[]));
    assert!(scan.errors.is_empty(), "{:?}", scan.errors);
    let sc4 = scan.plugins.iter().find(|p| p.id == "http://plugin.org.uk/swh-plugins/sc4").expect("sc4");
    assert_eq!((sc4.name.as_str(), sc4.inputs, sc4.outputs, sc4.problem.as_ref()), ("SC4", 2, 2, None));
    let keys: Vec<&str> = sc4.controls.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["rms_peak", "attack", "release", "threshold", "ratio", "knee", "makeup_gain"]);
    assert_eq!(sc4.control("makeup_gain").unwrap().max, Some(24.0));
    let Some(ff) = ffmpeg_with("lv2") else { return };
    let mut fx = sc4.new_fx();
    fx.set("attack", Some("10".into()));
    fx.set("ratio", Some("4".into()));
    let text = plugin_text(&fx).unwrap();
    run(&ff, "sine=f=440:d=0.5,aformat=channel_layouts=stereo", Some(&text), None).unwrap_or_else(|e| panic!("{text}: {e}"));
    // plugins offered as effects start with their defaults (a few swh plugins crash ffmpeg by themselves)
    let usable: Vec<&PluginInfo> = scan.plugins.iter().filter(|p| p.usable()).take(40).collect();
    let failed: Vec<String> = usable
        .iter()
        .filter_map(|p| run(&ff, "sine=f=440:d=0.2", Some(&plugin_text(&p.new_fx()).unwrap()), None).err().map(|e| format!("{}: {e}", p.name)))
        .collect();
    eprintln!("{} of {} LV2 plugins failed: {failed:#?}", failed.len(), usable.len());
    assert!(failed.len() * 2 < usable.len(), "{failed:#?}");
}

#[test]
fn ladspa_library_reads_and_runs() {
    let Some(file) = installed("/usr/lib/ladspa/amp_1181.so") else { return };
    let list = probe_file(file).unwrap();
    let mono = list.iter().find(|p| p.inputs == 1).expect("a mono amp");
    assert_eq!((mono.inputs, mono.outputs, mono.kind), (1, 1, PluginKind::Ladspa));
    assert_eq!(mono.controls.len(), 1);
    assert_eq!((mono.controls[0].key.as_str(), mono.controls[0].strict), ("c0", true));
    let Some(ff) = ffmpeg_with("ladspa") else { return };
    let mut fx = mono.new_fx();
    fx.set("c0", Some("-6".into()));
    let mut t = TrackEdit::original(0, "t".into(), 1.0);
    t.effects = vec![Effect::new(EffectKind::Plugin(fx))];
    let dir = std::env::temp_dir().join(format!("ffcut-plugins-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("tone.wav");
    assert!(Command::new(&ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]).arg(&src).status().unwrap().success());
    let out = dir.join("amp.flac");
    let args = audiofx::preview_render_args(&src, 0, &t, &out);
    let r = Command::new(&ff).args(&args).output().unwrap();
    assert!(r.status.success(), "{args:?}\n{}", String::from_utf8_lossy(&r.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    // controls without a default get a value, or ffmpeg refuses the plugin
    if let Some(f) = installed("/usr/lib/ladspa/allpass_1895.so") {
        let ap = probe_file(f).unwrap().into_iter().find(|p| p.label == "allpass_n").unwrap();
        assert_eq!(ap.controls[0].default, None);
        let text = plugin_text(&ap.new_fx()).unwrap();
        assert!(text.contains(":controls='c0=0|"), "{text}");
        run(&ff, "sine=f=440:d=0.2", Some(&text), None).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
    // a control without a value it accepts is left out
    let mut bad = mono.new_fx();
    bad.set("c0", Some("loud".into()));
    assert!(plugin_text(&bad).unwrap().starts_with(&format!("ladspa=file={}:plugin={},", file.display(), mono.label)));
}

#[test]
fn every_installed_ladspa_and_frei0r_library_reads() {
    let dirs = Dirs::new(&[]);
    for d in dirs.ladspa.iter().chain(&dirs.frei0r).filter(|d| d.is_dir()) {
        let mut n = 0;
        for f in std::fs::read_dir(d).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == LIB_EXT)) {
            let list = probe_file(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            for p in &list {
                assert!(!p.name.is_empty(), "{}", f.display());
                for c in &p.controls {
                    if let (Some(a), Some(b)) = (c.min, c.max) {
                        assert!(a <= b, "{} {}: {a} > {b}", p.name, c.name);
                    }
                }
            }
            n += list.len();
        }
        eprintln!("{}: {n} plugins", d.display());
    }
}

#[test]
fn frei0r_plugin_reads_and_runs() {
    let Some(file) = installed("/usr/lib/frei0r-1/glow.so") else { return };
    let glow = probe_file(file).unwrap().remove(0);
    assert_eq!((glow.kind, glow.id.as_str(), glow.problem.as_ref()), (PluginKind::Frei0r, "glow", None));
    assert_eq!(glow.controls[0].kind, CtlKind::Float);
    assert!(glow.controls[0].default.is_some());
    let Some(ff) = ffmpeg_with("frei0r") else { return };
    let mut fx = glow.new_fx();
    fx.set("0", Some("0.8".into()));
    let vfx = VideoFx { effects: vec![VEffect::new(VKind::Plugin(fx))], ..Default::default() };
    let chain = videofx::preview_chain(&vfx, 0, false, false, 160, 120).unwrap();
    run(&ff, "testsrc=size=160x120:duration=1", None, Some(&chain.graph)).unwrap_or_else(|e| panic!("{}: {e}", chain.graph));

    // every kind of parameter, with its default written out
    if let Some(tint) = installed("/usr/lib/frei0r-1/tint0r.so") {
        let p = probe_file(tint).unwrap().remove(0);
        let mut fx = p.new_fx();
        for c in &p.controls {
            fx.set(&c.key, c.default.clone());
        }
        let text = plugin_text(&fx).unwrap();
        assert!(text.contains('/'), "{text}");
        run(&ff, "testsrc=size=160x120:duration=1", None, Some(&text)).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
    // a text value with the separator of the values in it
    if let Some(curves) = installed("/usr/lib/frei0r-1/curves.so") {
        let p = probe_file(curves).unwrap().remove(0);
        let spline = p.controls.iter().position(|c| c.kind == CtlKind::Text).expect("a text parameter");
        let mut fx = p.new_fx();
        for c in &p.controls[..=spline] {
            fx.set(&c.key, c.default.clone());
        }
        fx.set(&spline.to_string(), Some("0;0#0;0#0.2;0.2|0.8;0.9#1;1#1;1".into()));
        if let Some(next) = p.controls.get(spline + 1) {
            fx.set(&next.key, next.default.clone());
        }
        let text = plugin_text(&fx).unwrap();
        run(&ff, "testsrc=size=160x120:duration=1", None, Some(&text)).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
    let sources = installed("/usr/lib/frei0r-1/nois0r.so").map(|f| probe_file(f).unwrap().remove(0));
    if let Some(s) = sources {
        assert!(s.problem.is_some());
    }
}
