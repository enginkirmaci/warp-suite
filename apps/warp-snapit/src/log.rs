//! Minimal file logger.
//!
//! Warp Station starts the companion without a visible console, so stderr is
//! usually invisible. A small append-only log next to the settings makes
//! startup and hook failures diagnosable without a logging framework.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn line(message: &str) {
    #[cfg(debug_assertions)]
    eprintln!("snapit: {message}");

    let path = crate::settings::snapit_root().join("snapit.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let _guard = LOG_LOCK.lock().expect("log mutex poisoned");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{secs}] {message}");
    }
}
