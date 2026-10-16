//! Export presets: a named bundle of encoding choices (not trim/crop/rotate — those belong to a clip).

use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub container: String,
    pub video_codec: VideoCodec,
    pub quality: Quality,
    pub preset: String,
    pub pix_fmt: Option<String>,
    pub audio_codec: AudioCodec,
    pub audio_bitrate_kbps: u32,
    pub scale: Scale,
}

impl Preset {
    pub fn capture(container: &str, s: &ExportSettings) -> Self {
        Preset {
            container: container.into(),
            video_codec: s.video_codec.clone(),
            quality: s.quality,
            preset: s.preset.clone(),
            pix_fmt: s.pix_fmt.clone(),
            audio_codec: s.audio_codec.clone(),
            audio_bitrate_kbps: s.audio_bitrate_kbps,
            scale: s.scale,
        }
    }

    /// Writes the preset's choices into `s`; returns the container to use.
    pub fn apply(&self, s: &mut ExportSettings) -> String {
        s.video_codec = self.video_codec.clone();
        s.quality = self.quality;
        s.preset = self.preset.clone();
        s.pix_fmt = self.pix_fmt.clone();
        s.audio_codec = self.audio_codec.clone();
        s.audio_bitrate_kbps = self.audio_bitrate_kbps;
        s.scale = self.scale;
        self.container.clone()
    }
}

pub type Presets = BTreeMap<String, Preset>;

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
        match serde_json::from_value::<Preset>(item.clone()).ok().or_else(|| from_legacy(item)) {
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
        container: st("container").unwrap_or("mp4").to_string(),
        video_codec: match video {
            "copy" => VideoCodec::Copy,
            "auto" => VideoCodec::Auto,
            e => VideoCodec::Encoder(e.to_string()),
        },
        quality: if st("qualityMode") == Some("bitrate") {
            Quality::Bitrate(num("videoBitrate").unwrap_or(8000.0) as u32)
        } else {
            Quality::Crf(num("crf").unwrap_or(23.0) as u32)
        },
        preset: st("preset").unwrap_or("medium").to_string(),
        pix_fmt: st("pixelFormat").filter(|p| *p != "auto").map(String::from),
        audio_codec: match st("audioCodec").unwrap_or("copy") {
            "copy" => AudioCodec::Copy,
            e => AudioCodec::Encoder(e.to_string()),
        },
        audio_bitrate_kbps: num("audioBitrate").unwrap_or(192.0) as u32,
        scale: Scale::default(),
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
        assert_eq!(back["Small 720p"].apply(&mut t), "mp4");
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
        assert_eq!((l[0].1.quality, l[0].1.audio_bitrate_kbps), (Quality::Crf(20), 128));
        assert!(parse_import("[1,2]").is_err());
    }
}
