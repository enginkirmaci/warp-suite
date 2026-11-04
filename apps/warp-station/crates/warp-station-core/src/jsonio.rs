//! Shared JSON plumbing: indented writes and the atomic replace, delegated to
//! the platform kit (unique temp file + rename in the target directory, so a
//! crash mid-write never leaves a truncated config behind).

use std::path::Path;

pub use warpui_kit_platform::io::write_atomically;

pub fn read_json(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Pretty-serialize `value` and replace `path` atomically.
pub fn write_json_atomically<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    warpui_kit_platform::io::write_json(path, value)
}
