//! Parity harness: the git2 read layer against real `git` CLI output on this
//! very repository. Skips silently when the repo (or git) is not available, so
//! the suite stays runnable anywhere.

use std::path::Path;
use std::process::Command;
use warp_station_core::git::read;

fn repo_root() -> Option<std::path::PathBuf> {
    // tests/ lives under the crate; the workspace repo is three levels up
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.ancestors().nth(2)?.to_path_buf();
    root.join(".git").is_dir().then_some(root)
}

fn git(args: &[&str]) -> Option<String> {
    let root = repo_root()?;
    let out = Command::new("git")
        .args(args)
        .current_dir(&root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn require() -> Option<std::path::PathBuf> {
    let root = repo_root()?;
    git(&["rev-parse"]).is_some().then_some(root)
}

#[test]
fn probe_matches_cli_status() {
    let Some(root) = require() else { return };
    let probe = read::probe(&root).expect("probe");

    let porcelain = git(&["status", "--porcelain=v1", "--untracked-files=all"]).unwrap_or_default();
    let cli_count = porcelain.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        probe.modified_count as usize, cli_count,
        "modified count vs porcelain"
    );

    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    assert_eq!(probe.branch_name.as_deref(), Some(branch.trim()));

    let head_count = git(&["rev-list", "--count", "HEAD"]).unwrap_or_default();
    assert!(head_count.trim().parse::<usize>().is_ok_and(|n| n > 0));
    assert!(probe.last_commit_at.is_some());
}

#[test]
fn change_snapshot_matches_cli_diffs() {
    let Some(root) = require() else { return };
    let snapshot = read::change_snapshot(&root).expect("snapshot");

    // staged side vs `git diff --cached --name-only` (status staged entries are
    // a superset only when renames/binary diverge — for this repo they match)
    let cached = git(&["diff", "--cached", "--name-only"]).unwrap_or_default();
    let cached_names: Vec<&str> = cached
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    for name in &cached_names {
        assert!(
            snapshot.staged_files.iter().any(|f| f.path == *name),
            "staged side missing {name}"
        );
    }

    // unstaged side vs `git diff --name-only` plus untracked
    let unstaged = git(&["diff", "--name-only"]).unwrap_or_default();
    for name in unstaged.lines().map(str::trim).filter(|l| !l.is_empty()) {
        assert!(
            snapshot.unstaged_files.iter().any(|f| f.path == name),
            "unstaged side missing {name}"
        );
    }
    let untracked = git(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();
    for name in untracked.lines().map(str::trim).filter(|l| !l.is_empty()) {
        assert!(
            snapshot
                .unstaged_files
                .iter()
                .any(|f| f.path == name && f.status_code == "?"),
            "untracked missing from unstaged: {name}"
        );
    }
}

#[test]
fn recent_commits_match_log() {
    let Some(root) = require() else { return };
    let commits = read::recent_commits(&root).expect("commits");
    let log = git(&["log", "-10", "--pretty=%s"]).unwrap_or_default();
    let subjects: Vec<&str> = log.lines().collect();
    assert_eq!(commits.len(), subjects.len());
    for (commit, subject) in commits.iter().zip(subjects) {
        assert_eq!(commit.subject, subject);
        assert_eq!(commit.hash.len(), 40);
    }
}

#[test]
fn branches_and_remotes_match_cli() {
    let Some(root) = require() else { return };
    let branches = read::branches(&root).expect("branches");
    let local =
        git(&["for-each-ref", "--format=%(refname:short)", "refs/heads"]).unwrap_or_default();
    let local_names: Vec<String> = local.lines().map(str::trim).map(str::to_string).collect();
    let ours_locals: Vec<&str> = branches
        .iter()
        .filter(|b| b.kind == warp_station_core::model::GitBranchKind::Local)
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(
        ours_locals,
        local_names.iter().map(|s| s.as_str()).collect::<Vec<_>>()
    );

    if read::default_remote_name(&root).is_some() {
        let remote = git(&["remote"]).unwrap_or_default();
        assert!(remote.trim().contains("origin"));
    }
}

#[test]
fn commit_prompt_context_matches_cli() {
    let Some(root) = require() else { return };
    let Some((patch, files, subjects)) = read::commit_prompt_context(&root) else {
        panic!("commit_prompt_context returned None");
    };

    // file list vs `git diff --cached --name-only`; a rename pair reads
    // "old -> new" and the CLI name-only line is the new side
    let cached = git(&["diff", "--cached", "--name-only"]).unwrap_or_default();
    let cli_names: Vec<&str> = cached
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let our_names: Vec<String> = files
        .iter()
        .map(|path| {
            path.rsplit_once(" -> ")
                .map(|(_, new)| new.to_string())
                .unwrap_or_else(|| path.clone())
        })
        .collect();
    assert_eq!(
        our_names.len(),
        cli_names.len(),
        "staged count vs CLI: {files:?}"
    );
    for (ours, cli) in our_names.iter().zip(&cli_names) {
        assert_eq!(ours, cli);
    }
    if cli_names.is_empty() {
        assert!(
            patch.is_empty(),
            "no staged files but a non-empty patch head"
        );
    } else {
        assert!(
            patch.contains("diff --git "),
            "patch head: {}",
            &patch[..patch.len().min(80)]
        );
    }

    let log = git(&["log", "-10", "--pretty=%s"]).unwrap_or_default();
    let cli_subjects: Vec<&str> = log.lines().collect();
    assert_eq!(subjects.len(), cli_subjects.len());
    for (ours, cli) in subjects.iter().zip(&cli_subjects) {
        assert_eq!(ours, cli);
    }
}

#[test]
fn unpushed_hashes_answerable() {
    let Some(root) = require() else { return };
    if git(&["remote"]).unwrap_or_default().trim().is_empty() {
        assert!(read::unpushed_hashes(&root).is_none());
    } else {
        assert!(read::unpushed_hashes(&root).is_some());
    }
}
