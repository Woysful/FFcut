//! Audio waveform peaks, computed by ffmpeg (decode -> mono 4 kHz s16 on a pipe).

use std::path::Path;
use std::process::Stdio;

/// Peaks per second of audio. 50 = one value per 20 ms: plenty for a timeline, tiny in memory.
pub const PEAKS_PER_SEC: usize = 50;
const RATE: usize = 4000;

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
