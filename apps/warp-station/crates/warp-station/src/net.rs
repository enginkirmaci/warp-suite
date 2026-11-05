//! The net thread: a dedicated OS thread with its own tokio runtime doing all
//! the blocking work (repo scans, git status passes) off the UI. Two unbounded
//! async_channels bridge the sides — commands in, events out. The net thread
//! keeps no repo state; commands carry their own inputs (the store snapshots
//! what a pass needs), so results can never race a stale view of the list.

use async_channel::{Receiver, Sender};
use std::path::{Path, PathBuf};
use warp_station_core::repo_store::{self, RepoEntry, StatusProbe};
use warp_station_core::scan;
use warp_station_core::settings::ReposSettings;
use warp_station_core::warp::ModelChoice;

pub enum NetCommand {
    /// The listing scan: direct children of each scan root, user tags and
    /// last-known status carried over from `previous`, cache saved after.
    Scan {
        settings: ReposSettings,
        previous: Vec<RepoEntry>,
    },
    /// One status pass over every folder, 4-way parallel, burst-applied.
    ProbeAll {
        folders: Vec<PathBuf>,
    },
    /// The periodic background fetch: `git fetch --prune` over every folder
    /// with a remote, then one status pass — errors are traced, not toasted.
    FetchAll {
        folders: Vec<PathBuf>,
    },
    /// The Add-Repositories dialog's deep scan under one root.
    DeepScan {
        root: String,
        depth: i32,
        settings: ReposSettings,
    },
    /// The Clone-from-URL dialog's clone into a parent directory.
    Clone {
        url: String,
        parent_dir: String,
        name: String,
    },
    /// The Changes tab's shared load: status snapshot + branches + history in
    /// one pass, settled together so the tab paints complete.
    LoadChanges {
        folder: PathBuf,
    },
    /// One mutation; the net thread reloads the repo's changes and probes its
    /// status after it settles, success or failure.
    Git {
        folder: PathBuf,
        action: GitAction,
    },
    /// One change row's expandable patch.
    GetChangePatch {
        folder: PathBuf,
        path: String,
        staged: bool,
    },
    /// The history list's deeper commit cut plus the unpushed set.
    LoadHistory {
        folder: PathBuf,
    },
    /// One commit's drill-down: files with counts + message body.
    GetCommitDetails {
        folder: PathBuf,
        hash: String,
    },
    /// One drill-down file's patch.
    GetCommitFilePatch {
        folder: PathBuf,
        hash: String,
        path: String,
    },
    /// The GitHub activity pass over every repo (2-way parallel gh spawns).
    GitHubPass {
        folders: Vec<PathBuf>,
        gh_exe: String,
    },
    /// The Overview tab's repo details.
    GitHubDetails {
        folder: PathBuf,
        gh_exe: String,
    },
    /// The closed pull requests or issues, for the tab that shows them.
    GitHubClosed {
        folder: PathBuf,
        gh_exe: String,
        pull_requests: bool,
    },
    /// One pull request's full view.
    GitHubPull {
        folder: PathBuf,
        gh_exe: String,
        number: u64,
    },
    GitHubMerge {
        folder: PathBuf,
        gh_exe: String,
        number: u64,
    },
    /// The Azure DevOps activity pass (2-way parallel REST probes).
    AzurePass {
        folders: Vec<PathBuf>,
        pat: String,
        custom_base: String,
    },
    /// One AI wand run: the prompt is built here, the turn runs in
    /// warp-agent over its door (launching it when merely closed).
    Wand {
        id: String,
        kind: WandKind,
        folder: PathBuf,
        choice: ModelChoice,
        warp_exe: String,
    },
    /// Stops one live wand run (the progress chip's Cancel). The run answers
    /// with its usual WandDone, so the busy flag always clears.
    CancelWand {
        id: String,
    },
    /// The warp-agent model catalog for one harness through its door (+
    /// cache). Passive: a closed door falls back to the cache, never a launch.
    LoadModels {
        harness: String,
    },
    /// The yesterday-summary wand: day activity across every repo → prompt →
    /// one run in warp-agent's General workspace → the summary file path.
    WandSummary {
        id: String,
        folders: Vec<(String, PathBuf)>,
        day: chrono::NaiveDate,
        choice: ModelChoice,
        warp_exe: String,
        summaries_dir: PathBuf,
    },
    /// Opens a repo as a warp-agent workspace (added when missing).
    WarpOpen {
        folder: PathBuf,
        warp_exe: String,
    },
    /// NuGet Local watch controls; the config is captured from settings at
    /// press time (the service holds no settings handle).
    NugetStart {
        config: warp_station_core::nuget::NugetConfig,
    },
    NugetStop,
    NugetSetFolder {
        folder: String,
    },
    NugetRegister,
    /// Opens the native folder picker on the net thread (a native modal must
    /// not block the UI thread); the pick settles as [UiEvent::FolderPicked].
    BrowseFolder,
    /// One prompt-template op from the Settings card; the settled state
    /// returns as [UiEvent::TemplateLoaded] for every op.
    TemplateIo {
        id: String,
        op: TemplateOp,
    },
    /// One cross-repo ripgrep sweep for the titlebar search popup; the hits
    /// return as [UiEvent::CodeSearchSettled] with the dispatching generation.
    CodeSearch {
        generation: u64,
        term: String,
        folders: Vec<PathBuf>,
        rg_exe: PathBuf,
    },
}

/// A prompt-template management op (Settings → AI features card).
#[derive(Clone, Debug)]
pub enum TemplateOp {
    Read,
    Write(String),
    Reset,
}

/// Which generator a wand run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WandKind {
    Commit,
    Readme,
}

/// Every state-changing git operation the detail pane drives. The label rides
/// the result for surfacing.
#[derive(Clone, Debug)]
pub enum GitAction {
    StageAll,
    UnstageAll,
    Stage(String),
    Unstage(String),
    DiscardStaged(Vec<String>),
    DiscardUnstaged(Vec<String>),
    DiscardFile {
        path: String,
        is_staged: bool,
    },
    Commit(String),
    Checkout(String),
    CreateBranch {
        name: String,
        start: Option<String>,
        checkout: bool,
    },
    DeleteBranch(String),
    Fetch,
    Pull,
    Push,
    Publish {
        branch: String,
    },
    /// Pull then push; a failed pull skips the push.
    Sync,
    /// `git revert --no-edit` on one commit (the drill-down's Revert).
    Revert(String),
}

/// The word a running git action's button shows. `label` is [`GitAction::label`].
pub fn git_working_word(label: &str) -> &'static str {
    match label {
        "fetch" => "Fetching…",
        "pull" => "Pulling…",
        "push" => "Pushing…",
        "publish" => "Publishing…",
        "sync" => "Syncing…",
        "commit" => "Committing…",
        "checkout" => "Switching…",
        "revert" => "Reverting…",
        _ => "Working…",
    }
}

/// The word the button holds for a moment once the action landed.
pub fn git_done_word(label: &str) -> &'static str {
    match label {
        "fetch" => "Fetched",
        "pull" => "Pulled",
        "push" => "Pushed",
        "publish" => "Published",
        "sync" => "Synced",
        "commit" => "Committed",
        _ => "Done",
    }
}

impl GitAction {
    pub fn label(&self) -> &'static str {
        match self {
            GitAction::StageAll => "stage all",
            GitAction::UnstageAll => "unstage all",
            GitAction::Stage(_) => "stage",
            GitAction::Unstage(_) => "unstage",
            GitAction::DiscardStaged(_) => "discard staged",
            GitAction::DiscardUnstaged(_) => "discard unstaged",
            GitAction::DiscardFile { .. } => "discard",
            GitAction::Commit(_) => "commit",
            GitAction::Checkout(_) => "checkout",
            GitAction::CreateBranch { .. } => "create branch",
            GitAction::DeleteBranch(_) => "delete branch",
            GitAction::Fetch => "fetch",
            GitAction::Pull => "pull",
            GitAction::Push => "push",
            GitAction::Publish { .. } => "publish",
            GitAction::Sync => "sync",
            GitAction::Revert(_) => "revert",
        }
    }

    /// The action's target (branch name) for toast texts.
    pub fn detail(&self) -> Option<String> {
        match self {
            GitAction::Checkout(target) | GitAction::Revert(target) => Some(target.clone()),
            GitAction::DeleteBranch(name) => Some(name.clone()),
            GitAction::CreateBranch { name, .. } | GitAction::Publish { branch: name } => {
                Some(name.clone())
            }
            _ => None,
        }
    }
}

/// One warp-agent workspace as the repo rows see it: the running chats'
/// titles (empty when the workspace is idle).
#[derive(Clone, Debug, Default)]
pub struct WarpWorkspaceStatus {
    pub connected: bool,
    pub running: Vec<String>,
}

/// Parses a door status snapshot's `workspaces` payload.
fn parse_warp_status(workspaces: &serde_json::Value) -> Vec<(String, WarpWorkspaceStatus)> {
    workspaces
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|ws| {
                    let path = ws.get("path")?.as_str()?.to_string();
                    let running: Vec<String> = ws
                        .get("sessions")
                        .and_then(serde_json::Value::as_array)
                        .map(|sessions| {
                            sessions
                                .iter()
                                .filter(|s| {
                                    s.get("running")
                                        .and_then(serde_json::Value::as_bool)
                                        .unwrap_or(false)
                                })
                                .filter_map(|s| {
                                    s.get("title")
                                        .and_then(serde_json::Value::as_str)
                                        .map(str::to_string)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((
                        path,
                        WarpWorkspaceStatus {
                            connected: ws
                                .get("connected")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                            running,
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub enum UiEvent {
    ScanSettled {
        repos: Vec<RepoEntry>,
        removed: Vec<String>,
    },
    /// `after_fetch` marks the pass a FetchAll ends with, so the store can
    /// re-read the selected repo's open workspace from the post-fetch state.
    StatusSettled {
        probes: Vec<StatusProbe>,
        after_fetch: bool,
    },
    DeepScanSettled {
        root: String,
        found: Vec<warp_station_core::scan::ScannedRepo>,
    },
    CloneSettled {
        url: String,
        parent_dir: String,
        name: String,
        result: warp_station_core::model::GitSyncResult,
    },
    ChangesLoaded {
        folder: String,
        snapshot: Option<warp_station_core::model::GitChangeSnapshot>,
        branches: Vec<warp_station_core::model::GitBranchRef>,
        recent: Vec<warp_station_core::model::GitCommitInfo>,
        unpushed: Option<std::collections::HashSet<String>>,
    },
    GitDone {
        folder: String,
        label: &'static str,
        error: Option<String>,
        /// A commit's short hash on success.
        commit_hash: Option<String>,
        /// The action's branch/commit target, for toast texts.
        detail: Option<String>,
    },
    ChangePatchLoaded {
        folder: String,
        path: String,
        staged: bool,
        patch: String,
    },
    HistoryLoaded {
        folder: String,
        commits: Vec<warp_station_core::model::GitCommitInfo>,
        unpushed: Option<std::collections::HashSet<String>>,
    },
    CommitDetailsLoaded {
        folder: String,
        details: Option<warp_station_core::model::GitCommitDetails>,
    },
    CommitFilePatchLoaded {
        folder: String,
        hash: String,
        path: String,
        patch: String,
    },
    GitHubSettled {
        items: Vec<(String, warp_station_core::github::GitHubSnapshot)>,
    },
    GitHubDetailsLoaded {
        folder: String,
        details: Option<warp_station_core::github::GitHubRepoDetails>,
    },
    GitHubClosedLoaded {
        folder: String,
        pull_requests: bool,
        items: Option<Vec<warp_station_core::github::GitHubItem>>,
    },
    GitHubPullLoaded {
        folder: String,
        number: u64,
        pull: Option<warp_station_core::github::GitHubPull>,
    },
    GitHubMerged {
        folder: String,
        number: u64,
        result: Result<(), String>,
    },
    AzureSettled {
        items: Vec<(String, warp_station_core::azure::AzureSnapshot)>,
    },
    WandLog {
        wand_id: String,
        line: String,
    },
    WandDone {
        wand_id: String,
        kind: WandKind,
        result: Result<String, String>,
    },
    ModelsLoaded {
        harness: String,
        catalog: warp_station_core::opencode::ModelCatalog,
    },
    /// The summary wand finished (Ok = the written file path).
    SummaryDone {
        wand_id: String,
        result: Result<String, String>,
    },
    /// warp-agent came up or went away; `connected` gates the status chip.
    WarpAvailability {
        connected: bool,
    },
    /// A warp-agent status snapshot: workspace folder → running chats.
    WarpStatus {
        workspaces: Vec<(String, WarpWorkspaceStatus)>,
    },
    /// A warp-agent open/handoff settled; the error toasts on failure.
    WarpOpened {
        folder: String,
        error: Option<String>,
    },
    /// A NuGet watch service state change (full snapshot).
    Nuget(warp_station_core::nuget::NugetSnapshot),
    /// The system-wide Ctrl+Shift+V fired; the copy already ran on the
    /// trigger thread (wl-copy / raw SetClipboardData — gpui's Wayland write
    /// is focus-gated), the store only shows the toast.
    ClipboardHotkey {
        message: &'static str,
    },
    /// The native folder picker closed; None = cancelled or unavailable.
    FolderPicked {
        path: Option<String>,
    },
    /// A prompt template's settled state after any TemplateIo op: the raw
    /// body plus whether it differs from the built-in default.
    TemplateLoaded {
        id: String,
        content: String,
        customized: bool,
    },
    /// The cross-repo code search settled; the store applies it only when the
    /// generation is still the latest (a newer keystroke wins).
    CodeSearchSettled {
        generation: u64,
        hits: Vec<warp_station_core::code_search::CodeHit>,
    },
}

pub fn spawn_net_thread() -> (
    std::thread::JoinHandle<()>,
    Sender<NetCommand>,
    Receiver<UiEvent>,
) {
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<NetCommand>();
    let (evt_tx, evt_rx) = async_channel::unbounded::<UiEvent>();
    let handle = std::thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            setup_clipboard_hotkey(evt_tx.clone());
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("net runtime");
            rt.block_on(net_main(cmd_rx, evt_tx));
        })
        .expect("net thread");
    (handle, cmd_tx, evt_rx)
}

/// Registers the system-wide Ctrl+Shift+V while the Clipboard Password tool
/// is enabled; a disabled tool must not hold a global key grab. Hyprland gets
/// a runtime bind (re-added each launch), Windows a RegisterHotKey thread.
/// The in-window shortcut in MainWindow stays as the fallback for sessions
/// where neither grab lands.
fn setup_clipboard_hotkey(evt_tx: Sender<UiEvent>) {
    let enabled = warp_station_core::settings::SettingsService::open()
        .get()
        .clipboard_password
        .enable_clipboard_password;
    if !enabled {
        return;
    }
    #[cfg(windows)]
    {
        if let Err(err) = warp_station_core::hotkey::spawn_ctrl_shift_v(move || {
            let message = warp_station_core::hotkey::copy_password_to_clipboard();
            let _ = evt_tx.send_blocking(UiEvent::ClipboardHotkey { message });
        }) {
            tracing::warn!("global Ctrl+Shift+V unavailable: {err:#}");
        }
    }
    #[cfg(target_os = "linux")]
    {
        // the linux trigger is the ping socket in net_main, not this channel
        let _ = evt_tx;
        match warp_station_core::hotkey::register_hyprland_bind() {
            Ok(()) => tracing::info!("global Ctrl+Shift+V bound via Hyprland"),
            Err(err) => tracing::warn!("global Ctrl+Shift+V unavailable: {err:#}"),
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = evt_tx;
    }
}

/// The ping socket the Hyprland bind's payload process connects to: one
/// "copy-password" line in, one ClipboardHotkey event out, "ok" back.
#[cfg(target_os = "linux")]
async fn listen_clipboard_ping(evt_tx: Sender<UiEvent>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some(path) = warp_station_core::hotkey::clipboard_socket_path() else {
        tracing::warn!("XDG_RUNTIME_DIR unset; global Ctrl+Shift+V ping socket disabled");
        return;
    };
    // a crashed previous run leaves a stale socket file behind
    let _ = std::fs::remove_file(&path);
    let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::warn!("clipboard ping socket bind failed: {err}");
            return;
        }
    };
    while let Ok((mut stream, _)) = listener.accept().await {
        let evt_tx = evt_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 32];
            if stream.read(&mut buf).await.unwrap_or(0) == 0 {
                return;
            }
            if buf.starts_with(b"copy-password") {
                let message = warp_station_core::hotkey::copy_password_to_clipboard().await;
                let _ = evt_tx.send(UiEvent::ClipboardHotkey { message }).await;
                let _ = stream.write_all(b"ok\n").await;
            }
        });
    }
}

/// Reads a repo's workspace off the async threads, as the event the views
/// take it from.
async fn workspace_loaded(folder: PathBuf) -> UiEvent {
    let (snapshot, branches, recent, unpushed) = tokio::task::spawn_blocking({
        let folder = folder.clone();
        move || {
            (
                warp_station_core::git::read::change_snapshot(&folder),
                warp_station_core::git::read::branches(&folder).unwrap_or_default(),
                warp_station_core::git::read::recent_commits(&folder).unwrap_or_default(),
                warp_station_core::git::read::unpushed_hashes(&folder),
            )
        }
    })
    .await
    .unwrap_or((None, Vec::new(), Vec::new(), None));
    UiEvent::ChangesLoaded {
        folder: folder.to_string_lossy().to_string(),
        snapshot,
        branches,
        recent,
        unpushed,
    }
}

/// Arrow-key browsing asks for one workspace per row; a short trailing wait
/// lets only the row the burst lands on pay for a full git read.
const SELECTION_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(100);

/// Per repo, the newest workspace read asked for. Reads run side by side, so
/// one that finishes behind a newer read, or behind a mutation of that repo,
/// must not land: it would put an older snapshot back.
#[derive(Clone, Default)]
struct WorkspaceReads(std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>>);

impl WorkspaceReads {
    /// Claims the newest slot for `folder`, staling every read before it.
    fn begin(&self, folder: &Path) -> u64 {
        let mut reads = self.0.lock().expect("workspace reads lock");
        let newest = reads.entry(folder.to_path_buf()).or_default();
        *newest += 1;
        *newest
    }

    fn is_newest(&self, folder: &Path, read: u64) -> bool {
        self.0.lock().expect("workspace reads lock").get(folder) == Some(&read)
    }
}

/// The command loop runs one command at a time and a pass holds it for as long
/// as the pass takes, so the reads Overview waits on must not queue behind it.
/// This takes them off the channel as they arrive and hands everything else on
/// to the loop, in order.
fn spawn_read_intake(
    cmd_rx: Receiver<NetCommand>,
    evt_tx: Sender<UiEvent>,
    workspace_reads: WorkspaceReads,
) -> Receiver<NetCommand> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    let (queue_tx, queue_rx) = async_channel::unbounded();
    tokio::spawn(async move {
        let details_reads = Arc::new(AtomicU64::new(0));
        while let Ok(cmd) = cmd_rx.recv().await {
            match cmd {
                NetCommand::LoadChanges { folder } => {
                    let read = workspace_reads.begin(&folder);
                    let reads = workspace_reads.clone();
                    let evt_tx = evt_tx.clone();
                    tokio::spawn(async move {
                        // a burst of selections (held arrow keys, a fast click
                        // through the list) must not start one full status +
                        // diff read per row: wait it out, then check again so
                        // only the newest read does any work
                        tokio::time::sleep(SELECTION_DEBOUNCE).await;
                        if !reads.is_newest(&folder, read) {
                            return;
                        }
                        let loaded = workspace_loaded(folder.clone()).await;
                        if reads.is_newest(&folder, read) {
                            let _ = evt_tx.send_blocking(loaded);
                        }
                    });
                }
                NetCommand::GitHubDetails { folder, gh_exe } => {
                    // `gh` is slow and ↑↓ through the list asks once per row,
                    // so the burst is waited out and only its last row asks
                    let generation = details_reads.fetch_add(1, Ordering::SeqCst) + 1;
                    let newest = details_reads.clone();
                    let evt_tx = evt_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        if newest.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let details =
                            warp_station_core::github::repo_details(&folder, &gh_exe).await;
                        let _ = evt_tx.send_blocking(UiEvent::GitHubDetailsLoaded {
                            folder: folder.to_string_lossy().to_string(),
                            details,
                        });
                    });
                }
                NetCommand::GitHubClosed {
                    folder,
                    gh_exe,
                    pull_requests,
                } => {
                    let evt_tx = evt_tx.clone();
                    tokio::spawn(async move {
                        let items = warp_station_core::github::closed_items(
                            &folder,
                            &gh_exe,
                            pull_requests,
                        )
                        .await;
                        let _ = evt_tx.send_blocking(UiEvent::GitHubClosedLoaded {
                            folder: folder.to_string_lossy().to_string(),
                            pull_requests,
                            items,
                        });
                    });
                }
                NetCommand::GitHubPull {
                    folder,
                    gh_exe,
                    number,
                } => {
                    let evt_tx = evt_tx.clone();
                    tokio::spawn(async move {
                        let pull =
                            warp_station_core::github::pull_request(&folder, &gh_exe, number).await;
                        let _ = evt_tx.send_blocking(UiEvent::GitHubPullLoaded {
                            folder: folder.to_string_lossy().to_string(),
                            number,
                            pull,
                        });
                    });
                }
                NetCommand::GitHubMerge {
                    folder,
                    gh_exe,
                    number,
                } => {
                    let evt_tx = evt_tx.clone();
                    tokio::spawn(async move {
                        let result =
                            warp_station_core::github::merge_pull_request(&folder, &gh_exe, number)
                                .await;
                        let _ = evt_tx.send_blocking(UiEvent::GitHubMerged {
                            folder: folder.to_string_lossy().to_string(),
                            number,
                            result,
                        });
                    });
                }
                queued => {
                    if queue_tx.send(queued).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    queue_rx
}

/// The background lane: repo scans and the activity passes run one at a time,
/// off the command loop, so a minutes-long `git fetch` or `gh` sweep never
/// holds a user action (stage, history, a chat send) behind it (audit B1).
async fn background_loop(rx: Receiver<NetCommand>, evt_tx: Sender<UiEvent>) {
    while let Ok(cmd) = rx.recv().await {
        match cmd {
            NetCommand::Scan { settings, previous } => {
                let scanned = tokio::task::spawn_blocking(move || scan::scan(&settings, None))
                    .await
                    .unwrap_or_default();
                let (merged, removed) = repo_store::merge_scan(&previous, &scanned.repos);
                if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &merged) {
                    tracing::warn!("could not save repo cache: {err}");
                }
                let _ = evt_tx.send_blocking(UiEvent::ScanSettled {
                    repos: merged,
                    removed,
                });
            }
            NetCommand::FetchAll { folders } => {
                warp_station_core::refresh::throttled_pass(
                    folders.clone(),
                    3,
                    |folder| async move {
                        let outcome = warp_station_core::git::cli::fetch(&folder).await;
                        if !outcome.ok {
                            tracing::warn!(
                                "auto fetch failed in {}: {}",
                                folder.display(),
                                outcome.error.as_deref().unwrap_or("git rejected it")
                            );
                        }
                    },
                )
                .await;
                // the settled probes refresh the ahead/behind badges through
                // the same burst-apply the manual passes use
                let probes = probe_pass(folders).await;
                let _ = evt_tx.send_blocking(UiEvent::StatusSettled {
                    probes,
                    after_fetch: true,
                });
            }
            NetCommand::GitHubPass { folders, gh_exe } => {
                use std::sync::{Arc, Mutex};
                let collected = Arc::new(Mutex::new(Vec::new()));
                let sink = collected.clone();
                let gh = gh_exe.clone();
                warp_station_core::refresh::throttled_pass(folders, 2, move |folder| {
                    let sink = sink.clone();
                    let gh = gh.clone();
                    async move {
                        let snapshot = warp_station_core::github::refresh(&folder, &gh).await;
                        sink.lock()
                            .unwrap()
                            .push((folder.to_string_lossy().to_string(), snapshot));
                    }
                })
                .await;
                let items = Arc::try_unwrap(collected)
                    .map(|m| m.into_inner().unwrap())
                    .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::GitHubSettled { items });
            }
            NetCommand::AzurePass {
                folders,
                pat,
                custom_base,
            } => {
                use std::sync::{Arc, Mutex};
                let collected = Arc::new(Mutex::new(Vec::new()));
                let sink = collected.clone();
                warp_station_core::refresh::throttled_pass(folders, 2, move |folder| {
                    let sink = sink.clone();
                    let pat = pat.clone();
                    let custom_base = custom_base.clone();
                    async move {
                        let snapshot =
                            warp_station_core::azure::refresh(&folder, &pat, &custom_base).await;
                        sink.lock()
                            .unwrap()
                            .push((folder.to_string_lossy().to_string(), snapshot));
                    }
                })
                .await;
                let items = Arc::try_unwrap(collected)
                    .map(|m| m.into_inner().unwrap())
                    .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::AzureSettled { items });
            }
            _ => {}
        }
    }
}

async fn net_main(cmd_rx: Receiver<NetCommand>, evt_tx: Sender<UiEvent>) {
    let workspace_reads = WorkspaceReads::default();
    let cmd_rx = spawn_read_intake(cmd_rx, evt_tx.clone(), workspace_reads.clone());

    // live wand runs: id → the run's kill signal
    let wands: std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, async_channel::Sender<()>>>,
    > = std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

    // the NuGet watch service outlives its page; its snapshots forward as
    // UiEvents on the same bridge
    let (nuget_events, nuget_receiver) =
        async_channel::unbounded::<warp_station_core::nuget::NugetSnapshot>();
    let nuget = std::sync::Arc::new(warp_station_core::nuget::NugetService::new(nuget_events));
    {
        let evt_tx = evt_tx.clone();
        tokio::spawn(async move {
            while let Ok(snapshot) = nuget_receiver.recv().await {
                let _ = evt_tx.send_blocking(UiEvent::Nuget(snapshot));
            }
        });
    }

    #[cfg(target_os = "linux")]
    {
        let evt_tx = evt_tx.clone();
        tokio::spawn(async move {
            listen_clipboard_ping(evt_tx).await;
        });
    }

    // the warp-agent door watcher: availability for the status chip and
    // status pushes for the repo rows. Passive — it never launches; runs
    // that need warp-agent launch it themselves.
    {
        let evt_tx = evt_tx.clone();
        tokio::spawn(async move {
            warp_status_loop(evt_tx).await;
        });
    }

    // Scans and activity passes run on their own lane: a long fetch or gh
    // sweep must not hold a user action in this queue behind it (audit B1).
    let (background_tx, background_rx) = async_channel::unbounded::<NetCommand>();
    {
        let evt_tx = evt_tx.clone();
        tokio::spawn(background_loop(background_rx, evt_tx));
    }

    while let Ok(cmd) = cmd_rx.recv().await {
        if matches!(
            &cmd,
            NetCommand::Scan { .. }
                | NetCommand::FetchAll { .. }
                | NetCommand::GitHubPass { .. }
                | NetCommand::AzurePass { .. }
        ) {
            let _ = background_tx.send(cmd).await;
            continue;
        }
        match cmd {
            // routed to the background lane before this match
            NetCommand::Scan { .. }
            | NetCommand::FetchAll { .. }
            | NetCommand::GitHubPass { .. }
            | NetCommand::AzurePass { .. } => {}
            NetCommand::ProbeAll { folders } => {
                let probes = probe_pass(folders).await;
                let _ = evt_tx.send_blocking(UiEvent::StatusSettled {
                    probes,
                    after_fetch: false,
                });
            }
            NetCommand::DeepScan {
                root,
                depth,
                settings,
            } => {
                // the dialog's scan is a user action: run it now rather than
                // queueing it behind a background pass
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let scan_root = PathBuf::from(root.clone());
                    let found = tokio::task::spawn_blocking(move || {
                        scan::scan_folder(&settings, &scan_root, Some(depth))
                    })
                    .await
                    .unwrap_or_default();
                    let _ = evt_tx.send_blocking(UiEvent::DeepScanSettled {
                        root,
                        found: found.repos,
                    });
                });
            }
            NetCommand::Clone {
                url,
                parent_dir,
                name,
            } => {
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let result =
                        warp_station_core::git::cli::clone(&url, Path::new(&parent_dir), &name)
                            .await;
                    let _ = evt_tx.send_blocking(UiEvent::CloneSettled {
                        url,
                        parent_dir,
                        name,
                        result,
                    });
                });
            }
            NetCommand::GetChangePatch {
                folder,
                path,
                staged,
            } => {
                let patch = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let path = path.clone();
                    move || warp_station_core::git::read::change_file_patch(&folder, &path, staged)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::ChangePatchLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    path,
                    staged,
                    patch,
                });
            }
            NetCommand::WandSummary {
                id,
                folders,
                day,
                choice,
                warp_exe,
                summaries_dir,
            } => {
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    // gather every repo's day activity (blocking reads)
                    let activities = tokio::task::spawn_blocking(move || {
                        let mut collected: Vec<(String, warp_station_core::model::GitDayActivity)> =
                            Vec::new();
                        for (name, folder) in &folders {
                            if let Some(activity) =
                                warp_station_core::git::read::day_activity(folder, day)
                                && !activity.is_empty()
                            {
                                collected.push((name.clone(), activity));
                            }
                        }
                        collected.sort_by(|a, b| a.0.cmp(&b.0));
                        collected
                    })
                    .await
                    .unwrap_or_default();

                    if activities.is_empty() {
                        let _ = evt_tx.send_blocking(UiEvent::SummaryDone {
                            wand_id: id,
                            result: Err("no repository activity that day".into()),
                        });
                        return;
                    }

                    let date_label = day.format("%A, %b %-d, %Y").to_string();
                    let prompt = warp_station_core::opencode::daily_summary_prompt(
                        &date_label,
                        &warp_station_core::opencode::format_repositories_block(&activities),
                    );

                    // the summary runs silently in warp-agent's General
                    // workspace: the note lands here, nothing surfaces there
                    let run = match warp_station_core::warp::connect_or_launch(
                        std::path::Path::new(&warp_exe),
                        None,
                    )
                    .await
                    {
                        Ok(mut client) => {
                            let logs = evt_tx.clone();
                            let log_id = id.clone();
                            let (_kill_tx, kill_rx) = async_channel::unbounded::<()>();
                            client
                                .run_prompt(
                                    warp_station_core::warp::GENERAL,
                                    &prompt,
                                    &choice,
                                    true,
                                    move |line| {
                                        let _ = logs.send_blocking(UiEvent::WandLog {
                                            wand_id: log_id.clone(),
                                            line,
                                        });
                                    },
                                    kill_rx,
                                )
                                .await
                        }
                        Err(err) => Err(err),
                    };

                    let outcome = match run {
                        Ok(summary) => {
                            let summary: String = summary.chars().take(30000).collect();
                            let path = summaries_dir.join(format!("{}.md", day.format("%Y-%m-%d")));
                            match std::fs::create_dir_all(&summaries_dir)
                                .and_then(|_| std::fs::write(&path, format!("{summary}\n")))
                            {
                                Ok(()) => Ok(path.to_string_lossy().to_string()),
                                Err(err) => Err(format!("write failed: {err}")),
                            }
                        }
                        Err(err) => Err(format!("{err:#}")),
                    };
                    let _ = evt_tx.send_blocking(UiEvent::SummaryDone {
                        wand_id: id,
                        result: outcome,
                    });
                });
            }
            NetCommand::LoadModels { harness } => {
                // passive on purpose: loading the catalog must never launch
                // warp-agent; only runs launch it. A closed door serves the
                // cache so the settings pickers keep working offline. Spawned
                // because the door round-trip can take the full command answer
                // timeout, which must not stall other user actions.
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let catalog = match warp_station_core::warp::Client::connect().await {
                        Ok(mut client) => match client.models(&harness).await {
                            Ok(catalog) => {
                                warp_station_core::opencode::store_harness_catalog(
                                    &harness, &catalog,
                                );
                                catalog
                            }
                            Err(_) => warp_station_core::opencode::cached_harness_catalog(&harness),
                        },
                        Err(_) => warp_station_core::opencode::cached_harness_catalog(&harness),
                    };
                    let _ = evt_tx.send_blocking(UiEvent::ModelsLoaded { harness, catalog });
                });
            }
            NetCommand::NugetStart { config } => nuget.start(config),
            NetCommand::NugetStop => nuget.stop(),
            NetCommand::NugetSetFolder { folder } => nuget.set_folder(folder),
            NetCommand::NugetRegister => nuget.register_source(),
            NetCommand::BrowseFolder => {
                let evt_tx = evt_tx.clone();
                tokio::task::spawn_blocking(move || {
                    let path = rfd::FileDialog::new()
                        .pick_folder()
                        .map(|folder| folder.to_string_lossy().to_string());
                    let _ = evt_tx.send_blocking(UiEvent::FolderPicked { path });
                });
            }
            NetCommand::TemplateIo { id, op } => {
                // unknown ids are ignored: no file, no event
                if let Some(template) = warp_station_core::opencode::PROMPT_TEMPLATES
                    .iter()
                    .find(|t| t.id == id)
                {
                    match op {
                        TemplateOp::Read => {}
                        TemplateOp::Write(content) => {
                            if let Err(err) =
                                warp_station_core::opencode::write_template(&id, &content)
                            {
                                tracing::warn!("template write failed: {err}");
                            }
                        }
                        TemplateOp::Reset => {
                            if let Err(err) = warp_station_core::opencode::reset_template(&id) {
                                tracing::warn!("template reset failed: {err}");
                            }
                        }
                    }
                    // every op answers with the settled on-disk state so the
                    // rows' badges and the open editor refresh together
                    let content = warp_station_core::opencode::read_template(&id)
                        .unwrap_or_else(|| template.default.to_string());
                    let customized = warp_station_core::opencode::is_customized(&id);
                    let _ = evt_tx.send_blocking(UiEvent::TemplateLoaded {
                        id,
                        content,
                        customized,
                    });
                }
            }
            NetCommand::CodeSearch {
                generation,
                term,
                folders,
                rg_exe,
            } => {
                // spawned, not awaited: an 8s ripgrep sweep must not stall the
                // net loop behind it
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let hits = warp_station_core::code_search::run_code_search(
                        &rg_exe,
                        &folders,
                        &term,
                        warp_station_core::code_search::CODE_SEARCH_CAP,
                    )
                    .await;
                    let _ = evt_tx.send_blocking(UiEvent::CodeSearchSettled { generation, hits });
                });
            }
            NetCommand::Wand {
                id,
                kind,
                folder,
                choice,
                warp_exe,
            } => {
                let evt_tx = evt_tx.clone();
                let wands = wands.clone();
                tokio::spawn(async move {
                    let (kill_tx, kill_rx) = async_channel::unbounded::<()>();
                    wands.lock().await.insert(id.clone(), kill_tx);

                    // build the prompt for the kind (blocking reads)
                    let prompt = tokio::task::spawn_blocking({
                        let folder = folder.clone();
                        move || build_wand_prompt(kind, &folder)
                    })
                    .await
                    .unwrap_or_default();

                    // the turn runs silently in this repo's warp-agent
                    // workspace: station shows the wand's progress and result,
                    // warp-agent's own UI stays untouched
                    let workspace = folder.to_string_lossy().to_string();
                    let result = match warp_station_core::warp::connect_or_launch(
                        std::path::Path::new(&warp_exe),
                        Some(&folder),
                    )
                    .await
                    {
                        Ok(mut client) => {
                            let logs = evt_tx.clone();
                            let log_id = id.clone();
                            client
                                .run_prompt(
                                    &workspace,
                                    &prompt,
                                    &choice,
                                    true,
                                    move |line| {
                                        let _ = logs.send_blocking(UiEvent::WandLog {
                                            wand_id: log_id.clone(),
                                            line,
                                        });
                                    },
                                    kill_rx,
                                )
                                .await
                        }
                        Err(err) => Err(err),
                    };

                    wands.lock().await.remove(&id);
                    let _ = evt_tx.send_blocking(UiEvent::WandDone {
                        wand_id: id,
                        kind,
                        result: result.map_err(|err| format!("{err:#}")),
                    });
                });
            }
            NetCommand::CancelWand { id } => {
                // the wand task answers its own WandDone after the run stops,
                // so the store's busy flag still clears through one path
                if let Some(kill) = wands.lock().await.get(&id) {
                    let _ = kill.send(()).await;
                }
            }
            NetCommand::WarpOpen { folder, warp_exe } => {
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let result = match warp_station_core::warp::connect_or_launch(
                        std::path::Path::new(&warp_exe),
                        Some(&folder),
                    )
                    .await
                    {
                        Ok(mut client) => client
                            .open_workspace(&folder)
                            .await
                            .map_err(|err| format!("{err:#}")),
                        Err(err) => Err(format!("{err:#}")),
                    };
                    let _ = evt_tx.send_blocking(UiEvent::WarpOpened {
                        folder: folder.to_string_lossy().to_string(),
                        error: result.err(),
                    });
                });
            }
            // the intake answers these before they reach this queue
            NetCommand::GitHubDetails { .. }
            | NetCommand::GitHubClosed { .. }
            | NetCommand::GitHubPull { .. }
            | NetCommand::GitHubMerge { .. }
            | NetCommand::LoadChanges { .. } => {}
            NetCommand::LoadHistory { folder } => {
                let (commits, unpushed) = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    move || {
                        (
                            warp_station_core::git::read::history_commits(&folder, 200)
                                .unwrap_or_default(),
                            warp_station_core::git::read::unpushed_hashes(&folder),
                        )
                    }
                })
                .await
                .unwrap_or((Vec::new(), None));
                let _ = evt_tx.send_blocking(UiEvent::HistoryLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    commits,
                    unpushed,
                });
            }
            NetCommand::GetCommitDetails { folder, hash } => {
                let details = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let hash = hash.clone();
                    move || warp_station_core::git::read::commit_details(&folder, &hash)
                })
                .await
                .ok()
                .flatten();
                let _ = evt_tx.send_blocking(UiEvent::CommitDetailsLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    details,
                });
            }
            NetCommand::GetCommitFilePatch { folder, hash, path } => {
                let patch = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let hash = hash.clone();
                    let path = path.clone();
                    move || warp_station_core::git::read::commit_file_patch(&folder, &hash, &path)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::CommitFilePatchLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    hash,
                    path,
                    patch,
                });
            }
            NetCommand::Git { folder, action } => {
                let (ok, error, commit_hash) = run_git_action(&folder, &action).await;
                // a remote pick lands on its local tracking branch; name that
                // in the toast instead of the remote-tracking ref the user
                // clicked
                let detail = match &action {
                    GitAction::Checkout(target) => {
                        warp_station_core::git::read::remote_tracking_local(&folder, target)
                            .map(|(local, _)| local)
                            .or_else(|| action.detail())
                    }
                    _ => action.detail(),
                };
                let _ = evt_tx.send_blocking(UiEvent::GitDone {
                    folder: folder.to_string_lossy().to_string(),
                    label: action.label(),
                    error: if ok { None } else { error },
                    commit_hash,
                    detail,
                });
                // every mutation re-checks the repo it touched: the fresh
                // status and the rebuilt changes snapshot land together, and
                // any read still in flight predates them
                workspace_reads.begin(&folder);
                let _ = evt_tx.send_blocking(workspace_loaded(folder.clone()).await);
                let _ = evt_tx.send_blocking(UiEvent::StatusSettled {
                    probes: probe_pass(vec![folder]).await,
                    after_fetch: false,
                });
            }
        }
    }
}

/// Builds a wand prompt for the kind from local git state.
fn build_wand_prompt(kind: WandKind, folder: &Path) -> String {
    match kind {
        WandKind::Commit => {
            let (patch, files, subjects) =
                warp_station_core::git::read::commit_prompt_context(folder).unwrap_or_default();
            let file_list = files.join(", ");
            let subjects: Vec<String> = subjects.into_iter().take(5).collect();
            let context = if subjects.is_empty() {
                String::new()
            } else {
                format!("Recent commit subjects for tone:\n{}", subjects.join("\n"))
            };
            warp_station_core::opencode::commit_message_prompt(&file_list, &patch, &context)
        }
        WandKind::Readme => {
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let tree = warp_station_core::opencode::file_tree(folder, 200);
            warp_station_core::opencode::readme_prompt(&name, &tree, "")
        }
    }
}

/// Watches the warp-agent door: availability changes and status pushes for
/// the repo-row indicators. Passive — a missing warp-agent only reports
/// "away"; the runs launch it themselves when they need it.
async fn warp_status_loop(evt_tx: Sender<UiEvent>) {
    let mut announced = false;
    loop {
        if !warp_station_core::settings::SettingsService::open()
            .get()
            .warp_agent
            .enable_warp_agent
        {
            announce_warp(&evt_tx, &mut announced, false);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        warp_status_cycle(
            warp_station_core::warp::Client::connect,
            &evt_tx,
            &mut announced,
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// Sends one availability change when it differs from what was already
/// announced, so the chip hears only the real transitions.
fn announce_warp(evt_tx: &Sender<UiEvent>, announced: &mut bool, connected: bool) {
    if *announced != connected {
        *announced = connected;
        let _ = evt_tx.send_blocking(UiEvent::WarpAvailability { connected });
    }
}

/// One watcher cycle: connect, announce green, ride the push stream until the
/// door drops, announce gray. `connect` is a seam so tests can point the cycle
/// at endpoint files in a temp dir.
async fn warp_status_cycle<F, Fut>(connect: F, evt_tx: &Sender<UiEvent>, announced: &mut bool)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<warp_station_core::warp::Client>>,
{
    let Ok(mut client) = connect().await else {
        announce_warp(evt_tx, announced, false);
        return;
    };
    announce_warp(evt_tx, announced, true);
    if let Ok(workspaces) = client.status().await {
        let _ = evt_tx.send_blocking(UiEvent::WarpStatus {
            workspaces: parse_warp_status(&workspaces),
        });
    }
    // ride the push stream until the door drops
    while let Ok(frame) = client.next_frame().await {
        if frame.get("event").and_then(serde_json::Value::as_str) == Some("status")
            && let Some(workspaces) = frame.pointer("/data/workspaces")
        {
            let _ = evt_tx.send_blocking(UiEvent::WarpStatus {
                workspaces: parse_warp_status(workspaces),
            });
        }
    }
    announce_warp(evt_tx, announced, false);
}

async fn run_git_action(
    folder: &Path,
    action: &GitAction,
) -> (bool, Option<String>, Option<String>) {
    use warp_station_core::git::cli;
    match action {
        GitAction::StageAll => cli::stage_all(folder).await.into_outcome(),
        GitAction::UnstageAll => cli::unstage_all(folder).await.into_outcome(),
        GitAction::Stage(path) => cli::stage(folder, path).await.into_outcome(),
        GitAction::Unstage(path) => cli::unstage(folder, path).await.into_outcome(),
        GitAction::DiscardStaged(paths) => {
            (cli::discard_paths(folder, paths, true).await, None, None)
        }
        GitAction::DiscardUnstaged(paths) => {
            (cli::discard_paths(folder, paths, false).await, None, None)
        }
        GitAction::DiscardFile { path, is_staged } => (
            cli::discard_file(folder, path, *is_staged).await,
            None,
            None,
        ),
        GitAction::Commit(message) => match cli::commit(folder, message).await {
            Some(hash) => (true, None, Some(hash)),
            None => (
                false,
                Some("nothing staged, or git rejected it".into()),
                None,
            ),
        },
        GitAction::Checkout(branch) => {
            let outcome = cli::checkout(folder, branch).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::CreateBranch {
            name,
            start,
            checkout,
        } => {
            let result = cli::create_branch(folder, name, start.as_deref(), *checkout).await;
            (result.success, result.error, None)
        }
        GitAction::DeleteBranch(name) => {
            let result = cli::delete_branch(folder, name).await;
            (result.success, result.error, None)
        }
        GitAction::Fetch => {
            let outcome = cli::fetch(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Pull => {
            let outcome = cli::pull(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Push => {
            let outcome = cli::push(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Publish { branch } => {
            let result = cli::publish_branch(folder, branch).await;
            (result.success, result.error, None)
        }
        GitAction::Sync => {
            // pull then push; a failed pull skips the push, first error wins
            let pulled = cli::pull(folder).await;
            if !pulled.ok {
                (false, pulled.error, None)
            } else {
                let pushed = cli::push(folder).await;
                (pushed.ok, pushed.error, None)
            }
        }
        GitAction::Revert(hash) => {
            let outcome = cli::revert_commit(folder, hash).await;
            (outcome.ok, outcome.error, None)
        }
    }
}

trait OutcomeExt {
    fn into_outcome(self) -> (bool, Option<String>, Option<String>);
}
impl OutcomeExt for warp_station_core::git::cli::GitOutcome {
    fn into_outcome(self) -> (bool, Option<String>, Option<String>) {
        (
            self.ok,
            self.error
                .or_else(|| (!self.ok).then(|| "git rejected it".into())),
            None,
        )
    }
}

/// One throttled status pass: git2 probes under a 4-way semaphore, every
/// failure settling as a zeroed snapshot for its folder. Collected, then
/// returned for one burst apply.
async fn probe_pass(folders: Vec<PathBuf>) -> Vec<StatusProbe> {
    use std::sync::{Arc, Mutex};
    let collected = Arc::new(Mutex::new(Vec::with_capacity(folders.len())));
    let sink = collected.clone();
    warp_station_core::refresh::throttled_pass(folders, 4, move |folder| {
        let collected = sink.clone();
        async move {
            let fetch_seed = repo_store::read_last_fetch_seed(&folder);
            let probe = warp_station_core::git::read::probe(&folder)
                .map(|p| StatusProbe {
                    folder_path: folder.to_string_lossy().to_string(),
                    branch_name: p.branch_name,
                    modified_count: p.modified_count,
                    ahead_count: p.ahead_count,
                    behind_count: p.behind_count,
                    last_commit_at: p.last_commit_at,
                    last_fetch_at: fetch_seed,
                    has_remote: p.has_remote,
                    branch_on_remote: p.branch_on_remote,
                })
                .unwrap_or_else(|| StatusProbe {
                    folder_path: folder.to_string_lossy().to_string(),
                    ..Default::default()
                });
            collected.lock().unwrap().push(probe);
        }
    })
    .await;
    let mut probes = Arc::try_unwrap(collected)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    // keep the list's burst-apply order stable (by folder path)
    probes.sort_by(|a, b| a.folder_path.cmp(&b.folder_path));
    probes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mutation stales the reads of its own repo only: dropping another
    /// repo's read would leave the rail waiting for an answer that never comes.
    #[test]
    fn a_mutation_stales_only_its_own_repos_reads() {
        let reads = WorkspaceReads::default();
        let mutated = Path::new("/probe/mutated");
        let other = Path::new("/probe/other");
        let stale = reads.begin(mutated);
        let unrelated = reads.begin(other);

        reads.begin(mutated);

        assert!(!reads.is_newest(mutated, stale));
        assert!(reads.is_newest(other, unrelated));
    }

    /// The availability announcements must ride the real transitions: green
    /// right after a door answers, gray right after the door drops. The old
    /// loop carried the green over the ride and delivered it only on the drop
    /// — the chip lit up a beat before going gray, exactly backwards.
    #[tokio::test]
    async fn warp_status_cycle_announces_up_then_away() {
        use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ipc.port"), format!("{port}\n")).unwrap();
        std::fs::write(dir.path().join("ipc.token"), "test-token\n").unwrap();

        // one door connection: auth, answer the status snapshot (echoing the
        // request's id), hang up — the hang-up ends the cycle's ride loop
        let door = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            write_half.write_all(b"{\"ok\": true}\n").await.unwrap();
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let request: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            let status = serde_json::json!({
                "id": request.get("id").cloned().unwrap_or(serde_json::json!(0)),
                "ok": true,
                "workspaces": [{
                    "path": "/repo",
                    "connected": true,
                    "sessions": [{"title": "fix the chip", "running": true}],
                }]
            });
            write_half
                .write_all(format!("{status}\n").as_bytes())
                .await
                .unwrap();
        });

        let (evt_tx, evt_rx) = async_channel::unbounded::<UiEvent>();
        let mut announced = false;
        warp_status_cycle(
            || warp_station_core::warp::Client::connect_in(dir.path()),
            &evt_tx,
            &mut announced,
        )
        .await;
        drop(evt_tx);
        door.await.unwrap();

        let events: Vec<UiEvent> = drain_events(&evt_rx);
        assert!(matches!(
            events.first(),
            Some(UiEvent::WarpAvailability { connected: true })
        ));
        let UiEvent::WarpStatus { workspaces } = &events[1] else {
            panic!("the second event should be the status snapshot");
        };
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].0, "/repo");
        assert!(workspaces[0].1.connected);
        assert_eq!(workspaces[0].1.running, vec!["fix the chip".to_string()]);
        assert!(matches!(
            events.last(),
            Some(UiEvent::WarpAvailability { connected: false })
        ));
        assert_eq!(events.len(), 3);
        assert!(!announced);
    }

    /// The dedupe on the away side: while the chip was never green, a missed
    /// door stays silent; once the door was announced up, the next missed
    /// door reports away exactly once.
    #[tokio::test]
    async fn warp_status_cycle_announces_away_on_missed_door() {
        let dir = tempfile::tempdir().unwrap();
        let (evt_tx, evt_rx) = async_channel::unbounded::<UiEvent>();
        let mut announced = false;

        // never green: a missed door dedupes to silence (the chip is already gray)
        warp_status_cycle(
            || warp_station_core::warp::Client::connect_in(dir.path()),
            &evt_tx,
            &mut announced,
        )
        .await;
        assert_eq!(drain_events(&evt_rx).len(), 0);
        assert!(!announced);

        // the door was up: the next missed door reports away once
        announced = true;
        warp_status_cycle(
            || warp_station_core::warp::Client::connect_in(dir.path()),
            &evt_tx,
            &mut announced,
        )
        .await;
        drop(evt_tx);
        let events = drain_events(&evt_rx);
        assert!(matches!(
            events.first(),
            Some(UiEvent::WarpAvailability { connected: false })
        ));
        assert_eq!(events.len(), 1);
        assert!(!announced);
    }

    /// async_channel's receiver has no try_iter; drain what is queued.
    fn drain_events(rx: &async_channel::Receiver<UiEvent>) -> Vec<UiEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }
}
