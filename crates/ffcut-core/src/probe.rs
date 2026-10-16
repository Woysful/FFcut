//! ffprobe -> SourceInfo

use crate::model::{AudioStream, SourceInfo};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub enum ProbeError {
    Spawn(std::io::Error),
    Failed(String),
    NoVideo,
    Json(serde_json::Error),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Spawn(e) => write!(f, "cannot start ffprobe: {e}"),
            ProbeError::Failed(s) => write!(f, "ffprobe failed: {s}"),
            ProbeError::NoVideo => write!(f, "no video stream in file"),
            ProbeError::Json(e) => write!(f, "bad ffprobe output: {e}"),
        }
    }
}

pub fn probe(ffprobe: &Path, file: &Path) -> Result<SourceInfo, ProbeError> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_format", "-show_streams", "-of", "json"])
        .arg(file)
        .output()
        .map_err(ProbeError::Spawn)?;
    if !out.status.success() {
        return Err(ProbeError::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    parse_ext(&out.stdout, &ext)
}

pub fn parse(json: &[u8]) -> Result<SourceInfo, ProbeError> {
    parse_ext(json, "")
}

/// ffprobe calls the whole MP4/MOV/3GP family "QuickTime / MOV". Tell them apart by the
/// ISO-BMFF major brand (and, for Matroska vs WebM, by the file extension).
pub fn container_label(format_name: &str, long_name: &str, brand: &str, ext: &str) -> String {
    let b = brand.trim();
    if format_name.contains("mp4") || format_name.contains("mov") {
        return match b {
            "qt" => "QuickTime MOV".into(),
            "M4A" => "M4A (MPEG-4 audio)".into(),
            "M4V" => "M4V".into(),
            "" => match ext {
                "mov" => "QuickTime MOV".into(),
                "m4a" => "M4A (MPEG-4 audio)".into(),
                "3gp" => "3GP".into(),
                "3g2" => "3GP2".into(),
                _ => "MP4".into(),
            },
            b if b.starts_with("3gp") => "3GP".into(),
            b if b.starts_with("3g2") => "3GP2".into(),
            _ => "MP4".into(), // isom, mp41, mp42, iso2, avc1, dash, ...
        };
    }
    if format_name.contains("matroska") {
        return if ext == "webm" { "WebM".into() } else { "Matroska".into() };
    }
    if long_name.is_empty() { format_name.to_string() } else { long_name.to_string() }
}

fn parse_ext(json: &[u8], ext: &str) -> Result<SourceInfo, ProbeError> {
    let v: Value = serde_json::from_slice(json).map_err(ProbeError::Json)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();

    // Cover art in mp3/mkv shows up as a "video" stream: skip attached pictures.
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or(ProbeError::NoVideo)?;

    let num = |x: &Value| -> Option<f64> { x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())) };

    let duration = num(&v["format"]["duration"]).or_else(|| num(&video["duration"])).unwrap_or(0.0);

    let audio = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .enumerate()
        .map(|(i, s)| AudioStream {
            index: i,
            codec: s["codec_name"].as_str().unwrap_or("").into(),
            channels: s["channels"].as_u64().unwrap_or(0) as u32,
            language: s["tags"]["language"].as_str().map(Into::into),
            title: s["tags"]["title"].as_str().map(Into::into),
        })
        .collect();

    // Display rotation: new ffprobe -> side data "rotation" (counter-clockwise, e.g. -90 = turn 90° cw),
    // old ffprobe -> tag "rotate" (clockwise).
    let mut rot_cw: i64 = 0;
    if let Some(sd) = video["side_data_list"].as_array() {
        for d in sd {
            if let Some(r) = num(&d["rotation"]) {
                rot_cw = -(r.round() as i64);
            }
        }
    } else if let Some(r) = num(&video["tags"]["rotate"]) {
        rot_cw = r.round() as i64;
    }
    let rotation = rot_cw.rem_euclid(360) as u32;
    let (mut width, mut height) = (video["width"].as_u64().unwrap_or(0) as u32, video["height"].as_u64().unwrap_or(0) as u32);
    if rotation == 90 || rotation == 270 {
        std::mem::swap(&mut width, &mut height);
    }

    Ok(SourceInfo {
        duration,
        width,
        height,
        rotation,
        video_codec: video["codec_name"].as_str().unwrap_or("").into(),
        pix_fmt: video["pix_fmt"].as_str().unwrap_or("").into(),
        fps: frame_rate(&video["avg_frame_rate"]).or_else(|| frame_rate(&video["r_frame_rate"])).unwrap_or(0.0),
        file_size: num(&v["format"]["size"]).unwrap_or(0.0) as u64,
        format: container_label(
            v["format"]["format_name"].as_str().unwrap_or(""),
            v["format"]["format_long_name"].as_str().unwrap_or(""),
            v["format"]["tags"]["major_brand"].as_str().unwrap_or(""),
            ext,
        ),
        brand: v["format"]["tags"]["major_brand"].as_str().unwrap_or("").trim().into(),
        bit_rate: num(&v["format"]["bit_rate"]).unwrap_or(0.0) as u64,
        audio,
    })
}

fn frame_rate(v: &Value) -> Option<f64> {
    let (n, d) = v.as_str()?.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d > 0.0 && n > 0.0).then(|| n / d)
}

/// What we need to know about a file added as an extra audio track.
#[derive(Debug, Clone)]
pub struct AudioFileInfo {
    pub duration: f64,
    pub label: String,
}

pub fn probe_audio(ffprobe: &Path, file: &Path) -> Result<AudioFileInfo, ProbeError> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name,channels,duration:format=duration", "-of", "json"])
        .arg(file)
        .output()
        .map_err(ProbeError::Spawn)?;
    if !out.status.success() {
        return Err(ProbeError::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(ProbeError::Json)?;
    let st = v["streams"].as_array().and_then(|a| a.first()).ok_or_else(|| ProbeError::Failed("no audio stream in this file".into()))?;
    let num = |x: &Value| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()));
    let duration = num(&v["format"]["duration"]).or_else(|| num(&st["duration"])).unwrap_or(0.0);
    if duration <= 0.0 {
        return Err(ProbeError::Failed("cannot determine the audio length".into()));
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(AudioFileInfo { duration, label: name })
}

/// Encoders this ffmpeg build offers.
#[derive(Debug, Default, Clone)]
pub struct Encoders {
    pub video: Vec<String>,
    pub audio: Vec<String>,
}

pub fn list_encoders(ffmpeg: &Path) -> Option<Encoders> {
    let out = Command::new(ffmpeg).args(["-hide_banner", "-encoders"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_encoders(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_encoders(text: &str) -> Encoders {
    let mut e = Encoders::default();
    for line in text.lines() {
        // " V....D libx264              libx264 H.264 ..."
        let mut it = line.split_whitespace();
        let (Some(flags), Some(name)) = (it.next(), it.next()) else { continue };
        if flags.len() != 6 || name == "=" {
            continue;
        }
        match flags.as_bytes()[0] {
            b'V' => e.video.push(name.to_string()),
            b'A' => e.audio.push(name.to_string()),
            _ => {}
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_output() {
        let j = br#"{"streams":[
          {"codec_type":"video","codec_name":"hevc","width":3840,"height":2160,"pix_fmt":"yuv420p10le","disposition":{"attached_pic":0}},
          {"codec_type":"audio","codec_name":"aac","channels":2,"tags":{"language":"eng"}},
          {"codec_type":"audio","codec_name":"opus","channels":6,"tags":{"title":"Commentary"}}],
          "format":{"duration":"12.345"}}"#;
        let s = parse(j).unwrap();
        assert_eq!((s.width, s.height, s.video_codec.as_str()), (3840, 2160, "hevc"));
        assert_eq!(s.duration, 12.345);
        assert_eq!(s.audio.len(), 2);
        assert_eq!(s.audio[1].index, 1);
        assert_eq!(s.audio[1].title.as_deref(), Some("Commentary"));
    }

    #[test]
    fn parses_encoder_list() {
        let t = " Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n V....D h264_nvenc           NVIDIA\n A....D aac                  AAC\n S..... ass                  ASS\n";
        let e = parse_encoders(t);
        assert_eq!(e.video, ["libx264", "h264_nvenc"]);
        assert_eq!(e.audio, ["aac"]);
    }

    #[test]
    fn frame_rate_fraction() {
        assert!((frame_rate(&Value::from("30000/1001")).unwrap() - 29.97).abs() < 0.01);
        assert!(frame_rate(&Value::from("0/0")).is_none());
    }

    #[test]
    fn phone_video_rotation_swaps_display_size() {
        let j = br#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
          "side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}]}],"format":{"duration":"1"}}"#;
        let s = parse(j).unwrap();
        assert_eq!((s.width, s.height, s.rotation), (1080, 1920, 90));
    }

    #[test]
    fn mp4_is_not_called_quicktime() {
        let j = |brand: &str| format!(r#"{{"streams":[{{"codec_type":"video","codec_name":"h264","width":2,"height":2}}],
          "format":{{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","format_long_name":"QuickTime / MOV","tags":{{"major_brand":"{brand}"}}}}}}"#);
        assert_eq!(parse(j("isom").as_bytes()).unwrap().format, "MP4");
        assert_eq!(parse(j("mp42").as_bytes()).unwrap().format, "MP4");
        assert_eq!(parse(j("qt  ").as_bytes()).unwrap().format, "QuickTime MOV");
        assert_eq!(parse(j("3gp4").as_bytes()).unwrap().format, "3GP");
        assert_eq!(container_label("matroska,webm", "Matroska / WebM", "", "webm"), "WebM");
        assert_eq!(container_label("matroska,webm", "Matroska / WebM", "", "mkv"), "Matroska");
        assert_eq!(container_label("avi", "AVI (Audio Video Interleaved)", "", "avi"), "AVI (Audio Video Interleaved)");
    }

    #[test]
    fn audio_only_file_is_an_error() {
        let j = br#"{"streams":[{"codec_type":"audio","codec_name":"mp3"}],"format":{}}"#;
        assert!(matches!(parse(j), Err(ProbeError::NoVideo)));
    }
}
