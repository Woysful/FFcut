//! Per-track audio processing: equalizer, compressor, normalization, fades and volume keyframes.
//!
//! Everything is expressed as plain ffmpeg filters, so the preview and the export run the very same
//! chain (`track_chain`), and what you read in the command is what you hear. Pure text building: the
//! loudness analysis that runs ffmpeg is `analysis::measure`.
//!
//! Chain order, per track: trim → equalizer → compressor → normalization → fades / keyframes →
//! (placement on the timeline) → track volume.

use crate::fmtnum;
use crate::hash::Fnv;
use crate::model::*;
use std::fmt::Display;
use std::hash::Hasher;
use std::path::Path;

/// Loudness normalization uses these fixed companions of the target loudness.
pub(crate) const TRUE_PEAK: f64 = -1.5;
pub(crate) const LRA: f64 = 11.0;

pub(crate) fn num(v: f64) -> String {
    fmtnum::num(v, 4)
}

/// `t+3.2` / `t-3.2` as an expression.
fn t_plus(c: f64) -> String {
    if c >= 0.0 {
        format!("(t+{})", num(c))
    } else {
        format!("(t-{})", num(-c))
    }
}

fn db_to_lin(db: f32) -> f64 {
    10f64.powf(db as f64 / 20.0)
}

/// `[in0][in1]...amix=...[out]`: mixes the labelled streams into one, keeping every level as it is.
pub(crate) fn amix<L: Display>(inputs: impl IntoIterator<Item = L>, out: &str) -> String {
    let mut ins = String::new();
    let mut count = 0;
    for l in inputs {
        ins.push_str(&format!("[{l}]"));
        count += 1;
    }
    format!("{ins}amix=inputs={count}:duration=longest:normalize=0[{out}]")
}

// ------------------------------------------------------------------ equalizer / compressor

pub(crate) fn eq_filters(bands: &[EqBand]) -> Vec<String> {
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

pub(crate) fn comp_filter(c: &Compressor) -> String {
    // acompressor takes linear values: threshold 0.001..1, makeup 1..64
    let thr = db_to_lin(c.threshold_db.clamp(-60.0, 0.0)).clamp(0.000976563, 1.0);
    let mk = db_to_lin(c.makeup_db.clamp(0.0, 36.0)).clamp(1.0, 64.0);
    format!(
        "acompressor=threshold={}:ratio={}:attack={}:release={}:makeup={}",
        fmtnum::num(thr, 6),
        num(c.ratio.clamp(1.0, 20.0) as f64),
        num(c.attack_ms.clamp(0.01, 2000.0) as f64),
        num(c.release_ms.clamp(0.01, 9000.0) as f64),
        fmtnum::num(mk, 6)
    )
}

/// Filters of the enabled effects before position `upto` of the stack: what a normalization at `upto`
/// is measured after.
pub(crate) fn chain_effects(t: &TrackEdit, upto: usize) -> Vec<String> {
    (0..upto.min(t.effects.len())).flat_map(|idx| effect_filters(t, idx)).collect()
}

/// Filters of one effect (empty when bypassed).
pub(crate) fn effect_filters(t: &TrackEdit, idx: usize) -> Vec<String> {
    let Some(e) = t.effects.get(idx).filter(|e| e.on) else { return vec![] };
    match &e.kind {
        EffectKind::Eq(bands) => eq_filters(bands),
        EffectKind::Comp(c) => vec![comp_filter(c)],
        EffectKind::Norm(n) => norm_filters(t, idx, n),
        EffectKind::DeEss(d) => {
            vec![format!("deesser=i={:.2}:m={:.2}:f={:.2}", d.intensity.clamp(0.0, 1.0), d.max.clamp(0.0, 1.0), d.freq.clamp(0.0, 1.0))]
        }
        EffectKind::Denoise(d) => vec![format!("afftdn=nr={:.1}:nf={:.1}:tn=1", d.reduction_db.clamp(0.01, 97.0), d.floor_db.clamp(-80.0, -20.0))],
        EffectKind::Filter(f) => crate::filters::filter_text(f).into_iter().collect(),
        EffectKind::Plugin(p) => crate::plugins::plugin_text(p).into_iter().collect(),
        EffectKind::Builtin(b) => crate::builtin::filter_text(b, crate::filters::Media::Audio).into_iter().collect(),
    }
}

// ------------------------------------------------------------------ normalization

/// Identifies what a measurement of the normalization at `idx` was made on: same source, same clip,
/// same effects before it, same mode (and, for loudness, the same target: the measured offset depends on it).
///
/// Costs one pass over the effects before `idx` (their settings are hashed, not turned into filters).
pub fn norm_key(t: &TrackEdit, idx: usize) -> u64 {
    let mut h = Fnv::new();
    h.json(&t.source);
    h.millis(t.trim_start);
    h.millis(t.trim_end);
    for e in t.effects.iter().take(idx).filter(|e| e.on) {
        h.json(&e.kind);
    }
    if let Some(EffectKind::Norm(n)) = t.effects.get(idx).map(|e| &e.kind) {
        h.json(&n.mode);
        if n.mode == NormMode::Loudness {
            h.write_u32(n.target.to_bits());
        }
    }
    h.finish()
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
                    num(m.i),
                    num(m.lra),
                    num(m.tp),
                    num(m.thresh),
                    num(m.offset)
                ));
            }
            // loudnorm works at 192 kHz internally
            vec![f, "aresample=48000".into()]
        }
    }
}

// ------------------------------------------------------------------ fades and keyframes

/// Keyframes sorted by time. The UI keeps them sorted, so this normally borrows.
fn sorted_env(t: &TrackEdit) -> std::borrow::Cow<'_, [EnvPoint]> {
    if t.envelope.is_sorted_by(|a, b| a.t <= b.t) {
        std::borrow::Cow::Borrowed(&t.envelope)
    } else {
        let mut e = t.envelope.clone();
        e.sort_by(|a, b| a.t.total_cmp(&b.t));
        std::borrow::Cow::Owned(e)
    }
}

/// Keyframe gain in dB at source time `s`; `e` is sorted and not empty.
fn env_db(e: &[EnvPoint], s: f64) -> f64 {
    let (first, last) = (e[0], e[e.len() - 1]);
    if s <= first.t {
        return first.db as f64;
    }
    if s >= last.t {
        return last.db as f64;
    }
    e.windows(2).find(|w| s >= w[0].t && s < w[1].t).map_or(last.db as f64, |w| {
        let (a, b) = (w[0], w[1]);
        a.db as f64 + (b.db - a.db) as f64 * (s - a.t) / (b.t - a.t).max(1e-9)
    })
}

/// Fades and keyframes as one linear gain at source time `s` (1.0 = unchanged). Same maths as `gain_expr`.
///
/// Called for every drawn waveform column: it does not allocate when the keyframes are in order.
pub fn gain_at(t: &TrackEdit, s: f64) -> f32 {
    let mut g = 1.0f64;
    if t.fade_in > TIME_EPS {
        g *= ((s - t.trim_start) / t.fade_in).clamp(0.0, 1.0);
    }
    if t.fade_out > TIME_EPS {
        g *= ((t.trim_end - s) / t.fade_out).clamp(0.0, 1.0);
    }
    if !t.envelope.is_empty() {
        g *= 10f64.powf(env_db(&sorted_env(t), s) / 20.0);
    }
    g as f32
}

/// `volume` expression for fades + keyframes. `ls` = source time where this chain's `atrim` starts
/// (the filter's own `t` is counted from there).
pub(crate) fn gain_expr(t: &TrackEdit, ls: f64) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if t.fade_in > TIME_EPS {
        parts.push(format!("clip({}/{},0,1)", t_plus(ls - t.trim_start), num(t.fade_in)));
    }
    if t.fade_out > TIME_EPS {
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
pub(crate) fn track_chain(t: &TrackEdit, ls: f64, le: f64, ls_abs: f64, delay_ms: i64) -> String {
    let mut c = format!("atrim=start={}:end={},asetpts=PTS-STARTPTS", fmtnum::num(ls, 3), fmtnum::num(le, 3));
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
    if (t.volume - 1.0).abs() > GAIN_EPS {
        c.push_str(&format!(",volume={:.3}", t.volume));
    }
    c
}

// ------------------------------------------------------------------ preview rendering

/// Everything that changes how the track sounds except volume, mute, solo (those are live in the preview).
pub fn render_key(t: &TrackEdit, clip_duration: f64) -> u64 {
    let mut h = Fnv::new();
    h.json(&(&t.source, t.trim_start, t.trim_end, t.shift, t.fade_in, t.fade_out, &t.envelope, &t.effects));
    h.millis(clip_duration);
    h.finish()
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
    let (input, out, graph) = (input.to_string_lossy(), out.to_string_lossy(), format!("[0:a:{ordinal}]{chain}[a]"));
    [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-i",
        &*input,
        "-filter_complex",
        &graph,
        "-map",
        "[a]",
        "-ar",
        "48000",
        "-c:a",
        "flac",
        "-compression_level",
        "0",
        &*out,
    ]
    .map(String::from)
    .to_vec()
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
    fn gain_at_is_the_same_for_sorted_and_unsorted_keyframes_and_never_panics() {
        let mut t = track();
        t.envelope = vec![EnvPoint { t: 2.0, db: -12.0 }, EnvPoint { t: 4.0, db: 0.0 }, EnvPoint { t: 8.0, db: -6.0 }];
        let mut u = t.clone();
        u.envelope.reverse();
        for s in [0.0, 2.0, 3.0, 4.0, 5.5, 8.0, 9.0] {
            assert_eq!(gain_at(&t, s), gain_at(&u, s), "at {s}");
        }
        assert!((gain_at(&t, 3.0) - 10f32.powf(-6.0 / 20.0)).abs() < 1e-5);
        // NaN anywhere must not panic
        let _ = gain_at(&t, f64::NAN);
        u.envelope.push(EnvPoint { t: f64::NAN, db: 3.0 });
        let _ = gain_at(&u, 5.0);
        let _ = gain_at(&u, f64::NAN);
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
        // a peak measurement does not depend on the target: changing it keeps the measurement
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Peak, target: -2.0, measured: Some(m.clone()) });
        assert_eq!(filt(&t), vec!["volume=2dB"]);
        // a peak measurement is not a loudness one
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Loudness, target: -16.0, measured: Some(m.clone()) });
        assert_eq!(filt(&t)[0], "loudnorm=I=-16:TP=-1.5:LRA=11");
        let m = Measured { key: norm_key(&t, 0), ..m };
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Loudness, target: -16.0, measured: Some(m.clone()) });
        let f = filt(&t);
        assert!(f[0].contains("loudnorm=I=-16:TP=-1.5:LRA=11:measured_I=-23") && f[0].ends_with(":linear=true") && f[1] == "aresample=48000", "{f:?}");
        // another loudness target needs its own measurement (the offset depends on it)
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Loudness, target: -14.0, measured: Some(m.clone()) });
        assert_eq!(filt(&t)[0], "loudnorm=I=-14:TP=-1.5:LRA=11");
        // an edit that changes the audio invalidates the measurement
        t.effects[0].kind = EffectKind::Norm(Normalize { mode: NormMode::Loudness, target: -16.0, measured: Some(m) });
        t.trim_start = 1.0;
        assert_eq!(filt(&t)[0], "loudnorm=I=-16:TP=-1.5:LRA=11");
    }

    #[test]
    fn keys_ignore_what_the_preview_does_live() {
        let mut t = track();
        t.effects = vec![Effect::new(EffectKind::Comp(Compressor::default())), Effect::new(EffectKind::Norm(Normalize::new(NormMode::Peak)))];
        let (rk, nk) = (render_key(&t, 20.0), norm_key(&t, 1));
        let mut u = t.clone();
        u.volume = 0.3;
        u.muted = true;
        u.solo = true;
        u.label = "other".into();
        assert_eq!((render_key(&u, 20.0), norm_key(&u, 1)), (rk, nk));
        u.shift = 1.0; // moves the sound in the preview, not what the normalization hears
        assert_ne!(render_key(&u, 20.0), rk);
        assert_eq!(norm_key(&u, 1), nk);
        u.effects[0].on = false;
        assert_ne!(norm_key(&u, 1), nk);
    }

    #[test]
    fn amix_labels() {
        assert_eq!(amix(["a0", "a1"], "aout"), "[a0][a1]amix=inputs=2:duration=longest:normalize=0[aout]");
    }
}
