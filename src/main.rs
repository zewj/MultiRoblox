#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! MultiRoblox: hold Roblox's single-instance lock so every launch opens a new window,
//! and give the user a tidy dashboard to watch and manage those windows.

mod app;
mod monitor;
mod platform;
mod settings;
mod theme;
mod tray;
mod util;
mod widgets;

use app::{App, StartOptions, Tab};

const HELP: &str = "MultiRoblox

USAGE:
    MultiRoblox.exe [--minimized] [--tab dashboard|instances|settings|log]

OPTIONS:
    --minimized    Start hidden in the system tray (used by the autostart entry)
    --tab <name>   Open on a specific tab
    -h, --help     Show this help
";

fn load_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let minimized_flag = args
        .iter()
        .any(|a| matches!(a.as_str(), "--minimized" | "--tray" | "--hidden"));
    let tab = args
        .iter()
        .position(|a| a == "--tab")
        .and_then(|i| args.get(i + 1))
        .and_then(|t| Tab::from_arg(t))
        .unwrap_or(Tab::Dashboard);

    // Only one copy of the tool should hold the lock; a second launch just raises the first.
    if !platform::claim_tool_instance() {
        platform::signal_existing_instance();
        return Ok(());
    }

    let settings = settings::Settings::load();
    let start_hidden = platform::SUPPORTED && (minimized_flag || settings.start_minimized);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MultiRoblox")
        .with_app_id("multiroblox")
        .with_inner_size([980.0, 800.0])
        .with_min_inner_size([800.0, 560.0])
        .with_visible(!start_hidden);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "MultiRoblox",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                settings,
                StartOptions { start_hidden, tab },
            )))
        }),
    )
}
