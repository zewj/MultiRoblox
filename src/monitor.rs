//! Background thread: owns the Roblox singleton lock, samples the running
//! Roblox processes once a second and keeps an activity log.

use crate::platform::{self, LockError, LockGuard};
use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug)]
pub struct Instance {
    pub pid: u32,
    /// Stable "#n" number, reused only after an instance exits.
    pub ordinal: u32,
    pub title: String,
    pub path: String,
    pub memory: u64,
    /// Percent of the whole machine (all cores).
    pub cpu: f32,
    pub uptime: Duration,
    pub hwnd: Option<isize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockState {
    Acquiring,
    Held,
    Blocked { roblox_running: bool },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub time: DateTime<Local>,
    pub level: Level,
    pub text: String,
}

pub struct Shared {
    pub instances: Vec<Instance>,
    pub lock: LockState,
    pub lock_since: Option<Instant>,
    pub total_memory: u64,
    pub total_cpu: f32,
    pub log: VecDeque<LogEntry>,
    /// Settings mirrored for the worker thread.
    pub tag_titles: bool,
    pub exit_when_done: bool,
    /// Set by the worker when the app should quit (exit_when_done fired).
    pub exit_requested: bool,
    pub ever_seen_roblox: bool,
    pub peak_instances: usize,
}

impl Shared {
    pub fn log(&mut self, level: Level, text: impl Into<String>) {
        self.log.push_back(LogEntry {
            time: Local::now(),
            level,
            text: text.into(),
        });
        while self.log.len() > 500 {
            self.log.pop_front();
        }
    }
}

#[derive(Clone)]
pub struct Monitor {
    pub shared: Arc<Mutex<Shared>>,
    poke_tx: Sender<()>,
}

impl Monitor {
    pub fn start(ctx: egui::Context) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            instances: Vec::new(),
            lock: LockState::Acquiring,
            lock_since: None,
            total_memory: 0,
            total_cpu: 0.0,
            log: VecDeque::new(),
            tag_titles: false,
            exit_when_done: false,
            exit_requested: false,
            ever_seen_roblox: false,
            peak_instances: 0,
        }));
        let (poke_tx, poke_rx) = mpsc::channel();
        let worker_shared = shared.clone();
        std::thread::Builder::new()
            .name("roblox-monitor".into())
            .spawn(move || worker(ctx, worker_shared, poke_rx))
            .expect("spawn monitor thread");
        Self { shared, poke_tx }
    }

    /// Ask the worker to re-sample right away (after a launch or kill).
    pub fn poke(&self) {
        let _ = self.poke_tx.send(());
    }

    pub fn log(&self, level: Level, text: impl Into<String>) {
        self.shared.lock().unwrap().log(level, text);
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = self.shared.lock().unwrap();
        Snapshot {
            instances: s.instances.clone(),
            lock: s.lock.clone(),
            lock_since: s.lock_since,
            total_memory: s.total_memory,
            total_cpu: s.total_cpu,
            peak_instances: s.peak_instances,
        }
    }
}

/// Cheap copy of the worker state for one UI frame.
pub struct Snapshot {
    pub instances: Vec<Instance>,
    pub lock: LockState,
    pub lock_since: Option<Instant>,
    pub total_memory: u64,
    pub total_cpu: f32,
    pub peak_instances: usize,
}

fn lowest_free_ordinal(used: &HashMap<u32, u32>) -> u32 {
    let taken: HashSet<u32> = used.values().copied().collect();
    (1..).find(|n| !taken.contains(n)).unwrap_or(1)
}

fn worker(ctx: egui::Context, shared: Arc<Mutex<Shared>>, poke_rx: Receiver<()>) {
    let mut guard: Option<LockGuard> = None;
    let mut prev_cpu: HashMap<u32, (Duration, Instant)> = HashMap::new();
    let mut ordinals: HashMap<u32, u32> = HashMap::new();
    let mut last_lock_error: Option<LockError> = None;
    let ncpu = platform::cpu_count().max(1) as f32;

    loop {
        let procs = platform::list_roblox_processes();
        let now = Instant::now();

        // --- singleton lock -------------------------------------------------
        if guard.is_none() {
            match platform::acquire_lock() {
                Ok(g) => {
                    guard = Some(g);
                    last_lock_error = None;
                    let mut s = shared.lock().unwrap();
                    s.lock = LockState::Held;
                    s.lock_since = Some(now);
                    s.log(Level::Ok, "Holding the Roblox single-instance lock. Every launch now opens its own window.");
                }
                Err(e) => {
                    let mut s = shared.lock().unwrap();
                    match &e {
                        LockError::AlreadyExists => {
                            let roblox_running = !procs.is_empty();
                            s.lock = LockState::Blocked { roblox_running };
                            if last_lock_error.as_ref() != Some(&e) {
                                if roblox_running {
                                    s.log(Level::Warn, "Roblox is already running and owns the lock. Close every Roblox window and MultiRoblox will take over automatically.");
                                } else {
                                    s.log(Level::Warn, "Another program owns the Roblox lock (maybe a second multi-instance tool). Close it and MultiRoblox will take over.");
                                }
                            }
                        }
                        LockError::Os(msg) => {
                            s.lock = LockState::Failed(msg.clone());
                            if last_lock_error.as_ref() != Some(&e) {
                                s.log(Level::Error, format!("Couldn't create the lock: {msg}"));
                            }
                        }
                    }
                    last_lock_error = Some(e);
                }
            }
        }

        // --- instances --------------------------------------------------------
        let live: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        // Free the numbers of instances that exited before handing out new ones.
        let gone: Vec<(u32, u32)> = ordinals
            .iter()
            .filter(|(pid, _)| !live.contains(pid))
            .map(|(p, o)| (*p, *o))
            .collect();
        for (pid, _) in &gone {
            ordinals.remove(pid);
            prev_cpu.remove(pid);
        }
        // Newcomers get the lowest free number, oldest process first.
        let mut newcomers: Vec<&platform::ProcessInfo> = procs
            .iter()
            .filter(|p| !ordinals.contains_key(&p.pid))
            .collect();
        newcomers.sort_by_key(|p| (p.started, p.pid));
        let new_pids: Vec<u32> = newcomers.iter().map(|p| p.pid).collect();
        for p in newcomers {
            let n = lowest_free_ordinal(&ordinals);
            ordinals.insert(p.pid, n);
        }

        let mut instances = Vec::with_capacity(procs.len());
        let mut total_memory = 0u64;
        let mut total_cpu = 0f32;
        for p in &procs {
            let ordinal = ordinals.get(&p.pid).copied().unwrap_or(0);
            let cpu = match prev_cpu.get(&p.pid) {
                Some((t, at)) if now > *at => {
                    let wall = now.duration_since(*at).as_secs_f32();
                    let busy = p.cpu_time.saturating_sub(*t).as_secs_f32();
                    if wall > 0.0 {
                        (busy / wall / ncpu * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };
            prev_cpu.insert(p.pid, (p.cpu_time, now));
            let uptime = p
                .started
                .and_then(|s| SystemTime::now().duration_since(s).ok())
                .unwrap_or_default();
            total_memory += p.memory;
            total_cpu += cpu;
            instances.push(Instance {
                pid: p.pid,
                ordinal,
                title: p.title.clone(),
                path: p.path.clone(),
                memory: p.memory,
                cpu,
                uptime,
                hwnd: p.hwnd,
            });
        }
        instances.sort_by_key(|i| i.ordinal);

        // --- publish ----------------------------------------------------------
        {
            let mut s = shared.lock().unwrap();
            for inst in instances.iter().filter(|i| new_pids.contains(&i.pid)) {
                s.log(
                    Level::Info,
                    format!("Roblox #{} started (PID {})", inst.ordinal, inst.pid),
                );
            }
            for (pid, ordinal) in &gone {
                s.log(Level::Info, format!("Roblox #{ordinal} exited (PID {pid})"));
            }
            if s.tag_titles {
                for inst in &mut instances {
                    if let Some(hwnd) = inst.hwnd {
                        if inst.title == "Roblox" {
                            let t = format!("Roblox #{}", inst.ordinal);
                            platform::set_window_title(hwnd, &t);
                            inst.title = t;
                        }
                    }
                }
            } else {
                for inst in &mut instances {
                    if let Some(hwnd) = inst.hwnd {
                        if inst.title.starts_with("Roblox #") {
                            platform::set_window_title(hwnd, "Roblox");
                            inst.title = "Roblox".into();
                        }
                    }
                }
            }
            if !instances.is_empty() {
                s.ever_seen_roblox = true;
                s.peak_instances = s.peak_instances.max(instances.len());
            } else if s.ever_seen_roblox && s.exit_when_done && !s.exit_requested {
                s.exit_requested = true;
                s.log(
                    Level::Info,
                    "Last Roblox instance closed; quitting as configured.",
                );
            }
            s.instances = instances;
            s.total_memory = total_memory;
            s.total_cpu = total_cpu;
        }
        ctx.request_repaint();

        // Sleep until the next tick or until somebody pokes us.
        let _ = poke_rx.recv_timeout(Duration::from_millis(1000));
        // Drain any extra pokes so a burst doesn't cause a burst of sampling.
        while poke_rx.try_recv().is_ok() {}
    }
}
