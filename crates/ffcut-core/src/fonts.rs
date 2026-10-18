//! System fonts for text blocks and captions: scan the usual folders and read each font's own name.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// "Arial", "DejaVu Sans Bold", ...
    pub name: String,
    pub path: PathBuf,
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        if let Some(w) = std::env::var_os("WINDIR") {
            v.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push("/System/Library/Fonts".into());
        v.push("/Library/Fonts".into());
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        v.push("/usr/share/fonts".into());
        v.push("/usr/local/share/fonts".into());
        if let Some(h) = std::env::var_os("HOME") {
            let h = PathBuf::from(h);
            v.push(h.join(".fonts"));
            v.push(h.join(".local/share/fonts"));
        }
    }
    v
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 5 {
                walk(&p, depth + 1, out);
            }
        } else if matches!(p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(), Some("ttf" | "otf")) {
            out.push(p);
        }
    }
}

/// Full name of the first face in the file ("Arial Bold"), from the font's name table.
fn face_name(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    let mut full: Option<String> = None;
    let mut family: Option<String> = None;
    let mut style: Option<String> = None;
    for n in face.names() {
        if !n.is_unicode() {
            continue;
        }
        // English (US) names; the table also holds translations
        if n.language() != ttf_parser::Language::English_UnitedStates && n.language_id != 0 {
            continue;
        }
        let Some(s) = n.to_string() else { continue };
        match n.name_id {
            ttf_parser::name_id::FULL_NAME => full = full.or(Some(s)),
            ttf_parser::name_id::TYPOGRAPHIC_FAMILY | ttf_parser::name_id::FAMILY => family = family.or(Some(s)),
            ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY | ttf_parser::name_id::SUBFAMILY => style = style.or(Some(s)),
            _ => {}
        }
    }
    full.or_else(|| match (family, style) {
        (Some(f), Some(s)) if s != "Regular" => Some(format!("{f} {s}")),
        (Some(f), _) => Some(f),
        _ => None,
    })
}

/// Every .ttf / .otf in the system font folders, sorted by name. Reads the files: run it off the UI thread.
pub fn scan() -> Vec<Font> {
    let mut files = Vec::new();
    for d in font_dirs() {
        walk(&d, 0, &mut files);
    }
    let mut out: Vec<Font> = files
        .into_iter()
        .filter_map(|p| {
            let data = std::fs::read(&p).ok()?;
            let name = face_name(&data).or_else(|| p.file_stem().map(|s| s.to_string_lossy().into_owned()))?;
            Some(Font { name, path: p })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.path.cmp(&b.path)));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// A good default: a bold, meme-friendly face if the system has one, else something plain.
pub fn pick_default(fonts: &[Font]) -> Option<&Font> {
    for want in ["Impact", "Arial Bold", "Liberation Sans Bold", "DejaVu Sans Bold", "Arial", "DejaVu Sans"] {
        if let Some(f) = fonts.iter().find(|f| f.name.eq_ignore_ascii_case(want)) {
            return Some(f);
        }
    }
    fonts.first()
}
