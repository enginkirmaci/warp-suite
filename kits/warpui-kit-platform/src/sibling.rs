//! Launching a sibling suite app.
//!
//! The bundle installs every binary in one `bin/`, so a sibling is found next
//! to `current_exe()` first and on `PATH` second. That is a different question
//! from warp-station's `exe::locate` (which resolves user-configured CLIs and
//! probes the user-level install dirs); this module only knows the family's
//! own executables, which ship together.
//!
//! Std only, so the headless crates can carry it too.

use std::path::PathBuf;

/// The executable's file name on this platform.
pub fn exe_name(app: &str) -> String {
    #[cfg(windows)]
    {
        format!("{app}.exe")
    }
    #[cfg(not(windows))]
    {
        app.to_string()
    }
}

/// Where a sibling's binary is, if it is installed.
///
/// `app` is the binary stem (`warp-station`, `warp-notes`, …), not the display
/// name. The bundle's own `bin/` wins over a stale copy earlier on `PATH`.
pub fn binary(app: &str) -> Option<PathBuf> {
    let name = exe_name(app);
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
    {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// Whether this app can offer "Open <sibling>" at all.
pub fn installed(app: &str) -> bool {
    binary(app).is_some()
}

/// Spawns a sibling detached: the window outlives the app that opened it.
///
/// Windows gets `CREATE_NO_WINDOW` so a GUI that is really a console binary
/// leaves no console behind.
pub fn open(app: &str) -> Result<(), String> {
    let Some(binary) = binary(app) else {
        return Err(format!(
            "{app} is not installed next to this app or on PATH"
        ));
    };
    let mut command = std::process::Command::new(&binary);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not start {app}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_name_carries_the_platform_suffix() {
        let name = exe_name("warp-notes");
        if cfg!(windows) {
            assert_eq!(name, "warp-notes.exe");
        } else {
            assert_eq!(name, "warp-notes");
        }
    }

    #[test]
    fn an_app_nobody_installed_is_not_found() {
        assert!(!installed("warp-suite-definitely-not-installed"));
        assert!(open("warp-suite-definitely-not-installed").is_err());
    }
}
