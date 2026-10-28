//! Every ready-made effect, at its defaults and with its controls moved, runs in the real ffmpeg.
use ffcut_core::builtin::{self, Ctl, Def};
use ffcut_core::filters::Media;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ffmpeg() -> Option<PathBuf> {
    let ff = ffcut_core::tools::find_tool("ffmpeg", None);
    if ff.is_none() {
        eprintln!("skipped: no ffmpeg");
    }
    ff
}

fn has(ff: &Path, filter: &str) -> bool {
    let out = Command::new(ff).args(["-hide_banner", "-filters"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().any(|l| l.split_whitespace().nth(1) == Some(filter))
}

fn run(ff: &Path, media: Media, chain: &str) -> Result<(), String> {
    let mut c = Command::new(ff);
    c.args(["-v", "error", "-f", "lavfi"]);
    match media {
        // AAC takes only named channel layouts: what comes out must still have one
        Media::Audio => c.args(["-i", "sine=f=440:d=0.4", "-af", chain, "-c:a", "aac", "-f", "adts", "-"]),
        Media::Video => c.args(["-i", "testsrc2=d=0.4:s=160x120", "-vf", chain, "-frames:v", "3", "-f", "null", "-"]),
    };
    let r = c.output().unwrap();
    if r.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&r.stderr).into_owned())
    }
}

/// Settings to try: the defaults, every control at three quarters of its range, and each choice.
fn variants(d: &Def, lut: &str) -> Vec<ffcut_core::BuiltinFx> {
    let mut base = d.new_fx();
    base.file = lut.to_string();
    let mut out = vec![base.clone()];
    let mut moved = base.clone();
    for p in d.params {
        match p.ctl {
            Ctl::Num { min, max, .. } => moved.set(p.key, min + 0.75 * (max - min)),
            Ctl::Int { min, max, .. } => moved.set(p.key, (min as f64 + 0.75 * (max - min) as f64).round()),
            Ctl::Toggle => moved.set(p.key, 1.0 - p.default),
            Ctl::Choice(_) | Ctl::File(_) => {}
        }
    }
    out.push(moved.clone());
    for p in d.params {
        if let Ctl::Choice(c) = p.ctl {
            for i in 0..c.len() {
                let mut v = moved.clone();
                v.set(p.key, i as f64);
                out.push(v);
            }
        }
    }
    out
}

#[test]
fn every_builtin_effect_runs_in_ffmpeg() {
    let Some(ff) = ffmpeg() else { return };
    let dir = std::env::temp_dir().join(format!("ffcut-builtin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // an identity LUT, in a folder whose name needs quoting
    let lut_dir = dir.join("it's a: look");
    std::fs::create_dir_all(&lut_dir).unwrap();
    let lut = lut_dir.join("id.cube");
    let mut cube = String::from("LUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                cube.push_str(&format!("{r} {g} {b}\n"));
            }
        }
    }
    std::fs::write(&lut, cube).unwrap();
    let mut failed = Vec::new();
    for d in builtin::DEFS {
        if let Some(missing) = d.covers.iter().find(|f| !has(&ff, f)) {
            eprintln!("skipped {}: this ffmpeg has no {missing}", d.id);
            continue;
        }
        for fx in variants(d, &lut.to_string_lossy()) {
            let Some(text) = builtin::filter_text(&fx, d.media) else { continue };
            if let Err(e) = run(&ff, d.media, &text) {
                failed.push(format!("{}: {text}\n{e}", d.id));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
