//! Locating ffmpeg / ffprobe: explicit override > next to our executable > PATH.

use std::path::{Path, PathBuf};

pub fn find_tool(name: &str, override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let c = dir.join(&file);
        if c.is_file() {
            return Some(c);
        }
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(&file)).find(|c| c.is_file()))
}
