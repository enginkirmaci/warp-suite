//! The in-process git read layer behind the status service: status probes,
//! change snapshots, branches, diffs, history — one repository handle per
//! operation, no shared state, no index locks, so reads never interfere with
//! the user's own git operations. Every failure maps to None/empty results.

use crate::model::*;
use chrono::{DateTime, Local, TimeZone as _};
use git2::{Repository, Status, StatusOptions};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Read cap for one file's patch row (commit detail and the expandable rows).
pub const FILE_PATCH_READ_CAP: usize = 64 * 1024;

/// Read cap for a commit-detail message body.
pub const COMMIT_BODY_READ_CAP: usize = 16 * 1024;

/// Prompt cap for the commit-message wand's staged patch head.
pub const COMMIT_PROMPT_PATCH_CAP: usize = 8000;

/// Upper bound on commits examined for one day's activity.
const MAX_DAY_SCAN_COMMITS: usize = 2000;

/// Per-commit file entries fed to the daily-summary prompt.
const MAX_DAY_COMMIT_FILE_ENTRIES: usize = 40;

const PATCH_TRUNCATION_SUFFIX: &str = "\n… (patch truncated)";

/// One repo's status probe: branch, change count, ahead/behind vs the upstream
/// as of the last fetch, the last commit's committer time, whether any remote
/// is configured, and whether the current branch has a remote-tracking
/// counterpart (the publish affordance's gate).
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub branch_name: Option<String>,
    pub modified_count: u32,
    pub ahead_count: u32,
    pub behind_count: u32,
    pub last_commit_at: Option<DateTime<Local>>,
    pub has_remote: bool,
    pub branch_on_remote: bool,
}

pub fn probe(folder: &Path) -> Option<Probe> {
    with_repo(folder, |repo| {
        let mut opts = StatusOptions::new();
        status_options(&mut opts);
        let statuses = repo.statuses(Some(&mut opts)).ok()?;
        let modified = statuses.len() as u32;

        let head = repo.find_reference("HEAD").ok()?;
        // git2 returns Ok(None) for a direct (detached) HEAD; `is_err()`
        // would miss it and report the literal "HEAD"
        let detached = head.symbolic_target().ok().flatten().is_none();
        let resolved = head.resolve().ok()?;
        let branch_name = if detached {
            "(detached)".to_string()
        } else {
            resolved.shorthand().ok().unwrap_or("HEAD").to_string()
        };

        let has_remote = repo.remotes().map(|r| !r.is_empty()).unwrap_or(false);
        let branch_on_remote = has_remote
            && !detached
            && repo
                .branches(Some(git2::BranchType::Remote))
                .ok()
                .is_some_and(|mut branches| {
                    branches.any(|b| {
                        b.ok()
                            .and_then(|(b, _)| b.name().ok().flatten().map(str::to_string))
                            .is_some_and(|name| name.ends_with(&format!("/{branch_name}")))
                    })
                });

        let (ahead, behind) = if detached {
            (0, 0)
        } else {
            ahead_behind(repo, &resolved)
        };

        Some(Probe {
            branch_name: Some(branch_name),
            modified_count: modified,
            ahead_count: ahead,
            behind_count: behind,
            last_commit_at: resolved.peel_to_commit().ok().map(|c| to_local(c.time())),
            has_remote,
            branch_on_remote,
        })
    })
    .flatten()
}

/// Ahead/behind of the branch against its upstream, as of the last fetch.
fn ahead_behind(repo: &Repository, reference: &git2::Reference<'_>) -> (u32, u32) {
    let Some(local_oid) = reference.target() else {
        return (0, 0);
    };
    let Some(branch_ref) = reference.name().ok() else {
        return (0, 0);
    };
    let upstream_ref = match repo.branch_upstream_name(branch_ref) {
        Ok(buf) => match buf.as_str().ok().filter(|s| !s.is_empty()) {
            Some(name) => name.to_string(),
            None => return (0, 0),
        },
        Err(_) => return (0, 0),
    };
    let Some(upstream_oid) = repo
        .find_reference(&upstream_ref)
        .ok()
        .and_then(|r| r.target())
    else {
        return (0, 0);
    };
    repo.graph_ahead_behind(local_oid, upstream_oid)
        .map(|(ahead, behind)| (ahead as u32, behind as u32))
        .unwrap_or((0, 0))
}

/// The Changes tab's shared snapshot: status entries shaped for both views
/// plus the per-side numstat maps. An empty status short-circuits without the
/// diff probes.
pub fn change_snapshot(folder: &Path) -> Option<GitChangeSnapshot> {
    with_repo(folder, |repo| {
        let mut opts = StatusOptions::new();
        status_options(&mut opts);
        let statuses = repo.statuses(Some(&mut opts)).ok()?;
        let mut snapshot = GitChangeSnapshot::default();
        if statuses.is_empty() {
            return Some(snapshot);
        }
        for entry in statuses.iter() {
            describe_status_entry(entry, &mut snapshot);
        }

        // Worktree diff = index vs worktree (git diff); cached = HEAD vs index
        // (git diff --cached). Untracked rows join the worktree side so their
        // gutters carry the file's line count, like the staged view's.
        let index = repo.index().ok()?;
        let mut diff_opts = diff_options(repo);
        diff_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);
        if let Ok(mut diff) = repo.diff_index_to_workdir(Some(&index), Some(&mut diff_opts))
            && find_renames(&mut diff).is_ok()
        {
            snapshot.worktree_counts = numstat_counts(&diff);
        }

        let head_tree = head_tree(repo);
        let index = repo.index().ok()?;
        let mut diff_opts = diff_options(repo);
        if let Ok(mut diff) =
            repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut diff_opts))
            && find_renames(&mut diff).is_ok()
        {
            snapshot.cached_counts = numstat_counts(&diff);
        }
        Some(snapshot)
    })
    .flatten()
}

/// Local then remote-tracking branches, each group name-sorted, HEAD aliases
/// dropped.
pub fn branches(folder: &Path) -> Option<Vec<GitBranchRef>> {
    with_repo(folder, |repo| {
        let mut locals: Vec<String> = Vec::new();
        let mut remotes: Vec<String> = Vec::new();
        for branch in repo.branches(None).ok()? {
            let Ok((branch, kind)) = branch else { continue };
            let Some(name) = branch.name().ok().flatten() else {
                continue;
            };
            if name.ends_with("/HEAD") {
                continue;
            }
            match kind {
                git2::BranchType::Local => locals.push(name.to_string()),
                git2::BranchType::Remote => remotes.push(name.to_string()),
            }
        }
        locals.sort();
        remotes.sort();
        Some(
            locals
                .into_iter()
                .map(|n| GitBranchRef {
                    name: n,
                    kind: GitBranchKind::Local,
                })
                .chain(remotes.into_iter().map(|n| GitBranchRef {
                    name: n,
                    kind: GitBranchKind::Remote,
                }))
                .collect(),
        )
    })
    .flatten()
}

/// Upper bound on the commits one compare lists; a branch longer than this is
/// a rebase waiting to happen, not a preview.
const MAX_COMPARE_COMMITS: usize = 200;

/// What `head` would bring to `base`: the files and the commits a new pull
/// request previews. The three-dot range is deliberate — the merge base is the
/// starting point, so a base that moved on since the branch forked does not
/// read as removals. `None` when either name does not resolve (a branch that
/// was deleted, a typo) or when the two are the same branch.
pub fn compare(folder: &Path, base: &str, head: &str) -> Option<ComparePreview> {
    let base = base.trim();
    let head = head.trim();
    if base.is_empty() || head.is_empty() || base == head {
        return None;
    }
    with_repo(folder, |repo| {
        let base_oid = repo.revparse_single(base).ok()?.peel_to_commit().ok()?.id();
        let head_commit = repo.revparse_single(head).ok()?.peel_to_commit().ok()?;
        let head_oid = head_commit.id();
        let merge_base = repo.merge_base(base_oid, head_oid).ok()?;
        let base_tree = repo.find_commit(merge_base).ok()?.tree().ok()?;
        let head_tree = head_commit.tree().ok()?;
        let mut options = diff_options(repo);
        let mut diff = repo
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut options))
            .ok()?;
        find_renames(&mut diff).ok();
        let mut files = files_from_diff(&diff);
        for (file, delta) in files.iter_mut().zip(diff.deltas()) {
            file.status_code = delta_letter(delta.status()).to_string();
        }
        Some(ComparePreview {
            files,
            commits: compare_commits(repo, head_oid, merge_base),
        })
    })
    .flatten()
}

/// The commits `head` carries that the merge base does not, newest first, each
/// with the number of files it touched.
fn compare_commits(
    repo: &Repository,
    head: git2::Oid,
    merge_base: git2::Oid,
) -> Vec<CompareCommit> {
    let Ok(mut walk) = repo.revwalk() else {
        return Vec::new();
    };
    if walk.push(head).is_err() || walk.hide(merge_base).is_err() {
        return Vec::new();
    }
    walk.take(MAX_COMPARE_COMMITS)
        .filter_map(|oid| repo.find_commit(oid.ok()?).ok())
        .map(|commit| CompareCommit {
            hash: commit.id().to_string(),
            subject: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author: commit.author().name().ok().map(str::to_string),
            committed_at: Some(to_local(commit.time()).to_rfc3339()),
            files: commit_file_count(repo, &commit),
        })
        .collect()
}

/// One commit's touched-file count: its tree against its first parent's.
fn commit_file_count(repo: &Repository, commit: &git2::Commit<'_>) -> usize {
    let Ok(tree) = commit.tree() else {
        return 0;
    };
    let parent_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
    repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
        .map(|diff| diff.deltas().len())
        .unwrap_or(0)
}

/// The rail's letter for a diff delta, in the app's own vocabulary.
fn delta_letter(status: git2::Delta) -> char {
    match status {
        git2::Delta::Added | git2::Delta::Untracked => 'A',
        git2::Delta::Deleted => 'D',
        git2::Delta::Renamed => 'R',
        git2::Delta::Copied => 'C',
        _ => 'M',
    }
}

/// Resolves a remote-tracking branch pick (`origin/main`) to the local branch
/// a checkout should land on: the name after the remote prefix, plus whether
/// that local branch already exists. None when `target` is not a
/// remote-tracking ref (a local branch, tag or commit hash) — those keep
/// plain checkout semantics, so a commit pick still detaches HEAD as intended.
/// Remote names are matched longest-first, so a slash in the remote name
/// still splits correctly.
pub fn remote_tracking_local(folder: &Path, target: &str) -> Option<(String, bool)> {
    with_repo(folder, |repo| {
        // an exact local branch always wins, even one whose name looks like
        // `origin/main` — plain checkout resolves it the same way
        if repo.find_reference(&format!("refs/heads/{target}")).is_ok() {
            return None;
        }
        repo.find_reference(&format!("refs/remotes/{target}"))
            .ok()?;
        let remotes = repo.remotes().ok()?;
        let mut names: Vec<&str> = remotes
            .iter()
            .filter_map(|remote| remote.ok().flatten())
            .collect();
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        let remote = names.into_iter().find(|name| {
            target
                .strip_prefix(*name)
                .is_some_and(|rest| rest.len() > 1 && rest.starts_with('/'))
        })?;
        let local = &target[remote.len() + 1..];
        let exists = repo.find_reference(&format!("refs/heads/{local}")).is_ok();
        Some((local.to_string(), exists))
    })
    .flatten()
}

/// The publish target remote: "origin" when configured, else the first.
pub fn default_remote_name(folder: &Path) -> Option<String> {
    with_repo(folder, |repo| {
        let remotes = repo.remotes().ok()?;
        if remotes.is_empty() {
            return None;
        }
        if remotes.iter().any(|r| r.ok().flatten() == Some("origin")) {
            Some("origin".to_string())
        } else {
            remotes.get(0).ok().flatten().map(str::to_string)
        }
    })
    .flatten()
}

/// One commit's changed files with line counts (empty for a merge commit,
/// whose combined diff prints nothing) plus the message body after the subject
/// block.
pub fn commit_details(folder: &Path, hash: &str) -> Option<GitCommitDetails> {
    with_repo(folder, |repo| {
        let commit = find_commit(repo, hash)?;
        let mut files = Vec::new();
        if commit.parent_count() <= 1 {
            let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
            let tree = commit.tree().ok()?;
            let mut opts = diff_options(repo);
            if let Ok(mut diff) =
                repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
                && find_renames(&mut diff).is_ok()
            {
                files = files_from_diff(&diff);
            }
        }
        Some(GitCommitDetails {
            hash: hash.to_string(),
            files,
            body: message_body(commit.message().unwrap_or_default()),
        })
    })
    .flatten()
}

/// One commit's patch restricted to a single file, capped. Rename detection
/// stays off: a renamed path then renders its full new content.
pub fn commit_file_patch(folder: &Path, hash: &str, path: &str) -> Option<String> {
    with_repo(folder, |repo| {
        let commit = find_commit(repo, hash)?;
        if commit.parent_count() > 1 {
            return Some(String::new());
        }
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
        let tree = commit.tree().ok()?;
        let mut opts = diff_options(repo);
        opts.pathspec(path);
        let mut diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
            .ok()?;
        let patch = patch_for_path(&mut diff, path)?;
        Some(cap(
            normalize_patch_prefixes(&patch),
            FILE_PATCH_READ_CAP,
            PATCH_TRUNCATION_SUFFIX,
        ))
    })
    .flatten()
}

/// One change row's patch: staged side diffs HEAD against the index, unstaged
/// the index against the working tree. Empty when the path carries no textual
/// delta (binary, untracked, mode-only).
pub fn change_file_patch(folder: &Path, path: &str, staged: bool) -> Option<String> {
    with_repo(folder, |repo| {
        let patch = if staged {
            let head_tree = head_tree(repo);
            let index = repo.index().ok()?;
            let mut opts = diff_options(repo);
            opts.pathspec(path);
            let mut diff = repo
                .diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))
                .ok()?;
            patch_for_path(&mut diff, path)?
        } else {
            let index = repo.index().ok()?;
            let mut opts = diff_options(repo);
            // the status listing counts untracked files, so the patch must
            // too — without include_untracked the delta is missing entirely,
            // and without show_untracked_content its patch text comes back
            // empty and the diff pane renders a blank body
            opts.include_untracked(true)
                .recurse_untracked_dirs(true)
                .show_untracked_content(true);
            opts.pathspec(path);
            let mut diff = repo
                .diff_index_to_workdir(Some(&index), Some(&mut opts))
                .ok()?;
            patch_for_path(&mut diff, path)?
        };
        Some(cap(
            normalize_patch_prefixes(&patch),
            FILE_PATCH_READ_CAP,
            PATCH_TRUNCATION_SUFFIX,
        ))
    })
    .flatten()
}

/// Whether the commit is reachable from any fetched remote-tracking branch.
/// No remote refs means not pushed; failures fail open (true).
pub fn commit_pushed(folder: &Path, hash: &str) -> bool {
    with_repo(folder, |repo| {
        let Some(commit) = find_commit(repo, hash) else {
            return Some(true);
        };

        let remote_tips: Vec<git2::Oid> = repo
            .branches(Some(git2::BranchType::Remote))
            .ok()
            .map(|branches| {
                branches
                    .flatten()
                    .filter_map(|(b, _)| b.get().target())
                    .collect()
            })
            .unwrap_or_default();
        if remote_tips.is_empty() {
            return Some(false);
        }

        let mut walker = repo.revwalk().ok()?;
        for tip in &remote_tips {
            walker.push(*tip).ok()?;
        }
        for oid in walker.flatten() {
            if oid == commit.id() {
                return Some(true);
            }
        }
        Some(false)
    })
    .flatten()
    .unwrap_or(true)
}

/// Every hash reachable from a local branch but from no remote-tracking ref.
/// None when there is no remote or the repo is unreadable (cannot be answered
/// honestly); an empty set means everything is pushed.
pub fn unpushed_hashes(folder: &Path) -> Option<HashSet<String>> {
    with_repo(folder, |repo| {
        if repo.remotes().map(|r| r.is_empty()).unwrap_or(true) {
            return None;
        }
        let mut local_tips = Vec::new();
        let mut remote_tips = Vec::new();
        for branch in repo.branches(None).ok()? {
            let Ok((branch, kind)) = branch else { continue };
            let Some(target) = branch.get().target() else {
                continue;
            };
            match kind {
                git2::BranchType::Local => local_tips.push(target),
                git2::BranchType::Remote => remote_tips.push(target),
            }
        }
        if local_tips.is_empty() {
            return Some(HashSet::new());
        }
        let mut walker = repo.revwalk().ok()?;
        for tip in &local_tips {
            walker.push(*tip).ok()?;
        }
        for tip in &remote_tips {
            walker.hide(*tip).ok()?;
        }
        Some(walker.flatten().map(|oid| oid.to_string()).collect())
    })
    .flatten()
}

/// The commit-message wand's context in one repo pass: the staged patch head
/// capped for the prompt, the staged paths (renames as "old -> new"), and the
/// recent subjects for tone.
pub fn commit_prompt_context(folder: &Path) -> Option<(String, Vec<String>, Vec<String>)> {
    with_repo(folder, |repo| {
        let head_tree = head_tree(repo);
        let index = repo.index().ok()?;
        let mut opts = diff_options(repo);
        let mut diff = repo
            .diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))
            .ok()?;
        let _ = find_renames(&mut diff);

        let mut files = Vec::new();
        for delta in diff.deltas() {
            let old = delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());
            let new = delta
                .new_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());
            if let Some(path) = delta_prompt_path(old.as_deref(), new.as_deref()) {
                files.push(path);
            }
        }
        let patch = cap(
            normalize_patch_prefixes(&patch_text(&diff)),
            COMMIT_PROMPT_PATCH_CAP,
            PATCH_TRUNCATION_SUFFIX,
        );

        let mut subjects = Vec::new();
        let mut walker = repo.revwalk().ok()?;
        walker.push_head().ok()?;
        for oid in walker.take(10).flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                subjects.push(
                    commit
                        .summary()
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        }
        Some((patch, files, subjects))
    })
    .flatten()
}

/// One delta's prompt path: a rename reads "old -> new", everything else its
/// single side (new for adds, old for deletes).
fn delta_prompt_path(old: Option<&str>, new: Option<&str>) -> Option<String> {
    match (old, new) {
        (Some(old), Some(new)) if old != new => Some(format!("{old} -> {new}")),
        (Some(path), _) | (None, Some(path)) => Some(path.to_string()),
        (None, None) => None,
    }
}

/// The most recent commits, newest first (the history list's deeper cut).
pub fn history_commits(folder: &Path, limit: usize) -> Option<Vec<GitCommitInfo>> {
    with_repo(folder, |repo| {
        let mut walker = repo.revwalk().ok()?;
        walker.push_head().ok()?;
        let mut commits = Vec::new();
        for oid in walker.take(limit).flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                commits.push(GitCommitInfo {
                    hash: commit.id().to_string(),
                    subject: commit
                        .summary()
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .to_string(),
                    author: commit.author().name().ok().map(str::to_string),
                    date: to_local(commit.time()),
                });
            }
        }
        Some(commits)
    })
    .flatten()
}

/// The most recent commits, newest first.
pub fn recent_commits(folder: &Path) -> Option<Vec<GitCommitInfo>> {
    with_repo(folder, |repo| {
        let mut walker = repo.revwalk().ok()?;
        walker.push_head().ok()?;
        let mut commits = Vec::new();
        for oid in walker.take(10).flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                commits.push(GitCommitInfo {
                    hash: commit.id().to_string(),
                    subject: commit
                        .summary()
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .to_string(),
                    author: commit.author().name().ok().map(str::to_string),
                    date: to_local(commit.time()),
                });
            }
        }
        Some(commits)
    })
    .flatten()
}

/// One local day's activity: commits committer-dated that day plus the
/// still-uncommitted working-tree state with merged staged+unstaged counts.
pub fn day_activity(folder: &Path, day: chrono::NaiveDate) -> Option<GitDayActivity> {
    with_repo(folder, |repo| {
        let mut commits = Vec::new();
        let mut walker = repo.revwalk().ok()?;
        walker.push_head().ok()?;
        for (examined, oid) in (&mut walker).enumerate() {
            if examined >= MAX_DAY_SCAN_COMMITS {
                break;
            }
            let Ok(commit) = repo.find_commit(oid.ok()?) else {
                continue;
            };
            if to_local(commit.time()).date_naive() != day {
                continue;
            }
            let mut files = Vec::new();
            if commit.parent_count() <= 1 {
                let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
                let tree = commit.tree().ok()?;
                let mut opts = diff_options(repo);
                if let Ok(mut diff) =
                    repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
                    && find_renames(&mut diff).is_ok()
                {
                    files = files_from_diff(&diff)
                        .into_iter()
                        .take(MAX_DAY_COMMIT_FILE_ENTRIES)
                        .collect();
                }
            }
            commits.push(GitDayCommit {
                hash: commit.id().to_string(),
                subject: commit
                    .summary()
                    .ok()
                    .flatten()
                    .unwrap_or_default()
                    .to_string(),
                author: commit.author().name().ok().map(str::to_string),
                when: to_local(commit.time()),
                files,
            });
        }

        let mut snapshot = GitChangeSnapshot::default();
        let mut opts = StatusOptions::new();
        status_options(&mut opts);
        if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
            for entry in statuses.iter() {
                describe_status_entry(entry, &mut snapshot);
            }
        }
        if !snapshot.status_files.is_empty() {
            let index = repo.index().ok()?;
            let mut diff_opts = diff_options(repo);
            // untracked rows carry their line counts like staged ones do
            diff_opts
                .include_untracked(true)
                .recurse_untracked_dirs(true)
                .show_untracked_content(true);
            if let Ok(mut diff) = repo.diff_index_to_workdir(Some(&index), Some(&mut diff_opts))
                && find_renames(&mut diff).is_ok()
            {
                snapshot.worktree_counts = numstat_counts(&diff);
            }
            let head_tree = head_tree(repo);
            if let Ok(index) = repo.index() {
                let mut diff_opts = diff_options(repo);
                if let Ok(mut diff) =
                    repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut diff_opts))
                    && find_renames(&mut diff).is_ok()
                {
                    snapshot.cached_counts = numstat_counts(&diff);
                }
            }
        }

        let uncommitted = snapshot
            .status_files
            .iter()
            .map(|file| {
                // untracked rows merge too: the worktree diff now includes
                // them, and their counts ride the same +N/−N gutters
                let wt = snapshot.worktree_counts.get(&file.path).copied();
                let ix = snapshot.cached_counts.get(&file.path).copied();
                let merge = |a: Option<u32>, b: Option<u32>| match (a, b) {
                    (None, None) => None,
                    (x, y) => Some(x.unwrap_or(0) + y.unwrap_or(0)),
                };
                GitChangedFile {
                    additions: merge(wt.and_then(|c| c.0), ix.and_then(|c| c.0)),
                    deletions: merge(wt.and_then(|c| c.1), ix.and_then(|c| c.1)),
                    ..file.clone()
                }
            })
            .collect();

        let branch_name = repo.find_reference("HEAD").ok().and_then(|head| {
            // Ok(None) is the detached HEAD; see `probe`
            if head.symbolic_target().ok().flatten().is_none() {
                Some("(detached)".to_string())
            } else {
                head.resolve()
                    .ok()
                    .map(|r| r.shorthand().ok().unwrap_or("HEAD").to_string())
            }
        });

        Some(GitDayActivity {
            branch_name,
            commits,
            uncommitted_files: uncommitted,
        })
    })
    .flatten()
}

/// The given paths the index knows (the discard flow's tracked/untracked
/// split).
pub fn indexed_paths(folder: &Path, paths: &[String]) -> Option<HashSet<String>> {
    with_repo(folder, |repo| {
        let index = repo.index().ok()?;
        let mut listed = HashSet::new();
        for path in paths {
            if index.get_path(Path::new(path), 0).is_some() {
                listed.insert(path.clone());
            }
        }
        Some(listed)
    })
    .flatten()
}

/// Whether the index knows the path; false on failure, like an empty
/// ls-files output.
pub fn is_tracked(folder: &Path, path: &str) -> bool {
    indexed_paths(folder, std::slice::from_ref(&path.to_string()))
        .is_some_and(|set| !set.is_empty())
}

// ---- shaping helpers --------------------------------------------------------

fn with_repo<T>(folder: &Path, action: impl FnOnce(&Repository) -> Option<T>) -> Option<Option<T>> {
    let discovered = Repository::discover(folder).ok()?;
    Some(action(&discovered))
}

fn find_commit<'r>(repo: &'r Repository, hash: &str) -> Option<git2::Commit<'r>> {
    if let Ok(oid) = git2::Oid::from_str(hash)
        && let Ok(commit) = repo.find_commit(oid)
    {
        return Some(commit);
    }
    // abbreviated hash: resolve by unique prefix
    let (min_len, len) = (4, hash.len());
    if len < min_len || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut matches = Vec::new();
    let mut walker = repo.revwalk().ok()?;
    walker.push_head().ok()?;
    let _ = walker.simplify_first_parent();
    for oid in walker.flatten() {
        if oid.to_string().starts_with(hash) {
            matches.push(oid);
            if matches.len() > 1 {
                return None;
            }
        }
    }
    matches.pop().and_then(|oid| repo.find_commit(oid).ok())
}

fn status_options(opts: &mut StatusOptions) {
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
}

/// Patch options matching the git CLI: the indent heuristic git anchors hunks
/// with by default, and the repo's configured diff.algorithm (libgit2 has no
/// histogram; patience anchors the same way on ordinary diffs).
fn diff_options(repo: &Repository) -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    let algorithm = repo
        .config()
        .ok()
        .and_then(|config| config.get_string("diff.algorithm").ok())
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    opts.indent_heuristic(true);
    match algorithm.as_str() {
        "histogram" | "patience" => {
            opts.patience(true);
        }
        "minimal" => {
            opts.minimal(true);
        }
        _ => {}
    }
    opts
}

/// git turns diff.renames on by itself since 2.9; libgit2 only honors the
/// config entry, so apply detection explicitly.
fn find_renames(diff: &mut git2::Diff<'_>) -> Result<(), git2::Error> {
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))
}

fn head_tree(repo: &Repository) -> Option<git2::Tree<'_>> {
    repo.find_reference("HEAD")
        .ok()?
        .peel_to_commit()
        .ok()?
        .tree()
        .ok()
}

fn to_local(time: git2::Time) -> DateTime<Local> {
    if let Some(fixed) = chrono::FixedOffset::east_opt(time.offset_minutes() * 60)
        .and_then(|tz| tz.timestamp_opt(time.seconds(), 0).single())
    {
        fixed.with_timezone(&Local)
    } else {
        Local
            .timestamp_opt(time.seconds(), 0)
            .single()
            .unwrap_or_else(Local::now)
    }
}

fn describe_status_entry(entry: git2::StatusEntry<'_>, snapshot: &mut GitChangeSnapshot) {
    let Ok(path) = entry.path() else { return };
    let path = path.to_string();
    if path.is_empty() {
        return;
    }
    let state = entry.status();

    // `path()` is the old name of a rename: each side lists the file as it
    // stands on that side
    let staged_path = entry
        .head_to_index()
        .and_then(|delta| delta.new_file().path().map(path_text))
        .unwrap_or_else(|| path.clone());
    let worktree_path = entry
        .index_to_workdir()
        .and_then(|delta| delta.new_file().path().map(path_text))
        .unwrap_or_else(|| staged_path.clone());

    let file = |path: &str, code: &str| GitChangedFile {
        path: path.to_string(),
        status_code: code.to_string(),
        additions: None,
        deletions: None,
    };
    let make = |code: &str| file(&worktree_path, code);

    // Porcelain's unmerged XY pair is not exposed by libgit2's status flags;
    // both views carry the "U" the split view always used.
    if state.contains(Status::CONFLICTED) {
        let file = make("U");
        snapshot.unstaged_files.push(file.clone());
        snapshot.status_files.push(file);
        return;
    }
    if state.contains(Status::WT_NEW) {
        let file = make("?");
        snapshot.unstaged_files.push(file.clone());
        snapshot.status_files.push(file);
        return;
    }

    let index_code = index_code(state);
    let worktree_code = worktree_code(state);
    if let Some(code) = index_code {
        snapshot
            .staged_files
            .push(file(&staged_path, &code.to_string()));
    }
    if let Some(code) = worktree_code {
        snapshot.unstaged_files.push(make(&code.to_string()));
    }
    snapshot.status_files.push(make(&format!(
        "{}{}",
        index_code.unwrap_or('.'),
        worktree_code.unwrap_or('.')
    )));
}

/// git hands status paths back with forward slashes on every platform.
fn path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn index_code(state: Status) -> Option<char> {
    if state.contains(Status::INDEX_NEW) {
        Some('A')
    } else if state.contains(Status::INDEX_DELETED) {
        Some('D')
    } else if state.contains(Status::INDEX_RENAMED) {
        Some('R')
    } else if state.contains(Status::INDEX_TYPECHANGE) {
        Some('T')
    } else if state.contains(Status::INDEX_MODIFIED) {
        Some('M')
    } else {
        None
    }
}

fn worktree_code(state: Status) -> Option<char> {
    if state.contains(Status::WT_DELETED) {
        Some('D')
    } else if state.contains(Status::WT_RENAMED) {
        Some('R')
    } else if state.contains(Status::WT_TYPECHANGE) {
        Some('T')
    } else if state.contains(Status::WT_MODIFIED) {
        Some('M')
    } else {
        None
    }
}

fn delta_path(delta: &git2::DiffDelta<'_>) -> String {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn files_from_diff(diff: &git2::Diff<'_>) -> Vec<GitChangedFile> {
    let mut files = Vec::new();
    for (idx, delta) in diff.deltas().enumerate() {
        let (additions, deletions) = git2::Patch::from_diff(diff, idx)
            .ok()
            .flatten()
            .and_then(|patch| patch.line_stats().ok())
            .map(|(_, additions, deletions)| (additions as u32, deletions as u32))
            .unwrap_or((0, 0));
        // Zero on both sides = binary or mode-only: the UI shows no counts.
        let counts = if additions == 0 && deletions == 0 {
            (None, None)
        } else {
            (Some(additions), Some(deletions))
        };
        files.push(GitChangedFile {
            path: delta_path(&delta),
            status_code: String::new(),
            additions: counts.0,
            deletions: counts.1,
        });
    }
    files
}

fn numstat_counts(diff: &git2::Diff<'_>) -> HashMap<String, (Option<u32>, Option<u32>)> {
    let mut counts = HashMap::new();
    for (idx, delta) in diff.deltas().enumerate() {
        let (additions, deletions) = git2::Patch::from_diff(diff, idx)
            .ok()
            .flatten()
            .and_then(|patch| patch.line_stats().ok())
            .map(|(_, additions, deletions)| (additions as u32, deletions as u32))
            .unwrap_or((0, 0));
        counts.insert(
            delta_path(&delta),
            if additions == 0 && deletions == 0 {
                (None, None)
            } else {
                (Some(additions), Some(deletions))
            },
        );
    }
    counts
}

fn patch_text(diff: &git2::Diff<'_>) -> String {
    let mut text = String::new();
    for idx in 0..diff.deltas().len() {
        if let Ok(Some(mut patch)) = git2::Patch::from_diff(diff, idx)
            && let Ok(buf) = patch.to_buf()
        {
            text.push_str(buf.as_str().unwrap_or_default());
        }
    }
    text
}

fn patch_for_path(diff: &mut git2::Diff<'_>, path: &str) -> Option<String> {
    for (idx, delta) in diff.deltas().enumerate() {
        if delta_path(&delta) == path {
            if let Ok(Some(mut patch)) = git2::Patch::from_diff(diff, idx)
                && let Ok(buf) = patch.to_buf()
            {
                return Some(buf.as_str().unwrap_or_default().to_string());
            }
            return Some(String::new());
        }
    }
    Some(String::new())
}

/// The %b semantics: everything after the subject block, trimmed and capped; a
/// subject-only message has no body.
fn message_body(message: &str) -> Option<String> {
    let body = match message.find("\n\n") {
        Some(idx) => message[idx + 2..].trim(),
        None => "",
    };
    if body.is_empty() {
        return None;
    }
    Some(body.chars().take(COMMIT_BODY_READ_CAP).collect())
}

/// Normalizes the mnemonic diff prefixes a user's diff.mnemonicPrefix makes
/// libgit2 emit back to the a/ b/ convention plain git diff prints (the old
/// side reads c/ or i/, the new side c/, i/ or w/). Anchored to the three
/// header forms, so a path genuinely starting with c/, i/ or w/ is never
/// touched.
fn normalize_patch_prefixes(patch: &str) -> String {
    let mut out = String::with_capacity(patch.len());
    for line in patch.split_inclusive('\n') {
        // Keep the terminator: these are header lines and the next line must
        // start on a new row. Dropping it merged `--- a/` into the `+++ b/`
        // line, which in turn hid the `@@` hunk header and reset the diff
        // line numbers to zero.
        let (body, ending) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        if let Some(rest) = body.strip_prefix("diff --git ") {
            if let Some((old, new)) = split_git_line(rest) {
                out.push_str(&format!("diff --git a/{old} b/{new}{ending}"));
                continue;
            }
        } else if let Some(path) = body
            .strip_prefix("--- c/")
            .or_else(|| body.strip_prefix("--- i/"))
        {
            out.push_str("--- a/");
            out.push_str(path);
            out.push_str(ending);
            continue;
        } else if let Some(path) = body
            .strip_prefix("+++ c/")
            .or_else(|| body.strip_prefix("+++ i/"))
            .or_else(|| body.strip_prefix("+++ w/"))
        {
            out.push_str("+++ b/");
            out.push_str(path);
            out.push_str(ending);
            continue;
        }
        out.push_str(line);
    }
    out
}

/// "c/path i/path" → ("path", "path") — each side a single-letter prefix dir,
/// optional quoting on the tokens.
fn split_git_line(rest: &str) -> Option<(String, String)> {
    let trimmed = rest.trim_end_matches('\n');
    let (old_token, new_token) = trimmed.split_once(' ')?;
    let strip = |token: &str, valid: &[u8]| -> Option<String> {
        let unquoted = token.trim_matches('"');
        let ub = unquoted.as_bytes();
        if ub.len() >= 2 && ub[1] == b'/' && valid.contains(&ub[0]) {
            Some(unquoted[2..].to_string())
        } else {
            None
        }
    };
    let old = strip(old_token, b"ci")?;
    let new = strip(new_token, b"ciw")?;
    Some((old, new))
}

fn cap(content: String, cap: usize, suffix: &str) -> String {
    if content.chars().count() >= cap {
        let head: String = content.chars().take(cap).collect();
        format!("{head}{suffix}")
    } else {
        content
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::model::GitChangeSnapshot;

    #[test]
    fn merged_files_layer_cached_over_worktree() {
        let mut snap = GitChangeSnapshot {
            status_files: vec![GitChangedFile {
                path: "a.txt".into(),
                status_code: "MM".into(),
                additions: None,
                deletions: None,
            }],
            ..Default::default()
        };
        snap.worktree_counts
            .insert("a.txt".into(), (Some(1), Some(2)));
        snap.cached_counts
            .insert("a.txt".into(), (Some(10), Some(20)));
        let merged = snap.merged_files();
        assert_eq!(merged[0].additions, Some(10));
        assert_eq!(merged[0].deletions, Some(20));
    }

    #[test]
    fn change_groups_apply_their_own_sides() {
        let mut snap = GitChangeSnapshot {
            staged_files: vec![GitChangedFile {
                path: "b.txt".into(),
                status_code: "A".into(),
                additions: None,
                deletions: None,
            }],
            unstaged_files: vec![GitChangedFile {
                path: "c.txt".into(),
                status_code: "M".into(),
                additions: None,
                deletions: None,
            }],
            ..Default::default()
        };
        snap.cached_counts.insert("b.txt".into(), (Some(3), None));
        snap.worktree_counts.insert("c.txt".into(), (None, Some(4)));
        let (staged, unstaged) = snap.change_groups();
        assert_eq!(staged[0].additions, Some(3));
        assert_eq!(unstaged[0].deletions, Some(4));
    }
}

#[cfg(test)]
mod untracked_patch_tests {
    use super::*;

    /// A real repo with one commit, so HEAD and the index exist. Each caller
    /// gets its own folder: the tests run in parallel and a shared `.git`
    /// locks up.
    fn scratch_repo(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ws-read-untracked-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        std::fs::write(dir.join("base.txt"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("base.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        dir
    }

    #[test]
    fn an_untracked_file_patches_as_pure_additions() {
        let dir = scratch_repo("patch");
        std::fs::write(dir.join("new.txt"), "line one\nline two\n").unwrap();
        let patch = change_file_patch(&dir, "new.txt", false).expect("untracked patch");
        assert!(patch.contains("+++ b/new.txt"), "patch was: {patch}");
        assert!(patch.contains("+line one"), "patch was: {patch}");
        assert!(patch.contains("+line two"), "patch was: {patch}");
        assert!(!patch.contains("-base"), "patch was: {patch}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_untracked_file_carries_line_counts_in_the_snapshot() {
        let dir = scratch_repo("counts");
        std::fs::write(dir.join("new.txt"), "a\nb\nc\n").unwrap();
        let snapshot = change_snapshot(&dir).expect("snapshot");
        // counts ride change_groups, not the raw status lists
        let (staged, unstaged) = snapshot.change_groups();
        assert!(staged.is_empty());
        let counts = unstaged
            .iter()
            .find(|file| file.path == "new.txt")
            .expect("untracked row");
        assert_eq!(counts.additions, Some(3));
        assert_eq!(counts.deletions, Some(0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod compare_tests {
    use super::*;

    /// A repo with `main` and a `feat` branch one commit ahead, so the compare
    /// has both a file and a commit to report.
    fn forked_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-compare-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        repo.set_head("refs/heads/main").ok();
        std::fs::write(dir.join("base.txt"), "base\n").unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("base.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let head = repo
            .find_commit(repo.revparse_single("main").unwrap().id())
            .unwrap();
        repo.branch("feat", &head, false).unwrap();

        std::fs::write(dir.join("feat.txt"), "one\ntwo\nthree\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("feat.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(
            Some("refs/heads/feat"),
            &sig,
            &sig,
            "add the feature",
            &tree,
            &[&head],
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_compare_lists_the_branchs_files_and_commits() {
        let dir = forked_repo("files");
        let preview = compare(&dir, "main", "feat").expect("a compare");

        assert_eq!(preview.files.len(), 1);
        assert_eq!(preview.files[0].path, "feat.txt");
        assert_eq!(preview.files[0].status_code, "A");
        assert_eq!(preview.files[0].additions, Some(3));
        assert_eq!(preview.commits.len(), 1);
        assert_eq!(preview.commits[0].subject, "add the feature");
        assert_eq!(preview.commits[0].files, 1);
        assert!(preview.commits[0].committed_at.is_some());
        assert_eq!(preview.line_counts(), (3, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Three-dot: a base that moved on does not show up as removals.
    #[test]
    fn a_moved_base_does_not_read_as_removals() {
        let dir = forked_repo("three-dot");
        let repo = git2::Repository::open(&dir).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        // main gains a file the feature never saw
        std::fs::write(dir.join("later.txt"), "later\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("later.txt")).unwrap();
        index.write().unwrap();
        let main = repo
            .find_commit(repo.revparse_single("main").unwrap().id())
            .unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(
            Some("refs/heads/main"),
            &sig,
            &sig,
            "later",
            &tree,
            &[&main],
        )
        .unwrap();

        let preview = compare(&dir, "main", "feat").expect("a compare");

        assert!(
            preview.files.iter().all(|file| file.path != "later.txt"),
            "the base's own file stays out of the preview"
        );
        assert_eq!(preview.files.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_branch_or_a_typo_is_no_compare() {
        let dir = forked_repo("guard");
        assert!(compare(&dir, "main", "main").is_none());
        assert!(compare(&dir, "main", "gone").is_none());
        assert!(compare(&dir, "  ", "feat").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod patch_prefix_tests {
    use super::*;

    #[test]
    fn rewrites_mnemonic_headers_without_losing_lines_or_paths() {
        let patch = "diff --git i/src/main.rs w/src/main.rs\n\
                     index 476a51b..bdf883c 100644\n\
                     --- i/src/main.rs\n\
                     +++ w/src/main.rs\n\
                     @@ -15,2 +15,3 @@\n\
                     \x20context\n\
                     +added\n";
        let normalized = normalize_patch_prefixes(patch);
        assert_eq!(
            normalized,
            "diff --git a/src/main.rs b/src/main.rs\n\
             index 476a51b..bdf883c 100644\n\
             --- a/src/main.rs\n\
             +++ b/src/main.rs\n\
             @@ -15,2 +15,3 @@\n\
             \x20context\n\
             +added\n",
        );
        // every header must still occupy its own row
        assert!(normalized.lines().any(|line| line == "+++ b/src/main.rs"));
        assert!(normalized.lines().any(|line| line.starts_with("@@ -15,2")));
    }

    #[test]
    fn keeps_plain_a_b_headers_and_a_missing_final_newline() {
        let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new";
        assert_eq!(normalize_patch_prefixes(patch), patch);
    }

    #[test]
    fn leaves_unrecognized_header_shapes_alone() {
        let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n";
        assert_eq!(normalize_patch_prefixes(patch), patch);
        assert_eq!(normalize_patch_prefixes(""), "");
    }
}

#[cfg(test)]
mod prompt_path_tests {
    use super::*;

    #[test]
    fn shapes_delta_paths_by_status() {
        assert_eq!(
            delta_prompt_path(Some("old.txt"), Some("new.txt")),
            Some("old.txt -> new.txt".into())
        );
        assert_eq!(
            delta_prompt_path(Some("a.rs"), Some("a.rs")),
            Some("a.rs".into())
        );
        assert_eq!(
            delta_prompt_path(None, Some("added.rs")),
            Some("added.rs".into())
        );
        assert_eq!(
            delta_prompt_path(Some("gone.rs"), None),
            Some("gone.rs".into())
        );
        assert_eq!(delta_prompt_path(None, None), None);
    }
}
