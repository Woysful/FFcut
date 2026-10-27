use super::audio::TrackEdit;
use super::video::VideoFx;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    R180,
    Ccw90,
}

impl Rotation {
    /// Clockwise degrees.
    pub(crate) fn degrees(self) -> i64 {
        match self {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::R180 => 180,
            Rotation::Ccw90 => 270,
        }
    }
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
/// *Which* tracks are audible is decided by the track list (removed / muted / solo), see [`audible`](super::audible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioMode {
    /// No audio at all.
    None,
    /// Every audible track stays its own audio stream.
    Separate,
    /// All audible tracks are mixed into a single stream.
    Merge,
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
    /// Video effects, covered areas, text and caption bars (see `videofx`).
    #[serde(default)]
    pub vfx: VideoFx,
    /// Export only the sound of a video file (no picture in the output).
    #[serde(default)]
    pub audio_only: bool,
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
            video_codec: VideoCodec::Auto,
            quality: Quality::Crf(23),
            preset: "medium".into(),
            pix_fmt: None,
            audio_codec: AudioCodec::Copy,
            audio_bitrate_kbps: 192,
            audio_mode: AudioMode::Separate,
            tracks: Vec::new(),
            vfx: Default::default(),
            audio_only: false,
        }
    }
}
