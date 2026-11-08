//! The tracked-repo store: file-backed repo list (repos.cache.json —
//! PascalCase layout read with `default`, so unknown fields are ignored on
//! read and missing keys fall back to defaults, keeping older cache files
//! loadable) plus the tag-merge rules that carry user tags across rescans.

use crate::jsonio;
use crate::paths::AppPaths;
use crate::scan::{self, ScannedRepo};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const FAVORITES_TAG: &str = "favorites";

/// Marks a repo as archived: it leaves the table and every background pass
/// until restored. A plain tag, so cache persistence and merge_scan survival
/// come free (no new persisted field).
pub const ARCHIVED_TAG: &str = "archived";

/// What the cache file serializes per repo. On-disk files from older builds
/// carry dozens of runtime/computed fields; unknown fields are ignored on
/// read and not rewritten, and missing keys deserialize as defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct CachedRepo {
    pub name: String,
    pub folder_path: String,
    #[serde(rename = "SolutionPath")]
    pub solution_path: Option<String>,
    pub tags: Vec<CachedTag>,
    /// The last known row status, so a launch paints the counts the previous
    /// session left instead of a blank row until the scan's probe lands. Not
    /// a freshness claim: the probe replaces these moments later.
    pub git_branch_name: Option<String>,
    pub git_modified_count: u32,
    pub git_to_push_count: u32,
    pub git_to_pull_count: u32,
    pub git_has_remote: bool,
    pub git_branch_published: bool,
    pub git_status_loaded: bool,
    /// The last GitHub pass's counts, restored alongside the git status.
    pub github_pr_count: u32,
    pub github_issue_count: u32,
    /// Committer time of HEAD at the last probe (instant "2w ago" labels on
    /// startup before the status pass lands).
    pub git_last_commit_at: Option<String>,
    pub git_last_fetch_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CachedTag {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct RepoCacheFile {
    pub repos: Vec<CachedRepo>,
}

/// The live repo record the app store holds: the persistent identity plus the
/// runtime git status the background pass fills in.
#[derive(Clone, Debug)]
pub struct RepoEntry {
    pub name: String,
    pub folder_path: PathBuf,
    pub solution_path: Option<String>,
    pub tags: Vec<String>,
    pub git_branch_name: Option<String>,
    pub git_modified_count: u32,
    pub git_to_push_count: u32,
    pub git_to_pull_count: u32,
    pub git_last_commit_at: Option<chrono::DateTime<chrono::Local>>,
    pub git_last_fetch_at: Option<chrono::DateTime<chrono::Local>>,
    pub git_has_remote: bool,
    pub git_branch_published: bool,
    pub git_status_loaded: bool,
    /// Open pull requests / issues from the GitHub pass (wave 4 fills these).
    pub github_pr_count: u32,
    pub github_issue_count: u32,
}

impl RepoEntry {
    pub fn is_favorite(&self) -> bool {
        self.tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(FAVORITES_TAG))
    }

    pub fn is_archived(&self) -> bool {
        self.tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(ARCHIVED_TAG))
    }
}

pub fn load_cache(path: &std::path::Path) -> Option<RepoCacheFile> {
    let json = jsonio::read_json(path)?;
    match serde_json::from_str::<RepoCacheFile>(&json) {
        Ok(cache) => Some(cache),
        Err(err) => {
            tracing::warn!("could not parse {}: {err}", path.display());
            None
        }
    }
}

pub fn save_cache(path: &std::path::Path, repos: &[RepoEntry]) -> anyhow::Result<()> {
    let file = RepoCacheFile {
        repos: repos.iter().map(cache_from_entry).collect(),
    };
    jsonio::write_json_atomically(path, &file)
}

fn cache_from_entry(repo: &RepoEntry) -> CachedRepo {
    CachedRepo {
        name: repo.name.clone(),
        folder_path: repo.folder_path.to_string_lossy().to_string(),
        solution_path: repo.solution_path.clone(),
        tags: repo
            .tags
            .iter()
            .map(|t| CachedTag { name: t.clone() })
            .collect(),
        git_branch_name: repo.git_branch_name.clone(),
        git_modified_count: repo.git_modified_count,
        git_to_push_count: repo.git_to_push_count,
        git_to_pull_count: repo.git_to_pull_count,
        git_has_remote: repo.git_has_remote,
        git_branch_published: repo.git_branch_published,
        git_status_loaded: repo.git_status_loaded,
        github_pr_count: repo.github_pr_count,
        github_issue_count: repo.github_issue_count,
        git_last_commit_at: repo.git_last_commit_at.map(|t| t.to_rfc3339()),
        git_last_fetch_at: repo.git_last_fetch_at.map(|t| t.to_rfc3339()),
    }
}

pub fn entry_from_cache(cached: &CachedRepo) -> RepoEntry {
    RepoEntry {
        name: cached.name.clone(),
        folder_path: PathBuf::from(&cached.folder_path),
        solution_path: cached.solution_path.clone(),
        tags: cached.tags.iter().map(|t| t.name.clone()).collect(),
        git_branch_name: cached.git_branch_name.clone(),
        git_modified_count: cached.git_modified_count,
        git_to_push_count: cached.git_to_push_count,
        git_to_pull_count: cached.git_to_pull_count,
        git_last_commit_at: cached
            .git_last_commit_at
            .clone()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|t| t.with_timezone(&chrono::Local)),
        git_last_fetch_at: cached
            .git_last_fetch_at
            .clone()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|t| t.with_timezone(&chrono::Local)),
        git_has_remote: cached.git_has_remote,
        git_branch_published: cached.git_branch_published,
        git_status_loaded: cached.git_status_loaded,
        github_pr_count: cached.github_pr_count,
        github_issue_count: cached.github_issue_count,
    }
}

/// Merges a fresh scan into the live list by folder path: user tags carry over
/// (except the auto-computed platform tag, which the scan owns), and the
/// last-known git status survives so a rescan of an unchanged repo does not
/// flip its card back to the checking placeholder. Returns the merged list
/// plus which repos vanished from disk.
pub fn merge_scan(
    previous: &[RepoEntry],
    scanned: &[ScannedRepo],
) -> (Vec<RepoEntry>, Vec<String>) {
    let mut merged = Vec::with_capacity(scanned.len());
    for repo in scanned {
        let prev = previous.iter().find(|p| {
            crate::paths::same_folder_text(
                &p.folder_path.to_string_lossy(),
                &repo.folder_path.to_string_lossy(),
            )
        });
        let mut entry = RepoEntry {
            name: repo.name.clone(),
            folder_path: repo.folder_path.clone(),
            solution_path: repo.solution_path.clone(),
            tags: repo.tags.clone(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: false,
            github_pr_count: 0,
            github_issue_count: 0,
        };
        if let Some(prev) = prev {
            for tag in &prev.tags {
                if tag.eq_ignore_ascii_case(scan::PLATFORM_TAG) {
                    continue;
                }
                if !entry.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    entry.tags.push(tag.clone());
                }
            }
            entry.git_branch_name = prev.git_branch_name.clone();
            entry.git_modified_count = prev.git_modified_count;
            entry.git_to_push_count = prev.git_to_push_count;
            entry.git_to_pull_count = prev.git_to_pull_count;
            entry.git_last_commit_at = prev.git_last_commit_at;
            entry.git_last_fetch_at = prev.git_last_fetch_at;
            entry.git_has_remote = prev.git_has_remote;
            entry.git_branch_published = prev.git_branch_published;
            entry.git_status_loaded = prev.git_status_loaded;
            entry.github_pr_count = prev.github_pr_count;
            entry.github_issue_count = prev.github_issue_count;
        }
        merged.push(entry);
    }
    let removed: Vec<String> = previous
        .iter()
        .filter(|p| {
            !scanned.iter().any(|s| {
                crate::paths::same_folder_text(
                    &s.folder_path.to_string_lossy(),
                    &p.folder_path.to_string_lossy(),
                )
            })
        })
        .map(|p| p.name.clone())
        .collect();
    (merged, removed)
}

/// One repo's status probe result (the fill for [`RepoEntry`]'s git fields).
#[derive(Clone, Debug, Default)]
pub struct StatusProbe {
    pub folder_path: String,
    pub branch_name: Option<String>,
    pub modified_count: u32,
    pub ahead_count: u32,
    pub behind_count: u32,
    pub last_commit_at: Option<chrono::DateTime<chrono::Local>>,
    pub last_fetch_at: Option<chrono::DateTime<chrono::Local>>,
    pub has_remote: bool,
    pub branch_on_remote: bool,
}

/// Applies a probe onto an entry; the fetch timestamp is newer-wins (a
/// FETCH_HEAD touched by an external fetch since the last probe must beat the
/// carried value).
pub fn apply_status_probe(entry: &mut RepoEntry, probe: &StatusProbe) {
    entry.git_branch_name = probe.branch_name.clone();
    entry.git_modified_count = probe.modified_count;
    entry.git_to_push_count = probe.ahead_count;
    entry.git_to_pull_count = probe.behind_count;
    entry.git_last_commit_at = probe.last_commit_at;
    entry.git_has_remote = probe.has_remote;
    entry.git_branch_published = probe.branch_on_remote;
    if let Some(fetch_at) = probe.last_fetch_at
        && entry
            .git_last_fetch_at
            .is_none_or(|existing| fetch_at > existing)
    {
        entry.git_last_fetch_at = Some(fetch_at);
    }
    entry.git_status_loaded = true;
}

/// `.git/FETCH_HEAD`'s last write time — an honest fetch age for repos fetched
/// outside the app; `None` when unreadable.
pub fn read_last_fetch_seed(folder: &std::path::Path) -> Option<chrono::DateTime<chrono::Local>> {
    let fetch_head = folder.join(".git").join("FETCH_HEAD");
    let meta = std::fs::metadata(&fetch_head).ok()?;
    let modified = meta.modified().ok()?;
    Some(chrono::DateTime::<chrono::Local>::from(modified))
}

pub fn cache_path() -> PathBuf {
    AppPaths::repos_cache_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_carries_user_tags_and_status_but_not_platform() {
        let prev = RepoEntry {
            name: "old-name".into(),
            folder_path: PathBuf::from("/x/repo"),
            solution_path: None,
            tags: vec!["favorites".into(), "platform".into(), "work".into()],
            git_branch_name: Some("main".into()),
            git_modified_count: 3,
            git_to_push_count: 1,
            git_to_pull_count: 2,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: true,
            git_branch_published: false,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
        };
        let scanned = ScannedRepo {
            name: "repo".into(),
            folder_path: PathBuf::from("/x/repo"),
            solution_path: None,
            tags: vec!["platform".into()],
        };
        let (merged, removed) = merge_scan(&[prev], &[scanned]);
        assert!(removed.is_empty());
        assert_eq!(
            merged[0].tags,
            vec![
                "platform".to_string(),
                "favorites".to_string(),
                "work".to_string()
            ]
        );
        assert_eq!(merged[0].git_branch_name.as_deref(), Some("main"));
        assert_eq!(merged[0].git_modified_count, 3);
        assert!(merged[0].is_favorite());
    }

    #[test]
    fn apply_probe_newer_fetch_wins() {
        let mut entry = RepoEntry {
            name: "r".into(),
            folder_path: PathBuf::from("/x/r"),
            solution_path: None,
            tags: vec![],
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: Some(chrono::Local::now() - chrono::Duration::hours(5)),
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: false,
            github_pr_count: 0,
            github_issue_count: 0,
        };
        let newer = chrono::Local::now() - chrono::Duration::hours(1);
        apply_status_probe(
            &mut entry,
            &StatusProbe {
                folder_path: "/x/r".into(),
                last_fetch_at: Some(newer),
                ..Default::default()
            },
        );
        assert_eq!(entry.git_last_fetch_at, Some(newer));
    }

    #[test]
    fn cache_round_trip_ignores_extra_fields() {
        let json = r#"{"Repos":[{"Name":"r","FolderPath":"/x/r","SolutionPath":"/x/r/a.sln",
            "Tags":[{"Name":"favorites"}],"GitBranchName":"main","GitModifiedCount":7,
            "IsFavorite":true,"FavoriteTooltip":"x","GitLastActivity":"2w ago"}]}"#;
        let file: RepoCacheFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.repos.len(), 1);
        let entry = entry_from_cache(&file.repos[0]);
        assert_eq!(entry.name, "r");
        assert!(entry.is_favorite());
        let out = serde_json::to_string(&RepoCacheFile {
            repos: vec![cache_from_entry(&entry)],
        })
        .unwrap();
        assert!(out.contains("\"FolderPath\""));
    }

    /// The cache carries the last known row status so a launch paints the
    /// counts the previous session left; an older file without them loads
    /// blank (and unprobed) instead of failing.
    #[test]
    fn cache_round_trips_the_last_known_counts() {
        let json = r#"{"Repos":[{"Name":"r","FolderPath":"/x/r",
            "Tags":[{"Name":"favorites"}],"GitBranchName":"main",
            "GitModifiedCount":7,"GitToPushCount":2,"GitToPullCount":1,
            "GitHasRemote":true,"GitBranchPublished":true,"GitStatusLoaded":true,
            "GithubPrCount":3,"GithubIssueCount":4}]}"#;
        let file: RepoCacheFile = serde_json::from_str(json).unwrap();
        let entry = entry_from_cache(&file.repos[0]);
        assert_eq!(entry.git_branch_name.as_deref(), Some("main"));
        assert_eq!(entry.git_modified_count, 7);
        assert_eq!(entry.git_to_push_count, 2);
        assert_eq!(entry.git_to_pull_count, 1);
        assert!(entry.git_has_remote);
        assert!(entry.git_branch_published);
        assert!(entry.git_status_loaded);
        assert_eq!(entry.github_pr_count, 3);
        assert_eq!(entry.github_issue_count, 4);

        // a save writes the same shape back
        let out = serde_json::to_string(&RepoCacheFile {
            repos: vec![cache_from_entry(&entry)],
        })
        .unwrap();
        let back: RepoCacheFile = serde_json::from_str(&out).unwrap();
        let back = entry_from_cache(&back.repos[0]);
        assert_eq!(back.git_modified_count, 7);
        assert_eq!(back.github_pr_count, 3);
        assert_eq!(back.github_issue_count, 4);
        assert!(back.git_status_loaded);
    }

    #[test]
    fn an_older_cache_without_counts_loads_blank() {
        let file: RepoCacheFile =
            serde_json::from_str(r#"{"Repos":[{"Name":"r","FolderPath":"/x/r"}]}"#).unwrap();
        let entry = entry_from_cache(&file.repos[0]);
        assert_eq!(entry.git_modified_count, 0);
        assert_eq!(entry.github_pr_count, 0);
        assert_eq!(entry.github_issue_count, 0);
        assert!(!entry.git_status_loaded);
    }
}
