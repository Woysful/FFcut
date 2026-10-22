//! Per-track audio processing: equalizer, compressor, normalization, fades and volume keyframes.
//!
//! Everything is expressed as plain ffmpeg filters, so the preview and the export run the very same
//! chain (`track_chain`), and what you read in the command is what you hear.
//!
//! Chain order, per track: trim → equalizer → compressor → normalization → fades / keyframes →
//! (placement on the timeline) → track volume.

use crate::model::*;
use std::path::Path;
use std::process::Stdio;

/// Loudness normalization uses these fixed companions of the target loudness.
const TRUE_PEAK: f64 = -1.5;
const LRA: f64 = 11.0;

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// `t+3.2` / `t-3.2` as an expression.
fn t_plus(c: f64) -> String {
    if c >= 0.0 { format!("(t+{})", num(c)) } else { format!("(t-{})", num(-c)) }
}

fn db_to_lin(db: f32) -> f64 {
    10f64.powf(db as f64 / 20.0)
}

// ------------------------------------------------------------------ equalizer / compressor

pub fn eq_filters(bands: &[EqBand]) -> Vec<String> {
    bands
        .iter()
        .filter(|b| b.on)
        .map(|b| {
            let (f, q, g) = (num(b.freq as f64), num(b.q.max(0.05) as f64), num(b.gain as f64));
            match b.kind {
                EqKind::Peak => format!("equalizer=f={f}:t=q:w={q}:g={g}"),
                EqKind::LowShelf => format!("lowshelf=f={f}:t=q:w={q}:g={g}"),
                EqKind::HighShelf => format!("highshelf=f={f}:t=q:w={q}:g={g}"),
                EqKind::HighPass => format!("highpass=f={f}"),
                EqKind::LowPass => format!("lowpass=f={f}"),
            }
        })
        .collect()
}

pub fn comp_filter(c: &Compressor) -> String {
    // acompressor takes linear values: threshold 0.001..1, makeup 1..64
    let thr = db_to_lin(c.threshold_db.clamp(-60.0, 0.0)).clamp(0.000976563, 1.0);
    let mk = db_to_lin(c.makeup_db.clamp(0.0, 36.0)).clamp(1.0, 64.0);
    format!(
        "acompressor=threshold={}:ratio={}:attack={}:release={}:makeup={}",
        num6(thr),
        num(c.ratio.clamp(1.0, 20.0) as f64),
        num(c.attack_ms.clamp(0.01, 2000.0) as f64),
        num(c.release_ms.clamp(0.01, 9000.0) as f64),
        num6(mk)
    )
}

fn num6(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Filters of the enabled effects before position `upto` of the stack: what a normalization at `upto`
/// is measured after.
pub fn chain_effects(t: &TrackEdit, upto: usize) -> Vec<String> {
    let mut v = Vec::new();
    for idx in 0..upto.min(t.effects.len()) {
        v.extend(effect_filters(t, idx));
    }
    v
}

/// Filters of one effect (empty when bypassed).
pub fn effect_filters(t: &TrackEdit, idx: usize) -> Vec<String> {
    let Some(e) = t.effects.get(idx).filter(|e| e.on) else { return vec![] };
    match &e.kind {
        EffectKind::Eq(bands) => eq_filters(bands),
        EffectKind::Comp(c) => vec![comp_filter(c)],
        EffectKind::Norm(n) => norm_filters(t, idx, n),
        EffectKind::DeEss(d) => vec![format!("deesser=i={:.2}:m={:.2}:f={:.2}", d.intensity.clamp(0.0, 1.0), d.max.clamp(0.0, 1.0), d.freq.clamp(0.0, 1.0))],
        EffectKind::Denoise(d) => vec![format!("afftdn=nr={:.1}:nf={:.1}:tn=1", d.reduction_db.clamp(0.01, 97.0), d.floor_db.clamp(-80.0, -20.0))],
    }
}

// ------------------------------------------------------------------ normalization

/// Identifies the audio a measurement was made on: same source, same clip, same effects before it.
pub fn norm_key(t: &TrackEdit, idx: usize) -> String {
    format!("{:?}|{:.3}|{:.3}|{}", t.source, t.trim_start, t.trim_end, chain_effects(t, idx).join(","))
}

/// The measurement of the normalization at `idx`, if it belongs to the current state of the track.
pub fn valid_measure(t: &TrackEdit, idx: usize) -> Option<&Measured> {
    match &t.effects.get(idx)?.kind {
        EffectKind::Norm(n) => n.measured.as_ref().filter(|m| m.key == norm_key(t, idx)),
        _ => None,
    }
}

fn norm_filters(t: &TrackEdit, idx: usize, n: &Normalize) -> Vec<String> {
    let m = valid_measure(t, idx);
    match n.mode {
        NormMode::Peak => match m {
            // true peak of the clip, so the result never exceeds the target
            Some(m) if m.tp.is_finite() => vec![format!("volume={}dB", num(n.target as f64 - m.tp))],
            _ => vec![],
        },
        NormMode::Loudness => {
            let mut f = format!("loudnorm=I={}:TP={}:LRA={}", num(n.target as f64), num(TRUE_PEAK), num(LRA));
            if let Some(m) = m {
                f.push_str(&format!(
                    ":measured_I={}:measured_LRA={}:measured_TP={}:measured_thresh={}:offset={}:linear=true",
                    num(m.i), num(m.lra), num(m.tp), num(m.thresh), num(m.offset)
                ));
            }
            // loudnorm works at 192 kHz internally
            vec![f, "aresample=48000".into()]
        }
    }
}

/// Runs ffmpeg's loudness analysis over the clip (after trim, equalizer and compressor).
pub fn measure(ffmpeg: &Path, file: &Path, ordinal: usize, t: &TrackEdit, idx: usize) -> Result<Measured, String> {
    let mut chain = vec![format!("atrim=start={}:end={}", num(t.trim_start), num(t.trim_end)), "asetpts=PTS-STARTPTS".into()];
    chain.extend(chain_effects(t, idx));
    chain.push(format!("loudnorm=I=-16:TP={}:LRA={}:print_format=json", num(TRUE_PEAK), num(LRA)));
    let out = crate::run::command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-nostats", "-i"])
        .arg(file)
        .args(["-map", &format!("0:a:{ordinal}"), "-vn", "-af", &chain.join(","), "-f", "null", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let err = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return Err(err.lines().last().unwrap_or("ffmpeg failed").to_string());
    }
    let (a, b) = (err.rfind('{'), err.rfind('}'));
    let json = match (a, b) {
        (Some(a), Some(b)) if b > a => &err[a..=b],
        _ => return Err("no measurement in the ffmpeg output".into()),
    };
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).and_then(|s| s.trim().parse::<f64>().ok());
    let (i, tp) = (get("input_i").ok_or("no loudness")?, get("input_tp").ok_or("no peak")?);
    if !i.is_finite() || !tp.is_finite() {
        return Err("the track is silent".into());
    }
    Ok(Measured {
        key: norm_key(t, idx),
        i,
        tp,
        lra: get("input_lra").unwrap_or(0.0),
        thresh: get("input_thresh").unwrap_or(-70.0),
        offset: get("target_offset").unwrap_or(0.0),
    })
}

// ------------------------------------------------------------------ fades and keyframes

/// Keyframes sorted by time.
fn sorted_env(t: &TrackEdit) -> Vec<EnvPoint> {
    let mut e = t.envelope.clone();
    e.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    e
}

/// Fades and keyframes as one linear gain at source time `s` (1.0 = unchanged). Same maths as `gain_expr`.
pub fn gain_at(t: &TrackEdit, s: f64) -> f32 {
    let mut g = 1.0f64;
    if t.fade_in > 0.001 {
        g *= ((s - t.trim_start) / t.fade_in).clamp(0.0, 1.0);
    }
    if t.fade_out > 0.001 {
        g *= ((t.trim_end - s) / t.fade_out).clamp(0.0, 1.0);
    }
    let e = sorted_env(t);
    if let Some(first) = e.first() {
        let db = if s <= first.t {
            first.db as f64
        } else if let Some(last) = e.last().filter(|l| s >= l.t) {
            last.db as f64
        } else {
            let k = e.windows(2).find(|w| s >= w[0].t && s < w[1].t).unwrap();
            let (a, b) = (k[0], k[1]);
            a.db as f64 + (b.db - a.db) as f64 * (s - a.t) / (b.t - a.t).max(1e-9)
        };
        g *= 10f64.powf(db / 20.0);
    }
    g as f32
}

/// `volume` expression for fades + keyframes. `ls` = source time where this chain's `atrim` starts
/// (the filter's own `t` is counted from there).
pub fn gain_expr(t: &TrackEdit, ls: f64) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if t.fade_in > 0.001 {
        parts.push(format!("clip({}/{},0,1)", t_plus(ls - t.trim_start), num(t.fade_in)));
    }
    if t.fade_out > 0.001 {
        parts.push(format!("clip(({}-t)/{},0,1)", num(t.trim_end - ls), num(t.fade_out)));
    }
    let e = sorted_env(t);
    if !e.is_empty() {
        let pts: Vec<(f64, f64)> = e.iter().map(|p| (p.t - ls, p.db as f64)).collect();
        // build from the end: before the first point = its value, after the last = its value
        let mut expr = num(pts[pts.len() - 1].1);
        for w in pts.windows(2).rev() {
            let ((t0, v0), (t1, v1)) = (w[0], w[1]);
            let seg = format!("{}+({})*(t-{})/{}", num(v0), num(v1 - v0), num(t0), num((t1 - t0).max(1e-6)));
            expr = format!("if(lt(t,{}),{},{})", num(t1), seg, expr);
        }
        expr = format!("if(lt(t,{}),{},{})", num(pts[0].0), num(pts[0].1), expr);
        parts.push(format!("pow(10,({expr})/20)"));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("volume='{}':eval=frame", parts.join("*")))
}

// ------------------------------------------------------------------ the chain

/// The filter chain of one track, without input / output labels.
///
/// * `ls`/`le`: source times to cut (`ls_abs` is the same start on the clip's own axis, ahead of any `-ss`);
/// * `delay_ms`: where the clip starts in the output.
pub fn track_chain(t: &TrackEdit, ls: f64, le: f64, ls_abs: f64, delay_ms: i64) -> String {
    let mut c = format!("atrim=start={}:end={},asetpts=PTS-STARTPTS", num3(ls), num3(le));
    for f in chain_effects(t, t.effects.len()) {
        c.push(',');
        c.push_str(&f);
    }
    if let Some(g) = gain_expr(t, ls_abs) {
        c.push(',');
        c.push_str(&g);
    }
    if delay_ms > 0 {
        c.push_str(&format!(",adelay={delay_ms}:all=1"));
    }
    if (t.volume - 1.0).abs() > 0.001 {
        c.push_str(&format!(",volume={:.3}", t.volume));
    }
    c
}

fn num3(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".into() } else { s.to_string() }
}

// ------------------------------------------------------------------ preview rendering

/// Everything that changes how the track sounds except volume, mute, solo (those are live in the preview).
pub fn render_key(t: &TrackEdit, clip_duration: f64) -> String {
    let mut c = t.clone();
    c.volume = 1.0;
    c.muted = false;
    c.solo = false;
    c.deleted = false;
    c.label.clear();
    format!("{:?}|{clip_duration:.3}", c)
}

/// ffmpeg arguments that render the track as it will be exported, from timeline 0, into `out` (FLAC).
/// `input` / `ordinal`: the file and the audio stream the track reads.
pub fn preview_render_args(input: &Path, ordinal: usize, t: &TrackEdit, out: &Path) -> Vec<String> {
    let (pa, pb) = t.placed();
    let vis_a = pa.max(0.0);
    let ls = (vis_a - t.shift).max(0.0);
    let le = (pb - t.shift).max(ls + 0.01);
    // the preview applies the track volume live
    let mut dry = t.clone();
    dry.volume = 1.0;
    let chain = track_chain(&dry, ls, le, ls, (vis_a * 1000.0).round() as i64);
    vec![
        "-hide_banner".into(),
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        format!("[0:a:{ordinal}]{chain}[a]"),
        "-map".into(),
        "[a]".into(),
        "-ar".into(),
        "48000".into(),
        "-c:a".into(),
        "flac".into(),
        "-compression_level".into(),
        "0".into(),
        out.to_string_lossy().into_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> TrackEdit {
        TrackEdit::original(0, "a".into(), 20.0)
    }

    #[test]
    fn plain_track_has_no_extra_filters() {
        let t = track();
        assert_eq!(track_chain(&t, 0.0, 20.0, 0.0, 0), "atrim=start=0:end=20,asetpts=PTS-STARTPTS");
        assert!(gain_expr(&t, 0.0).is_none() && !t.has_fx());
    }

    #[test]
    fn eq_and_compressor_become_filters() {
        let mut t = track();
        t.effects = vec![
            Effect::new(EffectKind::Eq(vec![
                EqBand { kind: EqKind::Peak, freq: 1000.0, gain: 3.0, q: 1.0, on: true },
                EqBand { kind: EqKind::HighPass, freq: 80.0, gain: 0.0, q: 0.7, on: true },
                EqBand { kind: EqKind::LowShelf, freq: 100.0, gain: 2.0, q: 0.7, on: false },
            ])),
            Effect::new(EffectKind::Comp(Compressor { threshold_db: -20.0, ratio: 4.0, attack_ms: 10.0, release_ms: 200.0, makeup_db: 6.0 })),
        ];
        let c = track_chain(&t, 0.0, 20.0, 0.0, 0);
        assert!(c.contains(",equalizer=f=1000:t=q:w=1:g=3,highpass=f=80,acompressor=threshold=0.1:ratio=4:attack=10:release=200:makeup=1.995262"), "{c}");
        assert!(!c.contains("lowshelf"), "{c}");
    }

    #[test]
    fn fades_and_keyframes_gain() {
        let mut t = track();
        t.fade_in = 2.0;
        t.fade_out = 4.0;
        assert!((gain_at(&t, 0.0)).abs() < 1e-6);
        assert!((gain_at(&t, 1.0) - 0.5).abs() < 1e-6);
        assert!((gain_at(&t, 10.0) - 1.0).abs() < 1e-6);
        assert!((gain_at(&t, 18.0) - 0.5).abs() < 1e-6);
        t.envelope = vec![EnvPoint { t: 10.0, db: -6.0 }, EnvPoint { t: 5.0, db: 0.0 }];
        // between the points, in dB: halfway = -3 dB; before / after = the first / last value
        assert!((gain_at(&t, 7.5) - 10f32.powf(-3.0 / 20.0)).abs() < 1e-4);
        assert!((gain_at(&t, 12.0) - 10f32.powf(-6.0 / 20.0) * 1.0).abs() < 1e-4);
        let e = gain_expr(&t, 0.0).unwrap();
        assert!(e.starts_with("volume='clip((t+0)/2,0,1)*clip((20-t)/4,0,1)*pow(10,(if(lt(t,5),0,if(lt(t,10),0+(-6)*(t-5)/5,-6)))/20)'"), "{e}");
        assert!(e.ends_with(":eval=frame"));
    }

    #[test]
    fn effects_follow_the_stack_order_and_bypass() {
        let mut t = track();
        let comp = Effect::new(EffectKind::Comp(Compressor::default()));
        let eq = Effect::new(EffectKind::Eq(vec![EqBand { kind: EqKind::HighPass, freq: 80.0, gain: 0.0, q: 0.7, on: true }]));
        t.effects = vec![comp.clone(), eq.clone()];
        let c = track_chain(&t, 0.0, 20.0, 0.0, 0);
        assert!(c.find("acompressor").unwrap() < c.find("highpass").unwrap(), "{c}");
        t.effects = vec![eq, Effect { on: false, ..comp }];
        let c = track_chain(&t, 0.0, 20.0, 0.0, 0);
        assert!(c.contains("highpass") && !c.contains("acompressor"), "{c}");
    }

    #[test]
    fn peak_and_loudness_need_a_matching_measurement() {
        let mut t = track();
        t.effects = vec![Effect::new(EffectKind::Norm(Normalize::new(NormMode::Peak)))];
        let filt = |t: &TrackEdit| effect_filters(t, 0);
        assert_eq!(filt(&t), Vec::<String>::new()); // not measured yet
        let m = Measured { key: norm_key(&t, 0), i: -23.0, tp: -4.0, lra: 5.0, thresh: -33.0, offset: 0.5 };
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Peak, target: -1.0, measured: Some(m.clone()) });
        assert_eq!(filt(&t), vec!["volume=3dB"]);
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Loudness, target: -16.0, measured: Some(m) });
        let f = filt(&t);
        assert!(f[0].contains("loudnorm=I=-16:TP=-1.5:LRA=11:measured_I=-23") && f[0].ends_with(":linear=true") && f[1] == "aresample=48000", "{f:?}");
        // an edit that changes the audio invalidates the measurement
        t.trim_start = 1.0;
        assert_eq!(filt(&t)[0], "loudnorm=I=-16:TP=-1.5:LRA=11");
    }
}
