//! OS integration. The real implementation lives in `windows.rs`; every other
//! OS gets a mock so the UI can be developed, tested and screenshotted anywhere.

use std::time::{Duration, SystemTime};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod mock;
#[cfg(not(windows))]
pub use self::mock::*;

/// True when running on the OS the tool is actually built for.
pub const SUPPORTED: bool = cfg!(windows);

/// Names of the kernel objects Roblox uses to enforce one client per session.
#[allow(dead_code)]
pub const ROBLOX_MUTEX: &str = "ROBLOX_singletonMutex";
#[allow(dead_code)]
pub const ROBLOX_EVENT: &str = "ROBLOX_singletonEvent";

/// A Roblox client process as seen by the monitor.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
    /// Working-set size in bytes.
    pub memory: u64,
    /// Kernel + user CPU time consumed so far.
    pub cpu_time: Duration,
    pub started: Option<SystemTime>,
    /// Main top-level window, if one is open.
    pub hwnd: Option<isize>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LockError {
    /// Somebody (normally a running Roblox client) already owns the mutex.
    AlreadyExists,
    Os(String),
}

pub fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}
