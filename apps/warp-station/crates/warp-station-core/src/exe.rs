//! Executable resolution for GUI-spawned CLIs: the GUI process runs with a
//! minimal PATH (no shell rc files), so a bare name must also be looked up in
//! the user-level install dirs. Results memoized per session.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// User-level install dirs probed after PATH (the GUI-minimal-PATH lookup):
/// the home-layout dirs, plus on Windows the per-user install roots — npm -g
/// puts opencode.cmd in %APPDATA%\npm, never on the GUI's PATH.
fn user_install_dirs() -> Vec<PathBuf> {
    let home = crate::paths::home();
    #[cfg(windows)]
    let extra: Vec<PathBuf> = {
        let appdata = std::env::var_os("APPDATA").unwrap_or_default();
        let local = std::env::var_os("LOCALAPPDATA").unwrap_or_default();
        vec![
            PathBuf::from(appdata).join("npm"),
            PathBuf::from(&local).join(r"Microsoft\WindowsApps"),
            PathBuf::from(&local).join("Programs"),
            home.join("scoop").join("shims"),
        ]
    };
    #[cfg(not(windows))]
    let extra: Vec<PathBuf> = Vec::new();
    extra
        .into_iter()
        .chain(
            [
                ".local/bin",
                ".opencode/bin",
                ".cargo/bin",
                "bin",
                ".deno/bin",
                ".nvm/versions/node/current/bin",
            ]
            .iter()
            .map(|dir| home.join(dir)),
        )
        .collect()
}

static CACHE: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);

/// Resolves an executable name or path to something spawnable: absolute paths
/// verify existence; bare names scan PATH, then the user dirs (exec bit on
/// Unix, PATHEXT suffixes on Windows). `None` when nothing resolves.
pub fn locate(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let name = name.trim();
    if name.contains('/') || name.contains('\\') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let mut cache = CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(name) {
        return hit.clone();
    }
    let resolved = resolve_on_path(name)
        .or_else(|| {
            user_install_dirs()
                .into_iter()
                .find_map(|dir| resolve_on_path(dir.join(name).to_string_lossy().as_ref()))
        })
        .inspect(|p| {
            tracing::debug!("resolved {name} -> {}", p.display());
        });
    if resolved.is_none() {
        tracing::debug!("'{name}' not found on PATH or in the user install dirs");
    }
    cache.insert(name.to_string(), resolved.clone());
    resolved
}

#[cfg(unix)]
fn resolve_on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let full = dir.join(name);
        if let Ok(metadata) = std::fs::metadata(&full)
            && metadata.is_file()
            && metadata.permissions().mode() & 0o111 != 0
        {
            return Some(full);
        }
    }
    None
}

/// Windows has no exec bit; executables are found by their PATHEXT suffix.
/// A bare extensionless match is rejected: npm-style dirs hold POSIX shim
/// scripts next to the real `.cmd`/`.exe`, and spawning those fails with
/// "is not a valid Win32 application" (os error 193).
#[cfg(windows)]
fn resolve_on_path(name: &str) -> Option<PathBuf> {
    use std::path::Path;
    let path_env = std::env::var_os("PATH")?;
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let bare_ok = Path::new(name).extension().is_some();
    for dir in std::env::split_paths(&path_env) {
        let mut candidates = Vec::new();
        if bare_ok {
            candidates.push(dir.join(name));
        }
        candidates.extend(exts.split(';').map(|ext| dir.join(format!("{name}{ext}"))));
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_common_binary() {
        // /bin/echo or similar is everywhere these tests run
        let resolved = locate("sh").or_else(|| locate("bash"));
        assert!(resolved.is_some(), "sh/bash should resolve somewhere");
    }

    #[test]
    fn missing_binary_is_none() {
        assert!(locate("warp-station-definitely-missing-binary").is_none());
    }
}
