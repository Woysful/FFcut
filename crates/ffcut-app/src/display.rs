//! Which windowing system to use on Linux.
//!
//! Why this exists: file drag & drop does not work on native Wayland (winit, the windowing library
//! under egui, has not implemented it yet; the cursor shows a "forbidden" sign). Running through
//! XWayland (X11) makes it work. Choose it in Settings, with `--x11`, or with `FFCUT_DISPLAY=x11`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    Wayland,
    X11,
    /// Windows / macOS: nothing to choose.
    #[allow(dead_code)]
    Native,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Wayland => "Wayland",
            Backend::X11 => "X11",
            Backend::Native => "native",
        }
    }
}

/// `pref`: "auto" | "wayland" | "x11" (anything else = auto). A forced backend that the session
/// cannot provide (no `DISPLAY` / no `WAYLAND_DISPLAY`) falls back to what is available.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn resolve(pref: &str) -> Backend {
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    let (wayland, x11) = (set("WAYLAND_DISPLAY"), set("DISPLAY"));
    match pref {
        "x11" if x11 => Backend::X11,
        "wayland" if wayland => Backend::Wayland,
        _ if wayland => Backend::Wayland, // what winit picks by itself
        _ => Backend::X11,
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn resolve(_pref: &str) -> Backend {
    Backend::Native
}

/// Tells winit which backend to open. Called from eframe's `event_loop_builder` hook.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn apply<T>(builder: &mut winit::event_loop::EventLoopBuilder<T>, backend: Backend) {
    use winit::platform::{wayland::EventLoopBuilderExtWayland, x11::EventLoopBuilderExtX11};
    match backend {
        Backend::X11 => {
            builder.with_x11();
        }
        Backend::Wayland => {
            builder.with_wayland();
        }
        Backend::Native => {}
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn apply<T>(_builder: &mut eframe::EventLoopBuilder<T>, _backend: Backend) {}

/// Starts a fresh copy of the app (used after changing the display server). Inside an AppImage the
/// extracted binary disappears with the old process, so the AppImage file itself is started.
pub fn restart(open: Option<&std::path::Path>) -> bool {
    let exe = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok());
    let Some(exe) = exe else { return false };
    let mut cmd = std::process::Command::new(exe);
    if let Some(p) = open {
        cmd.arg(p);
    }
    cmd.spawn().is_ok()
}
