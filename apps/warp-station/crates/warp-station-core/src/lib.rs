//! Headless Warp Station services: settings, repo scanning, git reads
//! and mutations, GitHub/Azure DevOps activity, opencode integration, notes.
//! No gpui dependencies — everything here must be testable without a display.

pub mod activity;
pub mod autostart;
pub mod azure;
pub mod clipboard_password;
pub mod code_search;
pub mod desktop_notifications;
pub mod exe;
pub mod git;
pub mod github;
pub mod hotkey;
pub mod jsonio;
pub mod keepawake;
pub mod model;
pub mod notes;
pub mod nuget;
pub mod opencode;
pub mod paths;
pub mod process;
pub mod rate;
pub mod refresh;
pub mod rel_time;
pub mod repo_store;
pub mod scan;
pub mod settings;
pub mod snapit;
pub mod terminal;
pub mod warp;
pub mod warp_notes;

/// Unit tests that mutate the process-global HOME (or read it) share this
/// lock: the harness runs them in parallel threads, so a scratch HOME left
/// set by one test would otherwise read as another test's home.
#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
