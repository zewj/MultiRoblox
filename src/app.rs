//! The egui application: header, tabs, dashboard, instance table, settings and log.

use crate::monitor::{Instance, Level, LockState, Monitor, Snapshot};
use crate::platform;
use crate::settings::Settings;
use crate::theme;
use crate::tray::{self, Tray, TrayCommand, TrayQueue};
use crate::util::{fmt_bytes, fmt_duration, parse_launch_target};
use crate::widgets::{self, ButtonKind};
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Id, Key, Label, Layout, Margin, Order, RichText,
    ScrollArea, Sense, Stroke, UiBuilder, ViewportCommand, WindowLevel,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Instances,
    Settings,
    Log,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Dashboard, Tab::Instances, Tab::Settings, Tab::Log];

    pub fn from_arg(s: &str) -> Option<Tab> {
        match s.to_ascii_lowercase().as_str() {
            "dashboard" | "home" => Some(Tab::Dashboard),
            "instances" => Some(Tab::Instances),
            "settings" => Some(Tab::Settings),
            "log" => Some(Tab::Log),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Instances => "Instances",
            Tab::Settings => "Settings",
            Tab::Log => "Log",
        }
    }
}

pub struct StartOptions {
    pub start_hidden: bool,
    pub tab: Tab,
}

struct Toast {
    until: Instant,
    text: String,
    color: Color32,
}

pub struct App {
    settings: Settings,
    settings_dirty: bool,
    monitor: Monitor,
    tray: Option<Tray>,
    tray_queue: TrayQueue,
    tab: Tab,
    join_input: String,
    launch_count: u32,
    really_quit: bool,
    hidden: bool,
    hide_hint_shown: bool,
    logo: Option<egui::TextureHandle>,
    roblox_exe: Option<PathBuf>,
    toast: Option<Toast>,
    confirm_kill_until: Option<Instant>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings, opts: StartOptions) -> Self {
        theme::apply(&cc.egui_ctx);

        let logo = image::load_from_memory(include_bytes!("../assets/icon.png"))
            .ok()
            .map(|img| {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    img.as_raw(),
                );
                cc.egui_ctx
                    .load_texture("logo", ci, egui::TextureOptions::LINEAR)
            });

        let monitor = Monitor::start(cc.egui_ctx.clone());
        {
            let mut s = monitor.shared.lock().unwrap();
            s.tag_titles = settings.tag_window_titles;
            s.exit_when_done = settings.exit_with_roblox;
            s.log(
                Level::Info,
                format!("MultiRoblox v{} started", env!("CARGO_PKG_VERSION")),
            );
            if let Ok(exe) = std::env::current_exe() {
                s.log(Level::Info, format!("Running from {}", exe.display()));
            }
            if !platform::SUPPORTED {
                s.log(
                    Level::Warn,
                    "Not running on Windows: preview mode with simulated Roblox instances.",
                );
            }
        }

        let tray_queue: TrayQueue = Default::default();
        let tray = match image::load_from_memory(include_bytes!("../assets/icon-32.png")) {
            Ok(img) => {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                match tray::install(
                    cc.egui_ctx.clone(),
                    tray_queue.clone(),
                    img.into_raw(),
                    w,
                    h,
                ) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        if platform::SUPPORTED {
                            monitor.log(Level::Warn, format!("Tray icon unavailable: {e}"));
                        }
                        None
                    }
                }
            }
            Err(e) => {
                monitor.log(Level::Warn, format!("Tray icon image failed to load: {e}"));
                None
            }
        };

        let mut settings = settings;
        if settings.start_with_windows {
            // Re-write the entry so it always points at wherever the exe lives now.
            if let Err(e) = platform::set_autostart(true) {
                monitor.log(
                    Level::Warn,
                    format!("Couldn't refresh the autostart entry: {e}"),
                );
            }
        } else if platform::autostart_enabled() {
            // An entry from an earlier install is still there; reflect reality in the UI.
            settings.start_with_windows = true;
        }

        {
            let q = tray_queue.clone();
            let ctx = cc.egui_ctx.clone();
            platform::spawn_show_listener(move || {
                q.lock().unwrap().push(TrayCommand::Show);
                ctx.request_repaint();
            });
        }

        let hidden = opts.start_hidden && tray.is_some();
        if opts.start_hidden && tray.is_none() {
            // No tray to come back from, so never start invisible.
            cc.egui_ctx
                .send_viewport_cmd(ViewportCommand::Visible(true));
        }

        let roblox_exe = platform::find_roblox_player();
        match &roblox_exe {
            Some(p) => monitor.log(Level::Info, format!("Roblox client: {}", p.display())),
            None if platform::SUPPORTED => monitor.log(
                Level::Warn,
                "No RobloxPlayerBeta.exe found yet. Launches will go through the roblox:// handler instead.",
            ),
            None => {}
        }

        Self {
            join_input: settings.last_join_input.clone(),
            launch_count: settings.launch_count.clamp(1, 10),
            settings,
            settings_dirty: false,
            monitor,
            tray,
            tray_queue,
            tab: opts.tab,
            really_quit: false,
            hidden,
            hide_hint_shown: false,
            logo,
            roblox_exe,
            toast: None,
            confirm_kill_until: None,
        }
    }

    // ----------------------------------------------------------------- actions

    fn toast(&mut self, text: impl Into<String>, color: Color32) {
        self.toast = Some(Toast {
            until: Instant::now() + Duration::from_secs(4),
            text: text.into(),
            color,
        });
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        if self.tray.is_none() {
            return;
        }
        self.hidden = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        if !self.hide_hint_shown {
            self.hide_hint_shown = true;
            self.monitor.log(
                Level::Info,
                "Hidden to the tray. The lock stays held; left-click the tray icon to come back.",
            );
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.really_quit = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn launch(&mut self, count: u32) {
        let count = count.clamp(1, 10);
        let lock = self.monitor.snapshot().lock;
        if lock != LockState::Held {
            self.toast(
                "The lock isn't held yet, so Roblox may just focus its existing window.",
                theme::WARN,
            );
        }
        let plural = if count == 1 { "" } else { "s" };
        self.toast(
            format!("Launching {count} Roblox instance{plural}…"),
            theme::ACCENT_HOVER,
        );
        let monitor = self.monitor.clone();
        let delay = Duration::from_millis(self.settings.launch_delay_ms);
        std::thread::spawn(move || {
            for i in 0..count {
                match platform::launch_home() {
                    Ok(msg) => monitor.log(Level::Ok, format!("Launch {}/{count}: {msg}", i + 1)),
                    Err(e) => {
                        monitor.log(
                            Level::Error,
                            format!("Launch {}/{count} failed: {e}", i + 1),
                        );
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(400));
                monitor.poke();
                if i + 1 < count {
                    std::thread::sleep(delay);
                }
            }
        });
        self.roblox_exe = platform::find_roblox_player();
    }

    fn join(&mut self) {
        match parse_launch_target(&self.join_input) {
            Err(e) => self.toast(e, theme::WARN),
            Ok(target) => {
                self.settings.last_join_input = self.join_input.clone();
                self.settings_dirty = true;
                let lock = self.monitor.snapshot().lock;
                if lock != LockState::Held {
                    self.toast(
                        "The lock isn't held yet, so Roblox may just focus its existing window.",
                        theme::WARN,
                    );
                } else {
                    self.toast(format!("Joining {}…", target.label), theme::ACCENT_HOVER);
                }
                let via_handler = self.settings.use_protocol_handler;
                let monitor = self.monitor.clone();
                std::thread::spawn(move || {
                    match platform::open_deeplink(&target.deeplink, via_handler) {
                        Ok(msg) => monitor.log(Level::Ok, msg),
                        Err(e) => monitor.log(Level::Error, e),
                    }
                    std::thread::sleep(Duration::from_millis(400));
                    monitor.poke();
                });
            }
        }
    }

    fn kill(&mut self, inst: &Instance) {
        match platform::kill_process(inst.pid) {
            Ok(()) => {
                self.monitor.log(
                    Level::Info,
                    format!("Closed Roblox #{} (PID {})", inst.ordinal, inst.pid),
                );
                self.toast(
                    format!("Closed Roblox #{}", inst.ordinal),
                    theme::TEXT_MUTED,
                );
            }
            Err(e) => {
                self.monitor.log(Level::Error, e.clone());
                self.toast(e, theme::DANGER);
            }
        }
        self.monitor.poke();
    }

    fn kill_all(&mut self) {
        let snap = self.monitor.snapshot();
        if snap.instances.is_empty() {
            self.toast("No Roblox instances to close.", theme::TEXT_MUTED);
            return;
        }
        let mut closed = 0;
        for inst in &snap.instances {
            match platform::kill_process(inst.pid) {
                Ok(()) => closed += 1,
                Err(e) => self.monitor.log(Level::Error, e),
            }
        }
        self.monitor.log(
            Level::Info,
            format!(
                "Closed {closed} of {} Roblox instance(s)",
                snap.instances.len()
            ),
        );
        self.toast(
            format!("Closed {closed} Roblox instance(s)"),
            theme::TEXT_MUTED,
        );
        self.monitor.poke();
    }

    fn focus(&mut self, inst: &Instance) {
        match inst.hwnd {
            Some(h) => {
                if let Err(e) = platform::focus_window(h) {
                    self.toast(e, theme::WARN);
                }
            }
            None => self.toast("That instance hasn't opened a window yet.", theme::WARN),
        }
    }

    /// Two-step "Close all": first click arms it for three seconds.
    fn kill_all_button(&mut self, ui: &mut egui::Ui, count: usize) {
        let armed = self.confirm_kill_until.is_some_and(|t| Instant::now() < t);
        if !armed {
            self.confirm_kill_until = None;
        }
        let label = if armed {
            "Confirm close all"
        } else {
            "Close all"
        };
        let resp = ui
            .add_enabled_ui(count > 0, |ui| {
                widgets::small_button(ui, label, ButtonKind::Danger)
            })
            .inner;
        if resp.clicked() {
            if armed {
                self.confirm_kill_until = None;
                self.kill_all();
            } else {
                self.confirm_kill_until = Some(Instant::now() + Duration::from_secs(3));
                self.toast(
                    format!("Click again to close all {count} instance(s)."),
                    theme::WARN,
                );
            }
        }
        if armed {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
    }

    // --------------------------------------------------------------------- ui

    fn lock_badge(lock: &LockState) -> (&'static str, Color32) {
        match lock {
            LockState::Held => ("Multi-instance active", theme::OK),
            LockState::Acquiring => ("Starting…", theme::TEXT_MUTED),
            LockState::Blocked {
                roblox_running: true,
            } => ("Roblox holds the lock", theme::WARN),
            LockState::Blocked {
                roblox_running: false,
            } => ("Another app holds the lock", theme::WARN),
            LockState::Failed(_) => ("Lock error", theme::DANGER),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if let Some(tex) = &self.logo {
                ui.add(
                    egui::Image::from_texture(tex)
                        .fit_to_exact_size(egui::vec2(36.0, 36.0))
                        .corner_radius(9),
                );
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(
                    RichText::new("MultiRoblox")
                        .size(20.0)
                        .family(theme::semibold())
                        .color(theme::TEXT),
                );
                ui.label(widgets::muted("Run as many Roblox windows as you like."));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.tray.is_some()
                    && widgets::small_button(ui, "Hide to tray", ButtonKind::Ghost).clicked()
                {
                    self.hide_window(&ctx);
                }
                let (text, color) = Self::lock_badge(&snap.lock);
                widgets::pill(ui, text, color);
            });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for tab in Tab::ALL {
                let selected = tab == self.tab;
                let text = RichText::new(tab.label())
                    .size(13.5)
                    .family(theme::medium())
                    .color(if selected {
                        theme::WHITE
                    } else {
                        theme::TEXT_MUTED
                    });
                let resp = ui.scope(|ui| {
                    let v = &mut ui.style_mut().visuals.widgets;
                    v.hovered.weak_bg_fill = theme::CARD_HOVER;
                    v.hovered.bg_stroke = Stroke::NONE;
                    v.active.bg_stroke = Stroke::NONE;
                    v.inactive.bg_stroke = Stroke::NONE;
                    let mut b = egui::Button::new(text)
                        .corner_radius(CornerRadius::same(7))
                        .min_size(egui::vec2(0.0, 30.0));
                    if selected {
                        b = b
                            .fill(theme::ACCENT_SOFT)
                            .stroke(Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.5)));
                    } else {
                        b = b.frame_when_inactive(false);
                    }
                    ui.add(b)
                });
                if resp.inner.clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(10.0);
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(widgets::small_dim(
                "Community tool. Not affiliated with Roblox Corporation.",
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(widgets::small_dim(&format!(
                    "v{}",
                    env!("CARGO_PKG_VERSION")
                )));
                ui.hyperlink_to(
                    RichText::new("GitHub").size(12.0),
                    env!("CARGO_PKG_REPOSITORY"),
                );
                if !platform::SUPPORTED && std::env::var_os("MULTIROBLOX_SCREENSHOT").is_none() {
                    widgets::pill(ui, "Preview mode", theme::WARN);
                }
            });
        });
    }

    fn status_card(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let (color, head, body): (Color32, &str, String) = match &snap.lock {
            LockState::Held => (
                theme::OK,
                "Multi-instance is active",
                "MultiRoblox owns Roblox's single-instance lock, so every launch opens a brand-new window. Keep this app running; the tray is fine.".into(),
            ),
            LockState::Acquiring => (theme::TEXT_MUTED, "Setting things up…", "Taking the Roblox lock.".into()),
            LockState::Blocked { roblox_running: true } => (
                theme::WARN,
                "Roblox got here first",
                "A Roblox client is already running and owns the lock, so extra launches would only focus that window. Close every Roblox window and MultiRoblox takes over automatically.".into(),
            ),
            LockState::Blocked { roblox_running: false } => (
                theme::WARN,
                "Another app owns the Roblox lock",
                "Probably a second multi-instance tool. Close it and MultiRoblox takes over automatically.".into(),
            ),
            LockState::Failed(e) => (theme::DANGER, "Couldn't create the lock", e.clone()),
        };
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, color);
                ui.painter().circle_stroke(
                    rect.center(),
                    7.5,
                    Stroke::new(1.5, color.gamma_multiply(0.35)),
                );
                ui.add_space(6.0);
                let right_w = 150.0;
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - right_w);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(
                        RichText::new(head)
                            .size(17.0)
                            .family(theme::semibold())
                            .color(theme::TEXT),
                    );
                    ui.add(Label::new(widgets::muted(&body)).wrap());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match &snap.lock {
                        LockState::Blocked {
                            roblox_running: true,
                        } => {
                            if widgets::button(ui, "Close all Roblox", ButtonKind::Danger).clicked()
                            {
                                self.kill_all();
                            }
                        }
                        LockState::Held => {
                            if let Some(since) = snap.lock_since {
                                ui.label(widgets::small_dim(&format!(
                                    "held for {}",
                                    fmt_duration(since.elapsed())
                                )));
                            }
                        }
                        _ => {}
                    }
                });
            });
        });
    }

    fn dashboard(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        self.status_card(ui, snap);
        ui.add_space(12.0);

        ui.columns(4, |cols| {
            let n = snap.instances.len();
            widgets::stat_tile(
                &mut cols[0],
                "Instances",
                &n.to_string(),
                &format!("running now · peak {}", snap.peak_instances),
                theme::TEXT,
            );
            widgets::stat_tile(
                &mut cols[1],
                "Memory",
                &fmt_bytes(snap.total_memory),
                "working set, all instances",
                theme::TEXT,
            );
            widgets::stat_tile(
                &mut cols[2],
                "CPU",
                &format!("{:.0}%", snap.total_cpu),
                "of the whole machine",
                theme::TEXT,
            );
            let (val, sub, color) = match (&snap.lock, snap.lock_since) {
                (LockState::Held, Some(since)) => (
                    fmt_duration(since.elapsed()),
                    "lock held".to_string(),
                    theme::OK,
                ),
                (LockState::Blocked { .. }, _) => (
                    "Waiting".to_string(),
                    "lock owned elsewhere".to_string(),
                    theme::WARN,
                ),
                (LockState::Failed(_), _) => (
                    "Error".to_string(),
                    "see the log".to_string(),
                    theme::DANGER,
                ),
                _ => ("—".to_string(), "starting".to_string(), theme::TEXT_MUTED),
            };
            widgets::stat_tile(&mut cols[3], "Lock", &val, &sub, color);
        });
        ui.add_space(12.0);

        ui.columns(2, |cols| {
            // Launch
            widgets::card(&mut cols[0], |ui| {
                widgets::section(
                    ui,
                    "Launch",
                    "Opens the Roblox app. Every click is a fresh window.",
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, "Launch Roblox", ButtonKind::Primary).clicked() {
                        self.launch(self.launch_count);
                    }
                    ui.add_space(6.0);
                    ui.label(widgets::muted("×"));
                    let before = self.launch_count;
                    ui.add(
                        egui::DragValue::new(&mut self.launch_count)
                            .range(1..=10)
                            .speed(0.05),
                    );
                    if self.launch_count != before {
                        self.settings.launch_count = self.launch_count;
                        self.settings_dirty = true;
                    }
                    ui.label(widgets::muted("at a time"));
                });
                ui.add_space(8.0);
                let client = match &self.roblox_exe {
                    Some(p) => {
                        let shown = p.display().to_string();
                        let (version, _) = path_tail(&shown);
                        format!(
                            "Client: {version}  ·  {} ms between launches",
                            self.settings.launch_delay_ms
                        )
                    }
                    None if platform::SUPPORTED => {
                        "Client: not found, launches use the roblox:// handler".to_string()
                    }
                    None => format!(
                        "Client: simulated  ·  {} ms between launches",
                        self.settings.launch_delay_ms
                    ),
                };
                ui.add(Label::new(widgets::small_dim(&client)).truncate());
            });
            // Join
            widgets::card(&mut cols[1], |ui| {
                widgets::section(
                    ui,
                    "Join a game",
                    "Place ID, roblox.com link, server link or roblox:// deep link.",
                );
                ui.add_space(12.0);
                let mut submit = false;
                ui.horizontal(|ui| {
                    let btn_w = 64.0;
                    let edit = egui::TextEdit::singleline(&mut self.join_input)
                        .hint_text("https://www.roblox.com/games/…")
                        .desired_width(ui.available_width() - btn_w - ui.spacing().item_spacing.x)
                        .margin(Margin::symmetric(10, 8))
                        .align(Align2::LEFT_CENTER);
                    let resp = ui.add(edit);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        submit = true;
                    }
                    if widgets::button(ui, "Join", ButtonKind::Primary).clicked() {
                        submit = true;
                    }
                });
                ui.add_space(8.0);
                let via = if self.settings.use_protocol_handler {
                    "via the roblox:// handler"
                } else {
                    "directly through RobloxPlayerBeta.exe"
                };
                ui.add(
                    Label::new(widgets::small_dim(&format!(
                        "Opens {via}. Change this in Settings."
                    )))
                    .truncate(),
                );
                if submit {
                    self.join();
                }
            });
        });
        ui.add_space(12.0);

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section(ui, "Instances", "Everything Roblox has open right now.");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.kill_all_button(ui, snap.instances.len());
                    if widgets::small_button(ui, "Refresh", ButtonKind::Ghost).clicked() {
                        self.monitor.poke();
                    }
                });
            });
            ui.add_space(10.0);
            self.instance_table(ui, &snap.instances, Some(5));
        });
    }

    fn instances_tab(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let n = snap.instances.len();
                let sub = if n == 0 {
                    "Nothing running.".to_string()
                } else {
                    format!(
                        "{n} running · {} · {:.0}% CPU · peak {} this session",
                        fmt_bytes(snap.total_memory),
                        snap.total_cpu,
                        snap.peak_instances
                    )
                };
                widgets::section(ui, "Instances", &sub);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.kill_all_button(ui, n);
                    if widgets::small_button(ui, "Launch another", ButtonKind::Primary).clicked() {
                        self.launch(1);
                    }
                    if widgets::small_button(ui, "Refresh", ButtonKind::Ghost).clicked() {
                        self.monitor.poke();
                    }
                });
            });
            ui.add_space(10.0);
            self.instance_table(ui, &snap.instances, None);
        });
    }

    fn instance_table(&mut self, ui: &mut egui::Ui, instances: &[Instance], limit: Option<usize>) {
        if instances.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new("No Roblox instances running")
                        .size(15.0)
                        .family(theme::medium())
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(4.0);
                ui.label(widgets::small_dim(
                    "Hit Launch Roblox. Every launch opens a fresh window while the lock is held.",
                ));
                ui.add_space(18.0);
            });
            return;
        }

        const NUM: f32 = 46.0;
        const PID: f32 = 72.0;
        const MEM: f32 = 92.0;
        const CPU: f32 = 64.0;
        const UP: f32 = 86.0;
        const ACTIONS: f32 = 140.0;
        const ROW_H: f32 = 46.0;
        let total_w = ui.available_width();
        let flex = (total_w - 20.0 - NUM - PID - MEM - CPU - UP - ACTIONS).max(90.0);

        let header = |ui: &mut egui::Ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 22.0), Sense::hover());
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(rect.shrink2(egui::vec2(10.0, 0.0)))
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let h = |ui: &mut egui::Ui, w: f32, t: &str| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 22.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_width(w - 12.0);
                                ui.set_max_width(w - 12.0);
                                ui.label(
                                    RichText::new(t)
                                        .size(11.0)
                                        .family(theme::medium())
                                        .color(theme::TEXT_DIM),
                                );
                            },
                        );
                        ui.add_space(12.0);
                    };
                    h(ui, NUM, "#");
                    h(ui, PID, "PID");
                    h(ui, flex, "WINDOW");
                    h(ui, MEM, "MEMORY");
                    h(ui, CPU, "CPU");
                    h(ui, UP, "UPTIME");
                },
            );
        };
        header(ui);
        widgets::divider(ui);

        let shown: Vec<Instance> = match limit {
            Some(n) => instances.iter().take(n).cloned().collect(),
            None => instances.to_vec(),
        };
        for inst in &shown {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, ROW_H), Sense::hover());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::CARD_HOVER);
            }
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(rect.shrink2(egui::vec2(10.0, 0.0)))
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let cell = |ui: &mut egui::Ui, w: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, ROW_H),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_width(w - 12.0);
                                ui.set_max_width(w - 12.0);
                                add(ui);
                            },
                        );
                        ui.add_space(12.0);
                    };
                    cell(ui, NUM, &mut |ui| {
                        ui.label(
                            RichText::new(format!("#{}", inst.ordinal))
                                .size(13.5)
                                .family(theme::semibold())
                                .color(theme::ACCENT_HOVER),
                        );
                    });
                    cell(ui, PID, &mut |ui| {
                        ui.label(widgets::muted(&inst.pid.to_string()));
                    });
                    cell(ui, flex, &mut |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let title = if inst.title.is_empty() {
                                "(no window yet)"
                            } else {
                                inst.title.as_str()
                            };
                            ui.add(
                                Label::new(
                                    RichText::new(title)
                                        .size(14.0)
                                        .family(theme::medium())
                                        .color(theme::TEXT),
                                )
                                .truncate(),
                            );
                            let (version, file) = path_tail(&inst.path);
                            let sub = if version.is_empty() {
                                file.to_string()
                            } else {
                                format!("{version} · {file}")
                            };
                            ui.add(Label::new(widgets::small_dim(&sub)).truncate());
                        });
                    });
                    cell(ui, MEM, &mut |ui| {
                        ui.label(
                            RichText::new(fmt_bytes(inst.memory))
                                .size(13.5)
                                .color(theme::TEXT),
                        );
                    });
                    cell(ui, CPU, &mut |ui| {
                        let c = if inst.cpu >= 50.0 {
                            theme::WARN
                        } else {
                            theme::TEXT
                        };
                        ui.label(
                            RichText::new(format!("{:.0}%", inst.cpu))
                                .size(13.5)
                                .color(c),
                        );
                    });
                    cell(ui, UP, &mut |ui| {
                        ui.label(widgets::muted(&fmt_duration(inst.uptime)));
                    });
                    ui.allocate_ui_with_layout(
                        egui::vec2(ACTIONS, ROW_H),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if widgets::small_button(ui, "Close", ButtonKind::Danger).clicked() {
                                self.kill(inst);
                            }
                            if widgets::small_button(ui, "Focus", ButtonKind::Ghost).clicked() {
                                self.focus(inst);
                            }
                        },
                    );
                },
            );
        }
        if let Some(n) = limit {
            if instances.len() > n {
                ui.add_space(8.0);
                let more = instances.len() - n;
                if ui
                    .link(
                        RichText::new(format!("Show {more} more in the Instances tab"))
                            .size(13.0)
                            .color(theme::ACCENT_HOVER),
                    )
                    .clicked()
                {
                    self.tab = Tab::Instances;
                }
            }
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                "Behaviour",
                "How MultiRoblox itself starts and stays out of your way.",
            );
            ui.add_space(10.0);

            let has_tray = self.tray.is_some();
            let mut v = self.settings.start_with_windows;
            if widgets::setting_row(
                ui,
                "Start with Windows",
                "Adds MultiRoblox to your startup apps. It starts hidden in the tray.",
                &mut v,
            ) {
                match platform::set_autostart(v) {
                    Ok(()) => {
                        self.settings.start_with_windows = v;
                        self.settings_dirty = true;
                        self.monitor.log(
                            Level::Info,
                            if v {
                                "Autostart enabled"
                            } else {
                                "Autostart disabled"
                            },
                        );
                    }
                    Err(e) => {
                        self.monitor.log(Level::Error, e.clone());
                        self.toast(e, theme::DANGER);
                    }
                }
            }
            widgets::divider(ui);

            let mut v = self.settings.start_minimized;
            let desc = if has_tray {
                "Don't show this window when the app starts; it waits in the tray."
            } else {
                "Needs the tray icon, which isn't available here."
            };
            ui.add_enabled_ui(has_tray, |ui| {
                if widgets::setting_row(ui, "Start minimised to the tray", desc, &mut v) {
                    self.settings.start_minimized = v;
                    self.settings_dirty = true;
                }
            });
            widgets::divider(ui);

            let mut v = self.settings.close_to_tray;
            let desc = if has_tray {
                "Closing the window keeps the lock alive in the background. Use Quit in the tray menu to exit."
            } else {
                "Needs the tray icon, which isn't available here."
            };
            ui.add_enabled_ui(has_tray, |ui| {
                if widgets::setting_row(ui, "Close button hides to the tray", desc, &mut v) {
                    self.settings.close_to_tray = v;
                    self.settings_dirty = true;
                }
            });
            widgets::divider(ui);

            let mut v = self.settings.always_on_top;
            if widgets::setting_row(
                ui,
                "Always on top",
                "Keep this window above other windows.",
                &mut v,
            ) {
                self.settings.always_on_top = v;
                self.settings_dirty = true;
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if v {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                }));
            }
            widgets::divider(ui);

            let mut v = self.settings.exit_with_roblox;
            if widgets::setting_row(
                ui,
                "Quit when the last Roblox closes",
                "Once every Roblox instance has exited, MultiRoblox exits too.",
                &mut v,
            ) {
                self.settings.exit_with_roblox = v;
                self.settings_dirty = true;
                self.monitor.shared.lock().unwrap().exit_when_done = v;
            }
        });
        ui.add_space(12.0);

        widgets::card(ui, |ui| {
            widgets::section(ui, "Roblox", "How instances are launched and shown.");
            ui.add_space(10.0);

            let mut v = self.settings.tag_window_titles;
            if widgets::setting_row(
                ui,
                "Number the Roblox windows",
                "Renames windows to \"Roblox #1\", \"Roblox #2\", … so you can tell them apart in the taskbar.",
                &mut v,
            ) {
                self.settings.tag_window_titles = v;
                self.settings_dirty = true;
                self.monitor.shared.lock().unwrap().tag_titles = v;
                self.monitor.poke();
            }
            widgets::divider(ui);

            let mut v = self.settings.use_protocol_handler;
            if widgets::setting_row(
                ui,
                "Join through the roblox:// handler",
                "Uses whatever handles Roblox links on your PC (Bloxstrap, Fishstrap, vanilla). Off = start RobloxPlayerBeta.exe directly.",
                &mut v,
            ) {
                self.settings.use_protocol_handler = v;
                self.settings_dirty = true;
            }
            widgets::divider(ui);

            ui.horizontal(|ui| {
                ui.set_min_height(44.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new("Launch delay")
                            .size(14.0)
                            .family(theme::medium())
                            .color(theme::TEXT),
                    );
                    ui.label(widgets::muted(
                        "Pause between launches when starting several instances at once.",
                    ));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let before = self.settings.launch_delay_ms;
                    ui.add(
                        egui::DragValue::new(&mut self.settings.launch_delay_ms)
                            .range(0..=20_000)
                            .speed(50)
                            .suffix(" ms"),
                    );
                    if before != self.settings.launch_delay_ms {
                        self.settings_dirty = true;
                    }
                });
            });
        });
        ui.add_space(12.0);

        widgets::card(ui, |ui| {
            widgets::section(ui, "Storage", "Where things live.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new("Settings file")
                            .size(14.0)
                            .family(theme::medium())
                            .color(theme::TEXT),
                    );
                    ui.add(
                        Label::new(
                            RichText::new(Settings::path().display().to_string())
                                .size(12.0)
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        )
                        .truncate(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::small_button(ui, "Reset to defaults", ButtonKind::Danger).clicked()
                    {
                        let keep_autostart = self.settings.start_with_windows;
                        self.settings = Settings::default();
                        if keep_autostart {
                            let _ = platform::set_autostart(false);
                        }
                        self.launch_count = 1;
                        self.settings_dirty = true;
                        {
                            let mut s = self.monitor.shared.lock().unwrap();
                            s.tag_titles = false;
                            s.exit_when_done = false;
                        }
                        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
                        self.toast("Settings reset to defaults.", theme::TEXT_MUTED);
                    }
                    if widgets::small_button(ui, "Open folder", ButtonKind::Ghost).clicked() {
                        let dir = Settings::dir();
                        let _ = std::fs::create_dir_all(&dir);
                        if let Err(e) = platform::shell_open(&dir.display().to_string()) {
                            self.toast(e, theme::WARN);
                        }
                    }
                });
            });
        });
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        let entries: Vec<_> = self
            .monitor
            .shared
            .lock()
            .unwrap()
            .log
            .iter()
            .cloned()
            .collect();
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section(
                    ui,
                    "Activity log",
                    &format!("{} entries · newest at the bottom", entries.len()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::small_button(ui, "Clear", ButtonKind::Ghost).clicked() {
                        self.monitor.shared.lock().unwrap().log.clear();
                    }
                    if widgets::small_button(ui, "Copy", ButtonKind::Ghost).clicked() {
                        let text: String = entries
                            .iter()
                            .map(|e| {
                                format!(
                                    "{} [{:?}] {}\n",
                                    e.time.format("%Y-%m-%d %H:%M:%S"),
                                    e.level,
                                    e.text
                                )
                            })
                            .collect();
                        ui.ctx().copy_text(text);
                        self.toast("Log copied to the clipboard.", theme::TEXT_MUTED);
                    }
                });
            });
            ui.add_space(10.0);
            let max_h = (ui.available_height() - 8.0).max(240.0);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(max_h)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for e in &entries {
                        let color = match e.level {
                            Level::Info => theme::TEXT_DIM,
                            Level::Ok => theme::OK,
                            Level::Warn => theme::WARN,
                            Level::Error => theme::DANGER,
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(e.time.format("%H:%M:%S").to_string())
                                    .size(12.0)
                                    .monospace()
                                    .color(theme::TEXT_DIM),
                            );
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.5, color);
                            ui.add(
                                Label::new(RichText::new(&e.text).size(13.0).color(theme::TEXT))
                                    .wrap(),
                            );
                        });
                    }
                    if entries.is_empty() {
                        ui.label(widgets::muted("Nothing yet."));
                    }
                });
        });
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(t) = &self.toast else { return };
        if Instant::now() >= t.until {
            self.toast = None;
            return;
        }
        let (text, color) = (t.text.clone(), t.color);
        egui::Area::new(Id::unique("toast"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-20.0, 92.0))
            .order(Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::CARD_HOVER)
                    .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.add(
                                Label::new(RichText::new(text).size(13.5).color(theme::TEXT))
                                    .wrap(),
                            );
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(150));
    }
}

/// Last two components of a path, split on either separator so Windows paths
/// read correctly even in the non-Windows preview build.
fn path_tail(path: &str) -> (&str, &str) {
    let mut parts = path.rsplit(['\\', '/']).filter(|p| !p.is_empty());
    let file = parts.next().unwrap_or("");
    let dir = parts.next().unwrap_or("");
    (dir, file)
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let commands: Vec<TrayCommand> = std::mem::take(&mut *self.tray_queue.lock().unwrap());
        for cmd in commands {
            match cmd {
                TrayCommand::Show => self.show_window(ctx),
                TrayCommand::Launch => self.launch(1),
                TrayCommand::KillAll => self.kill_all(),
                TrayCommand::Quit => self.quit(ctx),
            }
        }
        if self.monitor.shared.lock().unwrap().exit_requested {
            self.quit(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.really_quit
            && self.settings.close_to_tray
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_window(ctx);
        }
        if self.settings_dirty {
            self.settings_dirty = false;
            if let Err(e) = self.settings.save() {
                self.monitor
                    .log(Level::Error, format!("Couldn't save settings: {e}"));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let snap = self.monitor.snapshot();

        egui::Panel::top("header")
            .frame(Frame::new().fill(theme::SURFACE).inner_margin(Margin {
                left: 20,
                right: 20,
                top: 16,
                bottom: 0,
            }))
            .show(ui, |ui| self.header(ui, &snap));
        egui::Panel::bottom("footer")
            .frame(
                Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(Margin::symmetric(20, 10)),
            )
            .show(ui, |ui| self.footer(ui));
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Dashboard => self.dashboard(ui, &snap),
                        Tab::Instances => self.instances_tab(ui, &snap),
                        Tab::Settings => self.settings_tab(ui),
                        Tab::Log => self.log_tab(ui),
                    });
            });

        self.draw_toast(&ctx);
        if !self.hidden {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::path_tail;

    #[test]
    fn path_tail_handles_both_separators() {
        assert_eq!(
            path_tail(
                r"C:\Users\me\AppData\Local\Roblox\Versions\version-abc\RobloxPlayerBeta.exe"
            ),
            ("version-abc", "RobloxPlayerBeta.exe")
        );
        assert_eq!(path_tail("/opt/roblox/player"), ("roblox", "player"));
        assert_eq!(path_tail(""), ("", ""));
    }
}
