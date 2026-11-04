//! Small formatting and parsing helpers shared by the UI and the monitor.

use std::time::Duration;

/// Human readable byte count ("1.24 GB", "512 MB").
pub fn fmt_bytes(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b >= K * K * K {
        format!("{:.2} GB", b / (K * K * K))
    } else if b >= K * K {
        format!("{:.0} MB", b / (K * K))
    } else if b >= K {
        format!("{:.0} KB", b / K)
    } else {
        format!("{bytes} B")
    }
}

/// Compact duration ("42s", "3m 05s", "2h 14m", "1d 03h").
pub fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else if s < 86_400 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d {:02}h", s / 86_400, (s % 86_400) / 3600)
    }
}

/// What the "Join" box resolved the user's input into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchTarget {
    /// A `roblox://` deep link ready to hand to the Roblox client.
    pub deeplink: String,
    /// Short description for logs and toasts.
    pub label: String,
}

fn digits_after<'a>(hay: &'a str, needle: &str) -> Option<&'a str> {
    let lower = hay.to_ascii_lowercase();
    let idx = lower.find(&needle.to_ascii_lowercase())? + needle.len();
    let rest = &hay[idx..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

fn query_value<'a>(hay: &'a str, key: &str) -> Option<&'a str> {
    let lower = hay.to_ascii_lowercase();
    let pat = format!("{}=", key.to_ascii_lowercase());
    let idx = lower.find(&pat)? + pat.len();
    let rest = &hay[idx..];
    let end = rest
        .find(|c: char| c == '&' || c == '#' || c == '/' || c.is_whitespace())
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

/// Turn a place ID, a roblox.com link or a raw deep link into something launchable.
pub fn parse_launch_target(input: &str) -> Result<LaunchTarget, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("Enter a place ID or a Roblox game link first.".into());
    }
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("roblox://") || lower.starts_with("roblox-player:") {
        return Ok(LaunchTarget {
            deeplink: raw.to_string(),
            label: "custom deep link".into(),
        });
    }

    let place = if raw.chars().all(|c| c.is_ascii_digit()) {
        Some(raw)
    } else {
        digits_after(raw, "placeId=")
            .or_else(|| digits_after(raw, "/games/"))
            .or_else(|| digits_after(raw, "placeid/"))
    };
    let Some(place) = place else {
        return Err("Couldn't find a place ID in that. Paste a roblox.com/games/... link or the numeric ID.".into());
    };
    let place: u64 = place
        .parse()
        .map_err(|_| "That place ID is too long to be valid.".to_string())?;
    if place == 0 {
        return Err("A place ID can't be zero.".into());
    }

    let mut deeplink = format!("roblox://placeId={place}");
    let mut label = format!("place {place}");
    if let Some(job) = query_value(raw, "gameInstanceId") {
        deeplink.push_str(&format!("&gameInstanceId={job}"));
        label.push_str(" (specific server)");
    } else if let Some(code) = query_value(raw, "privateServerLinkCode") {
        deeplink.push_str(&format!("&linkCode={code}"));
        label.push_str(" (private server)");
    }
    Ok(LaunchTarget { deeplink, label })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(2048), "2 KB");
        assert_eq!(fmt_bytes(300 * 1024 * 1024), "300 MB");
        assert_eq!(fmt_bytes(1_610_612_736), "1.50 GB");
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(Duration::from_secs(5)), "5s");
        assert_eq!(fmt_duration(Duration::from_secs(185)), "3m 05s");
        assert_eq!(fmt_duration(Duration::from_secs(8040)), "2h 14m");
        assert_eq!(fmt_duration(Duration::from_secs(97_200)), "1d 03h");
    }

    #[test]
    fn parses_plain_id() {
        let t = parse_launch_target(" 920587237 ").unwrap();
        assert_eq!(t.deeplink, "roblox://placeId=920587237");
    }

    #[test]
    fn parses_game_url() {
        let t = parse_launch_target(
            "https://www.roblox.com/games/2753915549/Blox-Fruits?refPageId=abc",
        )
        .unwrap();
        assert_eq!(t.deeplink, "roblox://placeId=2753915549");
    }

    #[test]
    fn parses_server_url() {
        let t = parse_launch_target(
            "https://www.roblox.com/games/start?placeId=1234&gameInstanceId=5e1d-77aa",
        )
        .unwrap();
        assert_eq!(t.deeplink, "roblox://placeId=1234&gameInstanceId=5e1d-77aa");
    }

    #[test]
    fn parses_private_server() {
        let t = parse_launch_target(
            "https://www.roblox.com/games/1234/Name?privateServerLinkCode=987654",
        )
        .unwrap();
        assert_eq!(t.deeplink, "roblox://placeId=1234&linkCode=987654");
    }

    #[test]
    fn passes_deeplinks_through() {
        let t = parse_launch_target("roblox://placeId=1&foo=bar").unwrap();
        assert_eq!(t.deeplink, "roblox://placeId=1&foo=bar");
    }

    #[test]
    fn rejects_junk() {
        assert!(parse_launch_target("").is_err());
        assert!(parse_launch_target("hello").is_err());
        assert!(parse_launch_target("0").is_err());
    }
}
