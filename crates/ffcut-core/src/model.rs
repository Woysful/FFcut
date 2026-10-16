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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    R180,
    Ccw90,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `None` on a side = keep aspect ratio (ffmpeg's `-2`: also keeps the size even).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scale {
    pub w: Option<u32>,
    pub h: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VideoCodec {
    /// Stream copy. Only honoured when no filter is needed.
    Copy,
    /// Same family as the source (h264 -> libx264, hevc -> libx265, ...).
    Auto,
    Encoder(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioCodec {
    Copy,
    Encoder(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    /// Constant quality (CRF / CQ / QP — mapped per encoder).
    Crf(u32),
    /// Target bitrate in kbit/s.
    Bitrate(u32),
}

/// How the audible tracks end up in the output file.
/// *Which* tracks are audible is decided by the track list (removed / muted / solo), see [`audible`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioMode {
    /// No audio at all.
    None,
    /// Every audible track stays its own audio stream.
    Separate,
    /// All audible tracks are mixed into a single stream.
    Merge,
}

/// Where the audio of a timeline track comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrackSource {
    /// Audio stream of the opened file (position among audio streams).
    Original(usize),
    /// Another file (first audio stream is used). `duration` in seconds.
    External { path: String, duration: f64 },
}

/// One audio track as edited on the timeline.
///
/// Times are on the *source timeline* (the same axis as the video):
/// the audio taken from the source is `[trim_start, trim_end]` and it is placed on the timeline at
/// `[trim_start + shift, trim_end + shift]`. Untouched = `0..duration`, shift 0, volume 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackEdit {
    pub source: TrackSource,
    pub label: String,
    pub trim_start: f64,
    pub trim_end: f64,
    pub shift: f64,
    /// 1.0 = unchanged.
    pub volume: f32,
    /// Removed from the export (it can be restored).
    pub deleted: bool,
    /// Silenced: left out of the export, like in a video editor. Ignored while any track is soloed.
    #[serde(default)]
    pub muted: bool,
    /// When any track is soloed only soloed tracks are heard / exported.
    #[serde(default)]
    pub solo: bool,
}

impl TrackEdit {
    pub fn original(index: usize, label: String, duration: f64) -> Self {
        TrackEdit { source: TrackSource::Original(index), label, trim_start: 0.0, trim_end: duration, shift: 0.0, volume: 1.0, deleted: false, muted: false, solo: false }
    }
    pub fn external(path: String, label: String, duration: f64) -> Self {
        TrackEdit { source: TrackSource::External { path, duration }, label, trim_start: 0.0, trim_end: duration, shift: 0.0, volume: 1.0, deleted: false, muted: false, solo: false }
    }
    pub fn source_duration(&self, clip_duration: f64) -> f64 {
        match &self.source {
            TrackSource::Original(_) => clip_duration,
            TrackSource::External { duration, .. } => *duration,
        }
    }
    /// Does this track need an ffmpeg filter chain (anything but a plain copy of the original stream)?
    pub fn needs_filter(&self, clip_duration: f64) -> bool {
        matches!(self.source, TrackSource::External { .. })
            || self.trim_start > 0.001
            || self.trim_end < clip_duration - 0.001
            || self.shift.abs() > 0.001
            || (self.volume - 1.0).abs() > 0.001
    }
    /// Anything differs from the untouched original (including removed / muted / solo).
    pub fn is_modified(&self, clip_duration: f64) -> bool {
        self.deleted || self.muted || self.solo || self.needs_filter(clip_duration)
    }
    /// Timeline interval the clip occupies.
    pub fn placed(&self) -> (f64, f64) {
        (self.trim_start + self.shift, self.trim_end + self.shift)
    }
}

/// The tracks that end up in the output: not removed, and (solo if any track is soloed, else not muted).
pub fn audible(tracks: &[TrackEdit]) -> Vec<&TrackEdit> {
    let any_solo = tracks.iter().any(|t| t.solo && !t.deleted);
    tracks.iter().filter(|t| !t.deleted && if any_solo { t.solo } else { !t.muted }).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSettings {
    /// Seconds. 0 = from the start.
    pub trim_start: f64,
    /// Seconds. `None` = to the end.
    pub trim_end: Option<f64>,

    pub rotation: Rotation,
    /// Coordinates are in the *rotated* frame (what the user sees in the preview).
    pub crop: Option<Crop>,
    pub flip_h: bool,
    pub flip_v: bool,
    pub scale: Scale,

    pub video_codec: VideoCodec,
    pub quality: Quality,
    /// x264/x265 preset name ("medium", "slow", ...).
    pub preset: String,
    /// `None` = auto (keep source format if the encoder supports it).
    pub pix_fmt: Option<String>,

    pub audio_codec: AudioCodec,
    pub audio_bitrate_kbps: u32,
    pub audio_mode: AudioMode,
    /// The audio tracks (original streams + added files) and their edits. This is the single source of
    /// truth for audio: the sidebar and the multi-track timeline both edit this list.
    /// Empty = "use every audio stream of the file untouched".
    #[serde(default)]
    pub tracks: Vec<TrackEdit>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            trim_start: 0.0,
            trim_end: None,
            rotation: Rotation::None,
            crop: None,
            flip_h: false,
            flip_v: false,
            scale: Scale::default(),
            video_codec: VideoCodec::Copy,
            quality: Quality::Crf(23),
            preset: "medium".into(),
            pix_fmt: None,
            audio_codec: AudioCodec::Copy,
            audio_bitrate_kbps: 192,
            audio_mode: AudioMode::Separate,
            tracks: Vec::new(),
        }
    }
}
