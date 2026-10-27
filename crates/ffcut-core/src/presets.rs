//! Export presets: a named bundle of encoding choices (not trim/crop/rotate — those belong to a clip).

use crate::error::Error;
use crate::model::*;
use crate::videofx::{Caption, VEffect};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

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
    pub video_effects: Option<Vec<VEffect>>,
    /// The top and bottom caption bars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captions: Option<(Caption, Caption)>,
}

/// Declares `Part` and everything that goes part by part, from one table:
/// `Part => preset field, label, value taken from (container, settings), written back into settings`.
macro_rules! parts {
    ($($part:ident => $field:ident, $label:literal, take($c:ident, $s:ident) $take:expr, give($v:ident, $t:ident) $give:block)*) => {
        /// One thing a preset can contain. A preset holds only the parts it was given: applying it changes those and
        /// leaves everything else as it is.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Part {
            $($part,)*
        }

        impl Part {
            pub const ALL: [Part; 0 $(+ { let _ = Part::$part; 1 })*] = [$(Part::$part),*];

            pub fn label(self) -> &'static str {
                match self {
                    $(Part::$part => $label,)*
                }
            }
        }

        impl Preset {
            pub fn has(&self, p: Part) -> bool {
                match p {
                    $(Part::$part => self.$field.is_some(),)*
                }
            }

            pub fn remove(&mut self, p: Part) {
                match p {
                    $(Part::$part => self.$field = None,)*
                }
            }

            /// Takes (or refreshes) one part from the current settings.
            pub fn fill(&mut self, p: Part, container: &str, s: &ExportSettings) {
                match p {
                    $(Part::$part => {
                        let ($c, $s) = (container, s);
                        self.$field = Some($take);
                    })*
                }
            }

            /// Writes every part the preset has into `s` (the container is the caller's business).
            fn give(&self, s: &mut ExportSettings) {
                $(if let Some($v) = &self.$field {
                    let $t = &mut *s;
                    $give
                })*
            }
        }
    };
}

parts! {
    Container => container, "Container", take(c, _s) c.to_string(), give(_v, _t) {}
    VideoCodec => video_codec, "Video codec", take(_c, s) s.video_codec.clone(), give(v, t) { t.video_codec = v.clone() }
    Quality => quality, "Quality / bitrate", take(_c, s) s.quality, give(v, t) { t.quality = *v }
    Speed => preset, "Encode preset", take(_c, s) s.preset.clone(), give(v, t) { t.preset = v.clone() }
    PixFmt => pix_fmt, "Pixel format", take(_c, s) s.pix_fmt.clone().unwrap_or_else(|| "auto".into()), give(v, t) { t.pix_fmt = (v != "auto").then(|| v.clone()) }
    Resize => scale, "Resize", take(_c, s) s.scale, give(v, t) { t.scale = *v }
    AudioCodec => audio_codec, "Audio codec", take(_c, s) s.audio_codec.clone(), give(v, t) { t.audio_codec = v.clone() }
    AudioBitrate => audio_bitrate_kbps, "Audio bitrate", take(_c, s) s.audio_bitrate_kbps, give(v, t) { t.audio_bitrate_kbps = *v }
    AudioMode => audio_mode, "Audio tracks mode", take(_c, s) s.audio_mode, give(v, t) { t.audio_mode = *v }
    VideoEffects => video_effects, "Video effects & text", take(_c, s) s.vfx.effects.clone(), give(v, t) { t.vfx.effects = v.clone() }
    Captions => captions, "Captions", take(_c, s) (s.vfx.top.clone(), s.vfx.bottom.clone()), give(v, t) { (t.vfx.top, t.vfx.bottom) = v.clone() }
}

impl Preset {
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
        self.give(s);
        self.container.clone()
    }

    /// One line for the presets list: `.mp4 · libx265 (CRF 28) · audio aac · auto×720`.
    pub fn summary(&self) -> String {
        let mut v: Vec<String> = Vec::new();
        if let Some(c) = &self.container {
            v.push(format!(".{c}"));
        }
        match (&self.video_codec, self.quality) {
            (Some(VideoCodec::Copy), _) => v.push("video copy".into()),
            (vc, q) => {
                let name = match vc {
                    Some(VideoCodec::Auto) => Some("auto".to_string()),
                    Some(VideoCodec::Encoder(e)) => Some(e.clone()),
                    _ => None,
                };
                let q = q.map(|q| match q {
                    Quality::Crf(c) => format!("CRF {c}"),
                    Quality::Bitrate(k) => format!("{k} kbps"),
                });
                match (name, q) {
                    (Some(n), Some(q)) => v.push(format!("{n} ({q})")),
                    (Some(n), None) => v.push(n),
                    (None, Some(q)) => v.push(q),
                    _ => {}
                }
            }
        }
        match &self.audio_codec {
            Some(AudioCodec::Copy) => v.push("audio copy".into()),
            Some(AudioCodec::Encoder(e)) => v.push(format!("audio {e}")),
            None => {}
        }
        if let Some(sc) = self.scale {
            if sc.w.is_some() || sc.h.is_some() {
                v.push(format!("{}×{}", sc.w.map_or("auto".into(), |x| x.to_string()), sc.h.map_or("auto".into(), |x| x.to_string())));
            }
        }
        if let Some(fx) = &self.video_effects {
            v.push(format!("{} video effect(s)", fx.len()));
        }
        if let Some((t, b)) = &self.captions {
            v.push(format!("captions{}", if t.on || b.on { "" } else { " (off)" }));
        }
        if v.is_empty() {
            "empty: applying changes nothing".into()
        } else {
            v.join(" · ")
        }
    }
}

pub type Presets = BTreeMap<String, Preset>;

/// Named audio effect stacks ("Mic", "Game"…), applied to one track at a time. Kept apart from the export presets.
pub type FxPresets = BTreeMap<String, Vec<Effect>>;

/// Reads a JSON file; a missing or unreadable file gives the default (an empty list, for presets).
fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Writes pretty JSON to a temp file first, then renames it over `path`: a crash can never leave a
/// half-written file behind.
fn save_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

pub fn load_fx(path: &Path) -> FxPresets {
    load_json(path)
}

pub fn save_fx(path: &Path, p: &FxPresets) -> std::io::Result<()> {
    save_json_atomic(path, p)
}

pub fn load(path: &Path) -> Presets {
    load_json(path)
}

pub fn save(path: &Path, presets: &Presets) -> std::io::Result<()> {
    save_json_atomic(path, presets)
}

/// File format for sharing presets: `{"version": 2, "presets": { "<name>": {...} }}`.
pub fn export_json(presets: &Presets) -> Result<String, Error> {
    serde_json::to_string_pretty(&serde_json::json!({ "version": 2, "presets": presets })).map_err(|e| Error::Parse(format!("cannot write the presets: {e}")))
}

/// Reads presets from: our export format, a bare `{name: preset}` map, or the old Electron
/// FFcut export (`{"version":"1.0","presets":{name:{videoCodec,...}}}`).
/// Returns the presets and how many entries could not be understood.
pub fn parse_import(text: &str) -> Result<(Vec<(String, Preset)>, usize), Error> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| Error::Parse(format!("not a valid JSON file: {e}")))?;
    let map = v.get("presets").unwrap_or(&v).as_object().ok_or_else(|| Error::Invalid("this file does not contain presets".into()))?;
    let (mut ok, mut bad) = (Vec::new(), 0);
    for (name, item) in map {
        match if item.get("videoCodec").is_some() {
            from_legacy(item)
        } else {
            serde_json::from_value::<Preset>(item.clone()).ok().filter(|p| *p != Preset::default())
        } {
            Some(p) => ok.push((name.clone(), p)),
            None => bad += 1,
        }
    }
    if ok.is_empty() {
        return Err(Error::Invalid("no presets found in this file".into()));
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
        let s = ExportSettings {
            video_codec: VideoCodec::Encoder("libx265".into()),
            quality: Quality::Crf(28),
            scale: Scale { w: None, h: Some(720) },
            ..Default::default()
        };
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
        let (back, bad) = parse_import(&export_json(&all).unwrap()).unwrap();
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
    fn json_shape_is_unchanged() {
        let s = ExportSettings {
            video_codec: VideoCodec::Encoder("libx265".into()),
            quality: Quality::Crf(28),
            scale: Scale { w: None, h: Some(720) },
            ..Default::default()
        };
        let p = Preset::from_parts("mp4", &s, &Part::ALL);
        let j = serde_json::to_string(&p).unwrap();
        assert!(
            j.starts_with(r#"{"container":"mp4","video_codec":{"Encoder":"libx265"},"quality":{"Crf":28},"preset":"medium","pix_fmt":"auto","audio_codec":"Copy","audio_bitrate_kbps":192,"scale":{"w":null,"h":720},"audio_mode":"Separate","video_effects":[],"captions":[{"on":false,"#),
            "{j}"
        );
        assert_eq!(serde_json::from_str::<Preset>(&j).unwrap(), p);
        assert_eq!(serde_json::to_string(&Preset::from_parts("mkv", &s, &[Part::Quality])).unwrap(), r#"{"quality":{"Crf":28}}"#);
    }

    #[test]
    fn every_part_round_trips_through_fill_has_apply_remove() {
        let mut s = ExportSettings { pix_fmt: Some("yuv444p".into()), audio_mode: AudioMode::Merge, preset: "slow".into(), ..Default::default() };
        s.vfx.effects = vec![VEffect::new(crate::videofx::VKind::Sharpen(1.0))];
        s.vfx.top.on = true;
        for part in Part::ALL {
            let mut p = Preset::default();
            assert!(!p.has(part));
            p.fill(part, "webm", &s);
            assert!(p.has(part), "{}", part.label());
            let mut t = ExportSettings::default();
            let container = p.apply(&mut t);
            assert_eq!(container.is_some(), part == Part::Container);
            p.remove(part);
            assert_eq!(p, Preset::default());
        }
        let mut t = ExportSettings::default();
        assert_eq!(Preset::from_parts("webm", &s, &Part::ALL).apply(&mut t).as_deref(), Some("webm"));
        assert_eq!((t.pix_fmt.as_deref(), t.audio_mode, t.preset.as_str(), t.vfx.top.on), (Some("yuv444p"), AudioMode::Merge, "slow", true));
        assert_eq!(t.vfx.effects, s.vfx.effects);
        // "auto" pixel format means: let the encoder choose
        let mut t = ExportSettings { pix_fmt: Some("nv12".into()), ..Default::default() };
        Preset::from_parts("mp4", &ExportSettings::default(), &[Part::PixFmt]).apply(&mut t);
        assert_eq!(t.pix_fmt, None);
        assert_eq!(Part::ALL.len(), 11);
    }

    #[test]
    fn summary_lists_what_the_preset_holds() {
        let s = ExportSettings {
            video_codec: VideoCodec::Encoder("libx265".into()),
            quality: Quality::Crf(28),
            scale: Scale { w: None, h: Some(720) },
            ..Default::default()
        };
        assert_eq!(Preset::capture("mp4", &s).summary(), ".mp4 · libx265 (CRF 28) · audio copy · auto×720");
        assert_eq!(Preset::default().summary(), "empty: applying changes nothing");
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
