//! Audio waveform peaks, computed by ffmpeg (decode -> mono s16 on a pipe).

use std::path::Path;
use std::process::Stdio;

/// Peaks per second of audio. 200 = one value per 5 ms: sharp enough for a big waveform, still small in memory
/// (an hour is 720 000 values). Zoomed in further, `peaks_window` makes a finer picture of the visible part.
pub const PEAKS_PER_SEC: usize = 200;
const RATE: usize = 8000;

/// `audio_ordinal` = position among audio streams. Returns peaks in 0.0..=1.0.
pub fn peaks(ffmpeg: &Path, file: &Path, audio_ordinal: usize) -> Result<Vec<f32>, String> {
    let out = crate::run::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(file)
        .args(["-map", &format!("0:a:{audio_ordinal}"), "-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(peaks_from_pcm(&out.stdout))
}

/// Peaks of just `start .. start+len` seconds, `per_sec` of them per second (for the zoomed-in view).
/// Decodes at 48 kHz so a very fine picture still has samples in every bucket.
pub fn peaks_window(ffmpeg: &Path, file: &Path, audio_ordinal: usize, start: f64, len: f64, per_sec: f64) -> Result<Vec<f32>, String> {
    const HI_RATE: f64 = 48000.0;
    let out = crate::run::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-ss", &format!("{:.4}", start.max(0.0)), "-t", &format!("{:.4}", len), "-i"])
        .arg(file)
        .args(["-map", &format!("0:a:{audio_ordinal}"), "-ac", "1", "-ar", "48000", "-f", "s16le", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let per = ((HI_RATE / per_sec.max(1.0)).round() as usize).max(1);
    Ok(out.stdout.chunks(per * 2).map(|c| c.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0).collect())
}

pub fn peaks_from_pcm(bytes: &[u8]) -> Vec<f32> {
    let per = RATE / PEAKS_PER_SEC;
    bytes
        .chunks(per * 2)
        .map(|c| {
            c.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_follow_amplitude() {
        // 1 s silence then 1 s of full-scale
        let mut pcm = vec![0u8; RATE * 2];
        for _ in 0..RATE {
            pcm.extend_from_slice(&i16::MAX.to_le_bytes());
        }
        let p = peaks_from_pcm(&pcm);
        assert_eq!(p.len(), 2 * PEAKS_PER_SEC);
        assert_eq!(p[10], 0.0);
        assert!(p[PEAKS_PER_SEC + 10] > 0.99);
    }
}
