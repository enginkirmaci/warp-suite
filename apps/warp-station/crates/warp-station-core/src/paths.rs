//! Global per-user locations. Every app's data lives under `~/.warp` — one
//! shared suite root carrying each app's own folder, the kit's `ui.json` and
//! the SnapIt settings; the notes store is user content in `~/notes`. Never
//! relocate a folder without a startup migration (see [`migrate_legacy_data`]
//! and [`migrate_suite_layout`]).

use std::path::{Path, PathBuf};
use warpui_kit_platform::paths as suite;

pub struct AppPaths;

impl AppPaths {
    /// The shared suite root, `~/.warp`.
    pub fn user_data_root() -> PathBuf {
        suite::suite_root()
    }
    /// This app's own folder, `~/.warp/station`.
    pub fn app_dir() -> PathBuf {
        suite::app_dir("station")
    }
    /// The one logs folder every app writes into, `~/.warp/logs`.
    pub fn logs_dir() -> PathBuf {
        suite::logs_dir()
    }
    pub fn settings_file() -> PathBuf {
        Self::app_dir().join("settings.json")
    }
    pub fn repos_cache_file() -> PathBuf {
        Self::app_dir().join("repos.cache.json")
    }
    pub fn notes_dir() -> PathBuf {
        suite::notes_store_root()
    }
    /// The notes folder older builds used, `~/.warp/notes` (the notes app's
    /// own folder now). Only the migration needs it.
    pub fn legacy_notes_dir() -> PathBuf {
        Self::user_data_root().join("notes")
    }
    /// The assistant folder: one user-editable prompt template per wand
    /// (commit message, README, daily summary). General to the AI surface —
    /// it belongs to neither the OpenCode CLI nor warp-agent, so its name
    /// carries neither.
    pub fn assistant_dir() -> PathBuf {
        Self::app_dir().join("assistant")
    }
    /// The model catalog cache, beside the repo cache — both are throwaway
    /// catalogs the app rewrites from its source (the door, the repo tree).
    pub fn models_cache_file() -> PathBuf {
        Self::app_dir().join("models.cache.json")
    }
}

/// One-time layout compatibility: the prompt templates and the catalog cache
/// used to share one folder named `opencode` (and, briefly, `ai`) before the
/// AI surface moved to warp-agent. Startup splits it into
/// [`AppPaths::assistant_dir`] and [`AppPaths::models_cache_file`]; files
/// already in the new layout always win, and a missing old folder is a no-op.
pub fn migrate_legacy_ai_data() {
    migrate_legacy_ai_data_at(&AppPaths::user_data_root());
}

/// One-time root compatibility: the app's data lived under `~/.devtools`
/// before the shared `~/.warp` suite root. A missing target renames the
/// whole legacy root over; an existing target (the notes store, `ui.json`)
/// moves the legacy children over one by one — entries already in the new
/// layout always win, and a missing legacy root is a no-op.
pub fn migrate_legacy_data() {
    migrate_legacy_data_at(&AppPaths::user_data_root(), &home().join(".devtools"));
}

/// The per-app layout this build writes, adopted on every start: this app's
/// own files move from the `~/.warp` root into `~/.warp/station`, and note
/// content an older build kept in `~/.warp/notes` moves to the store at
/// `~/notes`. Both steps are idempotent, so they run before every launch.
pub fn migrate_suite_layout() {
    migrate_app_entries_at(&AppPaths::user_data_root(), &AppPaths::app_dir());
    if suite::notes_store_adopted() {
        return;
    }
    // ~/.warp/notes is the notes app's data folder now, not a store: scan it
    // only until the one-shot migration marker exists (audit C2). The marker
    // is the notes app's to write, because ~/.warp-notes may still hold its
    // pre-suite store; claiming the migration done here would strand those
    // notes in the app folder.
    let adopted = suite::adopt_notes_store(&AppPaths::legacy_notes_dir());
    let legacy_app = home().join(".warp-notes");
    let legacy_devtools = home().join(".devtools").join("notes");
    if adopted && !legacy_app.exists() && !legacy_devtools.exists() {
        suite::mark_notes_store_adopted();
    }
}

/// The entries the root used to carry for this app; only they move into
/// [`AppPaths::app_dir`] — `ui.json`, `notes/`, `snapit/` and `logs/` stay
/// where the suite keeps them.
const ROOT_APP_ENTRIES: [&str; 4] = [
    "settings.json",
    "repos.cache.json",
    "models.cache.json",
    "assistant",
];

/// The testable body of [migrate_suite_layout]'s first half: `root` is the
/// suite root and `app` this app's folder. A folder collision keeps both
/// sides for a manual pass; a superseded file is retired.
fn migrate_app_entries_at(root: &Path, app: &Path) {
    if !root.is_dir() {
        return;
    }
    let mut moved = 0usize;
    for name in ROOT_APP_ENTRIES {
        let source = root.join(name);
        if !source.exists() {
            continue;
        }
        let dest = app.join(name);
        if dest.exists() {
            if source.is_dir() {
                tracing::warn!(
                    "keeping the legacy {}: {} already exists",
                    source.display(),
                    dest.display()
                );
                continue;
            }
            let _ = std::fs::remove_file(&source);
            continue;
        }
        if std::fs::create_dir_all(app).is_err() {
            continue;
        }
        if std::fs::rename(&source, &dest).is_ok() {
            moved += 1;
        }
    }
    if moved > 0 {
        tracing::info!("moved {moved} item(s) into {}", app.display());
    }
}

/// The testable body of [migrate_legacy_data]; `root` is the data root and
/// `legacy` the old one.
fn migrate_legacy_data_at(root: &Path, legacy: &Path) {
    if !legacy.is_dir() || root == legacy {
        return;
    }
    if !root.exists() {
        match std::fs::rename(legacy, root) {
            Ok(()) => tracing::info!(
                "migrated the data root {} → {}",
                legacy.display(),
                root.display()
            ),
            Err(err) => tracing::warn!(
                "could not migrate the data root {}: {err}",
                legacy.display()
            ),
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
        let dest = root.join(entry.file_name());
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
        tracing::info!("moved {moved} item(s) into {}", root.display());
    }
    if !failed {
        // remove_dir only succeeds once the folder is empty: a leftover
        // keeps the legacy root for the next pass
        let _ = std::fs::remove_dir(legacy);
    }
}

/// The testable body of [migrate_legacy_ai_data]; `root` is the data root.
fn migrate_legacy_ai_data_at(root: &Path) {
    // `opencode` is the released name; `ai` was an intermediate one that never
    // shipped, migrated too so a stray local folder is cleaned up
    for name in ["opencode", "ai"] {
        migrate_legacy_dir(root, &root.join(name));
    }
}

/// Splits one legacy AI folder into the current layout and retires it.
fn migrate_legacy_dir(root: &Path, old: &Path) {
    if !old.is_dir() {
        return;
    }
    let assistant = root.join("assistant");
    let cache = root.join("models.cache.json");
    let Ok(entries) = std::fs::read_dir(old) else {
        return;
    };
    let mut copied = 0usize;
    let mut failed = false;
    for entry in entries.flatten() {
        let source = entry.path();
        if !source.is_file() {
            continue;
        }
        // the catalog cache goes to the data root, everything else is a
        // prompt template for the assistant folder
        let dest = if entry.file_name() == "models.cache.json" {
            cache.clone()
        } else {
            if !assistant.exists() {
                let _ = std::fs::create_dir_all(&assistant);
            }
            assistant.join(entry.file_name())
        };
        if dest.exists() {
            // the new layout wins: the legacy copy is superseded, not needed
            let _ = std::fs::remove_file(&source);
            continue;
        }
        match std::fs::copy(&source, &dest) {
            Ok(_) => {
                copied += 1;
                // retire the source once the new copy exists, so the emptied
                // legacy folder can go too
                let _ = std::fs::remove_file(&source);
            }
            Err(_) => failed = true,
        }
    }
    if copied > 0 {
        tracing::info!("migrated {copied} file(s) out of {}", old.display());
    }
    if !failed {
        // remove_dir only succeeds once the folder is empty: a leftover
        // (unreadable file, stray subfolder) keeps it for the next pass
        let _ = std::fs::remove_dir(old);
    }
}

/// $HOME on unix; USERPROFILE first on Windows — a POSIX-style $HOME leaked
/// from a Git Bash parent shell is not a valid Win32 path.
pub(crate) fn home() -> PathBuf {
    suite::home()
}

/// Folder-text identity: case-insensitive on Windows (the filesystem is),
/// exact on case-sensitive platforms.
pub fn same_folder_text(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    {
        a.eq_ignore_ascii_case(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_root_and_the_app_folder_sit_under_home() {
        // a sibling test parks a scratch HOME while it seeds templates
        let _guard = crate::test_support::HOME_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let root = AppPaths::user_data_root();
        assert_eq!(root, home().join(".warp"));
        assert_eq!(AppPaths::app_dir(), root.join("station"));
        assert_eq!(AppPaths::logs_dir(), root.join("logs"));
        assert_eq!(
            AppPaths::settings_file(),
            AppPaths::app_dir().join("settings.json")
        );
        assert_eq!(
            AppPaths::repos_cache_file(),
            AppPaths::app_dir().join("repos.cache.json")
        );
        assert_eq!(
            AppPaths::assistant_dir(),
            AppPaths::app_dir().join("assistant")
        );
        assert_eq!(
            AppPaths::models_cache_file(),
            AppPaths::app_dir().join("models.cache.json")
        );
        assert_eq!(AppPaths::notes_dir(), home().join("notes"));
        assert_eq!(AppPaths::legacy_notes_dir(), root.join("notes"));
    }

    #[test]
    fn the_root_app_entries_move_into_the_app_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".warp");
        let app = root.join("station");
        std::fs::create_dir_all(root.join("assistant")).unwrap();
        std::fs::write(root.join("assistant").join("readme.md"), "body").unwrap();
        std::fs::write(root.join("settings.json"), "{}").unwrap();
        std::fs::write(root.join("ui.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();

        migrate_app_entries_at(&root, &app);

        assert_eq!(
            std::fs::read_to_string(app.join("settings.json")).unwrap(),
            "{}"
        );
        assert!(app.join("assistant").join("readme.md").is_file());
        assert!(!root.join("settings.json").exists());
        assert!(!root.join("assistant").exists());
        assert!(
            root.join("ui.json").is_file(),
            "the shared ui.json stays at the root"
        );
        assert!(
            root.join("notes").is_dir(),
            "the notes app's folder is not this app's to move"
        );
    }

    #[test]
    fn a_migrated_root_is_a_noop_on_the_next_pass() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".warp");
        let app = root.join("station");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("settings.json"), "{}").unwrap();

        migrate_app_entries_at(&root, &app);
        migrate_app_entries_at(&root, &app);

        assert!(app.join("settings.json").is_file());
        assert!(!root.join("settings.json").exists());
    }

    #[test]
    fn a_missing_warp_root_renames_the_legacy_root() {
        let home = tempfile::tempdir().unwrap();
        let legacy = home.path().join(".devtools");
        std::fs::create_dir_all(legacy.join("logs")).unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();

        migrate_legacy_data_at(&home.path().join(".warp"), &legacy);

        let root = home.path().join(".warp");
        assert_eq!(
            std::fs::read_to_string(root.join("settings.json")).unwrap(),
            "{}"
        );
        assert!(root.join("logs").is_dir());
        assert!(!legacy.exists(), "the whole legacy root moved over");
    }

    #[test]
    fn an_existing_warp_root_merges_the_legacy_children() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".warp");
        let legacy = home.path().join(".devtools");
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(root.join("ui.json"), "{}").unwrap();
        std::fs::create_dir_all(legacy.join("snapit")).unwrap();
        std::fs::write(legacy.join("snapit").join("settings.json"), "{}").unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();

        migrate_legacy_data_at(&root, &legacy);

        assert!(root.join("settings.json").is_file());
        assert!(root.join("snapit").join("settings.json").is_file());
        assert_eq!(std::fs::read_to_string(root.join("ui.json")).unwrap(), "{}");
        assert!(root.join("notes").is_dir());
        assert!(!legacy.exists(), "the emptied legacy root is removed");
    }

    #[test]
    fn existing_warp_files_are_never_overwritten_by_the_legacy_root() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".warp");
        let legacy = home.path().join(".devtools");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("settings.json"), "current").unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.json"), "legacy").unwrap();

        migrate_legacy_data_at(&root, &legacy);

        assert_eq!(
            std::fs::read_to_string(root.join("settings.json")).unwrap(),
            "current"
        );
        assert!(!legacy.exists(), "the superseded legacy copy is retired");
    }

    #[test]
    fn a_folder_collision_keeps_both_sides_and_the_legacy_root() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".warp");
        let legacy = home.path().join(".devtools");
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(root.join("notes").join("live.md"), "live").unwrap();
        std::fs::create_dir_all(legacy.join("notes")).unwrap();
        std::fs::write(legacy.join("notes").join("old.md"), "old").unwrap();

        migrate_legacy_data_at(&root, &legacy);

        assert_eq!(
            std::fs::read_to_string(root.join("notes").join("live.md")).unwrap(),
            "live"
        );
        assert!(legacy.join("notes").join("old.md").is_file());
        assert!(
            legacy.exists(),
            "the colliding folder keeps the legacy root"
        );
    }

    #[test]
    fn a_missing_legacy_root_is_a_noop() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".warp");
        migrate_legacy_data_at(&root, &home.path().join(".devtools"));
        assert!(!root.exists());
    }

    #[test]
    fn legacy_opencode_dir_splits_into_assistant_and_cache() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("opencode")).unwrap();
        std::fs::write(
            root.path().join("opencode").join("readme.md"),
            "custom body",
        )
        .unwrap();
        std::fs::write(
            root.path().join("opencode").join("models.cache.json"),
            "[\"a/b\"]",
        )
        .unwrap();

        migrate_legacy_ai_data_at(root.path());

        assert_eq!(
            std::fs::read_to_string(root.path().join("assistant").join("readme.md")).unwrap(),
            "custom body"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("models.cache.json")).unwrap(),
            "[\"a/b\"]"
        );
        assert!(
            !root
                .path()
                .join("assistant")
                .join("models.cache.json")
                .exists(),
            "the cache never lands in the prompt folder"
        );
        assert!(
            !root.path().join("opencode").exists(),
            "the emptied legacy folder is removed"
        );
    }

    /// The never-released `ai` folder is split and retired too.
    #[test]
    fn an_intermediate_ai_folder_is_migrated_too() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("ai")).unwrap();
        std::fs::write(root.path().join("ai").join("readme.md"), "custom").unwrap();
        std::fs::write(
            root.path().join("ai").join("models.cache.json"),
            "[\"a/b\"]",
        )
        .unwrap();

        migrate_legacy_ai_data_at(root.path());

        assert_eq!(
            std::fs::read_to_string(root.path().join("assistant").join("readme.md")).unwrap(),
            "custom"
        );
        assert!(
            root.path().join("models.cache.json").exists(),
            "the cache reached the root"
        );
        assert!(
            !root.path().join("ai").exists(),
            "the intermediate folder is gone"
        );
    }

    #[test]
    fn files_already_in_the_new_layout_are_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("opencode")).unwrap();
        std::fs::write(root.path().join("opencode").join("readme.md"), "legacy").unwrap();
        std::fs::write(
            root.path().join("opencode").join("models.cache.json"),
            "[\"old\"]",
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("assistant")).unwrap();
        std::fs::write(root.path().join("assistant").join("readme.md"), "current").unwrap();
        std::fs::write(root.path().join("models.cache.json"), "[\"new\"]").unwrap();

        migrate_legacy_ai_data_at(root.path());

        assert_eq!(
            std::fs::read_to_string(root.path().join("assistant").join("readme.md")).unwrap(),
            "current"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("models.cache.json")).unwrap(),
            "[\"new\"]"
        );
    }

    /// An interrupted copy leaves a partial new layout: the next pass fills in
    /// the missing files without touching the ones already there.
    #[test]
    fn a_partial_layout_is_completed_from_the_legacy_folder() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("opencode")).unwrap();
        std::fs::write(root.path().join("opencode").join("readme.md"), "legacy").unwrap();
        std::fs::write(
            root.path().join("opencode").join("models.cache.json"),
            "[\"a/b\"]",
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("assistant")).unwrap();
        std::fs::write(root.path().join("assistant").join("readme.md"), "current").unwrap();

        migrate_legacy_ai_data_at(root.path());

        assert_eq!(
            std::fs::read_to_string(root.path().join("assistant").join("readme.md")).unwrap(),
            "current"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("models.cache.json")).unwrap(),
            "[\"a/b\"]"
        );
    }

    #[test]
    fn migration_without_legacy_folder_is_a_noop() {
        let root = tempfile::tempdir().unwrap();
        migrate_legacy_ai_data_at(root.path());
        assert!(!root.path().join("assistant").exists());
        assert!(!root.path().join("models.cache.json").exists());
    }
}
