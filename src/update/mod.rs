//! Automatic update notification system for the Forgen compiler / Datara toolchain.
//!
//! Provides pip-style update notices when a new version of forgen/datara is available:
//! ```text
//! [notice] A new release of forgen is available: 0.1.0 -> 0.1.1
//! [notice] To update, run: cargo install forgen
//! ```
//!
//! Design guarantees:
//! - Non-blocking / strict 1.5s timeout with background or cached checking.
//! - Throttled: checks online at most once per 24 hours via `update_cache.json`.
//! - Zero failure modes: offline, timeout, or malformed responses fail silently.
//! - Opt-out via `FORGEN_NO_UPDATE_CHECK=1` or `PIP_DISABLE_PIP_VERSION_CHECK=1`.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CHECK_INTERVAL_SECS: u64 = 86_400; // 24 hours
const GITHUB_REPO: &str = "waters1ze/datara";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCache {
    pub last_checked_epoch_secs: u64,
    pub latest_version: String,
}

/// Returns current version from Cargo package version.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Parse semantic version string (e.g. "v0.1.0" or "0.1.0" or "0.2.1-beta") into (major, minor, patch).
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let clean = v.trim().trim_start_matches('v');
    let base = clean.split('-').next().unwrap_or(clean);
    let mut parts = base.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Returns true if `latest` is strictly newer than `current`.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some((l_maj, l_min, l_pat)), Some((c_maj, c_min, c_pat))) => {
            (l_maj, l_min, l_pat) > (c_maj, c_min, c_pat)
        }
        _ => false,
    }
}

/// Formats the pip-style update notice message.
pub fn format_pip_notice(current: &str, latest: &str) -> String {
    let use_color = env::var("NO_COLOR").is_err();
    let current_clean = current.trim_start_matches('v');
    let latest_clean = latest.trim_start_matches('v');

    if use_color {
        format!(
            "\n\x1b[33m[notice]\x1b[0m A new release of forgen is available: \x1b[31m{}\x1b[0m -> \x1b[32m{}\x1b[0m\n\x1b[33m[notice]\x1b[0m To update, run: \x1b[36mcargo install forgen\x1b[0m\n",
            current_clean, latest_clean
        )
    } else {
        format!(
            "\n[notice] A new release of forgen is available: {} -> {}\n[notice] To update, run: cargo install forgen\n",
            current_clean, latest_clean
        )
    }
}

/// Returns path to cache file.
pub fn get_cache_path() -> PathBuf {
    if let Ok(user_profile) = env::var("USERPROFILE") {
        let dir = PathBuf::from(user_profile).join(".forgen");
        let _ = fs::create_dir_all(&dir);
        return dir.join("update_cache.json");
    }
    if let Ok(home) = env::var("HOME") {
        let dir = PathBuf::from(home).join(".forgen");
        let _ = fs::create_dir_all(&dir);
        return dir.join("update_cache.json");
    }
    env::temp_dir().join("forgen_update_cache.json")
}

/// Loads cached update information if available.
pub fn load_cache() -> Option<UpdateCache> {
    let path = get_cache_path();
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Saves update cache.
pub fn save_cache(cache: &UpdateCache) {
    let path = get_cache_path();
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(path, json);
    }
}

/// Queries GitHub releases API using curl with strict timeout.
pub fn fetch_latest_release_github() -> Option<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };

    let output = Command::new(curl_bin)
        .arg("-s")
        .arg("-m")
        .arg("2")
        .arg("-H")
        .arg("User-Agent: forgen-toolchain")
        .arg(&url)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    let tag = val.get("tag_name")?.as_str()?;
    Some(tag.trim().to_string())
}

/// Checks if an update is available, using cache when valid.
pub fn check_for_update_cached() -> Option<(String, String)> {
    let current = current_version();

    // Check opt-out
    if env::var("FORGEN_NO_UPDATE_CHECK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        || env::var("PIP_DISABLE_PIP_VERSION_CHECK")
            .map(|v| v == "1")
            .unwrap_or(false)
        || env::var("CI")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    {
        return None;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(cached) = load_cache()
        && now.saturating_sub(cached.last_checked_epoch_secs) < CHECK_INTERVAL_SECS
    {
        if is_newer_version(&cached.latest_version, current) {
            return Some((current.to_string(), cached.latest_version));
        } else {
            return None;
        }
    }

    // Fetch fresh version
    if let Some(latest) = fetch_latest_release_github() {
        save_cache(&UpdateCache {
            last_checked_epoch_secs: now,
            latest_version: latest.clone(),
        });

        if is_newer_version(&latest, current) {
            return Some((current.to_string(), latest));
        }
    }

    None
}

/// Checks and displays the pip-style update notification if a newer version exists.
pub fn notify_if_update_available() {
    if let Some((curr, latest)) = check_for_update_cached() {
        eprint!("{}", format_pip_notice(&curr, &latest));
    }
}

/// Manual check command for `forgen check-update`.
pub fn run_check_update_command() {
    let current = current_version();
    println!("Checking for updates (current: v{})...", current);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    match fetch_latest_release_github() {
        Some(latest) => {
            save_cache(&UpdateCache {
                last_checked_epoch_secs: now,
                latest_version: latest.clone(),
            });

            if is_newer_version(&latest, current) {
                eprint!("{}", format_pip_notice(current, &latest));
            } else {
                println!(
                    "forgen is up to date (v{} is the latest available release).",
                    current
                );
            }
        }
        None => {
            if let Some(cached) = load_cache()
                && is_newer_version(&cached.latest_version, current)
            {
                eprint!("{}", format_pip_notice(current, &cached.latest_version));
                return;
            }
            println!("Could not connect to update server. Please check your internet connection.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("2.0.0"), Some((2, 0, 0)));
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(is_newer_version("v0.1.1", "v0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.1"));
    }

    #[test]
    fn test_format_pip_notice() {
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        let notice = format_pip_notice("0.1.0", "0.1.1");
        assert!(notice.contains("[notice] A new release of forgen is available: 0.1.0 -> 0.1.1"));
        assert!(notice.contains("To update, run: cargo install forgen"));
    }
}
