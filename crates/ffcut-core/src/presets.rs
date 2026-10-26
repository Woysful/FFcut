//! Export presets: a named bundle of encoding choices (not trim/crop/rotate — those belong to a clip).

use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// One thing a preset can contain. A preset holds only the parts it was given: applying it changes those and
/// leaves everything else as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Container,
    VideoCodec,
    Quality,
    Speed,
    PixFmt,
    Resize,
    AudioCodec,
    AudioBitrate,
    AudioMode,
    VideoEffects,
    Captions,
}

impl Part {
    pub const ALL: [Part; 11] = [
        Part::Container,
        Part::VideoCodec,
        Part::Quality,
        Part::Speed,
        Part::PixFmt,
        Part::Resize,
        Part::AudioCodec,
        Part::AudioBitrate,
        Part::AudioMode,
        Part::VideoEffects,
        Part::Captions,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Part::Container => "Container",
            Part::VideoCodec => "Video codec",
            Part::Quality => "Quality / bitrate",
            Part::Speed => "Encode preset",
            Part::PixFmt => "Pixel format",
            Part::Resize => "Resize",
            Part::AudioCodec => "Audio codec",
            Part::AudioBitrate => "Audio bitrate",
            Part::AudioMode => "Audio tracks mode",
            Part::VideoEffects => "Video effects & text",
            Part::Captions => "Captions",
        }
    }
}

/// A named bundle of export choices (not trim / crop / rotate: those belong to a clip). Every part is optional;
/// files saved by older versions had the first eight set, and still read the same way.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<VideoCodec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// `"auto"` = let the encoder choose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pix_fmt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<AudioCodec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_mode: Option<AudioMode>,
    /// Video effects (colour, restoration, covered areas, text blocks) with their settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_effects: Option<Vec<crate::videofx::VEffect>>,
    /// The top and bottom caption bars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captions: Option<(crate::videofx::Caption, crate::videofx::Caption)>,
}

impl Preset {
    pub fn has(&self, p: Part) -> bool {
        match p {
            Part::Container => self.container.is_some(),
            Part::VideoCodec => self.video_codec.is_some(),
            Part::Quality => self.quality.is_some(),
            Part::Speed => self.preset.is_some(),
            Part::PixFmt => self.pix_fmt.is_some(),
            Part::Resize => self.scale.is_some(),
            Part::AudioCodec => self.audio_codec.is_some(),
            Part::AudioBitrate => self.audio_bitrate_kbps.is_some(),
            Part::AudioMode => self.audio_mode.is_some(),
            Part::VideoEffects => self.video_effects.is_some(),
            Part::Captions => self.captions.is_some(),
        }
    }

    pub fn remove(&mut self, p: Part) {
        match p {
            Part::Container => self.container = None,
            Part::VideoCodec => self.video_codec = None,
            Part::Quality => self.quality = None,
            Part::Speed => self.preset = None,
            Part::PixFmt => self.pix_fmt = None,
            Part::Resize => self.scale = None,
            Part::AudioCodec => self.audio_codec = None,
            Part::AudioBitrate => self.audio_bitrate_kbps = None,
            Part::AudioMode => self.audio_mode = None,
            Part::VideoEffects => self.video_effects = None,
            Part::Captions => self.captions = None,
        }
    }

    /// Takes (or refreshes) one part from the current settings.
    pub fn fill(&mut self, p: Part, container: &str, s: &ExportSettings) {
        match p {
            Part::Container => self.container = Some(container.into()),
            Part::VideoCodec => self.video_codec = Some(s.video_codec.clone()),
            Part::Quality => self.quality = Some(s.quality),
            Part::Speed => self.preset = Some(s.preset.clone()),
            Part::PixFmt => self.pix_fmt = Some(s.pix_fmt.clone().unwrap_or_else(|| "auto".into())),
            Part::Resize => self.scale = Some(s.scale),
            Part::AudioCodec => self.audio_codec = Some(s.audio_codec.clone()),
            Part::AudioBitrate => self.audio_bitrate_kbps = Some(s.audio_bitrate_kbps),
            Part::AudioMode => self.audio_mode = Some(s.audio_mode),
            Part::VideoEffects => self.video_effects = Some(s.vfx.effects.clone()),
            Part::Captions => self.captions = Some((s.vfx.top.clone(), s.vfx.bottom.clone())),
        }
    }

    /// What is worth saving by default: the encoding choices, plus effects and captions when there are any.
    pub fn default_parts(s: &ExportSettings) -> Vec<Part> {
        Part::ALL
            .into_iter()
            .filter(|p| match p {
                Part::AudioMode => false,
                Part::VideoEffects => !s.vfx.effects.is_empty(),
                Part::Captions => s.vfx.top.on || s.vfx.bottom.on,
                _ => true,
            })
            .collect()
    }

    pub fn from_parts(container: &str, s: &ExportSettings, parts: &[Part]) -> Self {
        let mut p = Preset::default();
        for part in parts {
            p.fill(*part, container, s);
        }
        p
    }

    /// Everything the encoding side has (the old behaviour).
    pub fn capture(container: &str, s: &ExportSettings) -> Self {
        let parts: Vec<Part> = Part::ALL.into_iter().filter(|p| !matches!(p, Part::AudioMode | Part::VideoEffects | Part::Captions)).collect();
        Preset::from_parts(container, s, &parts)
    }

    /// Writes the preset's parts into `s`; returns the container to use, if the preset has one.
    pub fn apply(&self, s: &mut ExportSettings) -> Option<String> {
        if let Some(v) = &self.video_codec {
            s.video_codec = v.clone();
        }
        if let Some(q) = self.quality {
            s.quality = q;
        }
        if let Some(p) = &self.preset {
            s.preset = p.clone();
        }
        if let Some(f) = &self.pix_fmt {
            s.pix_fmt = (f != "auto").then(|| f.clone());
        }
        if let Some(a) = &self.audio_codec {
            s.audio_codec = a.clone();
        }
        if let Some(b) = self.audio_bitrate_kbps {
            s.audio_bitrate_kbps = b;
        }
        if let Some(sc) = self.scale {
            s.scale = sc;
        }
        if let Some(m) = self.audio_mode {
            s.audio_mode = m;
        }
        if let Some(fx) = &self.video_effects {
            s.vfx.effects = fx.clone();
        }
        if let Some((top, bottom)) = &self.captions {
            s.vfx.top = top.clone();
            s.vfx.bottom = bottom.clone();
        }
        self.container.clone()
    }

    /// One line for the presets list.
    pub fn summary(&self) -> String {
        let parts: Vec<&str> = Part::ALL.into_iter().filter(|p| self.has(*p)).map(Part::label).collect();
        if parts.is_empty() { "empty".to_string() } else { parts.join(" · ") }
    }
}

pub type Presets = BTreeMap<String, Preset>;

/// Named audio effect stacks ("Mic", "Game"…), applied to one track at a time. Kept apart from the export presets.
pub type FxPresets = BTreeMap<String, Vec<Effect>>;

pub fn load_fx(path: &Path) -> FxPresets {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_fx(path: &Path, p: &FxPresets) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(p).unwrap())?;
    std::fs::rename(tmp, path)
}

pub fn load(path: &Path) -> Presets {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &Path, presets: &Presets) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // write to a temp file first so a crash can never leave a half-written presets file
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(presets).unwrap())?;
    std::fs::rename(tmp, path)
}

/// File format for sharing presets: `{"version": 2, "presets": { "<name>": {...} }}`.
pub fn export_json(presets: &Presets) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ "version": 2, "presets": presets })).unwrap()
}

/// Reads presets from: our export format, a bare `{name: preset}` map, or the old Electron
/// FFcut export (`{"version":"1.0","presets":{name:{videoCodec,...}}}`).
/// Returns the presets and how many entries could not be understood.
pub fn parse_import(text: &str) -> Result<(Vec<(String, Preset)>, usize), String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("not a valid JSON file: {e}"))?;
    let map = match v.get("presets") {
        Some(p) => p,
        None => &v,
    }
    .as_object()
    .ok_or("this file does not contain presets")?;
    let (mut ok, mut bad) = (Vec::new(), 0);
    for (name, item) in map {
        match if item.get("videoCodec").is_some() { from_legacy(item) } else { serde_json::from_value::<Preset>(item.clone()).ok().filter(|p| *p != Preset::default()) } {
            Some(p) => ok.push((name.clone(), p)),
            None => bad += 1,
        }
    }
    if ok.is_empty() {
        return Err("no presets found in this file".into());
    }
    Ok((ok, bad))
}

fn from_legacy(v: &serde_json::Value) -> Option<Preset> {
    let o = v.as_object()?;
    let st = |k: &str| o.get(k).and_then(|x| x.as_str());
    let num = |k: &str| o.get(k).and_then(|x| x.as_f64());
    let video = st("videoCodec")?;
    Some(Preset {
        container: Some(st("container").unwrap_or("mp4").to_string()),
        video_codec: Some(match video {
            "copy" => VideoCodec::Copy,
            "auto" => VideoCodec::Auto,
            e => VideoCodec::Encoder(e.to_string()),
        }),
        quality: Some(if st("qualityMode") == Some("bitrate") {
            Quality::Bitrate(num("videoBitrate").unwrap_or(8000.0) as u32)
        } else {
            Quality::Crf(num("crf").unwrap_or(23.0) as u32)
        }),
        preset: Some(st("preset").unwrap_or("medium").to_string()),
        pix_fmt: Some(st("pixelFormat").unwrap_or("auto").to_string()),
        audio_codec: Some(match st("audioCodec").unwrap_or("copy") {
            "copy" => AudioCodec::Copy,
            e => AudioCodec::Encoder(e.to_string()),
        }),
        audio_bitrate_kbps: Some(num("audioBitrate").unwrap_or(192.0) as u32),
        scale: Some(Scale::default()),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_apply() {
        let dir = std::env::temp_dir().join(format!("ffcut-presets-{}", std::process::id()));
        let path = dir.join("presets.json");
        let s = ExportSettings { video_codec: VideoCodec::Encoder("libx265".into()), quality: Quality::Crf(28), scale: Scale { w: None, h: Some(720) }, ..Default::default() };
        let mut all = Presets::new();
        all.insert("Small 720p".into(), Preset::capture("mp4", &s));
        save(&path, &all).unwrap();
        let back = load(&path);
        let mut t = ExportSettings::default();
        assert_eq!(back["Small 720p"].apply(&mut t).as_deref(), Some("mp4"));
        assert_eq!((t.video_codec, t.quality, t.scale), (s.video_codec, s.quality, s.scale));
        assert!(load(&dir.join("missing.json")).is_empty());
    }

    #[test]
    fn export_import_roundtrip_and_legacy() {
        let mut all = Presets::new();
        all.insert("A".into(), Preset::capture("mkv", &ExportSettings { video_codec: VideoCodec::Encoder("libx265".into()), ..Default::default() }));
        let (back, bad) = parse_import(&export_json(&all)).unwrap();
        assert_eq!((back.len(), bad), (1, 0));
        assert_eq!(back[0].1, all["A"]);

        let legacy = r#"{"version":"1.0","presets":{"Old":{"videoCodec":"libx264","audioCodec":"aac","qualityMode":"crf","crf":20,
            "audioBitrate":128,"preset":"slow","pixelFormat":"auto","container":"mp4","customCommand":"-c copy"},"Junk":5}}"#;
        let (l, bad) = parse_import(legacy).unwrap();
        assert_eq!(bad, 1);
        assert_eq!(l[0].0, "Old");
        assert_eq!((l[0].1.quality, l[0].1.audio_bitrate_kbps), (Some(Quality::Crf(20)), Some(128)));
        assert!(parse_import("[1,2]").is_err());
    }

    #[test]
    fn parts_apply_only_what_is_present() {
        let s = ExportSettings { quality: Quality::Crf(30), ..Default::default() };
        let p = Preset::from_parts("mkv", &s, &[Part::Quality]);
        assert!(p.has(Part::Quality) && !p.has(Part::Container));
        let mut t = ExportSettings::default();
        assert_eq!(p.apply(&mut t), None);
        assert_eq!(t.quality, Quality::Crf(30));
        // an old file that still carries audio_effects loads fine
        assert!(serde_json::from_str::<Preset>(r#"{"quality":{"Crf":20},"audio_effects":[]}"#).is_ok());
    }

    #[test]
    fn fx_presets_roundtrip() {
        use crate::model::{Effect, EffectKind};
        let path = std::env::temp_dir().join(format!("ffcut-fx-{}.json", std::process::id()));
        let mut all = FxPresets::new();
        all.insert("Mic".into(), vec![Effect { on: true, kind: EffectKind::DeEss(Default::default()) }]);
        save_fx(&path, &all).unwrap();
        assert_eq!(load_fx(&path), all);
    }
}
