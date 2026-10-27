use serde::{Deserialize, Deserializer, Serialize};

/// Time differences (seconds) below this are no edit at all: a trim of 0.4 ms is "untouched".
pub(crate) const TIME_EPS: f64 = 0.001;
/// Gain differences (linear ratio) below this are no edit at all.
pub(crate) const GAIN_EPS: f32 = 0.001;

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

/// Result of the analysis pass (see `analysis::measure`); valid only for the edit state it was measured on
/// (`key`, see `audiofx::norm_key`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    #[serde(deserialize_with = "measure_key")]
    pub key: u64,
    pub i: f64,
    pub tp: f64,
    pub lra: f64,
    pub thresh: f64,
    pub offset: f64,
}

/// Older versions stored the key as text. Such a measurement no longer matches anything (it is simply
/// measured again), but it must not make the whole file unreadable.
fn measure_key<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Key {
        Hash(u64),
        Legacy(#[allow(dead_code)] serde::de::IgnoredAny),
    }
    Ok(match Key::deserialize(d)? {
        Key::Hash(k) => k,
        Key::Legacy(_) => 0,
    })
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
    /// An untouched track: the whole `0..duration` of `source`, in place, at full volume, without effects.
    pub fn new(source: TrackSource, label: String, duration: f64) -> Self {
        TrackEdit {
            source,
            label,
            trim_start: 0.0,
            trim_end: duration,
            shift: 0.0,
            volume: 1.0,
            deleted: false,
            muted: false,
            solo: false,
            fade_in: 0.0,
            fade_out: 0.0,
            envelope: Vec::new(),
            effects: Vec::new(),
        }
    }
    pub fn original(index: usize, label: String, duration: f64) -> Self {
        Self::new(TrackSource::Original(index), label, duration)
    }
    pub fn external(path: String, label: String, duration: f64) -> Self {
        Self::new(TrackSource::External { path, duration }, label, duration)
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
            || self.trim_start > TIME_EPS
            || self.trim_end < clip_duration - TIME_EPS
            || self.shift.abs() > TIME_EPS
            || (self.volume - 1.0).abs() > GAIN_EPS
            || self.has_fx()
    }
    /// Fades, keyframes, equalizer, compressor or normalization: anything beyond trim / shift / volume.
    pub fn has_fx(&self) -> bool {
        self.fade_in > TIME_EPS || self.fade_out > TIME_EPS || !self.envelope.is_empty() || self.effects.iter().any(|e| e.on)
    }
    /// Does the preview need this track rendered by ffmpeg (shift, trim and effects cannot be done live)?
    pub fn needs_bake(&self, clip_duration: f64) -> bool {
        self.trim_start > TIME_EPS || self.trim_end < self.source_duration(clip_duration) - TIME_EPS || self.shift.abs() > TIME_EPS || self.has_fx()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_with_an_old_text_key_still_loads() {
        let old = r#"{"key":"Original(0)|0.000|20.000|","i":-20.0,"tp":-3.0,"lra":4.0,"thresh":-30.0,"offset":0.1}"#;
        assert_eq!(serde_json::from_str::<Measured>(old).unwrap().key, 0);
        let m = Measured { key: u64::MAX - 7, i: -20.0, tp: -3.0, lra: 4.0, thresh: -30.0, offset: 0.1 };
        assert_eq!(serde_json::from_str::<Measured>(&serde_json::to_string(&m).unwrap()).unwrap(), m);
    }
}
