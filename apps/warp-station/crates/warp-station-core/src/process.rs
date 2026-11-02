//! One-shot CLI runner for every spawned child (git, gh, opencode): spawn
//! with drained pipes, bound the run with a timeout, kill the
//! child's whole process tree when the bound expires, and optionally cap the
//! stdout read (the read task kills the tree once the head is captured, so a
//! chatty child never materializes megabytes). Spawn failures propagate as
//! `Err` so callers can keep their per-service "CLI unavailable" handling.

use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Set by Electron hosts (VS Code & forks) that the app may have been launched
/// from; it leaks into every child and turns Electron-packaged CLIs into bare
/// Node processes.
pub const ELECTRON_RUN_AS_NODE: &str = "ELECTRON_RUN_AS_NODE";

/// Every GUI-spawned CLI on Windows needs this, or the child pops a fresh
/// console window (the release exe is GUI-subsystem and has none to inherit).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
pub struct ProcessRunOptions<'a> {
    pub file_name: &'a str,
    pub args: &'a [String],
    pub working_dir: Option<&'a std::path::Path>,
    pub timeout: Option<Duration>,
    /// Gives CLIs that read piped stdin an instant EOF instead of a hang.
    pub close_stdin: bool,
    /// Removes ELECTRON_RUN_AS_NODE from the child's environment.
    pub strip_electron_variable: bool,
    /// Once this many bytes are captured the child's tree is killed and the
    /// run completes with the head (`truncated` flags it — the killed exit
    /// code is non-zero by contract).
    pub max_output_bytes: Option<usize>,
    /// Environment additions/overrides for the child (GIT_TERMINAL_PROMPT,
    /// GH_PROMPT, …).
    pub environment: Vec<(String, String)>,
}

pub struct ProcessRunResult {
    /// The child's exit code; `None` when it never completed (spawn failure or
    /// timeout).
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
}

impl ProcessRunResult {
    /// True only when the child ran and exited 0.
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }
}

pub async fn run(options: ProcessRunOptions<'_>) -> anyhow::Result<ProcessRunResult> {
    let mut command = Command::new(options.file_name);
    command
        .args(options.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if options.close_stdin {
            Stdio::null()
        } else {
            Stdio::inherit()
        });
    if let Some(dir) = options.working_dir {
        command.current_dir(dir);
    }
    if options.strip_electron_variable {
        command.env_remove(ELECTRON_RUN_AS_NODE);
    }
    for (name, value) in &options.environment {
        command.env(name, value);
    }
    // Own process group on Unix so a timeout can kill the whole tree with one
    // signal; Windows tree-kill goes through taskkill /T.
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command.spawn()?;

    let pid = child.id();
    let cap = options.max_output_bytes;
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
        let mut out = Vec::new();
        let Some(pipe) = stdout_pipe.as_mut() else {
            return out;
        };
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(cap) = cap {
                        let remaining = cap.saturating_sub(out.len());
                        out.extend_from_slice(&buf[..n.min(remaining)]);
                        if out.len() >= cap {
                            // head captured — kill the tree so the pipes EOF and
                            // the exit wait completes
                            kill_tree(pid);
                            break;
                        }
                    } else {
                        out.extend_from_slice(&buf[..n]);
                    }
                }
            }
        }
        out
    });
    let stderr_task = tokio::spawn(async move {
        let mut err = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut err).await;
        }
        err
    });

    let deadline = options.timeout.map(|t| Instant::now() + t);
    let mut timed_out = false;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(err) => return Err(err.into()),
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            kill_tree(pid);
            timed_out = true;
            break None;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };

    if timed_out {
        // the tree kill closes the pipes; give the tasks a beat to settle
        let _ = tokio::time::timeout(Duration::from_secs(3), stdout_task).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), stderr_task).await;
        return Ok(ProcessRunResult {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            truncated: false,
        });
    }

    // the child exited, but a grandchild that inherited its pipes (a git
    // hook daemon, a credential helper) can keep them open forever — bound
    // the drain like the timeout path does, or the net thread wedges here
    let stdout = match tokio::time::timeout(Duration::from_secs(3), stdout_task).await {
        Ok(Ok(bytes)) => bytes,
        _ => Vec::new(),
    };
    let stderr = match tokio::time::timeout(Duration::from_secs(3), stderr_task).await {
        Ok(Ok(bytes)) => bytes,
        _ => Vec::new(),
    };
    let truncated = options.max_output_bytes.is_some_and(|c| stdout.len() >= c);
    Ok(ProcessRunResult {
        exit_code: exit.and_then(|s| s.code()),
        stdout: String::from_utf8_lossy(&stdout).to_string(),
        stderr: String::from_utf8_lossy(&stderr).to_string(),
        timed_out: false,
        truncated,
    })
}

/// Kills a child's entire process tree: the process group on Unix (the spawn
/// put it in its own group), `taskkill /T /F` on Windows.
pub fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        // SAFETY: the spawn put the child in its own process group, so the
        // negative pid signals the whole tree; SIGKILL needs no preconditions
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("taskkill");
        command
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
        let _ = command.status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_output_and_exit_code() {
        let result = run(ProcessRunOptions {
            file_name: "echo",
            args: &["hello".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(result.succeeded());
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn timeout_kills_and_reports() {
        let result = run(ProcessRunOptions {
            file_name: "sleep",
            args: &["30".to_string()],
            timeout: Some(Duration::from_millis(300)),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(result.timed_out);
        assert!(!result.succeeded());
    }

    #[tokio::test]
    async fn cap_kills_chatty_child() {
        let result = run(ProcessRunOptions {
            file_name: "yes",
            args: &["x".to_string()],
            max_output_bytes: Some(4096),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(result.truncated);
        assert_eq!(result.stdout.len(), 4096);
    }
}
