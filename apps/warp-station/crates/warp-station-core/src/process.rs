//! One-shot CLI runner for every spawned child (git, gh, opencode): spawn
//! with drained pipes, bound the run with a timeout, let the UI stop it with a
//! cancel flag, kill the child's whole process tree when either fires, and
//! optionally cap the
//! stdout read (the read task kills the tree once the head is captured, so a
//! chatty child never materializes megabytes). Spawn failures propagate as
//! `Err` so callers can keep their per-service "CLI unavailable" handling.

use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Launchers (a terminal, the file manager, a sibling app) hand off a process
/// this one does not own; nobody waits for it, so on Unix it stays a zombie
/// until this process exits. `spawn_detached` reaps it on a detached thread.
pub trait DetachedSpawn {
    /// Spawn with the configured stdio/env and reap the child in the
    /// background; only the spawn failure is reported.
    fn spawn_detached(&mut self) -> std::io::Result<()>;
}

impl DetachedSpawn for std::process::Command {
    fn spawn_detached(&mut self) -> std::io::Result<()> {
        let mut child = self.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// Set by Electron hosts (VS Code & forks) that the app may have been launched
/// from; it leaks into every child and turns Electron-packaged CLIs into bare
/// Node processes.
pub const ELECTRON_RUN_AS_NODE: &str = "ELECTRON_RUN_AS_NODE";

/// Every GUI-spawned CLI on Windows needs this, or the child pops a fresh
/// console window (the release exe is GUI-subsystem and has none to inherit).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A one-way stop request for a running child: the UI flips it, the runner
/// kills the process tree and reports the run as cancelled. Used by the clone
/// so a large transfer has no wall-clock cap but can still be stopped.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct ProcessRunOptions<'a> {
    pub file_name: &'a str,
    pub args: &'a [String],
    pub working_dir: Option<&'a std::path::Path>,
    pub timeout: Option<Duration>,
    /// A user stop request: the flag flips, the tree is killed, and the result
    /// reports `cancelled` instead of a failure.
    pub cancel: Option<CancelFlag>,
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
    /// The child's exit code; `None` when it never completed (spawn failure,
    /// timeout or cancel).
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// The cancel flag flipped and the tree was killed.
    pub cancelled: bool,
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
            let mut buf = [0u8; 8192];
            loop {
                match pipe.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // capped like stdout, but never killed for it: stderr
                        // is diagnostic, and the process may still exit 0.
                        // Draining the rest keeps the child from blocking on a
                        // full pipe.
                        let keep = cap.map_or(n, |cap| cap.saturating_sub(err.len()).min(n));
                        err.extend_from_slice(&buf[..keep]);
                    }
                }
            }
        }
        err
    });

    let deadline = options.timeout;
    let cancel = options.cancel.clone();
    let mut timed_out = false;
    let mut cancelled = false;
    let mut exit = if deadline.is_none() && cancel.is_none() {
        Some(child.wait().await?)
    } else {
        tokio::select! {
            status = child.wait() => Some(status?),
            _ = async {
                match deadline {
                    Some(limit) => tokio::time::sleep(limit).await,
                    None => std::future::pending().await,
                }
            } => {
                timed_out = true;
                None
            }
            _ = async {
                match &cancel {
                    // one poll beats a wake-up channel: the UI flips a plain
                    // atomic and the lag is imperceptible next to a clone
                    Some(flag) => {
                        while !flag.is_cancelled() {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                    None => std::future::pending().await,
                }
            } => {
                cancelled = true;
                None
            }
        }
    };

    // A stop that fires in the same tick as the exit must not discard work
    // that already landed: the child's own status wins over the flag.
    if (timed_out || cancelled)
        && let Ok(Some(status)) = child.try_wait()
    {
        timed_out = false;
        cancelled = false;
        exit = Some(status);
    }

    if timed_out || cancelled {
        kill_tree(pid);
        // reap the killed child so it does not linger as a zombie
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
        // the tree kill closes the pipes; give the tasks a beat to settle
        let _ = tokio::time::timeout(Duration::from_secs(3), stdout_task).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), stderr_task).await;
        return Ok(ProcessRunResult {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out,
            cancelled,
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
        cancelled: false,
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
        assert!(!result.cancelled);
        assert!(!result.succeeded());
    }

    /// The dialog's Cancel is not a failure: the tree dies and the run reports
    /// `cancelled`, so a clone can tell "the user stopped it" from "git failed".
    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_kills_and_reports_cancelled() {
        let cancel = CancelFlag::new();
        let flip = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                cancel.cancel();
            })
        };
        let started = std::time::Instant::now();
        let result = run(ProcessRunOptions {
            file_name: "sleep",
            args: &["30".to_string()],
            cancel: Some(cancel),
            ..Default::default()
        })
        .await
        .unwrap();
        flip.await.unwrap();
        assert!(result.cancelled);
        assert!(!result.timed_out);
        assert!(!result.succeeded());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the kill returns promptly"
        );
    }

    /// stderr is capped too, but a chatty stderr must not kill a process that
    /// is otherwise succeeding (audit B12).
    #[cfg(unix)]
    #[tokio::test]
    async fn caps_stderr_without_killing_the_child() {
        let script = "i=0; while [ $i -lt 3000 ]; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\\n' >&2; i=$((i+1)); done";
        let args = ["-c".to_string(), script.to_string()];
        let result = run(ProcessRunOptions {
            file_name: "sh",
            args: &args,
            max_output_bytes: Some(4096),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(result.succeeded(), "stderr chatter is not a failure");
        assert_eq!(result.stderr.len(), 4096, "stderr honors the cap");
        assert!(result.stdout.is_empty());
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
