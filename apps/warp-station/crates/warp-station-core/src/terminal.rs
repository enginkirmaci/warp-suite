//! Opening things at a repo: folders and solutions through the desktop, VS
//! Code with the configured profile, and a command (opencode, a shell)
//! inside the configured terminal emulator — each emulator has its own
//! working-directory and -e argument shapes.

use crate::exe;
use std::path::{Path, PathBuf};

/// Suppresses the console window a console-subsystem helper would flash when
/// spawned by the GUI-subsystem exe (cmd /C start, .cmd/.bat shims). Never
/// applied to the terminal launch itself — a console terminal (powershell)
/// IS the window the user asked for.
fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// PowerShell single-quoted literal ('' escapes an embedded quote).
fn ps_single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Joins argv into a PowerShell command line; anything outside plain
/// path-ish tokens travels as a quoted literal so spaces and quotes survive.
fn ps_command_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-./:\\".contains(c))
            {
                arg.clone()
            } else {
                ps_single_quote(arg)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn powershell_argv(dir: &str, cmd: &[String]) -> Vec<String> {
    vec![
        "-NoExit".to_string(),
        "-Command".to_string(),
        format!(
            "Set-Location -LiteralPath {}; {}",
            ps_single_quote(dir),
            ps_command_line(cmd)
        ),
    ]
}

/// Opens a folder (or any path) with the desktop file manager.
pub fn open_folder(folder: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = "xdg-open";
    #[cfg(windows)]
    let command = "explorer";
    std::process::Command::new(command)
        .arg(folder)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("{command}: {err}"))
}

/// Opens an http(s) URL in the desktop's default browser — the rail's issue
/// rows hand GitHub links through here.
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("start: {err}"))
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let command = "open";
        #[cfg(all(unix, not(target_os = "macos")))]
        let command = "xdg-open";
        std::process::Command::new(command)
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("{command}: {err}"))
    }
}

/// Opens a solution file: a configured IDE wins, else the shell association
/// (Windows) or the Rider fallback.
pub fn open_solution(solution: &str, ide_executable: &str) -> Result<(), String> {
    let configured = ide_executable.trim();
    if !configured.is_empty() {
        let Some(path) = exe::locate(configured) else {
            return Err(format!(
                "'{configured}' not found on PATH or in the user install folders"
            ));
        };
        return std::process::Command::new(&path)
            .arg(solution)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("{}: {err}", path.display()));
    }
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", solution]);
        hide_console(&mut command);
        return command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("shell association: {err}"));
    }
    #[cfg(not(windows))]
    {
        let exe_name = "rider";
        let Some(path) = exe::locate(exe_name) else {
            return Err(format!(
                "'{exe_name}' not found on PATH or in the user install folders"
            ));
        };
        std::process::Command::new(&path)
            .arg(solution)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("{}: {err}", path.display()))
    }
}

/// Opens a repo folder in VS Code, with the configured profile when set.
pub fn open_in_vscode(folder: &str, vscode_executable: &str, profile: &str) -> Result<(), String> {
    let Some(path) = exe::locate(if vscode_executable.trim().is_empty() {
        "code"
    } else {
        vscode_executable
    }) else {
        return Err("VS Code executable not found".into());
    };
    let mut command = std::process::Command::new(&path);
    if !profile.trim().is_empty() {
        command.args(["--profile", profile.trim()]);
    }
    command.arg(folder);
    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        hide_console(&mut command);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// The launch plan for one code-search hit: VS Code at the file+line when the
/// configured executable resolves, else the system default editor. Pure, so
/// probes can assert the decision without spawning anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeOpenPlan {
    VsCode {
        executable: PathBuf,
        profile: String,
        file: String,
        line: u32,
    },
    SystemDefault {
        file: String,
    },
}

/// Decides how a code-search hit opens without launching anything: an empty
/// configured name falls back to `code`, and an unresolvable one to the
/// system default.
pub fn plan_code_open(
    file: &str,
    line: u32,
    vscode_executable: &str,
    profile: &str,
) -> CodeOpenPlan {
    let configured = if vscode_executable.trim().is_empty() {
        "code"
    } else {
        vscode_executable
    };
    match exe::locate(configured) {
        Some(executable) => CodeOpenPlan::VsCode {
            executable,
            profile: profile.trim().to_string(),
            file: file.to_string(),
            line,
        },
        None => CodeOpenPlan::SystemDefault {
            file: file.to_string(),
        },
    }
}

/// Executes a [CodeOpenPlan]: the VS Code launch carries `--profile` (only
/// when set) and `--goto file:line`; the fallback opens the file with the
/// desktop's default handler.
pub fn open_code_plan(plan: &CodeOpenPlan) -> Result<(), String> {
    match plan {
        CodeOpenPlan::VsCode {
            executable,
            profile,
            file,
            line,
        } => {
            let mut command = std::process::Command::new(executable);
            if !profile.is_empty() {
                command.args(["--profile", profile]);
            }
            command.args(["--goto", &format!("{file}:{line}")]);
            let lower = executable.to_string_lossy().to_lowercase();
            if lower.ends_with(".cmd") || lower.ends_with(".bat") {
                hide_console(&mut command);
            }
            command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("{}: {err}", executable.display()))
        }
        CodeOpenPlan::SystemDefault { file } => open_folder(file),
    }
}

/// Opens a file at its 1-based line in VS Code (the configured profile), or
/// with the system default editor when the executable does not resolve.
pub fn open_in_vscode_at(
    file: &str,
    line: u32,
    vscode_executable: &str,
    profile: &str,
) -> Result<(), String> {
    open_code_plan(&plan_code_open(file, line, vscode_executable, profile))
}

/// The configured terminal's base name, for shape lookup: split on either
/// separator, .exe stripped, lowercased on Windows where configs arrive as
/// full paths in any casing (WT.EXE, C:\...\WindowsTerminal.exe).
fn terminal_base(configured: &str) -> String {
    let base = configured
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    #[cfg(windows)]
    let base = base.to_lowercase();
    base.trim_end_matches(".exe").to_string()
}

/// Opens a plain shell at the folder using the configured terminal (or the
/// first probed one).
pub fn open_terminal(folder: &str, terminal_executable: &str) -> Result<(), String> {
    let configured = terminal_executable.trim();
    if !configured.is_empty() {
        let base = terminal_base(configured);
        let candidates = [configured.to_string(), base.clone()];
        for name in candidates {
            if let Some(path) = exe::locate(&name) {
                let mut command = std::process::Command::new(&path);
                apply_open_dir(&mut command, &base, folder);
                if Path::new(folder).exists() {
                    command.current_dir(folder);
                }
                return command
                    .spawn()
                    .map(|_| ())
                    .map_err(|err| format!("{}: {err}", path.display()));
            }
        }
        // configured-but-missing: fall through to probing only when unresolved
    }
    for name in probed_terminals() {
        if let Some(path) = exe::locate(name) {
            let mut command = std::process::Command::new(&path);
            apply_open_dir(&mut command, name, folder);
            if Path::new(folder).exists() {
                command.current_dir(folder);
            }
            return command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("{}: {err}", path.display()));
        }
    }
    Err("No terminal emulator found (tried the configured one plus the common ones)".into())
}

#[cfg(windows)]
fn probed_terminals() -> &'static [&'static str] {
    // wt first: the settings default and the stock Windows terminal
    &["wt", "alacritty", "wezterm", "pwsh", "powershell"]
}

#[cfg(not(windows))]
fn probed_terminals() -> &'static [&'static str] {
    &[
        "alacritty",
        "ghostty",
        "foot",
        "kitty",
        "wezterm",
        "gnome-terminal",
        "konsole",
        "xterm",
    ]
}

/// Runs a command (opencode, a shell, …) inside a terminal at the folder.
pub fn launch_in_terminal(
    folder: &str,
    command: &str,
    terminal_executable: &str,
) -> Result<(), String> {
    launch_argv_in_terminal(folder, &[command.to_string()], terminal_executable)
}

/// Runs a full argv (e.g. `opencode --model <id>`) inside a terminal at the
/// folder — the quick-launch path, where the command carries arguments.
pub fn launch_argv_in_terminal(
    folder: &str,
    argv: &[String],
    terminal_executable: &str,
) -> Result<(), String> {
    if argv.is_empty() {
        return Err("nothing to launch".into());
    }
    type ArgsFn = fn(&str, &[String]) -> Vec<String>;
    let table: &[(&str, ArgsFn)] = &[
        // wt resolves .cmd/.bat shims only through cmd (PATHEXT), so the
        // command runs under `cmd /k` — a direct commandline after `--` fails
        // with 0x80070002 for npm-style CLIs like opencode
        ("wt", |dir, cmd| {
            vec![
                "-d".to_string(),
                dir.to_string(),
                "--".to_string(),
                "cmd".to_string(),
                "/k".to_string(),
            ]
            .into_iter()
            .chain(cmd.iter().cloned())
            .collect()
        }),
        ("pwsh", powershell_argv),
        ("powershell", powershell_argv),
        ("alacritty", |dir, cmd| {
            vec!["--working-directory".into(), dir.into(), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("ghostty", |dir, cmd| {
            vec![format!("--working-directory={dir}"), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("foot", |dir, cmd| {
            vec![format!("--working-directory={dir}")]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("kitty", |dir, cmd| {
            vec!["--directory".into(), dir.into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("wezterm", |dir, cmd| {
            vec!["start".into(), "--cwd".into(), dir.into(), "--".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("gnome-terminal", |dir, cmd| {
            vec![format!("--working-directory={dir}"), "--".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("konsole", |dir, cmd| {
            vec!["--workdir".into(), dir.into(), "-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
        ("xterm", |_dir, cmd| {
            vec!["-e".into()]
                .into_iter()
                .chain(cmd.iter().cloned())
                .collect()
        }),
    ];
    // the configured terminal wins when it names a known shape
    let configured_base = terminal_base(terminal_executable);
    let mut ordered: Vec<(&str, ArgsFn)> = Vec::new();
    for entry in table {
        if !configured_base.is_empty() && *entry.0 == configured_base {
            ordered.insert(0, *entry);
        } else {
            ordered.push(*entry);
        }
    }
    for (name, args) in ordered {
        if let Some(path) = exe::locate(name) {
            let mut command_builder = std::process::Command::new(&path);
            command_builder.args(args(folder, argv));
            if Path::new(folder).exists() {
                command_builder.current_dir(folder);
            }
            match command_builder.spawn() {
                Ok(_) => {
                    tracing::info!("launched {name} running {:?} in {folder}", argv);
                    return Ok(());
                }
                Err(err) => return Err(err.to_string()),
            }
        }
    }
    Err("No terminal emulator found on PATH.".into())
}

fn apply_open_dir(command: &mut std::process::Command, terminal: &str, folder: &str) {
    match terminal {
        "wt" => {
            command.args(["-d", folder]);
        }
        "pwsh" | "powershell" => {
            command.args(["-NoExit", "-Command"]);
            command.arg(format!(
                "Set-Location -LiteralPath {}",
                ps_single_quote(folder)
            ));
        }
        "alacritty" => {
            command.args(["--working-directory", folder]);
        }
        "ghostty" | "foot" => {
            command.arg(format!("--working-directory={folder}"));
        }
        "kitty" => {
            command.args(["--directory", folder]);
        }
        "wezterm" => {
            command.args(["start", "--cwd", folder]);
        }
        "gnome-terminal" => {
            command.arg(format!("--working-directory={folder}"));
        }
        "konsole" => {
            command.args(["--workdir", folder]);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_quote_doubles_embedded_quotes() {
        assert_eq!(ps_single_quote("C:\\My Repos"), "'C:\\My Repos'");
        assert_eq!(ps_single_quote("it's"), "'it''s'");
    }

    #[test]
    fn ps_command_line_quotes_only_loose_tokens() {
        assert_eq!(
            ps_command_line(&["opencode".into(), "--model".into(), "my model".into()]),
            "opencode --model 'my model'"
        );
        assert_eq!(
            ps_command_line(&["C:\\tools\\my cli.exe".into()]),
            "'C:\\tools\\my cli.exe'"
        );
    }

    #[test]
    fn powershell_argv_sets_location_then_runs() {
        let argv = powershell_argv("C:\\repo", &["opencode".to_string()]);
        assert_eq!(argv[0], "-NoExit");
        assert_eq!(argv[1], "-Command");
        assert_eq!(argv[2], "Set-Location -LiteralPath 'C:\\repo'; opencode");
    }

    #[test]
    fn terminal_base_splits_any_separator() {
        assert_eq!(terminal_base("/usr/bin/alacritty"), "alacritty");
        assert_eq!(terminal_base("wezterm.exe"), "wezterm");
        assert_eq!(terminal_base("wt.exe.exe"), "wt");
        assert_eq!(terminal_base("  ghostty  "), "ghostty");
    }

    #[cfg(windows)]
    #[test]
    fn terminal_base_lowercases_windows_configs() {
        assert_eq!(terminal_base(r"C:\Tools\WT.EXE"), "wt");
        assert_eq!(terminal_base(r"..\Pwsh.Exe"), "pwsh");
    }
}
