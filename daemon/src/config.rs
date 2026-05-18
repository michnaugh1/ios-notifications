//! Configuration loader for ios-notifications.
//!
//! Reads `~/.config/ios-notifications/config.toml`. The pair helper creates
//! this file with just the `[device]` section; other sections fall back to
//! defaults.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub device: DeviceConfig,
    #[serde(default)]
    #[allow(dead_code)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub supervisor: SupervisorConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    pub mac: String,
    pub adapter: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct NotificationsConfig {
    pub show_connection_state: bool,
    pub connection_state_timeout_ms: u32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            show_connection_state: true,
            connection_state_timeout_ms: 2000,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct FilterConfig {
    pub mode: FilterMode,
    pub apps: Vec<String>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            mode: FilterMode::Blacklist,
            apps: vec![],
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    Blacklist,
    Whitelist,
    Off,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SupervisorConfig {
    pub backoff_initial_s: u32,
    pub backoff_max_s: u32,
    pub resume_grace_ms: u32,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            backoff_initial_s: 2,
            backoff_max_s: 60,
            resume_grace_ms: 1500,
        }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Self> {
        toml::from_str(s).context("parsing config TOML")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file: {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn default_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("ios-notifications").join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.device.mac, "AA:BB:CC:DD:EE:FF");
        assert!(cfg.device.adapter.is_none());
        assert_eq!(cfg.filter.mode, FilterMode::Blacklist);
        assert!(cfg.filter.apps.is_empty());
        assert_eq!(cfg.notifications.show_connection_state, true);
        assert_eq!(cfg.supervisor.backoff_initial_s, 2);
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
            [device]
            mac = "11:22:33:44:55:66"
            adapter = "hci1"

            [notifications]
            show_connection_state = false
            connection_state_timeout_ms = 5000

            [filter]
            mode = "whitelist"
            apps = ["com.apple.MobileSMS", "com.apple.mobilecal"]

            [supervisor]
            backoff_initial_s = 5
            backoff_max_s = 120
            resume_grace_ms = 3000
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.device.adapter.as_deref(), Some("hci1"));
        assert_eq!(cfg.filter.mode, FilterMode::Whitelist);
        assert_eq!(cfg.filter.apps.len(), 2);
        assert_eq!(cfg.supervisor.resume_grace_ms, 3000);
    }

    #[test]
    fn off_mode_parses() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"

            [filter]
            mode = "off"
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.filter.mode, FilterMode::Off);
    }

    #[test]
    fn malformed_toml_errors() {
        let toml = "this is not = valid = toml";
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn missing_device_section_errors() {
        let toml = "[notifications]\nshow_connection_state = false";
        let err = Config::parse(toml).unwrap_err();
        assert!(err.to_string().contains("device") || err.root_cause().to_string().contains("device"));
    }

    #[test]
    fn invalid_filter_mode_errors() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"
            [filter]
            mode = "nonsense"
        "#;
        assert!(Config::parse(toml).is_err());
    }
}
