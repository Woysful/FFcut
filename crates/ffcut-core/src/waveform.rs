//! Audio waveform peaks, computed by ffmpeg (decode -> mono s16 on a pipe).

use crate::error::Error;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

/// Peaks per second of audio. 200 = one value per 5 ms: sharp enough for a big waveform, still small in memory
/// (an hour is 720 000 values). Zoomed in further, `peaks_window` makes a finer picture of the visible part.
pub const PEAKS_PER_SEC: usize = 200;
const RATE: usize = 8000;
/// Decode rate of `peaks_window`: a very fine picture still has samples in every bucket.
const HI_RATE: usize = 48000;
const READ_CHUNK: usize = 64 * 1024;

/// `audio_ordinal` = position among audio streams. Returns peaks in 0.0..=1.0.
pub fn peaks(ffmpeg: &Path, file: &Path, audio_ordinal: usize) -> Result<Vec<f32>, Error> {
    decode_peaks(ffmpeg, file, audio_ordinal, None, RATE, RATE / PEAKS_PER_SEC)
}

/// Peaks of just `start .. start+len` seconds, `per_sec` of them per second (for the zoomed-in view).
pub fn peaks_window(ffmpeg: &Path, file: &Path, audio_ordinal: usize, start: f64, len: f64, per_sec: f64) -> Result<Vec<f32>, Error> {
    let per = ((HI_RATE as f64 / per_sec.max(1.0)).round() as usize).max(1);
    decode_peaks(ffmpeg, file, audio_ordinal, Some((start, len)), HI_RATE, per)
}

/// Peaks of 8 kHz mono s16le PCM, `PEAKS_PER_SEC` per second.
pub fn peaks_from_pcm(bytes: &[u8]) -> Vec<f32> {
    let mut p = Peaks::new(RATE / PEAKS_PER_SEC);
    p.feed(bytes);
    p.finish()
}

/// Decodes one audio stream to mono s16le at `rate` and reduces it to one peak per `per` samples while it
/// streams in: the PCM itself is never held in memory.
fn decode_peaks(ffmpeg: &Path, file: &Path, ordinal: usize, window: Option<(f64, f64)>, rate: usize, per: usize) -> Result<Vec<f32>, Error> {
    let mut cmd = crate::run::command(ffmpeg);
    cmd.args(["-v", "error", "-nostdin"]);
    if let Some((start, len)) = window {
        cmd.args(["-ss", &format!("{:.4}", start.max(0.0)), "-t", &format!("{len:.4}")]);
    }
    cmd.arg("-i").arg(file).args(["-map", &format!("0:a:{ordinal}"), "-ac", "1", "-ar", &rate.to_string(), "-f", "s16le", "-"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|source| Error::Spawn { tool: "ffmpeg".into(), source })?;

    // stderr is drained on its own thread so a chatty ffmpeg can never block on a full pipe
    let stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });

    let mut acc = Peaks::new(per);
    if let Some(mut out) = child.stdout.take() {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            match out.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => acc.feed(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Io(e));
                }
            }
        }
    }
    let status = child.wait()?;
    let stderr = err_thread.join().unwrap_or_default();
    if !status.success() {
        return Err(Error::Tool { tool: "ffmpeg".into(), stderr: stderr.trim().to_string() });
    }
    Ok(acc.finish())
}

/// Running peak of s16le samples in buckets of `per` samples; the last bucket may be shorter.
struct Peaks {
    bucket_bytes: usize,
    filled: usize,
    max: u16,
    low_byte: Option<u8>,
    out: Vec<f32>,
}

impl Peaks {
    fn new(per: usize) -> Self {
        Peaks { bucket_bytes: per.max(1) * 2, filled: 0, max: 0, low_byte: None, out: Vec::new() }
    }

    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match self.low_byte.take() {
                None => self.low_byte = Some(b),
                Some(lo) => self.max = self.max.max(i16::from_le_bytes([lo, b]).unsigned_abs()),
            }
            self.filled += 1;
            if self.filled == self.bucket_bytes {
                self.flush();
            }
        }
    }

    fn flush(&mut self) {
        self.out.push(self.max as f32 / 32768.0);
        self.filled = 0;
        self.max = 0;
        self.low_byte = None;
    }

    fn finish(mut self) -> Vec<f32> {
        if self.filled > 0 {
            self.flush();
        }
        self.out
    }
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

    /// The old whole-buffer version, as the reference.
    fn reference(bytes: &[u8], per: usize) -> Vec<f32> {
        bytes.chunks(per * 2).map(|c| c.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0).collect()
    }

    #[test]
    fn streaming_gives_the_same_peaks_in_any_pieces() {
        let pcm: Vec<u8> = (0..20_011u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for per in [1, 7, 40, 333] {
            for piece in [1, 3, 64, 4096, pcm.len()] {
                let mut p = Peaks::new(per);
                for c in pcm.chunks(piece) {
                    p.feed(c);
                }
                assert_eq!(p.finish(), reference(&pcm, per), "per {per}, piece {piece}");
            }
        }
    }
}
