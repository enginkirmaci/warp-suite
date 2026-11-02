//! Repo discovery: walks configured folders — direct children only for the
//! main listing, deeper for the Add-Repositories dialog — honoring leaf-name
//! exclusions, and auto-tags repos whose path contains the platform folder
//! name.

use crate::settings::ReposSettings;
use std::path::{Path, PathBuf};

/// The auto-tag for repos under the configured platform folder. Recomputed on
/// every scan, so it is excluded from the user-tag carry-over in the store.
pub const PLATFORM_TAG: &str = "platform";

#[derive(Clone, Debug)]
pub struct ScannedRepo {
    pub name: String,
    pub folder_path: PathBuf,
    pub solution_path: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RepoScanResult {
    pub repos: Vec<ScannedRepo>,
}

/// Scoped scan for the Add-Repositories dialog: `root` wins over the
/// configured listing roots, so typing a folder actually searches that folder
/// instead of silently re-scanning the whole settings list.
pub fn scan_folder(
    settings: &ReposSettings,
    root: &Path,
    max_depth: Option<i32>,
) -> RepoScanResult {
    let mut scoped = settings.clone();
    scoped.repo_scan_folders = Some(vec![root.to_string_lossy().to_string()]);
    scan(&scoped, max_depth)
}

/// One scan pass. `max_depth: None` = the listing scan (each scan root's
/// direct children only); the Add-Repositories dialog passes its configured
/// depth. Distinct by folder path, name-sorted.
pub fn scan(settings: &ReposSettings, max_depth: Option<i32>) -> RepoScanResult {
    let roots = settings.repo_scan_folders.clone().unwrap_or_default();
    let git_pattern = if settings.git_folder_pattern.is_empty() {
        ".git".to_string()
    } else {
        settings.git_folder_pattern.clone()
    };
    let platform = if settings.platform_folder_name.is_empty() {
        PLATFORM_TAG.to_string()
    } else {
        settings.platform_folder_name.clone()
    };
    let sln_patterns = parse_solution_patterns(&settings.solution_file_pattern);
    let excluded: Vec<String> = settings.excluded_folders.clone().unwrap_or_default();
    let effective_depth = max_depth.unwrap_or(1).max(1);

    let mut repos: Vec<ScannedRepo> = Vec::new();
    for root in &roots {
        let root_path = Path::new(root);
        if !root_path.is_dir() {
            continue;
        }
        for dir in find_git_dirs(root_path, &git_pattern, 1, effective_depth, &excluded) {
            let Some(parent) = dir.parent() else { continue };
            let solution = first_solution_file(parent, &sln_patterns);
            let path_str = parent.to_string_lossy().to_string();
            let mut tags = Vec::new();
            if path_str.to_lowercase().contains(&platform.to_lowercase()) {
                tags.push(PLATFORM_TAG.to_string());
            }
            repos.push(ScannedRepo {
                name: parent
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                folder_path: parent.to_path_buf(),
                solution_path: solution,
                tags,
            });
        }
    }

    repos.sort_by(|a, b| a.name.cmp(&b.name));
    repos.dedup_by(|a, b| {
        crate::paths::same_folder_text(
            &a.folder_path.to_string_lossy(),
            &b.folder_path.to_string_lossy(),
        )
    });
    RepoScanResult { repos }
}

/// Splits the solution pattern setting into individual patterns (comma- or
/// semicolon-separated); falls back to "*.sln".
fn parse_solution_patterns(pattern: &str) -> Vec<String> {
    let mut patterns: Vec<String> = pattern
        .split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if patterns.is_empty() {
        patterns.push("*.sln".into());
    }
    patterns
}

fn first_solution_file(dir: &Path, patterns: &[String]) -> Option<String> {
    let mut found: Vec<String> = Vec::new();
    for pattern in patterns {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if matches_pattern(&name, pattern) {
                    found.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
    }
    found.sort();
    found.into_iter().next()
}

/// Case-insensitive glob with a single '*' wildcard — the shape the settings
/// actually use ("*.sln", "*.slnx", "*.git").
fn matches_pattern(name: &str, pattern: &str) -> bool {
    let (name, pattern) = (name.to_lowercase(), pattern.to_lowercase());
    match pattern.split_once('*') {
        None => name == pattern,
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}

fn find_git_dirs(
    root: &Path,
    git_pattern: &str,
    depth: i32,
    max_depth: i32,
    excluded: &[String],
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if matches_pattern(&name, git_pattern) {
            found.push(entry.path());
        } else if !excluded.iter().any(|ex| ex.eq_ignore_ascii_case(&name)) {
            subdirs.push(entry.path());
        }
    }
    if depth < max_depth {
        for sub in subdirs {
            found.extend(find_git_dirs(
                &sub,
                git_pattern,
                depth + 1,
                max_depth,
                excluded,
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(roots: Vec<String>) -> ReposSettings {
        ReposSettings {
            repo_scan_folders: Some(roots),
            ..Default::default()
        }
    }

    #[test]
    fn finds_direct_children_only_by_default() {
        // the real settings point scan folders AT repos — .git is a direct child
        let tmp = std::env::temp_dir().join(format!("dt-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("repo-a/.git")).unwrap();
        std::fs::create_dir_all(tmp.join("repo-a/deep/.git")).unwrap();
        std::fs::write(tmp.join("repo-a/a.slnx"), "").unwrap();

        let result = scan(
            &settings(vec![tmp.join("repo-a").to_string_lossy().to_string()]),
            None,
        );
        assert_eq!(result.repos.len(), 1);
        assert_eq!(result.repos[0].name, "repo-a");
        assert_eq!(
            result.repos[0].solution_path.as_deref(),
            tmp.join("repo-a").join("a.slnx").to_str()
        );
        assert!(result.repos[0].tags.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn deep_scan_and_exclusions() {
        let tmp = std::env::temp_dir().join(format!("dt-scan-deep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("group/repo-b/.git")).unwrap();
        std::fs::create_dir_all(tmp.join("group/vendor/repo-c/.git")).unwrap();
        std::fs::create_dir_all(tmp.join("skip/repo-d/.git")).unwrap();

        let mut s = settings(vec![tmp.to_string_lossy().to_string()]);
        s.excluded_folders = Some(vec!["skip".into()]);
        let result = scan(&s, Some(4));
        let names: Vec<&str> = result.repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["repo-b", "repo-c"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn platform_folder_autotags() {
        let tmp = std::env::temp_dir().join("dt-scan-plat");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("platform/repo-p/.git")).unwrap();
        let result = scan(&settings(vec![tmp.to_string_lossy().to_string()]), Some(3));
        assert_eq!(result.repos[0].tags, vec![PLATFORM_TAG.to_string()]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dialog_scan_is_scoped_to_the_typed_folder() {
        let tmp = std::env::temp_dir().join(format!("dt-scan-scoped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("typed/repo-typed/.git")).unwrap();
        std::fs::create_dir_all(tmp.join("elsewhere/repo-elsewhere/.git")).unwrap();

        let configured = settings(vec![tmp.join("elsewhere").to_string_lossy().to_string()]);
        let result = scan_folder(&configured, &tmp.join("typed"), Some(3));
        let names: Vec<&str> = result.repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["repo-typed"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
