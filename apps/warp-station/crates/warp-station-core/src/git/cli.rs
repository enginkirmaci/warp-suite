//! The git CLI mutation layer: checkout/branch/clone, fetch/pull/push/publish,
//! stage/unstage/discard and commit — spawned through the shared process
//! runner with the prompt-proof environment and the per-class timeouts, so
//! hooks, gpg signing and credential helpers keep applying (in-process reads
//! never take this path). Args pass as a list; std applies the
//! CommandLineToArgvW-compatible quoting, so no manual quoting helper exists.

use crate::git::read;
use crate::model::GitSyncResult;
use crate::process::{self, ProcessRunOptions};
use std::path::Path;
use std::time::Duration;

/// Upper bound for a single local git invocation.
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for fetch/pull/push — network transfers grow with the payload.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(60);
/// Upper bound for clone — a full history download can legitimately run for
/// minutes.
pub const CLONE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct GitOutcome {
    pub ok: bool,
    /// git's actionable stderr line on failure (fatal:/error:/CONFLICT/rejected).
    pub error: Option<String>,
}

impl GitOutcome {
    fn from(result: &process::ProcessRunResult) -> Self {
        if result.succeeded() || result.truncated {
            Self {
                ok: true,
                error: None,
            }
        } else {
            Self {
                ok: false,
                error: summarize_sync_error(&result.stderr),
            }
        }
    }

    pub fn into_sync_result(self) -> GitSyncResult {
        if self.ok {
            GitSyncResult::ok()
        } else {
            GitSyncResult::err(self.error)
        }
    }
}

/// One git invocation: never prompts (GIT_TERMINAL_PROMPT=0), returns stdout on
/// success or the summarized stderr on failure.
async fn run_git(
    working_dir: &Path,
    args: &[String],
    timeout: Duration,
    max_output_bytes: Option<usize>,
) -> anyhow::Result<process::ProcessRunResult> {
    let result = process::run(ProcessRunOptions {
        file_name: "git",
        args,
        working_dir: Some(working_dir),
        timeout: Some(timeout),
        max_output_bytes,
        // a credential prompt has no console to appear in — fail fast instead
        // of blocking to the timeout
        environment: vec![("GIT_TERMINAL_PROMPT".into(), "0".into())],
        ..Default::default()
    })
    .await?;
    if !result.succeeded() && !result.truncated {
        let stderr = result.stderr.trim();
        let head: String = stderr.chars().take(500).collect();
        tracing::error!(
            "git {} exited {:?} in {}: {head}",
            args.join(" "),
            result.exit_code,
            working_dir.display()
        );
    }
    Ok(result)
}

/// Picks the actionable line from git's stderr — the first
/// fatal/error/conflict/rejection line, falling back to the last non-empty one
/// (transfer chatter like "From origin" would otherwise fill the message).
pub fn summarize_sync_error(stderr: &str) -> Option<String> {
    let mut fallback = None;
    for raw in stderr.split('\n') {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("fatal: ")
            || line.starts_with("error: ")
            || line.starts_with("CONFLICT")
            || line.starts_with("! [")
        {
            return Some(line.to_string());
        }
        fallback = Some(line.to_string());
    }
    fallback
}

pub async fn checkout(folder: &Path, branch: &str) -> GitOutcome {
    let result = run_git(
        folder,
        &["checkout".into(), branch.into()],
        PROCESS_TIMEOUT,
        None,
    )
    .await;
    match result {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn create_branch(
    folder: &Path,
    branch: &str,
    start_point: Option<&str>,
    checkout: bool,
) -> GitSyncResult {
    let mut args = if checkout {
        vec!["checkout".into(), "-b".into(), branch.into()]
    } else {
        vec!["branch".into(), branch.into()]
    };
    if let Some(start) = start_point.map(str::trim).filter(|s| !s.is_empty()) {
        args.push(start.to_string());
    }
    match run_git(folder, &args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r).into_sync_result(),
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

/// `git branch -D`: reflog keeps the tip recoverable, so the two-click
/// arm-confirm in the menu is the only guard it needs.
pub async fn delete_branch(folder: &Path, branch: &str) -> GitSyncResult {
    let args = vec!["branch".into(), "-D".into(), branch.into()];
    match run_git(folder, &args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r).into_sync_result(),
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

pub async fn clone(url: &str, parent_directory: &Path, repo_name: &str) -> GitSyncResult {
    // The working directory is the destination's PARENT — the one command that
    // runs outside any existing repo. A cancelled (killed) clone leaves a
    // partial worktree; remove it so a retry finds the slot free.
    let args = vec![
        "clone".into(),
        url.trim().to_string(),
        repo_name.trim().to_string(),
    ];
    let result = process::run(ProcessRunOptions {
        file_name: "git",
        args: &args,
        working_dir: Some(parent_directory),
        timeout: Some(CLONE_TIMEOUT),
        environment: vec![("GIT_TERMINAL_PROMPT".into(), "0".into())],
        ..Default::default()
    })
    .await;
    match result {
        Ok(r) if r.succeeded() => GitSyncResult::ok(),
        Ok(r) => {
            remove_partial_clone(parent_directory.join(repo_name)).await;
            // surface git's fatal line like every other mutation — "clone
            // failed" alone can't tell an auth failure from a bad URL or an
            // scp-style remote (host:repo is not a valid Windows folder name)
            GitOutcome::from(&r).into_sync_result()
        }
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

/// On Windows, handles on the killed clone's files (AV, indexer) can outlive
/// the taskkill; retry so a clone retry doesn't die on "folder already
/// exists".
async fn remove_partial_clone(path: std::path::PathBuf) {
    if !path.exists() {
        return;
    }
    for _ in 0..3 {
        if std::fs::remove_dir_all(&path).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn fetch(folder: &Path) -> GitOutcome {
    sync(folder, &["fetch".into(), "--prune".into()]).await
}

pub async fn pull(folder: &Path) -> GitOutcome {
    sync(folder, &["pull".into()]).await
}

pub async fn push(folder: &Path) -> GitOutcome {
    sync(folder, &["push".into()]).await
}

pub async fn publish_branch(folder: &Path, branch: &str) -> GitSyncResult {
    let Some(remote) = read::default_remote_name(folder) else {
        return GitSyncResult::err(Some("no remote configured".into()));
    };
    sync(folder, &["push".into(), "-u".into(), remote, branch.into()])
        .await
        .into_sync_result()
}

async fn sync(folder: &Path, args: &[String]) -> GitOutcome {
    match run_git(folder, args, SYNC_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn stage(folder: &Path, path: &str) -> GitOutcome {
    let result = run_git(
        folder,
        &["add".into(), "--".into(), path.into()],
        PROCESS_TIMEOUT,
        None,
    )
    .await;
    match result {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn stage_all(folder: &Path) -> GitOutcome {
    simple(folder, &["add".into(), "-A".into()]).await
}

pub async fn unstage(folder: &Path, path: &str) -> GitOutcome {
    simple(
        folder,
        &[
            "reset".into(),
            "-q".into(),
            "HEAD".into(),
            "--".into(),
            path.into(),
        ],
    )
    .await
}

pub async fn unstage_all(folder: &Path) -> GitOutcome {
    simple(folder, &["reset".into(), "-q".into(), "HEAD".into()]).await
}

async fn simple(folder: &Path, args: &[String]) -> GitOutcome {
    match run_git(folder, args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

/// Shared tail of the section discards: with `reset_first` the paths are
/// unstaged first (a staged discard's files return to HEAD), then every path
/// still in the index is checked out and every path that is not (untracked
/// files, just-unstaged additions) is deleted.
pub async fn discard_paths(folder: &Path, paths: &[String], reset_first: bool) -> bool {
    if paths.is_empty() {
        return false;
    }
    if reset_first {
        let mut args = vec!["reset".into(), "-q".into(), "HEAD".into(), "--".into()];
        args.extend(paths.iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }

    let in_index = read::indexed_paths(folder, paths).unwrap_or_default();
    let (tracked, untracked): (Vec<&String>, Vec<&String>) =
        paths.iter().partition(|p| in_index.contains(*p));

    if !tracked.is_empty() {
        let mut args = vec!["checkout".into(), "--".into()];
        args.extend(tracked.into_iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }
    if !untracked.is_empty() {
        let mut args = vec!["clean".into(), "-f".into(), "--".into()];
        args.extend(untracked.into_iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }
    true
}

/// Single-file discard: a staged row loses its staged state too (ends at
/// HEAD); an unstaged row keeps the index (a partially staged file keeps its
/// edits).
pub async fn discard_file(folder: &Path, path: &str, is_staged: bool) -> bool {
    if is_staged && !unstage(folder, path).await.ok {
        return false;
    }
    let in_index = read::is_tracked(folder, path);
    let args = if in_index {
        vec!["checkout".into(), "--".into(), path.into()]
    } else {
        vec!["clean".into(), "-f".into(), "--".into(), path.into()]
    };
    simple(folder, &args).await.ok
}

/// Commits with `-F` (the message may carry quotes, newlines or anything a
/// shell-style argument would mangle) and returns the short hash from git's
/// "[branch abc1234] subject" output line.
pub async fn commit(folder: &Path, message: &str) -> Option<String> {
    let message_file = std::env::temp_dir().join(format!(
        "warp-station-commit-{}-{}.msg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if let Err(err) = std::fs::write(&message_file, message) {
        tracing::debug!("commit message temp file failed: {err}");
        return None;
    }
    let outcome = run_git(
        folder,
        &[
            "commit".into(),
            "-F".into(),
            message_file.to_string_lossy().to_string(),
        ],
        PROCESS_TIMEOUT,
        None,
    )
    .await;
    let _ = std::fs::remove_file(&message_file);
    let result = outcome.ok()?;
    let first_line = result.stdout.lines().next().unwrap_or("");
    // "[branch abc1234] subject" — pick the hash
    let hash = first_line
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(inside, _)| inside.rsplit(' ').next())
        .unwrap_or("");
    Some(hash.chars().take(7).collect())
}

pub async fn revert_commit(folder: &Path, hash: &str) -> GitOutcome {
    simple(folder, &["revert".into(), "--no-edit".into(), hash.into()]).await
}
