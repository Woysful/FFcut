//! Grouped codec drop-downs, shared by the sidebar and the presets editor.

use crate::theme;
use eframe::egui;
use ffcut_core::probe::Encoders;
use ffcut_core::{AudioCodec, VideoCodec};

type Item = (&'static str, &'static str); // (ffmpeg encoder, label)

const V_CPU: &[Item] = &[
    ("libx264", "H.264 (x264)"),
    ("libx265", "H.265 / HEVC (x265)"),
    ("libsvtav1", "AV1 (SVT-AV1)"),
    ("libaom-av1", "AV1 (libaom)"),
    ("libvpx-vp9", "VP9"),
    ("libvpx", "VP8"),
    ("prores_ks", "ProRes"),
    ("dnxhd", "DNxHD"),
    ("mpeg4", "MPEG-4"),
    ("mpeg2video", "MPEG-2"),
    ("libtheora", "Theora"),
    ("ffv1", "FFV1 (lossless)"),
    ("huffyuv", "HuffYUV (lossless)"),
];
const V_NVENC: &[Item] = &[("h264_nvenc", "H.264"), ("hevc_nvenc", "H.265 / HEVC"), ("av1_nvenc", "AV1")];
const V_AMF: &[Item] = &[("h264_amf", "H.264"), ("hevc_amf", "H.265 / HEVC"), ("av1_amf", "AV1")];
const V_QSV: &[Item] = &[("h264_qsv", "H.264"), ("hevc_qsv", "H.265 / HEVC"), ("av1_qsv", "AV1"), ("vp9_qsv", "VP9"), ("mpeg2_qsv", "MPEG-2")];
const V_VAAPI: &[Item] =
    &[("h264_vaapi", "H.264"), ("hevc_vaapi", "H.265 / HEVC"), ("av1_vaapi", "AV1"), ("vp9_vaapi", "VP9"), ("vp8_vaapi", "VP8"), ("mpeg2_vaapi", "MPEG-2")];

const A_LOSSY: &[Item] =
    &[("aac", "AAC"), ("libopus", "Opus"), ("libmp3lame", "MP3"), ("libvorbis", "Vorbis"), ("ac3", "AC-3"), ("eac3", "E-AC-3"), ("mp2", "MP2")];
const A_LOSSLESS: &[Item] = &[("flac", "FLAC"), ("alac", "ALAC"), ("pcm_s16le", "PCM 16-bit"), ("pcm_s24le", "PCM 24-bit"), ("pcm_s32le", "PCM 32-bit")];

fn all_video() -> impl Iterator<Item = &'static Item> {
    [V_CPU, V_NVENC, V_AMF, V_QSV, V_VAAPI].into_iter().flat_map(|g| g.iter())
}
fn all_audio() -> impl Iterator<Item = &'static Item> {
    [A_LOSSY, A_LOSSLESS].into_iter().flat_map(|g| g.iter())
}

/// A group title in a drop-down: a full-width band with its own background, so the groups stand out
/// from the entries.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    let w = ui.available_width().max(160.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 24.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, theme::BAND);
    p.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), 2.0, theme::ACCENT);
    p.text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text.to_uppercase(),
        egui::FontId::proportional(theme::size::BODY),
        theme::TEXT_STRONG,
    );
    ui.add_space(2.0);
}

fn group<T: PartialEq + Clone>(ui: &mut egui::Ui, title: &str, items: &[Item], available: impl Fn(&str) -> bool, value: &mut T, make: impl Fn(&str) -> T) {
    let shown: Vec<&Item> = items.iter().filter(|(n, _)| available(n)).collect();
    if shown.is_empty() {
        return;
    }
    heading(ui, title);
    for (name, label) in shown {
        ui.selectable_value(value, make(name), format!("{label}  ·  {name}"));
    }
}

pub fn video_text(v: &VideoCodec) -> String {
    match v {
        VideoCodec::Copy => "Copy (lossless)".into(),
        VideoCodec::Auto => "Auto (same as source)".into(),
        VideoCodec::Encoder(e) => all_video().find(|(n, _)| n == e).map_or(e.clone(), |(n, l)| format!("{l}  ·  {n}")),
    }
}

pub fn audio_text(a: &AudioCodec) -> String {
    match a {
        AudioCodec::Copy => "Copy (lossless)".into(),
        AudioCodec::Encoder(e) => all_audio().find(|(n, _)| n == e).map_or(e.clone(), |(n, l)| format!("{l}  ·  {n}")),
    }
}

/// Encoders this ffmpeg really has. While the list is still loading everything is shown.
pub fn video_codec_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut VideoCodec,
    enc: &Option<Encoders>,
    width: f32,
) -> egui::Response {
    let has = |n: &str| enc.as_ref().is_none_or(|e| e.video.iter().any(|v| v == n));
    egui::ComboBox::from_id_salt(id)
        .selected_text(video_text(value))
        .width(width)
        .height(460.0)
        .show_ui(ui, |ui| {
            heading(ui, "Stream Copy");
            ui.selectable_value(value, VideoCodec::Copy, "Copy (lossless)");
            ui.selectable_value(value, VideoCodec::Auto, "Auto (same as source)");
            let mk = |n: &str| VideoCodec::Encoder(n.into());
            group(ui, "CPU Codecs", V_CPU, has, value, mk);
            group(ui, "Nvidia GPU - NVENC", V_NVENC, has, value, mk);
            group(ui, "AMD GPU - AMF", V_AMF, has, value, mk);
            group(ui, "Intel GPU - QSV", V_QSV, has, value, mk);
            group(ui, "VAAPI (Linux GPU)", V_VAAPI, has, value, mk);
        })
        .response
}

pub fn audio_codec_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut AudioCodec,
    enc: &Option<Encoders>,
    width: f32,
) -> egui::Response {
    let has = |n: &str| enc.as_ref().is_none_or(|e| e.audio.iter().any(|v| v == n));
    egui::ComboBox::from_id_salt(id)
        .selected_text(audio_text(value))
        .width(width)
        .height(460.0)
        .show_ui(ui, |ui| {
            heading(ui, "Stream Copy");
            ui.selectable_value(value, AudioCodec::Copy, "Copy (lossless)");
            let mk = |n: &str| AudioCodec::Encoder(n.into());
            group(ui, "Lossy", A_LOSSY, has, value, mk);
            group(ui, "Lossless", A_LOSSLESS, has, value, mk);
        })
        .response
}

pub fn is_lossless_audio(c: &str) -> bool {
    c.starts_with("pcm") || c == "flac" || c == "alac"
}
