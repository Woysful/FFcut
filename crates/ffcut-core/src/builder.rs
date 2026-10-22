//! ExportSettings + SourceInfo  ->  ffmpeg argv.
//!
//! Pure function, no I/O: that is what makes it trivially testable.
//! `argv[0]` is the program name ("ffmpeg"); the runner swaps in the real path.

use crate::model::*;
use std::path::Path;

pub fn build_args(src: &SourceInfo, s: &ExportSettings, input: &Path, output: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["ffmpeg".into(), "-hide_banner".into(), "-y".into()];

    // ---- trim (input-side seek: fast; with stream copy it snaps to a keyframe) ----
    if s.trim_start > 0.0 {
        a.push("-ss".into());
        a.push(secs(s.trim_start));
    }
    a.push("-i".into());
    a.push(input.to_string_lossy().into_owned());

    // Audio: the track list decides what is heard (removed / muted / solo); `audio_mode` how it is packed.
    let tracks = resolved_tracks(src, s);
    let heard = audible(&tracks);

    // Extra audio files used by timeline tracks become inputs 1, 2, ...
    let plan = AudioPlan::new(src, s, &heard);
    if let Some(seek) = plan.orig_seek {
        a.extend(["-ss".into(), secs(seek), "-i".into(), input.to_string_lossy().into_owned()]);
    }
    for path in &plan.external_paths {
        a.push("-i".into());
        a.push(path.clone());
    }
    let out_ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Several audible tracks into a container that holds one stream: one file per track (name_1.ext, name_2.ext…)
    let separate = (src.audio_only || s.audio_only) && s.audio_mode == AudioMode::Separate && heard.len() > 1 && !multi_stream_container(&out_ext);
    // `-t` belongs to the output that follows it, so with several outputs every one needs its own
    let mut length: Vec<String> = Vec::new();
    if let Some(end) = s.trim_end {
        let dur = end - s.trim_start;
        if dur > 0.0 && (src.duration <= 0.0 || end < src.duration - 0.001) {
            length = vec!["-t".into(), secs(dur)];
        }
    }
    if !separate {
        a.extend(length.iter().cloned());
    }

    // ---- video ----
    let no_video = src.audio_only || s.audio_only;
    if no_video {
        a.push("-vn".into());
    } else {
        push_video(&mut a, src, s);
    }

    // ---- audio ----
    let aenc = |a: &mut Vec<String>, force_encode: bool| {
        let codec = match (&s.audio_codec, force_encode) {
            (AudioCodec::Copy, false) if !no_video || copy_fits(src, &out_ext) => None,
            (AudioCodec::Copy, _) => Some(default_audio_encoder(&out_ext).to_string()),
            (AudioCodec::Encoder(e), _) => Some(e.clone()),
        };
        match codec {
            None => {
                a.push("-c:a".into());
                a.push("copy".into());
            }
            Some(c) => {
                let lossless = c.starts_with("pcm") || c == "flac" || c == "alac";
                a.push("-c:a".into());
                a.push(c);
                if !lossless {
                    a.push("-b:a".into());
                    a.push(format!("{}k", s.audio_bitrate_kbps));
                }
            }
        }
    };

    if plan.advanced {
        if plan.chains.is_empty() {
            a.push("-an".into());
        } else {
            let mix = s.audio_mode == AudioMode::Merge;
            let mut graph = plan.chains.iter().map(|c| c.0.clone()).collect::<Vec<_>>();
            let labels: Vec<&str> = plan.chains.iter().map(|c| c.1.as_str()).collect();
            if mix && labels.len() > 1 {
                let ins: String = labels.iter().map(|l| format!("[{l}]")).collect();
                graph.push(format!("{ins}amix=inputs={}:duration=longest:normalize=0[aout]", labels.len()));
                a.extend(["-filter_complex".into(), graph.join(";"), "-map".into(), "[aout]".into()]);
            } else if separate {
                a.extend(["-filter_complex".into(), graph.join(";")]);
                for (n, l) in labels.iter().enumerate() {
                    a.extend(["-map".into(), format!("[{l}]")]);
                    aenc(&mut a, true);
                    a.extend(length.iter().cloned());
                    a.push(numbered(output, n + 1).to_string_lossy().into_owned());
                }
                return a;
            } else {
                a.extend(["-filter_complex".into(), graph.join(";")]);
                for l in &labels {
                    a.extend(["-map".into(), format!("[{l}]")]);
                }
            }
            aenc(&mut a, true);
        }
        a.push(output.to_string_lossy().into_owned());
        return a;
    }

    // Plain path: only untouched original streams, so no filters are needed (except to mix).
    if s.audio_mode == AudioMode::None {
        a.push("-an".into());
    } else if heard.is_empty() {
        if !tracks.is_empty() {
            a.push("-an".into()); // there was audio, but nothing is left to hear
        }
    } else {
        let idx: Vec<usize> = heard.iter().filter_map(|t| if let TrackSource::Original(i) = t.source { Some(i) } else { None }).collect();
        match s.audio_mode {
            AudioMode::Merge if idx.len() > 1 => {
                let ins: String = idx.iter().map(|i| format!("[0:a:{i}]")).collect();
                a.extend([
                    "-filter_complex".into(),
                    format!("{ins}amix=inputs={}:duration=longest:normalize=0[aout]", idx.len()),
                    "-map".into(),
                    "[aout]".into(),
                ]);
                aenc(&mut a, true); // mixing always needs an encoder
            }
            _ if separate => {
                for (n, i) in idx.iter().enumerate() {
                    a.extend(["-map".into(), format!("0:a:{i}")]);
                    aenc(&mut a, false);
                    a.extend(length.iter().cloned());
                    a.push(numbered(output, n + 1).to_string_lossy().into_owned());
                }
                return a;
            }
            _ => {
                if idx.len() == src.audio.len() && idx.iter().enumerate().all(|(n, i)| n == *i) {
                    a.extend(["-map".into(), "0:a".into()]);
                } else {
                    for i in &idx {
                        a.extend(["-map".into(), format!("0:a:{i}")]);
                    }
                }
                aenc(&mut a, false);
            }
        }
    }

    a.push(output.to_string_lossy().into_owned());
    a
}

/// The track list, or (when the UI never filled it) every audio stream of the file, untouched.
fn resolved_tracks(src: &SourceInfo, s: &ExportSettings) -> Vec<TrackEdit> {
    if !s.tracks.is_empty() {
        return s.tracks.clone();
    }
    src.audio.iter().map(|a| TrackEdit::original(a.index, format!("#{}", a.index + 1), src.duration)).collect()
}

/// Timeline audio edits turned into ffmpeg filter chains.
///
/// Inputs: 0 = the file for video (seeked to the trim start); 1 = the same file again for the
/// *audio* of original tracks, seeked independently (a track shifted later needs sound from before
/// the trim start); then one input per external audio file.
struct AudioPlan {
    advanced: bool,
    /// Seek (seconds) of the second input of the main file, if any original track is used.
    orig_seek: Option<f64>,
    external_paths: Vec<String>,
    /// (filter chain text, output label)
    chains: Vec<(String, String)>,
}

impl AudioPlan {
    fn new(src: &SourceInfo, s: &ExportSettings, heard: &[&TrackEdit]) -> Self {
        let none = AudioPlan { advanced: false, orig_seek: None, external_paths: vec![], chains: vec![] };
        // removing / muting a track alone is not an "edit" that needs filters: it only changes which
        // streams are mapped. Filters are for trim, shift, volume and added files.
        let edited = heard.iter().any(|t| t.needs_filter(src.duration));
        if !edited || s.audio_mode == AudioMode::None {
            return none;
        }
        // Output covers timeline [t0, t1].
        let t0 = s.trim_start.max(0.0);
        let t1 = s.trim_end.unwrap_or(if src.duration > 0.0 { src.duration } else { f64::MAX });

        struct Item<'a> {
            t: &'a TrackEdit,
            vis_a: f64,
            vis_b: f64,
            ext_index: Option<usize>, // position among external files
        }
        let mut items: Vec<Item> = Vec::new();
        let mut external_paths: Vec<String> = Vec::new();
        for t in heard.iter().copied() {
            let mut ext_index = None;
            if let TrackSource::External { path, .. } = &t.source {
                ext_index = Some(external_paths.len());
                external_paths.push(path.clone()); // every kept file stays an input: indices stay stable
            }
            let (a, b) = t.placed();
            let (vis_a, vis_b) = (a.max(t0), b.min(t1));
            if vis_b - vis_a < 0.01 {
                continue; // nothing of this track falls inside the exported range
            }
            items.push(Item { t, vis_a, vis_b, ext_index });
        }

        // earliest source time any original track needs
        let orig_seek = items
            .iter()
            .filter(|i| i.ext_index.is_none())
            .map(|i| (i.vis_a - i.t.shift).max(0.0))
            .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
        let ext_base = if orig_seek.is_some() { 2 } else { 1 };

        let mut chains = Vec::new();
        for it in &items {
            let (input, ordinal, seek_off) = match (&it.t.source, it.ext_index) {
                (TrackSource::Original(i), _) => (1usize, *i, orig_seek.unwrap_or(0.0)),
                (_, Some(e)) => (ext_base + e, 0usize, 0.0),
                _ => continue,
            };
            let ls = it.vis_a - it.t.shift - seek_off;
            let le = it.vis_b - it.t.shift - seek_off;
            let delay_ms = ((it.vis_a - t0) * 1000.0).round() as i64;
            let label = format!("a{}", chains.len());
            let body = crate::audiofx::track_chain(it.t, ls.max(0.0), le, (it.vis_a - it.t.shift).max(0.0), delay_ms);
            let c = format!("[{input}:a:{ordinal}]{body}[{label}]");
            chains.push((c, label));
        }
        AudioPlan { advanced: true, orig_seek, external_paths, chains }
    }
}

fn secs(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".into() } else { s.to_string() }
}

fn video_filters(src: &SourceInfo, s: &ExportSettings) -> Vec<String> {
    use crate::videofx as vfx;
    let mut f = Vec::new();
    // colour / restoration work on the decoded picture, before it is turned
    f.extend(vfx::pixel_filters(&s.vfx));
    match s.rotation {
        Rotation::None => {}
        Rotation::Cw90 => f.push("transpose=1".into()),
        Rotation::R180 => f.push("hflip,vflip".into()),
        Rotation::Ccw90 => f.push("transpose=2".into()),
    }
    // Crop comes after rotate+flip: its coordinates are exactly what the preview shows.
    if s.flip_h {
        f.push("hflip".into());
    }
    if s.flip_v {
        f.push("vflip".into());
    }
    let (fw, fh) = match s.rotation {
        Rotation::Cw90 | Rotation::Ccw90 => (src.height, src.width),
        _ => (src.width, src.height),
    };
    // areas and text sit in that same frame, so the crop does not move them
    f.extend(vfx::overlay_filters(&s.vfx, fw, fh, s.trim_start));
    let (mut cw, mut ch) = (fw, fh);
    if let Some(c) = s.crop {
        let full = c.x == 0 && c.y == 0 && c.w == fw && c.h == fh;
        if !full && c.w > 0 && c.h > 0 {
            f.push(format!("crop={}:{}:{}:{}", c.w, c.h, c.x, c.y));
            cw = c.w;
            ch = c.h;
        }
    }
    f.extend(vfx::caption_filters(&s.vfx, cw, ch));
    if s.scale.w.is_some() || s.scale.h.is_some() {
        let w = s.scale.w.map_or(-2, |v| v as i64);
        let h = s.scale.h.map_or(-2, |v| v as i64);
        f.push(format!("scale={w}:{h}"));
        f.push("setsar=1".into());
    }
    f
}

pub fn auto_encoder(source_codec: &str) -> &'static str {
    match source_codec {
        "h264" => "libx264",
        "hevc" => "libx265",
        "vp8" => "libvpx",
        "vp9" => "libvpx-vp9",
        "av1" => "libsvtav1",
        "prores" => "prores_ks",
        "mpeg4" => "mpeg4",
        "dnxhd" => "dnxhd",
        _ => "libx264",
    }
}

fn push_video(a: &mut Vec<String>, src: &SourceInfo, s: &ExportSettings) {
    let filters = video_filters(src, s);
    a.push("-map".into());
    a.push("0:v:0".into());
    // Stream copy cannot apply filters: fall back to a same-family re-encode.
    let vcodec = match &s.video_codec {
        VideoCodec::Copy if filters.is_empty() => None,
        VideoCodec::Copy | VideoCodec::Auto => Some(auto_encoder(&src.video_codec).to_string()),
        VideoCodec::Encoder(e) => Some(e.clone()),
    };

    match vcodec {
        None => {
            a.push("-c:v".into());
            a.push("copy".into());
        }
        Some(enc) => {
            if !filters.is_empty() {
                a.push("-vf".into());
                a.push(filters.join(","));
            }
            a.push("-c:v".into());
            a.push(enc.clone());
            push_quality(a, &enc, s.quality);
            if matches!(enc.as_str(), "libx264" | "libx265") && !s.preset.is_empty() {
                a.push("-preset".into());
                a.push(s.preset.clone());
            }
            if let Some(pf) = pick_pix_fmt(&enc, s.pix_fmt.as_deref(), &src.pix_fmt) {
                a.push("-pix_fmt".into());
                a.push(pf);
            }
        }
    }
}

/// Containers that carry several audio streams (the rest get one file per track).
pub fn multi_stream_container(ext: &str) -> bool {
    matches!(ext, "mp4" | "mkv" | "mka" | "mov" | "webm")
}

/// `dir/name.ext` → `dir/name_N.ext`: the files of a per-track audio export.
pub fn numbered(output: &Path, n: usize) -> std::path::PathBuf {
    let stem = output.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = match output.extension() {
        Some(e) => format!("{stem}_{n}.{}", e.to_string_lossy()),
        None => format!("{stem}_{n}"),
    };
    output.with_file_name(name)
}

/// The encoder an audio container normally carries (used when the stream cannot just be copied).
pub fn audio_encoder_for(out_ext: &str) -> &'static str {
    match out_ext {
        "webm" | "opus" => "libopus",
        "ogg" | "oga" => "libvorbis",
        "mp3" => "libmp3lame",
        "flac" => "flac",
        "wav" => "pcm_s16le",
        _ => "aac",
    }
}

fn default_audio_encoder(out_ext: &str) -> &'static str {
    audio_encoder_for(out_ext)
}

/// Can the first audio stream of an audio file be copied into this container as it is?
fn copy_fits(src: &SourceInfo, out_ext: &str) -> bool {
    let Some(c) = src.audio.first().map(|a| a.codec.as_str()) else { return true };
    match out_ext {
        "mp3" => c == "mp3",
        "flac" => c == "flac",
        "m4a" | "aac" => matches!(c, "aac" | "alac"),
        "ogg" | "oga" => matches!(c, "vorbis" | "opus" | "flac"),
        "opus" => c == "opus",
        "wav" => c.starts_with("pcm"),
        "mka" | "mkv" | "mp4" | "mov" => true,
        "webm" => matches!(c, "opus" | "vorbis"),
        _ => false,
    }
}

fn push_quality(a: &mut Vec<String>, enc: &str, q: Quality) {
    match q {
        Quality::Bitrate(k) => {
            a.extend(["-b:v".into(), format!("{k}k")]);
        }
        Quality::Crf(v) => {
            let v = v.to_string();
            if enc.contains("nvenc") {
                a.extend(["-rc".into(), "vbr".into(), "-cq".into(), v]);
            } else if enc.contains("qsv") {
                a.extend(["-global_quality".into(), v]);
            } else if enc.contains("vaapi") || enc.contains("amf") {
                a.extend(["-qp".into(), v]);
            } else if enc == "libvpx" || enc == "libvpx-vp9" {
                a.extend(["-crf".into(), v, "-b:v".into(), "0".into()]);
            } else if enc.starts_with("prores") || enc == "dnxhd" || enc == "mpeg4" {
                // no CRF concept here: let the encoder defaults decide
            } else {
                a.extend(["-crf".into(), v]);
            }
        }
    }
}

/// Pixel formats an encoder is known to accept. `None` = unknown, we don't guess.
fn supported_pix_fmts(enc: &str) -> Option<&'static [&'static str]> {
    Some(match enc {
        "libx264" => &["yuv420p", "yuvj420p", "yuv422p", "yuv444p", "nv12", "yuv420p10le", "yuv422p10le", "yuv444p10le"],
        "libx265" => &["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "yuv444p10le", "yuv420p12le"],
        "h264_nvenc" => &["yuv420p", "yuv444p", "nv12"],
        "hevc_nvenc" => &["yuv420p", "nv12", "p010le", "yuv444p"],
        "h264_qsv" | "h264_vaapi" | "h264_amf" => &["yuv420p", "nv12"],
        "hevc_qsv" | "hevc_vaapi" | "hevc_amf" => &["yuv420p", "yuv420p10le", "nv12"],
        "libvpx" => &["yuv420p"],
        "libvpx-vp9" => &["yuv420p", "yuv422p", "yuv444p", "yuv420p10le", "yuv422p10le", "yuv444p10le"],
        "libsvtav1" => &["yuv420p", "yuv420p10le"],
        "libaom-av1" => &["yuv420p", "yuv422p", "yuv444p", "yuv420p10le"],
        "prores_ks" | "prores" => &["yuv422p10le", "yuv444p10le"],
        _ => return None,
    })
}

fn pick_pix_fmt(enc: &str, user: Option<&str>, source: &str) -> Option<String> {
    if let Some(u) = user {
        return Some(u.to_string());
    }
    // Hardware encoders usually need frames uploaded in their own way; don't force.
    if enc.contains("vaapi") {
        return None;
    }
    let ok = supported_pix_fmts(enc)?;
    if ok.contains(&source) {
        Some(source.to_string())
    } else {
        Some(ok[0].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn src() -> SourceInfo {
        SourceInfo {
            duration: 60.0,
            fps: 25.0,
            width: 1920,
            height: 1080,
            video_codec: "hevc".into(),
            pix_fmt: "yuv420p10le".into(),
            rotation: 0,
            audio: (0..2).map(|i| AudioStream { index: i, codec: "aac".into(), channels: 2, language: None, title: None }).collect(),
            ..Default::default()
        }
    }
    fn build(s: &ExportSettings) -> String {
        crate::cmdline::to_text(&build_args(&src(), s, &PathBuf::from("in.mkv"), &PathBuf::from("out.mp4")))
    }

    #[test]
    fn audio_only_export_of_a_video() {
        let mut src = src();
        src.audio = (0..2).map(|i| AudioStream { index: i, codec: "aac".into(), channels: 2, language: None, title: None }).collect();
        let s = ExportSettings { audio_only: true, ..Default::default() };
        let go = |out: &str| crate::cmdline::to_text(&build_args(&src, &s, &PathBuf::from("in.mkv"), &PathBuf::from(out)));
        let mkv = go("o.mkv");
        assert!(mkv.contains("-vn") && !mkv.contains("0:v") && mkv.ends_with("o.mkv"), "{mkv}");
        assert_eq!(mkv.matches("o.").count(), 1);
        let mp3 = go("dir/o.mp3");
        assert!(mp3.contains("dir/o_1.mp3") && mp3.contains("dir/o_2.mp3"), "{mp3}");
        assert_eq!(mp3.matches("-map 0:a:").count(), 2);
        // a trim length must reach every output, not just the first
        let s2 = ExportSettings { audio_only: true, trim_start: 5.0, trim_end: Some(15.0), ..Default::default() };
        let c = crate::cmdline::to_text(&build_args(&src, &s2, &PathBuf::from("in.mkv"), &PathBuf::from("o.mp3")));
        assert_eq!(c.matches("-t 10").count(), 2, "{c}");
    }

    #[test]
    fn audio_file_export_has_no_video() {
        let src = SourceInfo { duration: 30.0, audio_only: true, audio: vec![AudioStream { index: 0, codec: "mp3".into(), channels: 2, language: None, title: None }], ..Default::default() };
        let s = ExportSettings { trim_start: 2.0, trim_end: Some(10.0), ..Default::default() };
        let go = |out: &str| crate::cmdline::to_text(&build_args(&src, &s, &PathBuf::from("in.mp3"), &PathBuf::from(out)));
        let c = go("out.mp3");
        assert!(c.contains("-vn") && !c.contains("0:v") && !c.contains("-c:v"), "{c}");
        assert!(c.contains("-c:a copy"), "{c}");
        // a codec the container cannot carry is re-encoded instead of failing
        assert!(go("out.flac").contains("-c:a flac"));
        assert!(go("out.ogg").contains("-c:a libvorbis"));
    }

    #[test]
    fn lossless_trim() {
        let s = ExportSettings { trim_start: 5.0, trim_end: Some(15.5), ..Default::default() };
        assert_eq!(build(&s), "ffmpeg -hide_banner -y -ss 5 -i in.mkv -t 10.5 -map 0:v:0 -c:v copy -map 0:a -c:a copy out.mp4");
    }

    #[test]
    fn trim_to_end_has_no_t() {
        let s = ExportSettings { trim_start: 5.0, trim_end: Some(60.0), ..Default::default() };
        assert!(!build(&s).contains("-t "));
    }

    #[test]
    fn copy_with_filters_falls_back_to_same_family() {
        let s = ExportSettings { flip_h: true, ..Default::default() };
        let c = build(&s);
        assert!(c.contains("-vf hflip"));
        assert!(c.contains("-c:v libx265"));
        assert!(c.contains("-pix_fmt yuv420p10le")); // source 10-bit preserved
    }

    #[test]
    fn rotate_then_crop_uses_rotated_frame() {
        let s = ExportSettings {
            rotation: Rotation::Cw90,
            crop: Some(Crop { x: 0, y: 0, w: 1080, h: 1920 }), // full rotated frame -> no crop
            ..Default::default()
        };
        assert!(!build(&s).contains("crop="));
        let s = ExportSettings { crop: Some(Crop { x: 10, y: 20, w: 640, h: 480 }), rotation: Rotation::Cw90, ..Default::default() };
        assert!(build(&s).contains("-vf transpose=1,crop=640:480:10:20"));
    }

    #[test]
    fn scale_adds_setsar_and_keeps_aspect() {
        let s = ExportSettings { scale: Scale { w: None, h: Some(720) }, video_codec: VideoCodec::Encoder("libx264".into()), ..Default::default() };
        assert!(build(&s).contains("-vf scale=-2:720,setsar=1 -c:v libx264 -crf 23 -preset medium"));
    }

    #[test]
    fn flip_is_applied_before_crop() {
        let s = ExportSettings { flip_h: true, crop: Some(Crop { x: 0, y: 0, w: 100, h: 100 }), ..Default::default() };
        assert!(build(&s).contains("-vf hflip,crop=100:100:0:0"));
    }

    #[test]
    fn nvenc_quality_flags() {
        let s = ExportSettings { video_codec: VideoCodec::Encoder("hevc_nvenc".into()), quality: Quality::Crf(28), ..Default::default() };
        let c = build(&s);
        assert!(c.contains("-rc vbr -cq 28"));
        assert!(!c.contains("-preset"));
    }

    #[test]
    fn merge_tracks_uses_amix_and_encodes() {
        let s = ExportSettings { audio_mode: AudioMode::Merge, ..Default::default() };
        let c = build(&s);
        assert!(c.contains("-filter_complex [0:a:0][0:a:1]amix=inputs=2:duration=longest:normalize=0[aout] -map [aout] -c:a aac -b:a 192k"));
    }

    #[test]
    fn audio_mode_none_and_no_audible_tracks_give_no_audio() {
        let s = ExportSettings { audio_mode: AudioMode::None, ..Default::default() };
        assert!(build(&s).contains("-an"));
    }

    fn tracks(n: usize) -> Vec<TrackEdit> {
        (0..n).map(|i| TrackEdit::original(i, format!("#{i}"), 60.0)).collect()
    }

    #[test]
    fn untouched_tracks_keep_the_simple_path() {
        let s = ExportSettings { tracks: tracks(2), ..Default::default() };
        assert!(build(&s).contains("-map 0:a -c:a copy"));
    }

    #[test]
    fn deleting_a_track_only_changes_the_mapping_and_keeps_copy() {
        let mut t = tracks(2);
        t[0].deleted = true;
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-map 0:a:1 -c:a copy"), "{c}");
        assert!(!c.contains("-filter_complex") && !c.contains("-ss 0"), "{c}");
        let mut t = tracks(2);
        t[0].deleted = true;
        t[1].deleted = true;
        assert!(build(&ExportSettings { tracks: t, ..Default::default() }).contains("-an"));
    }

    #[test]
    fn deleted_track_plus_another_edit_still_uses_filters() {
        let mut t = tracks(2);
        t[0].deleted = true;
        t[1].volume = 0.5;
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-filter_complex [1:a:1]atrim=start=0:end=60,asetpts=PTS-STARTPTS,volume=0.500[a0] -map [a0] -c:a aac -b:a 192k"), "{c}");
    }

    #[test]
    fn shift_volume_and_trim_make_a_chain() {
        let mut t = tracks(1);
        t[0].trim_start = 2.0;
        t[0].trim_end = 12.0;
        t[0].shift = 3.0;     // clip placed at 5..15 on the timeline
        t[0].volume = 0.5;
        // export 0..60 -> delay 5 s; the audio input is seeked to 2 s, so local 0..10 = source 2..12
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-ss 2 -i in.mkv"), "{c}");
        assert!(c.contains("atrim=start=0:end=10,asetpts=PTS-STARTPTS,adelay=5000:all=1,volume=0.500[a0]"), "{c}");
    }

    #[test]
    fn audio_shifted_later_reads_before_the_trim_start() {
        let mut t = tracks(1);
        t[0].shift = 4.0; // whole track 4 s later; export starts at 10 s -> needs source from 6 s
        let s = ExportSettings { tracks: t, trim_start: 10.0, trim_end: Some(20.0), ..Default::default() };
        let c = build(&s);
        assert!(c.contains("-ss 10 -i in.mkv -ss 6 -i in.mkv"), "{c}");
        assert!(c.contains("[1:a:0]atrim=start=0:end=10"), "{c}"); // 6..16 of the source
    }

    #[test]
    fn external_track_is_a_new_input_and_mix_works() {
        let mut t = tracks(1);
        t.push(TrackEdit::external("/m/music.wav".into(), "music".into(), 30.0));
        let s = ExportSettings { tracks: t, audio_mode: AudioMode::Merge, ..Default::default() };
        let c = build(&s);
        assert!(c.contains("-i /m/music.wav"), "{c}");
        assert!(c.contains("[2:a:0]atrim=start=0:end=30"), "{c}");
        assert!(c.contains("[a0][a1]amix=inputs=2"), "{c}");
    }

    #[test]
    fn track_outside_the_exported_range_is_skipped() {
        let mut t = tracks(2);
        t[1].trim_start = 50.0;
        t[1].trim_end = 60.0;
        let s = ExportSettings { tracks: t, trim_end: Some(20.0), ..Default::default() };
        let c = build(&s);
        assert!(!c.contains("a1"), "{c}");
    }

    #[test]
    fn muted_track_is_left_out_like_a_removed_one() {
        let mut t = tracks(2);
        t[0].muted = true;
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-map 0:a:1 -c:a copy"), "{c}");
        assert!(!c.contains("-filter_complex"), "{c}");
    }

    #[test]
    fn solo_wins_over_everything_else() {
        let mut t = tracks(3);
        t[2].solo = true;
        t[1].muted = true;
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-map 0:a:2 -c:a copy") && !c.contains("0:a:0") && !c.contains("0:a:1"), "{c}");
        // a removed track cannot be heard even if it is soloed
        let mut t = tracks(2);
        t[0].solo = true;
        t[0].deleted = true;
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-map 0:a:1 -c:a copy"), "{c}"); // solo of a removed track does not silence the rest
    }

    #[test]
    fn everything_muted_means_no_audio() {
        let mut t = tracks(2);
        t[0].muted = true;
        t[1].muted = true;
        assert!(build(&ExportSettings { tracks: t, ..Default::default() }).contains("-an"));
    }

    /// The scenario that used to conflict: drop all originals but one, add a music file, mix everything.
    #[test]
    fn mix_after_removing_originals_and_adding_a_file() {
        let mut t = tracks(2);
        t[0].deleted = true;
        t.push(TrackEdit::external("/m/music.wav".into(), "music".into(), 30.0));
        let c = build(&ExportSettings { tracks: t, audio_mode: AudioMode::Merge, ..Default::default() });
        assert!(c.contains("-i /m/music.wav"), "{c}");
        assert!(c.contains("[1:a:1]atrim") && c.contains("[2:a:0]atrim"), "{c}");
        assert!(c.contains("[a0][a1]amix=inputs=2"), "{c}");
        assert!(!c.contains("[1:a:0]"), "{c}");
    }

    #[test]
    fn separate_mode_keeps_added_files_as_own_streams() {
        let mut t = tracks(1);
        t.push(TrackEdit::external("/m/music.wav".into(), "music".into(), 30.0));
        let c = build(&ExportSettings { tracks: t, ..Default::default() });
        assert!(c.contains("-map [a0] -map [a1]") && !c.contains("amix"), "{c}");
    }
}
