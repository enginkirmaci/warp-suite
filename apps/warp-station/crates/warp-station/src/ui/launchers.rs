//! The repo launchers: one list behind the head's menu and the window's letter
//! keys, so a row can never advertise a key that does nothing. The first entry
//! is the primary action (the head's split button, on ↵); the list follows the
//! settings' enable flags.
//!
//! An entry describes its launch as a plain value: building the list starts
//! nothing, which is what lets a test pin it.

use crate::store::Store;
use gpui::{App, Entity};
use std::path::PathBuf;
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::AppSettings;
use warp_station_core::terminal;

/// What a launcher does when pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    VsCode {
        exe: String,
        profile: String,
    },
    Solution {
        solution: String,
        ide: String,
    },
    WarpAgent,
    /// A CLI in a new terminal at the repo.
    Cli {
        argv: Vec<String>,
        terminal_exe: String,
    },
    /// A CLI that needs a model nobody picked yet: pressing it says so.
    MissingModel {
        name: &'static str,
    },
    Folder,
    Terminal {
        terminal_exe: String,
    },
    Notes {
        exe: String,
        store_root: PathBuf,
        repo: String,
    },
}

impl Launch {
    /// Starts it; a launch that cannot start surfaces as an error toast.
    pub fn run(&self, folder: &str, store: &Entity<Store>, cx: &mut App) {
        let started = match self {
            Launch::VsCode { exe, profile } => terminal::open_in_vscode(folder, exe, profile),
            Launch::Solution { solution, ide } => terminal::open_solution(solution, ide),
            Launch::WarpAgent => {
                store.update(cx, |s, cx| s.open_in_warp(folder.to_string(), cx));
                Ok(())
            }
            Launch::Cli { argv, terminal_exe } => {
                terminal::launch_argv_in_terminal(folder, argv, terminal_exe)
            }
            Launch::MissingModel { name } => Err(format!("No {name} model — set it in Settings")),
            Launch::Folder => terminal::open_folder(folder),
            Launch::Terminal { terminal_exe } => terminal::open_terminal(folder, terminal_exe),
            Launch::Notes {
                exe,
                store_root,
                repo,
            } => warp_station_core::warp_notes::launch(
                exe,
                store_root,
                warp_station_core::warp_notes::NotesScope::Repo(repo),
                None,
            ),
        };
        if let Err(err) = started {
            store.update(cx, |s, cx| {
                s.notify_error(err);
                cx.notify();
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launcher {
    pub label: &'static str,
    /// The compact name the Overview's Open in card uses, where a 150px cell
    /// cannot hold the menu's verb ("Launch OpenCode CLI").
    pub short: &'static str,
    pub icon: &'static str,
    /// The letter the window keymap answers to, as the menu's keycap shows it.
    pub key: Option<&'static str>,
    pub launch: Launch,
}

/// The repo's launchers for these settings, primary first. Never empty: the
/// folder has no enable flag. Built lazily so the list pane, which only shows
/// the primary, does not allocate the whole vec per row and frame (audit B8).
fn launcher_iter<'a>(
    settings: &'a AppSettings,
    repo: &'a RepoEntry,
) -> impl Iterator<Item = Launcher> + 'a {
    let mut stage = 0u8;
    std::iter::from_fn(move || {
        loop {
            let current = stage;
            stage += 1;
            let next = match current {
                0 => settings.repos.enable_vscode.then(|| Launcher {
                    label: "Open in VS Code",
                    short: "VS Code",
                    icon: "icons/icon-code.svg",
                    key: None,
                    launch: Launch::VsCode {
                        exe: settings.repos.vscode_executable.clone(),
                        profile: settings.repos.vscode_profile.clone(),
                    },
                }),
                1 => {
                    // Visual Studio exists only on Windows
                    if cfg!(windows) && settings.repos.enable_visual_studio {
                        repo.solution_path.as_ref().map(|solution| Launcher {
                            label: "Open solution in Visual Studio",
                            short: "Visual Studio",
                            icon: "icons/icon-sln-box.svg",
                            key: Some("V"),
                            launch: Launch::Solution {
                                solution: solution.clone(),
                                ide: settings.repos.ide_executable.clone(),
                            },
                        })
                    } else {
                        None
                    }
                }
                2 => settings.warp_agent.enable_warp_agent.then_some(Launcher {
                    label: "Open in warp-agent",
                    short: "Warp Agent",
                    icon: "icons/icon-app-window.svg",
                    key: Some("A"),
                    launch: Launch::WarpAgent,
                }),
                3 => settings.opencode.enable_opencode.then(|| Launcher {
                    label: "Launch OpenCode CLI",
                    short: "OpenCode",
                    icon: "icons/icon-command.svg",
                    key: Some("O"),
                    launch: cli_launch(
                        settings,
                        &settings.repos.opencode_executable,
                        "opencode",
                        "OpenCode",
                        true,
                    ),
                }),
                4 => settings.deepseek.enable_deepseek.then(|| Launcher {
                    label: "Launch DeepSeek CLI",
                    short: "DeepSeek",
                    icon: "icons/icon-deepseek.svg",
                    key: Some("D"),
                    launch: cli_launch(
                        settings,
                        &settings.repos.deepseek_executable,
                        "dsh",
                        "DeepSeek",
                        false,
                    ),
                }),
                5 => Some(Launcher {
                    label: "Open folder",
                    short: "Folder",
                    icon: "icons/icon-folder.svg",
                    key: Some("F"),
                    launch: Launch::Folder,
                }),
                6 => settings.repos.enable_terminal.then(|| Launcher {
                    label: "Open terminal",
                    short: "Terminal",
                    icon: "icons/icon-terminal.svg",
                    key: Some("T"),
                    launch: Launch::Terminal {
                        terminal_exe: settings.repos.terminal_executable.clone(),
                    },
                }),
                7 => settings.warp_notes.enable_warp_notes.then(|| Launcher {
                    label: "Notes for this repo",
                    short: "Notes",
                    icon: "icons/icon-note.svg",
                    key: Some("N"),
                    launch: Launch::Notes {
                        exe: settings.warp_notes.executable.clone(),
                        store_root: warp_station_core::notes::resolve_store_root(
                            &settings.general.notes_store_path,
                        ),
                        repo: repo.name.clone(),
                    },
                }),
                _ => return None,
            };
            if next.is_some() {
                return next;
            }
            if stage > 7 {
                return None;
            }
        }
    })
}

/// The repo's launchers for these settings, primary first.
pub fn launchers(settings: &AppSettings, repo: &RepoEntry) -> Vec<Launcher> {
    launcher_iter(settings, repo).collect()
}

/// The launcher the list row shows. The folder launcher is unconditional, so
/// this is never `None`; the iterator stops before allocating the rest.
pub fn primary(settings: &AppSettings, repo: &RepoEntry) -> Launcher {
    launcher_iter(settings, repo)
        .next()
        .expect("the folder launcher is unconditional")
}

/// One CLI launch. The selected model belongs to the AI harness, so opencode
/// only launches with it when that harness is opencode; the dsh CLI boots its
/// own profile and takes no model.
fn cli_launch(
    settings: &AppSettings,
    exe_setting: &str,
    fallback: &'static str,
    name: &'static str,
    pass_model: bool,
) -> Launch {
    let exe = if exe_setting.trim().is_empty() {
        fallback.to_string()
    } else {
        exe_setting.trim().to_string()
    };
    let terminal_exe = settings.repos.terminal_executable.clone();
    if !pass_model || settings.warp_agent.selected_harness() != fallback {
        return Launch::Cli {
            argv: vec![exe],
            terminal_exe,
        };
    }
    let model = settings.opencode.model.trim();
    if model.is_empty() {
        return Launch::MissingModel { name };
    }
    Launch::Cli {
        argv: vec![exe, "--model".into(), model.to_string()],
        terminal_exe,
    }
}

/// Runs the launcher a window key names: ↵ is the primary, a letter its row.
/// Returns whether one answered.
pub fn run_key(key: &str, store: &Entity<Store>, cx: &mut App) -> bool {
    let hit = {
        let state = store.read(cx);
        state.selected_entry().and_then(|repo| {
            let mut list = launchers(&state.settings(), repo).into_iter();
            let launcher = if key == "enter" {
                list.next()
            } else {
                list.find(|launcher| {
                    launcher
                        .key
                        .is_some_and(|letter| letter.eq_ignore_ascii_case(key))
                })
            }?;
            let folder = repo.folder_path.to_string_lossy().to_string();
            Some((launcher.launch, folder))
        })
    };
    let Some((launch, folder)) = hit else {
        return false;
    };
    launch.run(&folder, store, cx);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> RepoEntry {
        RepoEntry {
            name: "alpha".into(),
            folder_path: PathBuf::from("/probe/alpha"),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: true,
            git_branch_published: true,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
        }
    }

    fn all_enabled() -> AppSettings {
        let mut settings = AppSettings::default();
        settings.repos.enable_vscode = true;
        settings.repos.enable_terminal = true;
        settings.opencode.enable_opencode = true;
        settings.deepseek.enable_deepseek = true;
        settings.warp_agent.enable_warp_agent = true;
        settings.warp_notes.enable_warp_notes = true;
        settings
    }

    fn cli_argv(list: &[Launcher], key: &str) -> Vec<String> {
        let launcher = list
            .iter()
            .find(|launcher| launcher.key == Some(key))
            .expect("the launcher is listed");
        match &launcher.launch {
            Launch::Cli { argv, .. } => argv.clone(),
            other => panic!("{key} is not a CLI launch: {other:?}"),
        }
    }

    #[test]
    fn the_editor_leads_and_every_letter_is_unique() {
        let list = launchers(&all_enabled(), &repo());

        assert_eq!(list[0].label, "Open in VS Code");
        assert_eq!(list[0].key, None, "the primary row answers to ↵");

        let letters: Vec<&str> = list.iter().filter_map(|launcher| launcher.key).collect();
        assert_eq!(letters, ["A", "O", "D", "F", "T", "N"]);
    }

    #[test]
    fn disabled_launchers_leave_and_the_folder_stays() {
        let mut settings = all_enabled();
        settings.repos.enable_vscode = false;
        settings.repos.enable_terminal = false;
        settings.opencode.enable_opencode = false;
        settings.deepseek.enable_deepseek = false;
        settings.warp_agent.enable_warp_agent = false;
        settings.warp_notes.enable_warp_notes = false;

        let list = launchers(&settings, &repo());

        assert_eq!(list.len(), 1, "the folder has no enable flag");
        assert_eq!(list[0].launch, Launch::Folder);
    }

    #[test]
    fn opencode_takes_the_model_only_under_its_own_harness() {
        let mut settings = all_enabled();
        settings.opencode.model = " zai/glm ".into();
        settings.repos.opencode_executable = "/opt/opencode".into();

        settings.warp_agent.harness = "opencode".into();
        assert_eq!(
            cli_argv(&launchers(&settings, &repo()), "O"),
            ["/opt/opencode", "--model", "zai/glm"]
        );

        settings.warp_agent.harness = "dsh".into();
        assert_eq!(
            cli_argv(&launchers(&settings, &repo()), "O"),
            ["/opt/opencode"],
            "the model belongs to the selected harness"
        );
    }

    #[test]
    fn a_missing_model_is_reported_instead_of_launching() {
        let mut settings = all_enabled();
        settings.warp_agent.harness = "opencode".into();
        settings.opencode.model = "  ".into();

        let list = launchers(&settings, &repo());
        let opencode = list
            .iter()
            .find(|launcher| launcher.key == Some("O"))
            .expect("listed while enabled");

        assert_eq!(opencode.launch, Launch::MissingModel { name: "OpenCode" });
    }

    /// The list row asks for the primary launcher only; it must be exactly
    /// the first entry the full list would produce, and the folder launcher
    /// is the unconditional floor (audit B8).
    #[test]
    fn primary_is_the_first_listed_launcher() {
        let settings = all_enabled();
        assert_eq!(
            primary(&settings, &repo()),
            launchers(&settings, &repo())[0]
        );

        let mut bare = AppSettings::default();
        bare.repos.enable_vscode = false;
        bare.repos.enable_terminal = false;
        bare.warp_agent.enable_warp_agent = false;
        bare.opencode.enable_opencode = false;
        bare.deepseek.enable_deepseek = false;
        assert_eq!(primary(&bare, &repo()).label, "Open folder");
    }

    #[test]
    fn deepseek_falls_back_to_its_binary_and_takes_no_model() {
        let mut settings = all_enabled();
        settings.opencode.model = "zai/glm".into();
        settings.repos.deepseek_executable = String::new();

        assert_eq!(cli_argv(&launchers(&settings, &repo()), "D"), ["dsh"]);
    }
}
