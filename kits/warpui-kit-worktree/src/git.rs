//! The `git` command line, once: the prompt-proof environment, a timeout, and
//! the porcelain parser behind [`crate::list`].
//!
//! The crate shells out rather than linking `git2`, because warp-agent has no
//! libgit2 and a C dependency for four commands is not worth its build cost.

use anyhow::{Result, anyhow};
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Upper bound for one invocation. `worktree add` checks out a whole branch, so
/// this is generous compared with a status read.
const TIMEOUT: Duration = Duration::from_secs(30);

/// One finished invocation.
pub(crate) struct Run {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

impl Run {
    /// The line worth showing a user: git's own actionable stderr line, or the
    /// last thing it said when none of them looks like an error.
    pub(crate) fn message(&self) -> String {
        if self.timed_out {
            return "git timed out".to_string();
        }
        let lines: Vec<&str> = self
            .stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        lines
            .iter()
            .find(|line| {
                line.starts_with("fatal:")
                    || line.starts_with("error:")
                    || line.starts_with("could not")
            })
            .or_else(|| lines.last())
            .map(|line| (*line).to_string())
            .unwrap_or_else(|| "git failed".to_string())
    }
}

/// Runs git with string arguments.
pub(crate) fn run(dir: &Path, args: &[&str]) -> Result<Run> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    run_os(dir, &args)
}

/// Runs git with arguments that may carry a path, so a repository under a
/// non-UTF-8 folder still works.
pub(crate) fn run_os(dir: &Path, args: &[&OsStr]) -> Result<Run> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|err| anyhow!("git could not start: {err}"))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(anyhow!("git produced no output pipes"));
    };
    // Drain both pipes on their own threads: a child that fills one while the
    // parent waits for it to exit would otherwise deadlock.
    let stdout = std::thread::spawn(move || read_all(stdout));
    let stderr = std::thread::spawn(move || read_all(stderr));

    let deadline = Instant::now() + TIMEOUT;
    let mut timed_out = false;
    let mut ok = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                ok = status.success();
                break;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(Run {
        ok,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
    })
}

fn read_all(mut pipe: impl Read + Send + 'static) -> Vec<u8> {
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

/// One entry of `git worktree list --porcelain`.
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) locked: bool,
    pub(crate) prunable: bool,
}

/// Parses porcelain output: one entry per blank-line-separated block, the path
/// keeping its spaces and the branch stripped of `refs/heads/`.
pub(crate) fn parse_worktrees(porcelain: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;
    for line in porcelain.lines() {
        if line.trim().is_empty() {
            entries.extend(current.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        let value = value.trim();
        match key {
            "worktree" => {
                entries.extend(current.take());
                current = Some(Entry {
                    path: PathBuf::from(value),
                    branch: None,
                    head: None,
                    locked: false,
                    prunable: false,
                });
            }
            "HEAD" => {
                if let Some(entry) = current.as_mut() {
                    entry.head = Some(value.to_string());
                }
            }
            "branch" => {
                if let Some(entry) = current.as_mut() {
                    entry.branch = Some(value.trim_start_matches("refs/heads/").to_string());
                }
            }
            "locked" => {
                if let Some(entry) = current.as_mut() {
                    entry.locked = true;
                }
            }
            "prunable" => {
                if let Some(entry) = current.as_mut() {
                    entry.prunable = true;
                }
            }
            _ => {}
        }
    }
    entries.extend(current.take());
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_blocks_become_entries() {
        let porcelain = "\
worktree /repos/one
HEAD 8e4b6afa
branch refs/heads/main

worktree /repos/one/.worktrees/agent-fix
HEAD 1a2b3c4d
branch refs/heads/agent/fix
locked own reason

worktree /repos/one/.worktrees/gone
HEAD 5e6f7a8b
detached
prunable stale
";
        let entries = parse_worktrees(porcelain);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, PathBuf::from("/repos/one"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(entries[1].branch.as_deref(), Some("agent/fix"));
        assert!(entries[1].locked);
        assert!(!entries[1].prunable);
        assert_eq!(entries[2].branch, None);
        assert!(entries[2].prunable);
    }

    #[test]
    fn a_path_with_spaces_survives_the_split() {
        let entries = parse_worktrees("worktree /home/some one/repo\nHEAD abc\n");
        assert_eq!(entries[0].path, PathBuf::from("/home/some one/repo"));
    }
}
