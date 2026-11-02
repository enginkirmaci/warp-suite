//! Launches the external warp-notes app (the Notes window of the warp
//! family): fire-and-forget, detached from Warp Station' lifetime, over the
//! shared ~/.warp/notes store. A missing binary only fails that one launch.

use crate::exe;
use std::path::Path;

/// One warp-notes launch. The store root is always passed so a customized
/// `notes_store_path` setting follows; the repo scope and the note to open
/// are optional (the repo row's notes icon pins the scope, a global-search
/// note hit opens the note).
pub fn launch(
    executable: &str,
    store_root: &Path,
    repo: Option<&str>,
    note: Option<&Path>,
) -> Result<(), String> {
    let Some(path) = exe::locate(executable.trim()) else {
        return Err(format!(
            "warp-notes executable not found: {}",
            executable.trim()
        ));
    };
    let mut command = std::process::Command::new(&path);
    command.arg("--store").arg(store_root);
    if let Some(repo) = repo {
        command.arg("--repo").arg(repo);
    }
    if let Some(note) = note {
        command.arg("--note").arg(note);
    }
    // the window outlives Warp Station: fully detached, no --parent watcher
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not start warp-notes: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_executable_names_the_setting() {
        let err = launch(
            "warp-station-definitely-missing-warp-notes",
            Path::new("/tmp/notes"),
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }
}
