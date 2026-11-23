//! Launching and interrogating a sibling suite app.
//!
//! The bundle installs every binary in one `bin/`, so a sibling is found next
//! to `current_exe()` first and on `PATH` second. That is a different question
//! from warp-station's `exe::locate` (which resolves user-configured CLIs and
//! probes the user-level install dirs); this module only knows the family's
//! own executables, which ship together.
//!
//! Std only, so the headless crates can carry it too.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a sibling gets to answer `--version`.
const VERSION_DEADLINE: Duration = Duration::from_secs(2);

/// The executable's file name on this platform.
pub fn exe_name(app: &str) -> String {
    #[cfg(windows)]
    {
        format!("{app}.exe")
    }
    #[cfg(not(windows))]
    {
        app.to_string()
    }
}

/// Where a sibling's binary is, if it is installed.
///
/// `app` is the binary stem (`warp-station`, `warp-notes`, …), not the display
/// name. The bundle's own `bin/` wins over a stale copy earlier on `PATH`.
pub fn binary(app: &str) -> Option<PathBuf> {
    let name = exe_name(app);
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
    {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// Whether this app can offer "Open <sibling>" at all.
pub fn installed(app: &str) -> bool {
    binary(app).is_some()
}

/// The facts a sibling prints for `--version`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub channel: String,
}

/// Ask an installed sibling for its version line.
///
/// Every suite app answers `--version` as `<name> <version> (<commit>) · <channel>`
/// before it opens a window, so this is the one fact source that works for a
/// binary this process did not compile. `None` means missing, a failed exit or
/// an unreadable line — or a child whose first line never arrives before the
/// deadline. A child that prints it is stopped as soon as the line arrives, so
/// it does not have to exit on its own.
pub fn version(app: &str) -> Option<VersionInfo> {
    version_from(&binary(app)?)
}

fn version_from(binary: &Path) -> Option<VersionInfo> {
    let mut child = command(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_DEADLINE;
    // Read stdout on its own thread. A descendant that inherits the pipe keeps
    // writing it after the child is gone, so waiting for EOF here would block
    // forever; the thread sends what it reads and this side stops at the
    // deadline.
    let mut pipe = child.stdout.take()?;
    let (sender, reader) = std::sync::mpsc::channel();
    let _ = std::thread::spawn(move || {
        use std::io::Read as _;

        let mut buffer = [0u8; 1024];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if sender.send(buffer[..read].to_vec()).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
    let mut stdout = Vec::new();
    loop {
        // The answer is the line, not the exit: stop the child as soon as a
        // complete line arrives, so a launcher that prints and then keeps
        // running (or opens a window) costs one line, not the deadline.
        match reader.recv_timeout(Duration::from_millis(10)) {
            Ok(bytes) => {
                stdout.extend_from_slice(&bytes);
                if stdout.contains(&b'\n') {
                    let _ = child.kill();
                    let _ = child.wait();
                    return first_line(&stdout);
                }
            }
            // EOF: the last line may have no terminator, so parse what arrived
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                return first_line(&stdout);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if Instant::now() < deadline => {}
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    // The child is gone, so the bytes it wrote are all there is; wait only for
    // its first line, not for the pipe to close.
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match reader.recv_timeout(remaining) {
            Ok(bytes) => {
                stdout.extend_from_slice(&bytes);
                if stdout.contains(&b'\n') {
                    break;
                }
            }
            // EOF: the last line may have no terminator, so parse what arrived
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    first_line(&stdout)
}

/// The child's first line, parsed.
fn first_line(stdout: &[u8]) -> Option<VersionInfo> {
    let stdout = String::from_utf8_lossy(stdout);
    parse_version_line(stdout.lines().next()?)
}

fn parse_version_line(line: &str) -> Option<VersionInfo> {
    let version = line.split_whitespace().find(|token| is_version(token))?;
    // The current line carries `(<commit>)`; an older install may still say
    // `(build 42 · <commit>)`, so the commit is the last segment in the parens.
    let commit = line
        .find('(')
        .and_then(|open| {
            let rest = &line[open + 1..];
            let close = rest.find(')')?;
            Some(rest[..close].rsplit('·').next().unwrap_or_default().trim())
        })
        .filter(|commit| !commit.is_empty())
        .unwrap_or("unknown");
    let channel = line
        .rsplit_once('·')
        .map(|(_, channel)| channel.trim())
        .filter(|channel| !channel.is_empty())
        .unwrap_or("dev");
    Some(VersionInfo {
        version: version.to_string(),
        commit: commit.to_string(),
        channel: channel.to_string(),
    })
}

fn is_version(token: &str) -> bool {
    let core = token.split(['-', '+']).next().unwrap_or(token);
    let mut parts = core.split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    [major, minor, patch]
        .into_iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn command(binary: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new(binary);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(binary)
    }
}

/// Spawns a sibling detached: the window outlives the app that opened it.
///
/// Windows gets `CREATE_NO_WINDOW` so a GUI that is really a console binary
/// leaves no console behind.
pub fn open(app: &str) -> Result<(), String> {
    let Some(binary) = binary(app) else {
        return Err(format!(
            "{app} is not installed next to this app or on PATH"
        ));
    };
    command(&binary)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not start {app}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_name_carries_the_platform_suffix() {
        let name = exe_name("warp-notes");
        if cfg!(windows) {
            assert_eq!(name, "warp-notes.exe");
        } else {
            assert_eq!(name, "warp-notes");
        }
    }

    #[test]
    fn an_app_nobody_installed_is_not_found() {
        assert!(!installed("warp-suite-definitely-not-installed"));
        assert!(version("warp-suite-definitely-not-installed").is_none());
        assert!(open("warp-suite-definitely-not-installed").is_err());
    }

    #[test]
    fn the_version_line_yields_the_app_facts() {
        assert_eq!(
            parse_version_line("Warp Station 0.1.51 (51c13d0) · dev"),
            Some(VersionInfo {
                version: "0.1.51".into(),
                commit: "51c13d0".into(),
                channel: "dev".into(),
            })
        );
        assert_eq!(
            parse_version_line("warp-snapit 0.1.4 (abc1234) · stable"),
            Some(VersionInfo {
                version: "0.1.4".into(),
                commit: "abc1234".into(),
                channel: "stable".into(),
            })
        );
        assert_eq!(parse_version_line("not a version line"), None);
        assert_eq!(parse_version_line("Warp Notes 1.2 (abc) · dev"), None);
        assert_eq!(
            parse_version_line("Warp Station 0.1.8 (build 42 · d904edc) · dev"),
            Some(VersionInfo {
                version: "0.1.8".into(),
                commit: "d904edc".into(),
                channel: "dev".into(),
            })
        );
        assert_eq!(
            parse_version_line("Warp Notes 0.1.4"),
            Some(VersionInfo {
                version: "0.1.4".into(),
                commit: "unknown".into(),
                channel: "dev".into(),
            })
        );
    }

    #[test]
    fn a_version_token_is_three_numeric_parts() {
        assert!(is_version("0.1.51"));
        assert!(is_version("0.2.0-rc.1"));
        assert!(!is_version("0.1"));
        assert!(!is_version("0.1.x"));
        assert!(!is_version("v0.1.51"));
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_script_answers_its_version_line() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("warp-fake");
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf 'Warp Fake 1.2.3 (abc1234) · dev\\n'\n",
        )
        .expect("write script");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");

        assert_eq!(
            version_from(&path),
            Some(VersionInfo {
                version: "1.2.3".into(),
                commit: "abc1234".into(),
                channel: "dev".into(),
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_holder_after_the_child_exits_does_not_block_the_probe() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("warp-fake");
        // the shell is done at once, but the background sleep inherits stdout
        // and keeps the pipe open: waiting for EOF would take the whole sleep
        std::fs::write(&path, "#!/bin/sh\nsleep 30 &\nprintf 'line\\n'\n").expect("write script");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");

        let started = Instant::now();
        // "line" is not a version line, but it must be read and rejected
        // rather than waiting on the pipe holder
        assert_eq!(version_from(&path), None);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the probe waited on the pipe holder for {:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_launcher_that_prints_and_keeps_running_is_stopped_after_its_line() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("warp-fake");
        // the line is the answer; the sleep must not hold the probe to the
        // deadline, and killing the shell's `exec` target leaves no orphan
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf 'Warp Fake 1.2.3 (abc1234) · dev\\n'\nexec sleep 30\n",
        )
        .expect("write script");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");

        let started = Instant::now();
        assert_eq!(
            version_from(&path),
            Some(VersionInfo {
                version: "1.2.3".into(),
                commit: "abc1234".into(),
                channel: "dev".into(),
            })
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the probe waited for the launcher to exit for {:?}",
            started.elapsed()
        );
    }
}
