//! Launches the external warp-notes app (the Notes window of the warp
//! family): fire-and-forget, detached from Warp Station' lifetime, over the
//! shared ~/notes store. A missing binary only fails that one launch.

use crate::exe;
use crate::process::DetachedSpawn;
use std::ffi::OsString;
use std::path::Path;

/// Which slice of the store the new window opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesScope<'a> {
    /// The whole store, at its root: the titlebar's notes button and the
    /// surfaces that cross repositories (a global-search note hit, the
    /// summary's "Open in Notes"). The app must land here even when it
    /// remembered another scope, so the launch says so outright.
    WholeStore,
    /// One repository's notes folder: the repo launchers.
    Repo(&'a str),
}

/// One warp-notes launch. The store root is always passed so a customized
/// `notes_store_path` setting follows; the scope is explicit so the titlebar
/// never inherits the selected repo, and the note to open is optional.
pub fn launch(
    executable: &str,
    store_root: &Path,
    scope: NotesScope<'_>,
    note: Option<&Path>,
) -> Result<(), String> {
    let Some(path) = exe::locate(executable.trim()) else {
        return Err(format!(
            "warp-notes executable not found: {}",
            executable.trim()
        ));
    };
    let mut command = std::process::Command::new(&path);
    command.args(launch_args(store_root, scope, note));
    // the window outlives Warp Station: fully detached, no --parent watcher
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn_detached()
        .map_err(|err| format!("could not start warp-notes: {err}"))
}

/// The arguments after the executable: the store, the scope flag and the note.
fn launch_args(store_root: &Path, scope: NotesScope<'_>, note: Option<&Path>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--store"),
        store_root.as_os_str().to_os_string(),
    ];
    match scope {
        // --root overrides the scope the app remembered from its last window
        NotesScope::WholeStore => args.push(OsString::from("--root")),
        NotesScope::Repo(repo) => {
            args.push(OsString::from("--repo"));
            args.push(OsString::from(repo));
        }
    }
    if let Some(note) = note {
        args.push(OsString::from("--note"));
        args.push(note.as_os_str().to_os_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn missing_executable_names_the_setting() {
        let err = launch(
            "warp-station-definitely-missing-warp-notes",
            Path::new("/tmp/notes"),
            NotesScope::WholeStore,
            None,
        )
        .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn the_titlebar_launch_asks_for_the_whole_store() {
        let args = launch_args(Path::new("/tmp/notes"), NotesScope::WholeStore, None);
        assert_eq!(argv(&args), ["--store", "/tmp/notes", "--root"]);
    }

    #[test]
    fn a_note_hit_opens_the_whole_store_at_that_note() {
        let args = launch_args(
            Path::new("/tmp/notes"),
            NotesScope::WholeStore,
            Some(Path::new("/tmp/notes/beta/beta.md")),
        );
        assert_eq!(
            argv(&args),
            [
                "--store",
                "/tmp/notes",
                "--root",
                "--note",
                "/tmp/notes/beta/beta.md"
            ]
        );
    }

    #[test]
    fn a_repo_launch_pins_that_repo() {
        let args = launch_args(Path::new("/tmp/notes"), NotesScope::Repo("alpha"), None);
        assert_eq!(argv(&args), ["--store", "/tmp/notes", "--repo", "alpha"]);
    }
}
