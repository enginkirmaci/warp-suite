//! The suite's per-user layout: one `~/.warp` tree holds every app's own
//! data — settings, caches, logs — plus the shared text size and the SnapIt
//! settings. The notes store is user content rather than app data, so it
//! lives in `~/notes`:
//!
//! ```text
//! ~/.warp/
//! ├── ui.json      the shared text size and palette (warpui-kit-core owns it)
//! ├── logs/        every app's daily rolling log
//! ├── station/     warp-station: settings, repo and model caches, prompts
//! ├── agent/       warp-agent: settings, chat state, the door endpoint
//! ├── notes/       warp-notes: its own settings
//! └── snapit/      the SnapIt settings and layouts Warp Station reads
//! ~/notes/         the notes store, one folder per repository
//! ```
//!
//! The `adopt_*` helpers move an older layout in. Every one is idempotent: a
//! missing source is a no-op, and what already sits in the new place always
//! wins over the copy it replaces — except for the notes store, where a name
//! collision keeps both sides rather than risk a note.

use std::path::{Path, PathBuf};

/// `$HOME` on unix; `USERPROFILE` first on Windows — a POSIX-style `$HOME`
/// leaked from a Git Bash parent shell is not a valid Win32 path.
pub fn home() -> PathBuf {
    let keys: [&str; 2] = if cfg!(windows) {
        ["USERPROFILE", "HOME"]
    } else {
        ["HOME", "USERPROFILE"]
    };
    keys.iter()
        .find_map(|key| std::env::var_os(key).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The suite's data root, `~/.warp`, shared by every app.
pub fn suite_root() -> PathBuf {
    home().join(".warp")
}

/// One app's own folder inside the suite root, e.g. `~/.warp/agent`.
pub fn app_dir(app: &str) -> PathBuf {
    suite_root().join(app)
}

/// The one logs folder, `~/.warp/logs`; the apps' file prefixes keep their
/// daily files apart.
pub fn logs_dir() -> PathBuf {
    suite_root().join("logs")
}

/// The notes store, `~/notes` — user content, not app data.
pub fn notes_store_root() -> PathBuf {
    home().join("notes")
}

/// Move a legacy app-data folder into its new place. A missing target renames
/// the whole folder; an existing one moves the children over — a superseded
/// file (the new layout wins) is retired, a folder collision keeps both sides
/// and the legacy folder for a manual pass. A missing legacy folder is a
/// no-op, so this runs on every start.
pub fn adopt_legacy_dir(legacy: &Path, target: &Path) {
    if !legacy.is_dir() || legacy == target {
        return;
    }
    if !target.exists() {
        match std::fs::rename(legacy, target) {
            Ok(()) => tracing::info!(
                "migrated the app data {} → {}",
                legacy.display(),
                target.display()
            ),
            Err(err) => {
                tracing::warn!("could not migrate the app data {}: {err}", legacy.display())
            }
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
    let mut moved = 0usize;
    let mut failed = false;
    for entry in entries.flatten() {
        let source = entry.path();
        let dest = target.join(entry.file_name());
        if dest.exists() {
            if source.is_dir() {
                // a folder collision could hold live data — keep it for a
                // manual pass instead of deleting either side
                tracing::warn!(
                    "keeping the legacy {}: {} already exists",
                    source.display(),
                    dest.display()
                );
                failed = true;
                continue;
            }
            // the new layout wins: the legacy copy is superseded, not needed
            let _ = std::fs::remove_file(&source);
            continue;
        }
        match std::fs::rename(&source, &dest) {
            Ok(()) => moved += 1,
            Err(_) => failed = true,
        }
    }
    if moved > 0 {
        tracing::info!("moved {moved} item(s) into {}", target.display());
    }
    if !failed {
        // remove_dir only succeeds once the folder is empty: a leftover keeps
        // the legacy root for the next pass
        let _ = std::fs::remove_dir(legacy);
    }
}

/// Move a legacy app's log files into the shared [`logs_dir`]. Logs are
/// throwaway, so a name collision replaces the legacy copy; a missing folder
/// is a no-op.
pub fn adopt_legacy_logs(legacy: &Path) {
    adopt_log_files(legacy, &logs_dir());
}

fn adopt_log_files(legacy: &Path, target: &Path) {
    if !legacy.is_dir() || legacy == target {
        return;
    }
    if std::fs::create_dir_all(target).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
    let mut moved = 0usize;
    let mut failed = false;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            failed = true;
            continue;
        }
        let dest = target.join(entry.file_name());
        if dest.exists() {
            let _ = std::fs::remove_file(entry.path());
            continue;
        }
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => moved += 1,
            Err(_) => failed = true,
        }
    }
    if moved > 0 {
        tracing::info!("moved {moved} log file(s) into {}", target.display());
    }
    if !failed {
        let _ = std::fs::remove_dir(legacy);
    }
}

/// Adopt an older notes store into `~/notes`: its repository folders and the
/// flat `.md` notes at its root move over, and the app's own entries
/// (`settings.json`, `ui.json`, `logs/`, anything dot-prefixed) stay where
/// they are. A name collision keeps both sides and the legacy folder — user
/// content is never overwritten or deleted. Idempotent, so it runs on every
/// start.
pub fn adopt_notes_store(legacy: &Path) {
    adopt_notes_at(legacy, &notes_store_root());
}

/// The entries a store folder may hold that belong to the app, not the store.
const APP_LOCAL_NAMES: [&str; 3] = ["settings.json", "ui.json", "logs"];

fn adopt_notes_at(legacy: &Path, target: &Path) {
    if !legacy.is_dir() || legacy == target {
        return;
    }
    if std::fs::create_dir_all(target).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
    let mut moved = 0usize;
    let mut failed = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || APP_LOCAL_NAMES
                .iter()
                .any(|local| local.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        let is_note = entry.file_type().is_ok_and(|kind| kind.is_dir())
            || path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !is_note {
            continue;
        }
        let dest = target.join(entry.file_name());
        if dest.exists() {
            tracing::warn!(
                "keeping the legacy notes entry {}: {} already exists",
                path.display(),
                dest.display()
            );
            failed = true;
            continue;
        }
        match std::fs::rename(&path, &dest) {
            Ok(()) => moved += 1,
            Err(_) => failed = true,
        }
    }
    if moved > 0 {
        tracing::info!("moved {moved} note item(s) into {}", target.display());
    }
    if !failed {
        let _ = std::fs::remove_dir(legacy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_hangs_off_the_home_suite_root() {
        assert_eq!(app_dir("agent"), suite_root().join("agent"));
        assert_eq!(logs_dir(), suite_root().join("logs"));
        assert_eq!(notes_store_root(), home().join("notes"));
    }

    #[test]
    fn a_missing_target_takes_the_whole_legacy_folder() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        let target = dir.path().join("target");
        std::fs::create_dir_all(legacy.join("logs")).unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();

        adopt_legacy_dir(&legacy, &target);

        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            "{}"
        );
        assert!(target.join("logs").is_dir());
        assert!(!legacy.exists(), "the whole legacy folder moved over");
    }

    #[test]
    fn an_existing_target_merges_the_legacy_children() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(target.join("settings.json"), "current").unwrap();
        std::fs::write(legacy.join("settings.json"), "legacy").unwrap();
        std::fs::write(legacy.join("ui.json"), "{}").unwrap();

        adopt_legacy_dir(&legacy, &target);

        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            "current",
            "the new layout wins"
        );
        assert!(target.join("ui.json").is_file(), "a missing file moves in");
        assert!(!legacy.exists(), "the emptied legacy folder is retired");
    }

    #[test]
    fn a_folder_collision_keeps_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        let target = dir.path().join("target");
        std::fs::create_dir_all(target.join("assistant")).unwrap();
        std::fs::write(target.join("assistant").join("new.md"), "new").unwrap();
        std::fs::create_dir_all(legacy.join("assistant")).unwrap();
        std::fs::write(legacy.join("assistant").join("old.md"), "old").unwrap();

        adopt_legacy_dir(&legacy, &target);

        assert!(target.join("assistant").join("new.md").is_file());
        assert!(legacy.join("assistant").join("old.md").is_file());
        assert!(
            legacy.exists(),
            "the colliding folder keeps the legacy root"
        );
    }

    #[test]
    fn a_missing_legacy_folder_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        adopt_legacy_dir(&dir.path().join("nope"), &target);
        assert!(!target.exists());
    }

    #[test]
    fn legacy_logs_move_over_and_a_name_collision_keeps_the_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy-logs");
        let target = dir.path().join("logs");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(legacy.join("warpagent-.log.2026-11-04"), "old").unwrap();
        std::fs::write(legacy.join("warpagent-.log.2026-11-05"), "stale").unwrap();
        std::fs::write(target.join("warpagent-.log.2026-11-05"), "new").unwrap();

        adopt_log_files(&legacy, &target);

        assert_eq!(
            std::fs::read_to_string(target.join("warpagent-.log.2026-11-04")).unwrap(),
            "old"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("warpagent-.log.2026-11-05")).unwrap(),
            "new",
            "the running app's file wins"
        );
        assert!(!legacy.exists(), "the emptied legacy folder is retired");
    }

    #[test]
    fn the_notes_store_adopts_folders_and_md_but_leaves_app_files() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("old-store");
        let target = dir.path().join("notes");
        std::fs::create_dir_all(legacy.join("alpha")).unwrap();
        std::fs::write(legacy.join("alpha").join("note.md"), "# alpha").unwrap();
        std::fs::write(legacy.join("flat.md"), "# flat").unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();
        std::fs::create_dir_all(legacy.join("logs")).unwrap();

        adopt_notes_at(&legacy, &target);

        assert!(target.join("alpha").join("note.md").is_file());
        assert!(target.join("flat.md").is_file());
        assert!(legacy.join("settings.json").is_file(), "app files stay put");
        assert!(legacy.join("logs").is_dir());
        assert!(legacy.exists(), "the app folder is not retired");
    }

    #[test]
    fn a_note_name_collision_keeps_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("old-store");
        let target = dir.path().join("notes");
        std::fs::create_dir_all(legacy.join("alpha")).unwrap();
        std::fs::create_dir_all(target.join("alpha")).unwrap();
        std::fs::write(legacy.join("alpha").join("old.md"), "old").unwrap();
        std::fs::write(target.join("alpha").join("new.md"), "new").unwrap();

        adopt_notes_at(&legacy, &target);

        assert!(legacy.join("alpha").join("old.md").is_file());
        assert!(target.join("alpha").join("new.md").is_file());
        assert!(legacy.exists(), "a collision keeps the legacy store");
    }
}
