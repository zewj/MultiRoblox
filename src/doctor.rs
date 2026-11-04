//! `--doctor`: a quick, GUI-free diagnostic that exercises every OS integration
//! and writes a plain-text report the user can read or paste into a bug report.

use crate::platform::{self, LockError};
use crate::settings::Settings;
use crate::util::fmt_bytes;
use std::fmt::Write as _;
use std::path::PathBuf;

pub fn report() -> String {
    let mut r = String::new();
    let _ = writeln!(r, "MultiRoblox doctor report");
    let _ = writeln!(r, "version      : {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        r,
        "time         : {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let _ = writeln!(
        r,
        "platform     : {}{}",
        std::env::consts::OS,
        if platform::SUPPORTED {
            ""
        } else {
            " (preview build, Windows calls are simulated)"
        }
    );
    if let Ok(exe) = std::env::current_exe() {
        let _ = writeln!(r, "executable   : {}", exe.display());
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[tool instance]");
    if platform::claim_tool_instance() {
        let _ = writeln!(r, "  no other copy of MultiRoblox is running");
    } else {
        let _ = writeln!(
            r,
            "  another copy of MultiRoblox is running (its lock will show as held below)"
        );
    }
    let _ = writeln!(r);

    let procs = platform::list_roblox_processes();
    let _ = writeln!(r, "[roblox lock]  ({})", platform::ROBLOX_MUTEX);
    match platform::acquire_lock() {
        Ok(_guard) => {
            let _ = writeln!(r, "  OK: acquired the lock, then released it");
        }
        Err(LockError::AlreadyExists) => {
            if procs.is_empty() {
                let _ = writeln!(r, "  BLOCKED: another program owns the lock (MultiRoblox itself, or a similar tool)");
            } else {
                let _ = writeln!(
                    r,
                    "  BLOCKED: a running Roblox client owns the lock; close Roblox first"
                );
            }
        }
        Err(LockError::Os(e)) => {
            let _ = writeln!(r, "  FAILED: {e}");
        }
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[roblox processes]  {} found", procs.len());
    for p in &procs {
        let _ = writeln!(
            r,
            "  pid {:<7} {:<10} {:>9}  window={:?}  {}",
            p.pid,
            p.name,
            fmt_bytes(p.memory),
            if p.title.is_empty() {
                "(none)".to_string()
            } else {
                p.title.clone()
            },
            p.path
        );
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[roblox client]");
    match platform::find_roblox_player() {
        Some(p) => {
            let _ = writeln!(r, "  {}", p.display());
        }
        None => {
            let _ = writeln!(r, "  RobloxPlayerBeta.exe not found in the usual folders; launches fall back to roblox://");
        }
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[protocol handlers]");
    for scheme in ["roblox", "roblox-player"] {
        match platform::protocol_handler_command(scheme) {
            Some(cmd) => {
                let _ = writeln!(r, "  {scheme:<14}: {cmd}");
            }
            None => {
                let _ = writeln!(r, "  {scheme:<14}: not registered");
            }
        }
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[autostart]");
    let _ = writeln!(
        r,
        "  registry entry: {}",
        if platform::autostart_enabled() {
            "present"
        } else {
            "absent"
        }
    );
    let _ = writeln!(r);

    let _ = writeln!(r, "[settings]");
    let path = Settings::path();
    let _ = writeln!(
        r,
        "  file: {} ({})",
        path.display(),
        if path.exists() {
            "exists"
        } else {
            "not created yet"
        }
    );
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(_) => {
                let _ = writeln!(r, "  parses OK");
            }
            Err(e) => {
                let _ = writeln!(r, "  INVALID JSON ({e}); defaults will be used");
            }
        },
        Err(_) => {
            let _ = writeln!(r, "  defaults in use");
        }
    }
    let _ = writeln!(r);

    let _ = writeln!(r, "[machine]");
    let _ = writeln!(r, "  logical cpus: {}", platform::cpu_count());
    r
}

/// Write the report next to the settings file and return its path.
pub fn write_report(text: &str) -> Result<PathBuf, String> {
    let dir = Settings::dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join("doctor-report.txt");
    std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}
