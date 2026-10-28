#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod app;
mod catalog;
mod display;
mod fx;
mod hotkeys;
mod player;
mod settings;
mod state;
mod theme;
mod ui;
mod undo;
mod vfx;

// compiled once, in the library (its integration tests use them too)
use ffcut_app::{audiomix, install, layout, update};

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--probe-plugins") {
        std::process::exit(ffcut_core::plugins::probe_main(&args[2..]));
    }
    install::cleanup_in_background();
    let settings = settings::Settings::load();
    // ffmpeg and the player (in this process) find frei0r and LV2 plugins through these; set before any thread starts
    let dirs = settings.plugin_dirs();
    for (k, v) in dirs.env() {
        std::env::set_var(k, v);
    }

    // flags: --version, --x11, --wayland ; the first other argument is a file to open
    let mut open = None;
    let mut force: Option<String> = std::env::var("FFCUT_DISPLAY").ok().filter(|v| !v.is_empty());
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--version" => {
                println!("FFcut {}", update::VERSION);
                return Ok(());
            }
            "--x11" => force = Some("x11".into()),
            "--wayland" => force = Some("wayland".into()),
            _ if !a.starts_with("--") && open.is_none() => open = Some(std::path::PathBuf::from(a)),
            _ => {}
        }
    }
    let pref = force.unwrap_or(settings.display_server);
    let backend = display::resolve(&pref);

    let icon = image::load_from_memory(include_bytes!("../../../packaging/ffcut.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FFcut")
        .with_app_id("ffcut")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([1000.0, 640.0])
        .with_drag_and_drop(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    eframe::run_native(
        "FFcut",
        eframe::NativeOptions { viewport, event_loop_builder: Some(Box::new(move |b| display::apply(b, backend))), ..Default::default() },
        Box::new(move |cc| match app::App::new(cc, open, backend) {
            Ok(app) => Ok(Box::new(app)),
            Err(e) => Err(e.into()),
        }),
    )
}
