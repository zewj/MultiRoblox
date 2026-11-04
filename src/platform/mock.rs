//! Simulated backend for non-Windows builds (development, CI, screenshots).
#![allow(dead_code)]

use super::{LockError, ProcessInfo};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

pub struct LockGuard;

struct Fake {
    pid: u32,
    started: Instant,
    base_mem: u64,
    cpu_share: f32,
}

fn fakes() -> &'static Mutex<Vec<Fake>> {
    static FAKES: OnceLock<Mutex<Vec<Fake>>> = OnceLock::new();
    FAKES.get_or_init(|| {
        let now = Instant::now();
        Mutex::new(vec![
            Fake {
                pid: 18240,
                started: now - Duration::from_secs(1543),
                base_mem: 1_391_000_000,
                cpu_share: 0.11,
            },
            Fake {
                pid: 21716,
                started: now - Duration::from_secs(812),
                base_mem: 1_120_000_000,
                cpu_share: 0.08,
            },
            Fake {
                pid: 9032,
                started: now - Duration::from_secs(97),
                base_mem: 684_000_000,
                cpu_share: 0.19,
            },
        ])
    })
}

pub fn acquire_lock() -> Result<LockGuard, LockError> {
    Ok(LockGuard)
}

pub fn list_roblox_processes() -> Vec<ProcessInfo> {
    let now = Instant::now();
    fakes()
        .lock()
        .unwrap()
        .iter()
        .map(|f| {
            let age = now.duration_since(f.started);
            let wobble = ((age.as_secs_f64() / 7.0).sin() * 40_000_000.0) as i64;
            ProcessInfo {
                pid: f.pid,
                name: "RobloxPlayerBeta.exe".into(),
                path: r"C:\Users\you\AppData\Local\Roblox\Versions\version-6f1a2c\RobloxPlayerBeta.exe".into(),
                memory: (f.base_mem as i64 + wobble).max(0) as u64,
                cpu_time: age.mul_f32(f.cpu_share),
                started: Some(SystemTime::now() - age),
                hwnd: Some(f.pid as isize * 16),
                title: "Roblox".into(),
            }
        })
        .collect()
}

pub fn kill_process(pid: u32) -> Result<(), String> {
    let mut f = fakes().lock().unwrap();
    let before = f.len();
    f.retain(|p| p.pid != pid);
    if f.len() == before {
        Err(format!("No simulated process with PID {pid}"))
    } else {
        Ok(())
    }
}

pub fn focus_window(_hwnd: isize) -> Result<(), String> {
    Ok(())
}

pub fn set_window_title(_hwnd: isize, _title: &str) {}

pub fn find_roblox_player() -> Option<PathBuf> {
    None
}

fn spawn_fake() -> u32 {
    let mut f = fakes().lock().unwrap();
    let pid = f.iter().map(|p| p.pid).max().unwrap_or(1000) + 4;
    f.push(Fake {
        pid,
        started: Instant::now(),
        base_mem: 420_000_000,
        cpu_share: 0.25,
    });
    pid
}

pub fn launch_home() -> Result<String, String> {
    let pid = spawn_fake();
    Ok(format!("Simulated Roblox instance started (PID {pid})"))
}

pub fn open_deeplink(url: &str, _via_handler: bool) -> Result<String, String> {
    let pid = spawn_fake();
    Ok(format!("Simulated launch of {url} (PID {pid})"))
}

pub fn shell_open(target: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn set_autostart(_enable: bool) -> Result<(), String> {
    Ok(())
}

pub fn autostart_enabled() -> bool {
    false
}

pub fn claim_tool_instance() -> bool {
    true
}

pub fn signal_existing_instance() {}

pub fn spawn_show_listener(_on_show: impl Fn() + Send + 'static) {}
