//! Win32 backend: the singleton lock, process discovery, window control,
//! launching the client, autostart and single-instance signalling.

use super::{LockError, Priority, ProcessInfo, ROBLOX_EVENT, ROBLOX_MUTEX};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::ProcessStatus::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TOOL_MUTEX: &str = "MultiRoblox.SingleInstance";
const TOOL_SHOW_EVENT: &str = "MultiRoblox.ShowWindow";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "MultiRoblox";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn last_error() -> String {
    // SAFETY: plain FFI call with no arguments.
    let code = unsafe { GetLastError() };
    format!("Win32 error {code}")
}

/// Owns the Roblox singleton objects for as long as it lives.
pub struct LockGuard {
    mutex: HANDLE,
    event: HANDLE,
}

// SAFETY: kernel handles may be used from any thread.
unsafe impl Send for LockGuard {}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // SAFETY: both handles were created by us and are released exactly once.
        unsafe {
            if !self.mutex.is_null() {
                ReleaseMutex(self.mutex);
                CloseHandle(self.mutex);
            }
            if !self.event.is_null() {
                CloseHandle(self.event);
            }
        }
    }
}

pub fn acquire_lock() -> Result<LockGuard, LockError> {
    let mutex_name = wide(ROBLOX_MUTEX);
    let event_name = wide(ROBLOX_EVENT);
    // SAFETY: names are NUL-terminated; return values are checked.
    unsafe {
        let mutex = CreateMutexW(null(), 1, mutex_name.as_ptr());
        if mutex.is_null() {
            return Err(LockError::Os(last_error()));
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            return Err(LockError::AlreadyExists);
        }
        let event = CreateEventW(null(), 1, 0, event_name.as_ptr());
        Ok(LockGuard { mutex, event })
    }
}

fn is_roblox_exe(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "robloxplayerbeta.exe" || n == "windows10universal.exe"
}

fn filetime_u64(ft: &FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn filetime_to_system_time(ft: &FILETIME) -> Option<SystemTime> {
    const EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;
    let t = filetime_u64(ft);
    if t < EPOCH_DIFF_100NS {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_nanos((t - EPOCH_DIFF_100NS) * 100))
}

fn inspect(pid: u32, name: &str) -> Option<ProcessInfo> {
    let mut info = ProcessInfo {
        pid,
        name: name.to_string(),
        ..Default::default()
    };
    // SAFETY: handle is checked for null and closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // We can still list it; we just can't read its stats.
            return Some(info);
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
            info.path = String::from_utf16_lossy(&buf[..len as usize]);
        }
        // The Store build shares a generic exe name; only count it when it lives in a Roblox package.
        if name.eq_ignore_ascii_case("windows10universal.exe")
            && !info.path.to_ascii_lowercase().contains("roblox")
        {
            CloseHandle(h);
            return None;
        }
        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
            info.memory = pmc.WorkingSetSize as u64;
        }
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
            std::mem::zeroed();
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0 {
            info.cpu_time = Duration::from_nanos((filetime_u64(&k) + filetime_u64(&u)) * 100);
            info.started = filetime_to_system_time(&c);
        }
        CloseHandle(h);
    }
    Some(info)
}

struct WindowInfo {
    pid: u32,
    hwnd: isize,
    title: String,
}

unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the Vec we passed to EnumWindows on this very call stack.
    let list = unsafe { &mut *(lparam as *mut Vec<WindowInfo>) };
    // SAFETY: hwnd is a valid window handle supplied by EnumWindows.
    unsafe {
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return 1;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        list.push(WindowInfo {
            pid,
            hwnd: hwnd as isize,
            title: String::from_utf16_lossy(&buf[..n]),
        });
    }
    1
}

fn top_level_windows() -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = Vec::new();
    // SAFETY: the callback only runs during this call, while `list` is alive.
    unsafe {
        EnumWindows(Some(enum_windows_cb), &mut list as *mut _ as LPARAM);
    }
    list
}

pub fn list_roblox_processes() -> Vec<ProcessInfo> {
    let mut out = Vec::new();
    // SAFETY: snapshot handle is validated and closed; PROCESSENTRY32W is sized before use.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let name = from_wide(&pe.szExeFile);
                if is_roblox_exe(&name) {
                    if let Some(p) = inspect(pe.th32ProcessID, &name) {
                        out.push(p);
                    }
                }
                if Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }

    if !out.is_empty() {
        let windows = top_level_windows();
        for p in &mut out {
            let best = windows
                .iter()
                .filter(|w| w.pid == p.pid)
                .max_by_key(|w| w.title.to_ascii_lowercase().contains("roblox"));
            if let Some(w) = best {
                p.hwnd = Some(w.hwnd);
                p.title = w.title.clone();
            }
        }
    }
    out.sort_by_key(|p| (p.started, p.pid));
    out
}

pub fn kill_process(pid: u32) -> Result<(), String> {
    // SAFETY: handle checked and closed.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return Err(format!(
                "Couldn't open PID {pid} for termination ({})",
                last_error()
            ));
        }
        let ok = TerminateProcess(h, 1);
        let err = last_error();
        CloseHandle(h);
        if ok == 0 {
            Err(format!("TerminateProcess failed for PID {pid} ({err})"))
        } else {
            Ok(())
        }
    }
}

pub fn focus_window(hwnd: isize) -> Result<(), String> {
    let h = hwnd as HWND;
    // SAFETY: an invalid HWND makes these calls fail harmlessly.
    unsafe {
        if IsWindow(h) == 0 {
            return Err("That window no longer exists".into());
        }
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h) == 0 {
            return Err("Windows refused to bring that window to the front".into());
        }
    }
    Ok(())
}

pub fn set_window_title(hwnd: isize, title: &str) {
    let t = wide(title);
    // SAFETY: NUL-terminated string; invalid HWND just fails.
    unsafe {
        SetWindowTextW(hwnd as HWND, t.as_ptr());
    }
}

/// Newest RobloxPlayerBeta.exe across the vanilla installer and the common bootstrappers.
pub fn find_roblox_player() -> Option<PathBuf> {
    let mut version_roots: Vec<PathBuf> = Vec::new();
    let mut direct: Vec<PathBuf> = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        for sub in [
            "Roblox",
            "Bloxstrap",
            "Fishstrap",
            "Voidstrap",
            "Froststrap",
        ] {
            version_roots.push(local.join(sub).join("Versions"));
            direct.push(
                local
                    .join(sub)
                    .join("Roblox")
                    .join("Player")
                    .join("RobloxPlayerBeta.exe"),
            );
        }
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(pf) = std::env::var(var) {
            version_roots.push(PathBuf::from(pf).join("Roblox").join("Versions"));
        }
    }

    let mut best: Option<(SystemTime, PathBuf)> = None;
    let mut consider = |exe: PathBuf| {
        if let Ok(md) = std::fs::metadata(&exe) {
            if md.is_file() {
                let t = md.modified().unwrap_or(UNIX_EPOCH);
                if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                    best = Some((t, exe));
                }
            }
        }
    };
    for exe in direct {
        consider(exe);
    }
    for root in version_roots {
        if let Ok(rd) = std::fs::read_dir(&root) {
            for entry in rd.flatten() {
                consider(entry.path().join("RobloxPlayerBeta.exe"));
            }
        }
    }
    best.map(|(_, p)| p)
}

fn spawn(exe: &Path, args: &[&str]) -> Result<(), String> {
    std::process::Command::new(exe)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to start {}: {e}", exe.display()))
}

pub fn shell_open(target: &str) -> Result<(), String> {
    let op = wide("open");
    let file = wide(target);
    // SAFETY: NUL-terminated strings; result is an error code disguised as HINSTANCE.
    let r = unsafe {
        ShellExecuteW(
            null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if (r as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows couldn't open \"{target}\" (ShellExecute code {})",
            r as isize
        ))
    }
}

/// Open the Roblox app (home screen) as a fresh instance.
pub fn launch_home() -> Result<String, String> {
    if let Some(exe) = find_roblox_player() {
        spawn(&exe, &["--app"])?;
        return Ok(format!("Started {}", exe.display()));
    }
    shell_open("roblox://")?;
    Ok("No RobloxPlayerBeta.exe found; opened roblox:// through the system handler".into())
}

/// Hand a `roblox://` deep link to the client, either through the registered
/// protocol handler or straight to the newest RobloxPlayerBeta.exe.
pub fn open_deeplink(url: &str, via_handler: bool) -> Result<String, String> {
    if !via_handler {
        if let Some(exe) = find_roblox_player() {
            spawn(&exe, &["--app", "--deeplink", url])?;
            return Ok(format!(
                "Started {} with {url}",
                exe.file_name().and_then(|f| f.to_str()).unwrap_or("Roblox")
            ));
        }
    }
    shell_open(url)?;
    Ok(format!("Handed {url} to the roblox:// protocol handler"))
}

fn open_run_key(access: u32) -> Result<HKEY, String> {
    let sub = wide(RUN_KEY);
    let mut hkey: HKEY = null_mut();
    // SAFETY: valid NUL-terminated subkey; out-pointer is a live local.
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            access,
            null(),
            &mut hkey,
            null_mut(),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("Couldn't open the Run registry key (error {rc})"));
    }
    Ok(hkey)
}

pub fn set_autostart(enable: bool) -> Result<(), String> {
    let hkey = open_run_key(KEY_SET_VALUE | KEY_QUERY_VALUE)?;
    let name = wide(RUN_VALUE);
    // SAFETY: key handle is valid until RegCloseKey below; buffers outlive the calls.
    let rc = unsafe {
        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let value = wide(&format!("\"{}\" --minimized", exe.display()));
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            )
        } else {
            match RegDeleteValueW(hkey, name.as_ptr()) {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                other => other,
            }
        }
    };
    // SAFETY: closing the key we opened.
    unsafe { RegCloseKey(hkey) };
    if rc != ERROR_SUCCESS {
        return Err(format!("Registry update failed (error {rc})"));
    }
    Ok(())
}

pub fn autostart_enabled() -> bool {
    let Ok(hkey) = open_run_key(KEY_QUERY_VALUE) else {
        return false;
    };
    let name = wide(RUN_VALUE);
    let mut size = 0u32;
    // SAFETY: querying only the value size.
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            null(),
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(hkey) };
    rc == ERROR_SUCCESS
}

struct SendHandle(HANDLE);
// SAFETY: kernel handles may be used from any thread.
unsafe impl Send for SendHandle {}

/// Claim the tool's own single-instance mutex. Returns false if another copy runs.
pub fn claim_tool_instance() -> bool {
    let name = wide(TOOL_MUTEX);
    // SAFETY: the handle is intentionally kept open for the life of the process.
    unsafe {
        let h = CreateMutexW(null(), 1, name.as_ptr());
        if h.is_null() {
            return true;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(h);
            return false;
        }
        // Deliberately never closed: the mutex must outlive everything else in the process.
        true
    }
}

/// Ask the already-running copy to show its window.
pub fn signal_existing_instance() {
    let name = wide(TOOL_SHOW_EVENT);
    // SAFETY: handle checked and closed.
    unsafe {
        let e = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if !e.is_null() {
            SetEvent(e);
            CloseHandle(e);
        }
    }
}

/// Wait (on a background thread) for `signal_existing_instance` and run `on_show`.
pub fn spawn_show_listener(on_show: impl Fn() + Send + 'static) {
    let name = wide(TOOL_SHOW_EVENT);
    // SAFETY: auto-reset event owned by the listener thread.
    let e = unsafe { CreateEventW(null(), 0, 0, name.as_ptr()) };
    if e.is_null() {
        return;
    }
    let handle = SendHandle(e);
    std::thread::Builder::new()
        .name("show-listener".into())
        .spawn(move || {
            let handle = handle;
            loop {
                // SAFETY: waiting on our own event handle.
                if unsafe { WaitForSingleObject(handle.0, INFINITE) } == WAIT_OBJECT_0 {
                    on_show();
                } else {
                    break;
                }
            }
        })
        .ok();
}

/// Arrange the given windows in a grid on the monitor that holds the first one.
pub fn tile_windows(hwnds: &[isize]) -> Result<usize, String> {
    let n = hwnds.len();
    if n == 0 {
        return Ok(0);
    }
    let mut area = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: structs are sized before use; every handle is checked by the callee.
    unsafe {
        let monitor = MonitorFromWindow(hwnds[0] as HWND, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0 {
            area = info.rcWork;
        } else if SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut area as *mut RECT as *mut core::ffi::c_void,
            0,
        ) == 0
        {
            return Err("Couldn't read the desktop work area".into());
        }
    }
    let cols = (n as f64).sqrt().ceil().max(1.0) as i32;
    let rows = (n as u32).div_ceil(cols as u32) as i32;
    let cell_w = (area.right - area.left) / cols;
    let cell_h = (area.bottom - area.top) / rows;
    if cell_w < 200 || cell_h < 150 {
        return Err("Too many windows to tile sensibly on this monitor".into());
    }
    let mut placed = 0;
    for (i, &hwnd) in hwnds.iter().enumerate() {
        let col = i as i32 % cols;
        let row = i as i32 / cols;
        let x = area.left + col * cell_w;
        let y = area.top + row * cell_h;
        // SAFETY: an invalid HWND makes these calls fail harmlessly.
        unsafe {
            let h = hwnd as HWND;
            if IsWindow(h) == 0 {
                continue;
            }
            if IsIconic(h) != 0 || IsZoomed(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            if SetWindowPos(
                h,
                null_mut(),
                x,
                y,
                cell_w,
                cell_h,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            ) != 0
            {
                placed += 1;
            }
        }
    }
    Ok(placed)
}

/// PID that owns the current foreground window.
pub fn foreground_pid() -> Option<u32> {
    // SAFETY: plain queries; a null HWND is handled.
    unsafe {
        let h = GetForegroundWindow();
        if h.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        (pid != 0).then_some(pid)
    }
}

pub fn set_process_priority(pid: u32, priority: Priority) -> Result<(), String> {
    let class = match priority {
        Priority::Normal => NORMAL_PRIORITY_CLASS,
        Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
    };
    // SAFETY: handle checked and closed.
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if h.is_null() {
            return Err(format!(
                "Couldn't open PID {pid} to change its priority ({})",
                last_error()
            ));
        }
        let ok = SetPriorityClass(h, class);
        let err = last_error();
        CloseHandle(h);
        if ok == 0 {
            Err(format!("SetPriorityClass failed for PID {pid} ({err})"))
        } else {
            Ok(())
        }
    }
}

/// The command registered for a URL scheme such as `roblox` or `roblox-player`.
pub fn protocol_handler_command(scheme: &str) -> Option<String> {
    let sub = wide(&format!("{scheme}\\shell\\open\\command"));
    let mut hkey: HKEY = null_mut();
    // SAFETY: key handle validated and closed; buffer sized from the first query.
    unsafe {
        if RegOpenKeyExW(
            HKEY_CLASSES_ROOT,
            sub.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let mut size = 0u32;
        let mut ty = 0u32;
        if RegQueryValueExW(hkey, null(), null(), &mut ty, null_mut(), &mut size) != ERROR_SUCCESS
            || size == 0
        {
            RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        let rc = RegQueryValueExW(
            hkey,
            null(),
            null(),
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        (rc == ERROR_SUCCESS).then(|| from_wide(&buf))
    }
}

pub fn show_message(title: &str, text: &str) {
    let t = wide(title);
    let x = wide(text);
    // SAFETY: NUL-terminated strings.
    unsafe {
        MessageBoxW(
            null_mut(),
            x.as_ptr(),
            t.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
