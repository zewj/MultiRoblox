//! System tray icon with a small context menu (Windows only).

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TrayCommand {
    Show,
    Launch,
    KillAll,
    Quit,
}

pub type TrayQueue = Arc<Mutex<Vec<TrayCommand>>>;

pub struct Tray {
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
}

#[cfg(windows)]
pub fn install(
    ctx: egui::Context,
    queue: TrayQueue,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<Tray, String> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id("show", "Open MultiRoblox", true, None);
    let launch = MenuItem::with_id("launch", "Launch Roblox", true, None);
    let kill = MenuItem::with_id("killall", "Close all Roblox instances", true, None);
    let quit = MenuItem::with_id("quit", "Quit MultiRoblox", true, None);
    let menu = Menu::with_items(&[
        &show,
        &PredefinedMenuItem::separator(),
        &launch,
        &kill,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .map_err(|e| e.to_string())?;

    let icon = Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MultiRoblox")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;

    let (q, c) = (queue.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let cmd = match e.id.0.as_str() {
            "show" => TrayCommand::Show,
            "launch" => TrayCommand::Launch,
            "killall" => TrayCommand::KillAll,
            "quit" => TrayCommand::Quit,
            _ => return,
        };
        q.lock().unwrap().push(cmd);
        c.request_repaint();
    }));
    let (q, c) = (queue, ctx);
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        let show = matches!(
            e,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        );
        if show {
            q.lock().unwrap().push(TrayCommand::Show);
            c.request_repaint();
        }
    }));

    Ok(Tray { _icon: tray })
}

#[cfg(not(windows))]
pub fn install(
    _ctx: egui::Context,
    _queue: TrayQueue,
    _rgba: Vec<u8>,
    _w: u32,
    _h: u32,
) -> Result<Tray, String> {
    Err("tray icon is only available on Windows".into())
}
