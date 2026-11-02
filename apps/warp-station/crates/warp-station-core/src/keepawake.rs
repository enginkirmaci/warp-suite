//! Keep the Windows machine awake: blocks idle system sleep and display
//! off via SetThreadExecutionState, which needs no admin rights. Manual
//! sleep (power menu, lid close) still works by design.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

#[cfg(windows)]
const ES_CONTINUOUS: u32 = 0x8000_0000;
#[cfg(windows)]
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
#[cfg(windows)]
const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;

#[cfg(windows)]
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The execution state belongs to the calling thread, so every caller must
/// run on the app's long-lived main UI thread — a dying thread would drop
/// the request and re-allow sleep.
#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let flags = if enabled {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    // SAFETY: documented kernel32 FFI with no invariants beyond the calling
    // thread owning the state (the doc contract above); 0 is the failure value
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        return Err("SetThreadExecutionState failed".to_string());
    }
    ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

#[cfg(not(windows))]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Keep Awake is Windows-only".to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn non_windows_is_stubbed() {
        assert!(!super::is_enabled());
        assert!(super::set_enabled(true).is_err());
        assert!(super::set_enabled(false).is_ok());
    }
}
