//! Analysis passes that run ffmpeg over a track (the filter text itself comes from `audiofx`).

use crate::audiofx::{chain_effects, norm_key, num, LRA, TRUE_PEAK};
use crate::error::Error;
use crate::model::*;
use crate::run::{command, run_capture};
use std::path::Path;

/// loudnorm accepts integrated loudness targets in this range (LUFS).
const LOUDNORM_I_RANGE: (f64, f64) = (-70.0, -5.0);
/// Target of the analysis when the result does not depend on it (a peak normalization only needs the true peak).
const DEFAULT_I: f64 = -16.0;

/// Runs ffmpeg's loudness analysis over the clip (after trim and the effects before the normalization at `idx`),
/// toward the normalization's own target.
pub fn measure(ffmpeg: &Path, file: &Path, ordinal: usize, t: &TrackEdit, idx: usize) -> Result<Measured, Error> {
    let target = match t.effects.get(idx).map(|e| &e.kind) {
        Some(EffectKind::Norm(n)) if n.mode == NormMode::Loudness => (n.target as f64).clamp(LOUDNORM_I_RANGE.0, LOUDNORM_I_RANGE.1),
        _ => DEFAULT_I,
    };
    let mut chain = vec![format!("atrim=start={}:end={}", num(t.trim_start), num(t.trim_end)), "asetpts=PTS-STARTPTS".into()];
    chain.extend(chain_effects(t, idx));
    chain.push(format!("loudnorm=I={}:TP={}:LRA={}:print_format=json", num(target), num(TRUE_PEAK), num(LRA)));
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-i"]).arg(file).args([
        "-map",
        &format!("0:a:{ordinal}"),
        "-vn",
        "-af",
        &chain.join(","),
        "-f",
        "null",
        "-",
    ]);
    let out = run_capture("ffmpeg", &mut cmd).map_err(|e| match e {
        // without -v error the whole stream description comes first: the reason is the last line
        Error::Tool { tool, stderr } => Error::Tool { tool, stderr: stderr.lines().last().unwrap_or("").trim().to_string() },
        e => e,
    })?;
    let err = String::from_utf8_lossy(&out.stderr);
    parse_loudnorm(&err).map(|m| Measured { key: norm_key(t, idx), ..m })
}

/// The values of loudnorm's JSON report (the last `{...}` of the log). `key` is left 0.
fn parse_loudnorm(log: &str) -> Result<Measured, Error> {
    let json = match (log.rfind('{'), log.rfind('}')) {
        (Some(a), Some(b)) if b > a => &log[a..=b],
        _ => return Err(Error::Parse("no measurement in the ffmpeg output".into())),
    };
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| Error::Parse(format!("bad measurement from ffmpeg: {e}")))?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).and_then(|s| s.trim().parse::<f64>().ok());
    let i = get("input_i").ok_or_else(|| Error::Parse("no loudness in the measurement".into()))?;
    let tp = get("input_tp").ok_or_else(|| Error::Parse("no peak in the measurement".into()))?;
    if !i.is_finite() || !tp.is_finite() {
        return Err(Error::Invalid("the track is silent".into()));
    }
    Ok(Measured {
        key: 0,
        i,
        tp,
        lra: get("input_lra").unwrap_or(0.0),
        thresh: get("input_thresh").unwrap_or(-70.0),
        offset: get("target_offset").unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_loudnorm_report() {
        let log = "[Parsed_loudnorm_2 @ 0x1] \n{\n\t\"input_i\" : \"-23.05\",\n\t\"input_tp\" : \"-6.01\",\n\t\"input_lra\" : \"0.00\",\n\t\"input_thresh\" : \"-33.05\",\n\t\"target_offset\" : \"0.04\"\n}\n";
        let m = parse_loudnorm(log).unwrap();
        assert_eq!((m.i, m.tp, m.lra, m.thresh, m.offset), (-23.05, -6.01, 0.0, -33.05, 0.04));
        let silent = log.replace("-23.05", "-inf");
        assert_eq!(parse_loudnorm(&silent).unwrap_err().to_string(), "the track is silent");
        assert!(matches!(parse_loudnorm("nothing here"), Err(Error::Parse(_))));
    }
}
