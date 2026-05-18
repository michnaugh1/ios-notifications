//! Per-app notification filtering rules.
//!
//! Three modes:
//! - `Blacklist`: drop notifications from apps in the list, pass all others
//! - `Whitelist`: only pass notifications from apps in the list
//! - `Off`: pass everything regardless of the list
//!
//! Bundle IDs are matched case-sensitively.

use std::collections::HashSet;

pub use crate::config::{FilterConfig, FilterMode};

#[derive(Debug)]
pub struct Filter {
    mode: FilterMode,
    apps: HashSet<String>,
}

impl Filter {
    pub fn new(cfg: FilterConfig) -> Self {
        Self {
            mode: cfg.mode,
            apps: cfg.apps.into_iter().collect(),
        }
    }

    pub fn should_show(&self, app_id: &str) -> bool {
        match self.mode {
            FilterMode::Off => true,
            FilterMode::Blacklist => !self.apps.contains(app_id),
            FilterMode::Whitelist => self.apps.contains(app_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: FilterMode, apps: &[&str]) -> FilterConfig {
        FilterConfig {
            mode,
            apps: apps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn off_mode_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Off, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn blacklist_blocks_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks", "com.apple.news"]));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.news"));
        assert!(f.should_show("com.apple.MobileSMS"));
    }

    #[test]
    fn blacklist_empty_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &[]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn whitelist_only_allows_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &["com.apple.MobileSMS"]));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.mobilecal"));
    }

    #[test]
    fn whitelist_empty_blocks_everything() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &[]));
        assert!(!f.should_show("anything"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.stocks"));
        assert!(!f.should_show("com.apple.Stocks"));
    }
}
