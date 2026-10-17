//! What the window title and the file-info line in the menu bar show.
//! Both are user-configurable (Settings → Interface); this module only builds the text.

use ffcut_core::SourceInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_TITLE: &str = "{file} — {app}";

/// Variables the title template may use: (name, what it is).
pub const TITLE_VARS: &[(&str, &str)] = &[
    ("app", "FFcut"),
    ("version", "FFcut version"),
    ("file", "File name"),
    ("stem", "File name without extension"),
    ("path", "Full path"),
    ("dir", "Folder"),
    ("format", "Container (MP4, Matroska…)"),
    ("resolution", "Width×height"),
    ("duration", "Length"),
];

fn fmt_len(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// A path with the folder separators replaced by `sep` (empty = leave the path as it is).
/// "/home/me/a.mp4" with " > " -> "home > me > a.mp4".
pub fn fmt_path(p: &Path, sep: &str) -> String {
    let s = p.display().to_string();
    if sep.is_empty() {
        return s;
    }
    s.split(['/', '\\']).filter(|c| !c.is_empty()).collect::<Vec<_>>().join(sep)
}

fn title_value(name: &str, clip: Option<(&Path, &SourceInfo)>, sep: &str) -> String {
    match (name, clip) {
        ("app", _) => "FFcut".into(),
        ("version", _) => crate::update::VERSION.into(),
        ("file", Some((p, _))) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ("stem", Some((p, _))) => p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ("path", Some((p, _))) => fmt_path(p, sep),
        ("dir", Some((p, _))) => p.parent().map(|d| fmt_path(d, sep)).unwrap_or_default(),
        ("format", Some((_, i))) => i.format.clone(),
        ("resolution", Some((_, i))) => format!("{}×{}", i.width, i.height),
        ("duration", Some((_, i))) => fmt_len(i.duration),
        _ => String::new(),
    }
}

/// Fills `{variables}` in the template. Unknown names stay as typed. With nothing open the file
/// variables are empty, and the separators left dangling around them are trimmed ("{file} — {app}" -> "FFcut").
pub fn render_title(template: &str, clip: Option<(&Path, &SourceInfo)>, sep: &str) -> String {
    let t = if template.trim().is_empty() { DEFAULT_TITLE } else { template };
    let mut out = String::new();
    let mut rest = t;
    while let Some(a) = rest.find('{') {
        out.push_str(&rest[..a]);
        let after = &rest[a + 1..];
        match after.find('}') {
            Some(b) if TITLE_VARS.iter().any(|(n, _)| *n == &after[..b]) => {
                out.push_str(&title_value(&after[..b], clip, sep));
                rest = &after[b + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let s = out.trim_matches(|c: char| c.is_whitespace() || "—–-·|:/".contains(c)).to_string();
    if s.is_empty() { "FFcut".into() } else { s }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoItem {
    pub key: String,
    pub on: bool,
}

/// (key, label, on by default) in the default order.
pub const INFO_FIELDS: &[(&str, &str, bool)] = &[
    ("codec", "Video codec", true),
    ("resolution", "Resolution", true),
    ("fps", "Frame rate", true),
    ("pix_fmt", "Pixel format", true),
    ("audio", "Audio (first track, track count)", true),
    ("bitrate", "Overall bitrate", true),
    ("size", "File size", true),
    ("container", "Container", true),
    ("duration", "Length", false),
    ("file", "File name", false),
    ("path", "Full path", false),
];

pub fn info_label(key: &str) -> &'static str {
    INFO_FIELDS.iter().find(|(k, ..)| *k == key).map_or("?", |(_, l, _)| l)
}

pub fn default_info_items() -> Vec<InfoItem> {
    INFO_FIELDS.iter().map(|(k, _, on)| InfoItem { key: (*k).into(), on: *on }).collect()
}

/// Keeps the user's order, drops fields this version does not know, appends new ones (switched off).
pub fn normalize_info_items(items: &mut Vec<InfoItem>) {
    if items.is_empty() {
        *items = default_info_items();
        return;
    }
    items.retain(|i| INFO_FIELDS.iter().any(|(k, ..)| *k == i.key));
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.key.clone()));
    for (k, _, _) in INFO_FIELDS {
        if !items.iter().any(|i| i.key == *k) {
            items.push(InfoItem { key: (*k).into(), on: false });
        }
    }
}

pub fn info_line(path: &Path, i: &SourceInfo, items: &[InfoItem], sep: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for it in items.iter().filter(|i| i.on) {
        let v = match it.key.as_str() {
            "codec" => i.video_codec.to_uppercase(),
            "resolution" => format!("{}×{}", i.width, i.height),
            "fps" if i.fps > 0.0 => format!("{:.2} fps", i.fps),
            "pix_fmt" => i.pix_fmt.clone(),
            "audio" => match i.audio.first() {
                Some(a) => format!("{} {}ch{}", a.codec.to_uppercase(), a.channels, if i.audio.len() > 1 { format!(" ({} tracks)", i.audio.len()) } else { String::new() }),
                None => "no audio".into(),
            },
            "bitrate" if i.bit_rate > 0 => format!("{:.2} Mbps", i.bit_rate as f64 / 1_000_000.0),
            "size" => format!("{:.1} MB", i.file_size as f64 / 1_048_576.0),
            "container" => i.format.clone(),
            "duration" => fmt_len(i.duration),
            "file" => title_value("file", Some((path, i)), sep),
            "path" => fmt_path(path, sep),
            _ => String::new(),
        };
        if !v.is_empty() {
            parts.push(v);
        }
    }
    parts.join("  ·  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info() -> SourceInfo {
        SourceInfo { duration: 125.0, width: 1920, height: 1080, video_codec: "hevc".into(), fps: 25.0, format: "MP4".into(), file_size: 1_048_576, ..Default::default() }
    }

    #[test]
    fn title_template() {
        let p = PathBuf::from("/videos/trip.mp4");
        let i = info();
        assert_eq!(render_title("{file} — {app}", Some((&p, &i)), ""), "trip.mp4 — FFcut");
        assert_eq!(render_title("{app} {version} | {path}", Some((&p, &i)), ""), format!("FFcut {} | /videos/trip.mp4", crate::update::VERSION));
        assert_eq!(render_title("{stem} [{resolution}, {duration}]", Some((&p, &i)), ""), "trip [1920×1080, 2:05]");
        // nothing open: dangling separators go away; unknown names stay as typed; empty = default
        assert_eq!(render_title("{file} — {app}", None, ""), "FFcut");
        assert_eq!(render_title("{nope} {app}", None, ""), "{nope} FFcut");
        assert_eq!(render_title("", Some((&p, &i)), ""), "trip.mp4 — FFcut");
        assert_eq!(render_title("{file}", None, ""), "FFcut");
    }

    #[test]
    fn path_separator() {
        assert_eq!(fmt_path(Path::new("/home/me/a.mp4"), " > "), "home > me > a.mp4");
        assert_eq!(fmt_path(Path::new("C:\\Videos\\a.mp4"), " > "), "C: > Videos > a.mp4");
        assert_eq!(fmt_path(Path::new("/home/me/a.mp4"), ""), "/home/me/a.mp4");
        let i = info();
        assert_eq!(render_title("{dir}", Some((Path::new("/v/x/a.mp4"), &i)), " > "), "v > x");
    }

    #[test]
    fn info_line_follows_choice_and_order() {
        let p = PathBuf::from("/v/a.mp4");
        let items = vec![
            InfoItem { key: "resolution".into(), on: true },
            InfoItem { key: "codec".into(), on: true },
            InfoItem { key: "fps".into(), on: false },
            InfoItem { key: "file".into(), on: true },
        ];
        assert_eq!(info_line(&p, &info(), &items, ""), "1920×1080  ·  HEVC  ·  a.mp4");
    }

    #[test]
    fn normalizing_keeps_order_and_adds_missing() {
        let mut v = vec![InfoItem { key: "size".into(), on: true }, InfoItem { key: "gone".into(), on: true }, InfoItem { key: "size".into(), on: false }];
        normalize_info_items(&mut v);
        assert_eq!(v[0], InfoItem { key: "size".into(), on: true });
        assert_eq!(v.len(), INFO_FIELDS.len());
        assert!(v.iter().skip(1).all(|i| !i.on));
    }
}
