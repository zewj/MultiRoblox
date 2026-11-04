//! Persistent user settings, stored as JSON in the per-user config folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Register in HKCU\...\Run so the tool starts with Windows (minimised).
    pub start_with_windows: bool,
    /// Start hidden in the tray instead of showing the window.
    pub start_minimized: bool,
    /// The window close button hides to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Keep the window above other windows.
    pub always_on_top: bool,
    /// Quit automatically once every Roblox instance has exited.
    pub exit_with_roblox: bool,
    /// Rename Roblox windows to "Roblox #1", "Roblox #2", ...
    pub tag_window_titles: bool,
    /// Join links go through the registered `roblox://` handler (Bloxstrap friendly)
    /// instead of starting RobloxPlayerBeta.exe directly.
    pub use_protocol_handler: bool,
    /// Drop CPU priority of Roblox windows that aren't in the foreground.
    pub deprioritize_background: bool,
    /// Pause between launches when starting several instances at once.
    pub launch_delay_ms: u64,
    /// Last value of the "launch N instances" spinner.
    pub launch_count: u32,
    /// Last thing typed in the join box.
    pub last_join_input: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            start_minimized: false,
            close_to_tray: true,
            always_on_top: false,
            exit_with_roblox: false,
            tag_window_titles: false,
            use_protocol_handler: true,
            deprioritize_background: false,
            launch_delay_ms: 2000,
            launch_count: 1,
            last_join_input: String::new(),
        }
    }
}

impl Settings {
    pub fn dir() -> PathBuf {
        if cfg!(windows) {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata).join("MultiRoblox");
            }
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("multiroblox");
        }
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".config").join("multiroblox")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("settings.json")
    }

    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let path = Self::path();
        std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let s = Settings {
            launch_delay_ms: 1234,
            tag_window_titles: true,
            ..Default::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
        // Unknown / missing fields fall back to defaults instead of failing.
        let partial: Settings = serde_json::from_str(r#"{"launch_count": 4, "bogus": 1}"#).unwrap();
        assert_eq!(partial.launch_count, 4);
        assert!(partial.close_to_tray);
    }
}
