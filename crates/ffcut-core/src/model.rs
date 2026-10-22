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
    /// Fade in from silence over this many seconds, from the start of the clip.
    #[serde(default)]
    pub fade_in: f64,
    /// Fade out to silence over this many seconds, ending at the end of the clip.
    #[serde(default)]
    pub fade_out: f64,
    /// Volume keyframes (gain in dB at a point of the source), joined by straight lines. Empty = none.
    #[serde(default)]
    pub envelope: Vec<EnvPoint>,
    /// The effect stack, applied top to bottom.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// One effect of a track's stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    /// Off = bypassed (kept in the list, not applied).
    pub on: bool,
    pub kind: EffectKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    /// Parametric equalizer: its bands, applied in order.
    Eq(Vec<EqBand>),
    Comp(Compressor),
    Norm(Normalize),
    DeEss(DeEsser),
    Denoise(Denoise),
}

impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        Effect { on: true, kind }
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            EffectKind::Eq(_) => "Equalizer",
            EffectKind::Comp(_) => "Compressor",
            EffectKind::Norm(_) => "Normalize",
            EffectKind::DeEss(_) => "De-esser",
            EffectKind::Denoise(_) => "Noise reduction",
        }
    }
}

/// A volume keyframe. `t` is on the source axis (it stays with the sound when the clip is moved or trimmed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvPoint {
    pub t: f64,
    pub db: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqKind {
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    pub kind: EqKind,
    pub freq: f32,
    /// dB (ignored by the pass filters).
    pub gain: f32,
    pub q: f32,
    pub on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Compressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Compressor { threshold_db: -18.0, ratio: 3.0, attack_ms: 20.0, release_ms: 250.0, makeup_db: 3.0 }
    }
}

/// Sibilance reducer (ffmpeg `deesser`); all values 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeEsser {
    pub intensity: f32,
    /// Largest reduction allowed.
    pub max: f32,
    /// Centre of the "s" band (ffmpeg's relative scale).
    pub freq: f32,
}

impl Default for DeEsser {
    fn default() -> Self {
        DeEsser { intensity: 0.5, max: 0.5, freq: 0.5 }
    }
}

/// Broadband noise reduction (ffmpeg `afftdn`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Denoise {
    /// How much to take away, dB (1..40).
    pub reduction_db: f32,
    /// Noise floor, dB (-80..-20).
    pub floor_db: f32,
}

impl Default for Denoise {
    fn default() -> Self {
        Denoise { reduction_db: 12.0, floor_db: -50.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NormMode {
    /// Raise / lower the track so its peak reaches `target` dB.
    Peak,
    /// EBU R128 loudness (two passes) to `target` LUFS.
    Loudness,
}

/// Result of the analysis pass (see `audiofx::measure`); valid only for the edit state it was measured on (`key`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    pub key: String,
    pub i: f64,
    pub tp: f64,
    pub lra: f64,
    pub thresh: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Normalize {
    pub mode: NormMode,
    pub target: f32,
    #[serde(default)]
    pub measured: Option<Measured>,
}

/// The analysis result is a cache the app fills in by itself, not something the user set: two settings
/// that differ only in it are the same edit (this is what keeps a finished measurement out of the undo history).
impl PartialEq for Normalize {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode && self.target == other.target
    }
}

impl Normalize {
    pub fn new(mode: NormMode) -> Self {
        Normalize { mode, target: if mode == NormMode::Peak { -1.0 } else { -16.0 }, measured: None }
    }
}

impl TrackEdit {
    pub fn original(index: usize, label: String, duration: f64) -> Self {
        TrackEdit { source: TrackSource::Original(index), label, trim_start: 0.0, trim_end: duration, shift: 0.0, volume: 1.0, deleted: false, muted: false, solo: false, fade_in: 0.0, fade_out: 0.0, envelope: Vec::new(), effects: Vec::new() }
    }
    pub fn external(path: String, label: String, duration: f64) -> Self {
        TrackEdit { source: TrackSource::External { path, duration }, label, trim_start: 0.0, trim_end: duration, shift: 0.0, volume: 1.0, deleted: false, muted: false, solo: false, fade_in: 0.0, fade_out: 0.0, envelope: Vec::new(), effects: Vec::new() }
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
            || self.has_fx()
    }
    /// Fades, keyframes, equalizer, compressor or normalization: anything beyond trim / shift / volume.
    pub fn has_fx(&self) -> bool {
        self.fade_in > 0.001 || self.fade_out > 0.001 || !self.envelope.is_empty() || self.effects.iter().any(|e| e.on)
    }
    /// Does the preview need this track rendered by ffmpeg (shift, trim and effects cannot be done live)?
    pub fn needs_bake(&self, clip_duration: f64) -> bool {
        self.trim_start > 0.001 || self.trim_end < self.source_duration(clip_duration) - 0.001 || self.shift.abs() > 0.001 || self.has_fx()
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
    /// Video effects, covered areas, text and caption bars (see `videofx`).
    #[serde(default)]
    pub vfx: crate::videofx::VideoFx,
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
            video_codec: VideoCodec::Copy,
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
