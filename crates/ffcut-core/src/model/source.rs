use serde::{Deserialize, Serialize};

/// What we know about the opened file (filled from ffprobe).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub pix_fmt: String,
    /// Clockwise degrees the container asks players to rotate by (phone videos). `width`/`height`
    /// are already the *display* size (swapped when this is 90/270).
    #[serde(default)]
    pub rotation: u32,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub file_size: u64,
    /// Human container name ("MP4", "QuickTime MOV", "Matroska"...), see `probe::container_label`.
    #[serde(default)]
    pub format: String,
    /// ISO-BMFF major brand ("isom", "qt  ", ...), empty for other containers.
    #[serde(default)]
    pub brand: String,
    /// Overall bitrate, bit/s.
    #[serde(default)]
    pub bit_rate: u64,
    /// The file has no (real) video stream: the app works in audio-file mode.
    #[serde(default)]
    pub audio_only: bool,
    pub audio: Vec<AudioStream>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioStream {
    /// Position among audio streams (0-based): this is what `-map 0:a:N` wants.
    pub index: usize,
    pub codec: String,
    pub channels: u32,
    pub language: Option<String>,
    pub title: Option<String>,
}
