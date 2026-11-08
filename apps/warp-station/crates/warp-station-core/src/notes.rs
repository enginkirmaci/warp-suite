//! The notes store's read side for the global search: per-repo note folders
//! under `<store root>/projects` (default store ~/notes, shared with the
//! warp-notes app), capped content search. The read/search layer itself is the platform
//! kit's (`warpui_kit_platform::notes`) — warp-notes rides the same code for
//! its page search; tree loading, CRUD and the markdown editing ops live
//! app-side in warp-notes.

use crate::paths::AppPaths;
use std::path::PathBuf;

pub use warpui_kit_platform::notes::{NotesSearchHit, repo_notes_root, search_capped};

pub fn resolve_store_root(configured: &str) -> PathBuf {
    warpui_kit_platform::notes::resolve_store_root(configured, &AppPaths::notes_dir())
}
