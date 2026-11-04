//! The app store: one entity owning all app state; views hold only ephemeral
//! UI state, snapshot `store.read(cx)` at render top and end mutations with
//! `cx.notify()`. The store never holds UI handles.

pub mod projection;

use crate::net::{GitAction, NetCommand, TemplateOp, UiEvent, WandKind, WarpWorkspaceStatus};
use gpui::{App, Context};
use std::sync::Arc;
use warp_station_core::repo_store::{self, RepoEntry};
use warp_station_core::settings::{AppSettings, RepoSortMode, ReposSettings, SettingsService};

/// How long a button holds its past tense.
const FINISHED_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);

pub struct Store {
    pub net: async_channel::Sender<NetCommand>,
    settings: Arc<SettingsService>,
    pub repos: Vec<RepoEntry>,
    /// Checked tag-filter chips (OR semantics: a repo matches when it has any).
    pub tag_filters: Vec<String>,
    /// Session-only: while on, archived repos join the list below the live
    /// ones. Never persisted.
    pub show_archived: bool,
    /// Session-only scope: only repos that need attention — uncommitted work,
    /// an open issue, or commits still to push. Never persisted.
    pub attention_only: bool,
    pub sort_mode: RepoSortMode,
    /// false = the mode's default "best-first" read, true = flipped. Seeded
    /// from the persisted direction when set, else the mode's default.
    pub sort_ascending: bool,
    /// Session-only solo filter: while set, only that folder group's rows
    /// are projected (the header's filter icon; Clear removes it).
    pub group_filter: Option<String>,
    /// Collapsible folder groups on/off; seeded from the persisted
    /// setting (or the WARP_STATION_GROUPS E2E hook), flipped by the Settings save.
    pub group_by_root: bool,
    /// Folder levels the group headers stand at, counting up from the scan
    /// roots' shared folder (1 = that folder); seeded from the persisted
    /// setting, flipped by the Settings save.
    pub group_depth: usize,
    /// Collapsed group keys (folder paths, or "other"); mirrored into
    /// `ReposSettings::collapsed_groups` on every toggle.
    pub collapsed_groups: Option<Vec<String>>,
    /// The list's rows: folder group headers interleaved with repo
    /// indices (flat repo rows when grouping is off).
    pub projected: Vec<projection::ProjectedRow>,
    /// Folder path of the repo the rail and the workspace sheet describe.
    pub selected_repo: Option<String>,
    pub scanning: bool,
    /// Dialog-bound net results, drained by the page's render pass.
    pub deep_scan_result: Option<Vec<warp_station_core::scan::ScannedRepo>>,
    pub clone_result: Option<(String, String, warp_station_core::model::GitSyncResult)>,
    /// The Browse… folder picker's answer, drained into the Add-Repositories
    /// dialog's path field.
    pub folder_pick_result: Option<String>,

    // ---- the selected repo's workspace ----
    changes: Option<warp_station_core::model::GitChangeSnapshot>,
    /// Bumped on every `changes` replacement; views cache derived lists
    /// against it instead of re-cloning the snapshot each frame.
    changes_generation: u64,
    pub changes_loaded_for: Option<String>,
    pub branches: Vec<warp_station_core::model::GitBranchRef>,
    pub recent_commits: Vec<warp_station_core::model::GitCommitInfo>,
    pub unpushed: Option<std::collections::HashSet<String>>,
    /// Running git work by folder: [`GitAction::label`]. One job per repository;
    /// the others stay free.
    pub git_jobs: std::collections::HashMap<String, &'static str>,
    /// The job that just landed, for the moment its button says so.
    pub git_finished: Option<(String, &'static str)>,
    finished_generation: u64,
    /// Folder names being cloned: each shows as a row at the top of the list.
    pub cloning: Vec<String>,
    /// The star just pressed: (folder, press count). Only that one jumps.
    pub star_pressed: Option<(String, u64)>,
    /// One freshly loaded change-row patch, drained by the Changes view.
    pub change_patch_result: Option<(String, bool, String)>,
    history: Vec<warp_station_core::model::GitCommitInfo>,
    history_generation: u64,
    /// GitHub activity per folder — feeds the PR/Issues tabs; the entry
    /// counts on the repo rows come from these snapshots. Arc-shared so a
    /// view render clones a pointer, not the whole snapshot.
    pub github_by_folder:
        std::collections::HashMap<String, Arc<warp_station_core::github::GitHubSnapshot>>,
    pub azure_by_folder:
        std::collections::HashMap<String, Arc<warp_station_core::azure::AzureSnapshot>>,
    pub github_details: Option<warp_station_core::github::GitHubRepoDetails>,
    /// Closed pull requests or issues by (folder, pull_requests), loaded when
    /// a tab first asks.
    pub github_closed:
        std::collections::HashMap<(String, bool), Arc<Vec<warp_station_core::github::GitHubItem>>>,
    /// The pull request taking over the detail pane.
    pub open_pull: Option<OpenPull>,
    closed_requested: std::collections::HashSet<(String, bool)>,

    // ---- wand (warp-agent) state ----
    pub wand_busy: Option<WandKind>,
    /// The repo the running wand was kicked for — its result may only land
    /// while that repo is still selected.
    pub wand_repo: Option<String>,
    pub wand_error: Option<String>,
    /// The commit-message wand's draft, drained into the Changes commit box.
    pub commit_message_draft: Option<String>,
    /// Set when a commit succeeded: the view drains it to empty the box.
    pub commit_message_clear: bool,
    /// The README wand's draft, rendered by the README tab with Save/Discard.
    pub readme_draft: Option<String>,
    /// warp-agent's harness options; the door is the source, with a built-in
    /// fallback until its first answer or a cache exists.
    pub harnesses: Vec<warp_station_core::opencode::CatalogHarness>,
    /// The model catalog (harness-qualified and carrying each model's
    /// variants), cache-backed for offline pickers.
    pub catalog: warp_station_core::opencode::ModelCatalog,
    /// The freshest settled prompt template (id, body, customized-vs-default)
    /// from the net thread; the Settings page's editor drains it reactively.
    pub template_loaded: Option<(String, String, bool)>,
    pub summary_busy: bool,
    /// A running warp-agent answered the door (status chip + row hints).
    pub warp_connected: bool,
    /// Live warp-agent status per workspace folder (running chat titles).
    pub warp_running: std::collections::HashMap<String, WarpWorkspaceStatus>,
    /// Toast stack (newest last), capped; the window overlay renders it and
    /// the per-second tick expires it.
    pub notices: Vec<Notice>,
    notice_generation: u64,
    /// Global-search results (repos by name/path + note content hits).
    pub global_search: Vec<GlobalSearchHit>,
    /// E2E hook: WARP_STATION_NOTES=1 asks for Notes on launch; the main
    /// window's render drains it into a warp-notes launch.
    pub notes_requested: bool,
    /// A note file to open in the notes app (the Summary detail's Open in
    /// Notes); the main window's render drains it into that launch.
    pub notes_open: Option<std::path::PathBuf>,
    /// The tab open in the detail pane.
    pub detail_tab: WorkspaceTab,
    /// Repositories opened in a tab of their own, in strip order.
    pub repo_tabs: Vec<RepoTab>,
    /// Tabs from the last run whose repos haven't landed yet. The first full
    /// scan settles them: found, they join the strip; gone, they are dropped.
    pending_tabs: Vec<String>,
    /// The resting detail tabs the last run left each pending tab on.
    pending_views: std::collections::HashMap<String, WorkspaceTab>,
    /// The strip tab the last run left on screen, still waiting for its repo
    /// to land. Resolves in [Self::ensure_row_selected] like pending_tabs.
    pending_open_tab: Option<String>,
    /// The repo row the last run showed, waiting for the scan to name it.
    /// Resolves like pending_open_tab; a repo that left for good falls back
    /// to the first row.
    pending_selection: Option<String>,
    /// The list pane's width while it is dragged; the settings hold it after.
    list_width: Option<f32>,
    /// The tab on screen; None is the Repositories tab with the list.
    pub active_repo_tab: Option<String>,
    /// What the Repositories tab was showing when a repo tab took over.
    home_repo: Option<String>,
    home_tab: WorkspaceTab,
    /// Drill-down for the selected history commit, drained by the view.
    pub commit_details: Option<warp_station_core::model::GitCommitDetails>,
    pub commit_file_patch: Option<(String, String)>,
    /// When the next periodic `git fetch --prune` fires; None = the feature is
    /// off. Armed by [Self::refresh_auto_fetch_schedule], checked on the UI's
    /// once-per-second tick.
    pub next_auto_fetch: Option<std::time::Instant>,

    /// Write-behind cache saves: bumped on every tag/favorite mutation; the
    /// deferred flush only writes when its generation is still current.
    save_generation: u64,
    /// Latest NuGet watch service snapshot (watching, count, activity log).
    pub nuget: Arc<warp_station_core::nuget::NugetSnapshot>,
    /// Distinct tags across the list (filter chip row), recomputed on
    /// reproject so the toolbar does not rescan every render.
    tags_cache: Vec<String>,
    /// Debounce sequence for the global search; a keystroke invalidates any
    /// in-flight background search.
    search_generation: u32,
    /// True while the debounced global search is running (the popup shows a
    /// hint instead of a stale "No matches.").
    pub global_search_pending: bool,
    /// Cross-repo ripgrep hits for the current search term (rendered after
    /// the repo/note rows); empty when the term is short or the feature is off.
    pub code_search_hits: Vec<warp_station_core::code_search::CodeHit>,
    /// Debounce sequence for the code-search sweep; a settled result applies
    /// only while its generation is still the latest.
    code_search_generation: u64,
    /// True while a ripgrep sweep is in flight (the popup's searching hint).
    pub code_searching: bool,
}

impl Store {
    pub fn new(_cx: &App, net: async_channel::Sender<NetCommand>) -> Self {
        Self::with_settings(net, Arc::new(SettingsService::open()))
    }

    /// The probe seam: probes pass a scratch-backed service so a settings
    /// mutation never lands in the real ~/.warp/settings.json.
    pub fn with_settings(
        net: async_channel::Sender<NetCommand>,
        settings: Arc<SettingsService>,
    ) -> Self {
        let app_settings = settings.get();
        let catalog = warp_station_core::opencode::cached_catalog();
        let harnesses = catalog.harnesses_or_default();
        let cached = repo_store::load_cache(&repo_store::cache_path())
            .map(|c| c.repos.iter().map(repo_store::entry_from_cache).collect())
            .unwrap_or_default();

        let mut store = Self {
            net,
            settings,
            repos: cached,
            tag_filters: Vec::new(),
            show_archived: app_settings.repos.show_archived.unwrap_or(false),
            attention_only: false,
            sort_mode: app_settings.repos.sort_mode,
            sort_ascending: app_settings
                .repos
                .sort_ascending
                .unwrap_or_else(|| app_settings.repos.sort_mode.default_ascending()),
            group_filter: None,
            group_by_root: std::env::var("WARP_STATION_GROUPS").as_deref() == Ok("1")
                || app_settings.repos.group_repos_by_root.unwrap_or(false),
            group_depth: app_settings.repos.group_root_depth.unwrap_or(1).clamp(1, 4) as usize,
            collapsed_groups: app_settings.repos.collapsed_groups.clone(),
            projected: Vec::new(),
            selected_repo: None,
            scanning: false,
            deep_scan_result: None,
            clone_result: None,
            folder_pick_result: None,
            changes: None,
            changes_generation: 0,
            changes_loaded_for: None,
            branches: Vec::new(),
            recent_commits: Vec::new(),
            unpushed: None,
            git_jobs: std::collections::HashMap::new(),
            git_finished: None,
            finished_generation: 0,
            cloning: Vec::new(),
            star_pressed: None,
            change_patch_result: None,
            history: Vec::new(),
            history_generation: 0,
            github_by_folder: std::collections::HashMap::new(),
            azure_by_folder: std::collections::HashMap::new(),
            github_details: None,
            github_closed: std::collections::HashMap::new(),
            open_pull: None,
            closed_requested: std::collections::HashSet::new(),
            wand_busy: None,
            wand_repo: None,
            wand_error: None,
            commit_message_draft: None,
            commit_message_clear: false,
            readme_draft: None,
            harnesses,
            catalog,
            template_loaded: None,
            summary_busy: false,
            warp_connected: false,
            warp_running: std::collections::HashMap::new(),
            notices: Vec::new(),
            notice_generation: 0,
            global_search: Vec::new(),
            // E2E hook: WARP_STATION_NOTES=1 opens Notes (warp-notes) on launch
            notes_requested: std::env::var("WARP_STATION_NOTES").as_deref() == Ok("1"),
            notes_open: None,
            // E2E hook: WARP_STATION_SHEET=<tab> opens that detail tab on launch
            detail_tab: std::env::var("WARP_STATION_SHEET")
                .ok()
                .and_then(|tab| WorkspaceTab::parse(&tab))
                .unwrap_or_default(),
            repo_tabs: Vec::new(),
            pending_tabs: Vec::new(),
            pending_views: std::collections::HashMap::new(),
            pending_open_tab: None,
            pending_selection: None,
            list_width: None,
            active_repo_tab: None,
            home_repo: None,
            home_tab: WorkspaceTab::default(),
            commit_details: None,
            commit_file_patch: None,
            next_auto_fetch: None,
            save_generation: 0,
            nuget: Arc::new(Default::default()),
            tags_cache: Vec::new(),
            search_generation: 0,
            global_search_pending: false,
            code_search_hits: Vec::new(),
            code_search_generation: 0,
            code_searching: false,
        };
        store.pending_tabs = store.settings().repos.repo_tabs.unwrap_or_default();
        // the rest of the last run's arrangement waits for its repos to land:
        // each tab's resting view, the on-screen tab and the selected row
        // resolve in ensure_row_selected as the scan names them
        store.pending_views = app_settings
            .repos
            .tab_views
            .iter()
            .flatten()
            .filter_map(|view| WorkspaceTab::parse(&view.tab).map(|tab| (view.folder.clone(), tab)))
            .collect();
        store.pending_open_tab = app_settings.repos.open_tab.clone();
        store.pending_selection = app_settings.repos.selected_repo.clone();
        if let Some(view) = &app_settings.repos.home_view {
            store.home_repo = (!view.folder.trim().is_empty()).then(|| view.folder.clone());
            store.home_tab = WorkspaceTab::parse(&view.tab).unwrap_or_default();
            // nothing is open yet, so the detail pane rests where the
            // Repositories tab left it
            store.detail_tab = store.home_tab;
        }
        store.reproject();
        store.refresh_auto_fetch_schedule();

        // Cache first for instant labels, then the session scan + a status
        // pass over whatever we know (the scan's settle re-probes the merged
        // list). Archived repos stay out of the passes; the scan still gets
        // the full list so their tags survive the merge.
        let folders: Vec<std::path::PathBuf> = store
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|r| r.folder_path.clone())
            .collect();
        let _ = store.net.send_blocking(NetCommand::ProbeAll {
            folders: folders.clone(),
        });
        store.kick_github_pass(&folders);
        store.kick_azure_pass(&folders);
        store.scanning = true;
        let _ = store.net.send_blocking(NetCommand::Scan {
            settings: app_settings.repos.clone(),
            previous: store.repos.clone(),
        });

        // E2E hook: WARP_STATION_SELECT=<index-or-name-substring> picks the repo the
        // app opens on, so automation can verify the workspace surfaces
        if let Ok(pattern) = std::env::var("WARP_STATION_SELECT")
            && let Some(entry) = store
                .repos
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(&pattern))
                .or_else(|| {
                    store
                        .repos
                        .get(pattern.parse::<usize>().unwrap_or(usize::MAX))
                })
        {
            let folder = entry.folder_path.to_string_lossy().to_string();
            // the hook is an explicit ask: it outranks the restored selection
            store.pending_selection = None;
            store.pending_open_tab = None;
            store.load_repo_workspace(&folder);
            store.selected_repo = Some(folder);
        }
        // Notes stay closed until the user asks for them (titlebar button,
        // repo-row note icon, tools menu, or a global-search hit).
        store
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.get()
    }

    // ---- derived-cache change tokens + guarded snapshots ----

    pub fn changes(&self) -> Option<&warp_station_core::model::GitChangeSnapshot> {
        self.changes.as_ref()
    }

    pub fn changes_generation(&self) -> u64 {
        self.changes_generation
    }

    /// Replaces the change snapshot, bumping the generation views cache
    /// against (the direct field is private for that reason).
    pub fn set_changes(&mut self, snapshot: Option<warp_station_core::model::GitChangeSnapshot>) {
        self.changes = snapshot;
        self.changes_generation += 1;
    }

    pub fn history(&self) -> &[warp_station_core::model::GitCommitInfo] {
        &self.history
    }

    pub fn history_generation(&self) -> u64 {
        self.history_generation
    }

    pub fn set_history(&mut self, commits: Vec<warp_station_core::model::GitCommitInfo>) {
        self.history = commits;
        self.history_generation += 1;
    }

    pub fn update_settings(&self, mutate: impl FnOnce(&mut AppSettings)) {
        if let Err(err) = self.settings.update(mutate) {
            tracing::warn!("could not save settings: {err}");
        }
    }

    // ---- NuGet Local watch controls (config captured at press time) ----

    pub fn start_nuget(&mut self) {
        let section = self.settings().nuget_local;
        let _ = self.net.send_blocking(NetCommand::NugetStart {
            config: warp_station_core::nuget::NugetConfig {
                enable: section.enable_nuget,
                watch_folder: section.watch_folder,
                filter: section.nuget_package_filter,
                copy_delay: std::time::Duration::from_millis(u64::from(section.file_copy_delay_ms)),
                count_window: std::time::Duration::from_secs(u64::from(
                    section.count_reset_interval_seconds,
                )),
            },
        });
    }

    pub fn stop_nuget(&mut self) {
        let _ = self.net.send_blocking(NetCommand::NugetStop);
    }

    pub fn register_nuget_source(&mut self) {
        let _ = self.net.send_blocking(NetCommand::NugetRegister);
    }

    /// Persists the watch folder and applies it to a running watch.
    pub fn set_nuget_folder(&mut self, folder: String) {
        let trimmed = folder.trim().to_string();
        self.update_settings(|s| {
            s.nuget_local.watch_folder = (!trimmed.is_empty()).then(|| trimmed.clone());
        });
        let _ = self
            .net
            .send_blocking(NetCommand::NugetSetFolder { folder: trimmed });
    }

    pub fn kick_scan(&mut self, settings: &AppSettings) {
        self.scanning = true;
        let _ = self.net.send_blocking(NetCommand::Scan {
            settings: settings.repos.clone(),
            previous: self.repos.clone(),
        });
    }

    /// Manual refresh: rescan with the current settings, then re-probe.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings();
        self.kick_scan(&settings);
        cx.notify();
    }

    pub fn on_net_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        match event {
            UiEvent::ScanSettled { repos, removed: _ } => {
                self.repos = repos;
                self.scanning = false;
                self.reproject();
                // statuses re-check after every rescan without coordination
                let folders: Vec<std::path::PathBuf> = self
                    .repos
                    .iter()
                    .filter(|repo| !repo.is_archived())
                    .map(|r| r.folder_path.clone())
                    .collect();
                let _ = self.net.send_blocking(NetCommand::ProbeAll {
                    folders: folders.clone(),
                });
                self.kick_github_pass(&folders);
                self.kick_azure_pass(&folders);
                cx.notify();
            }
            UiEvent::StatusSettled {
                probes,
                after_fetch,
            } => {
                for probe in &probes {
                    if let Some(entry) = self
                        .repos
                        .iter_mut()
                        .find(|r| r.folder_path.to_string_lossy() == probe.folder_path)
                    {
                        repo_store::apply_status_probe(entry, probe);
                    }
                }
                // a fetch pass ends here: the selected repo's open workspace
                // re-reads so the rail's file list and branches show the
                // post-fetch state without a manual reselect
                if after_fetch
                    && let Some(selected) = self.selected_repo.clone()
                    && probes.iter().any(|p| p.folder_path == selected)
                {
                    let _ = self.net.send_blocking(NetCommand::LoadChanges {
                        folder: std::path::PathBuf::from(selected),
                    });
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::DeepScanSettled { found, .. } => {
                self.deep_scan_result = Some(found);
                cx.notify();
            }
            UiEvent::CloneSettled {
                parent_dir,
                name,
                result,
                ..
            } => {
                self.cloning.retain(|cloning| cloning != &name);
                self.clone_result = Some((parent_dir, name, result));
                cx.notify();
            }
            UiEvent::FolderPicked { path } => {
                self.folder_pick_result = path;
                cx.notify();
            }
            UiEvent::ChangesLoaded {
                folder,
                snapshot,
                branches,
                recent,
                unpushed,
            } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.set_changes(snapshot);
                    self.changes_loaded_for = Some(folder);
                    self.branches = branches;
                    if !recent.is_empty() {
                        self.recent_commits = recent;
                    }
                    self.unpushed = unpushed;
                }
                cx.notify();
            }
            UiEvent::ChangePatchLoaded {
                folder,
                path,
                staged,
                patch,
            } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.change_patch_result = Some((path, staged, patch));
                }
                cx.notify();
            }
            UiEvent::HistoryLoaded {
                folder,
                commits,
                unpushed,
            } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.set_history(commits);
                    self.unpushed = unpushed;
                }
                cx.notify();
            }
            UiEvent::AzureSettled { items } => {
                for (folder, snapshot) in items {
                    self.azure_by_folder.insert(folder, Arc::new(snapshot));
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::GitHubSettled { items } => {
                for (folder, snapshot) in items {
                    if let Some(entry) = self
                        .repos
                        .iter_mut()
                        .find(|r| r.folder_path.to_string_lossy() == folder)
                    {
                        entry.github_pr_count = snapshot.pulls.len() as u32;
                        entry.github_issue_count = snapshot.issues.len() as u32;
                    }
                    self.github_by_folder.insert(folder, Arc::new(snapshot));
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::GitHubDetailsLoaded { folder, details } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.github_details = details;
                }
                cx.notify();
            }
            UiEvent::GitHubClosedLoaded {
                folder,
                pull_requests,
                items,
            } => {
                match items {
                    Some(items) => {
                        self.github_closed
                            .insert((folder, pull_requests), Arc::new(items));
                    }
                    None => self.notify_error("GitHub did not answer with the closed list"),
                }
                cx.notify();
            }
            UiEvent::GitHubPullLoaded {
                folder,
                number,
                pull,
            } => {
                if let Some(open) = self
                    .open_pull
                    .as_mut()
                    .filter(|open| open.folder == folder && open.item.number == number)
                {
                    open.failed = pull.is_none();
                    if pull.is_some() {
                        open.pull = pull;
                    }
                }
                cx.notify();
            }
            UiEvent::GitHubMerged {
                folder,
                number,
                result,
            } => {
                if let Some(open) = self
                    .open_pull
                    .as_mut()
                    .filter(|open| open.folder == folder && open.item.number == number)
                {
                    open.merging = false;
                }
                match result {
                    Ok(()) => {
                        self.notify_success(format!("Merged #{number}"));
                        self.reload_open_pull();
                        self.kick_github_pass(&[std::path::PathBuf::from(&folder)]);
                        self.github_closed.remove(&(folder, true));
                    }
                    Err(err) if err.is_empty() => {
                        self.notify_error(format!("Could not merge #{number}"));
                    }
                    Err(err) => self.notify_error(err),
                }
                cx.notify();
            }
            UiEvent::ModelsLoaded { harness, catalog } => {
                self.apply_catalog(&harness, catalog);
                cx.notify();
            }
            UiEvent::TemplateLoaded {
                id,
                content,
                customized,
            } => {
                self.template_loaded = Some((id, content, customized));
                cx.notify();
            }
            UiEvent::CodeSearchSettled { generation, hits } => {
                // a newer keystroke's sweep wins; stale results drop silently
                if generation == self.code_search_generation {
                    self.code_search_hits = hits;
                    self.code_searching = false;
                    cx.notify();
                }
            }
            UiEvent::Nuget(snapshot) => {
                self.nuget = Arc::new(snapshot);
                cx.notify();
            }
            UiEvent::ClipboardHotkey { message } => {
                // the copy ran on the trigger thread (gpui's Wayland
                // clipboard write is focus-gated); here we only toast
                let kind = NoticeKind::for_copy_message(message);
                self.notify(kind, message.to_string());
                cx.notify();
            }
            UiEvent::SummaryDone { result, .. } => {
                self.summary_busy = false;
                match result {
                    Ok(path) => self.notify_success(format!("summary written: {path}")),
                    Err(err) => self.notify_error(format!("summary failed: {err}")),
                }
                cx.notify();
            }
            UiEvent::WandLog { line, .. } => {
                tracing::info!("wand: {line}");
            }
            UiEvent::WarpAvailability { connected } => {
                self.warp_connected = connected;
                if !connected {
                    self.warp_running.clear();
                }
                cx.notify();
            }
            UiEvent::WarpStatus { workspaces } => {
                self.warp_running = workspaces.into_iter().collect();
                cx.notify();
            }
            UiEvent::WarpOpened { folder, error } => {
                match error {
                    Some(err) => {
                        tracing::warn!("warp-agent open failed for {folder}: {err}");
                        self.notify_error(format!("warp-agent: {err}"));
                    }
                    None => {
                        let name = self
                            .repos
                            .iter()
                            .find(|r| r.folder_path.to_string_lossy() == folder)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| folder.clone());
                        self.notify_success(format!("opened {name} in warp-agent"));
                    }
                }
                cx.notify();
            }
            UiEvent::WandDone { kind, result, .. } => {
                // a run kicked for repo A must never land its draft once
                // another repo is selected
                let for_selected = self.wand_repo.as_deref() == self.selected_repo.as_deref();
                self.wand_busy = None;
                self.wand_repo = None;
                match (&result, kind) {
                    (Ok(text), WandKind::Commit) => {
                        if for_selected {
                            self.commit_message_draft = Some(text.trim().to_string());
                        }
                    }
                    (Ok(text), WandKind::Readme) => {
                        if for_selected {
                            self.readme_draft = Some(text.trim().to_string());
                        }
                    }
                    (Err(err), WandKind::Commit) => {
                        tracing::warn!("wand failed: {err}");
                        if for_selected {
                            self.wand_error = Some(err.clone());
                        }
                        self.notify_error("Could not generate a commit message");
                    }
                    (Err(err), WandKind::Readme) => {
                        tracing::warn!("wand failed: {err}");
                        if for_selected {
                            self.wand_error = Some(err.clone());
                        }
                        self.notify_error("Could not generate a README");
                    }
                }
                cx.notify();
            }
            UiEvent::CommitDetailsLoaded { folder, details } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.commit_details = details;
                }
                cx.notify();
            }
            UiEvent::CommitFilePatchLoaded {
                folder,
                hash,
                path,
                patch,
            } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.commit_file_patch = Some((format!("{hash}|{path}"), patch));
                }
                cx.notify();
            }
            UiEvent::GitDone {
                folder,
                label,
                error,
                commit_hash,
                detail,
            } => {
                self.git_jobs.remove(&folder);
                if error.is_none() {
                    self.show_finished(folder.clone(), label, cx);
                }
                let name = self
                    .repos
                    .iter()
                    .find(|r| r.folder_path.to_string_lossy() == folder)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| folder.clone());
                if let Some(err) = error {
                    tracing::warn!("git {label} failed in {folder}: {err}");
                    self.notify_error(format!(
                        "{} failed for {}: {err}",
                        capitalized_label(label),
                        name
                    ));
                } else {
                    self.notify_success(match (label, commit_hash.as_deref()) {
                        ("commit", Some(hash)) => format!("Committed {hash} to {name}"),
                        ("fetch", _) => format!("Fetched {name}"),
                        ("pull", _) => format!("Pulled {name}"),
                        ("push", _) => format!("Pushed {name}"),
                        ("sync", _) => format!("Synced {name}"),
                        ("publish", _) => format!(
                            "Published {} — it tracks the remote",
                            detail.as_deref().unwrap_or(&name)
                        ),
                        ("checkout", _) => format!(
                            "Checked out {} in {name}",
                            detail.as_deref().unwrap_or(&name)
                        ),
                        ("create branch", _) => {
                            format!("Created {} in {name}", detail.as_deref().unwrap_or(&name))
                        }
                        ("delete branch", _) => {
                            format!("Deleted {} in {name}", detail.as_deref().unwrap_or(&name))
                        }
                        ("revert", _) => format!("Reverted in {name}"),
                        _ => format!("{label} ok"),
                    });
                    if let Some(hash) = commit_hash {
                        tracing::info!("committed {hash} in {folder}");
                    }
                    if label == "commit" && self.selected_repo.as_deref() == Some(folder.as_str()) {
                        self.commit_message_clear = true;
                    }
                }
                cx.notify();
            }
        }
    }

    /// Fires the GitHub activity pass when the column is enabled and gh
    /// resolves; a disabled column spawns no processes at all.
    pub fn kick_github_pass(&mut self, folders: &[std::path::PathBuf]) {
        let settings = self.settings();
        if !settings.repos.enable_git_hub || folders.is_empty() {
            return;
        }
        let Some(gh_path) = warp_station_core::exe::locate(&settings.repos.git_hub_executable)
        else {
            tracing::warn!(
                "GitHub column enabled but the gh CLI could not be located ({})",
                settings.repos.git_hub_executable
            );
            return;
        };
        let _ = self.net.send_blocking(NetCommand::GitHubPass {
            folders: folders.to_vec(),
            gh_exe: gh_path.to_string_lossy().to_string(),
        });
    }

    /// Fires the Azure DevOps pass when the column is enabled and a PAT
    /// resolves (settings or the environment fallbacks).
    pub fn kick_azure_pass(&mut self, folders: &[std::path::PathBuf]) {
        let settings = self.settings();
        if !settings.repos.enable_azure_dev_ops || folders.is_empty() {
            return;
        }
        let Some(pat) = warp_station_core::azure::resolve_pat(&settings.repos.azure_dev_ops_pat)
        else {
            tracing::warn!(
                "Azure DevOps column enabled but no personal access token is configured; set one in Repos settings (or the AZURE_DEVOPS_PAT environment variable)"
            );
            return;
        };
        let _ = self.net.send_blocking(NetCommand::AzurePass {
            folders: folders.to_vec(),
            pat,
            custom_base: settings.repos.azure_dev_ops_url.clone(),
        });
    }

    /// Resolves the warp-agent binary when the AI surface is enabled; None =
    /// the features are off (not installed or toggled off). The warp-agent
    /// switch stands alone: runs go through its door, so the OpenCode CLI
    /// switch — which only arms the repo row's launch button — never gates
    /// them.
    fn warp_ready(&self) -> Option<std::path::PathBuf> {
        let settings = self.settings();
        if !ai_enabled(&settings) {
            return None;
        }
        warp_station_core::exe::locate(&settings.warp_agent.executable)
    }

    /// Kicks the yesterday-summary wand across every tracked repo.
    /// Opens one note in the external notes app; the window drains this.
    pub fn open_note_in_notes(&mut self, path: std::path::PathBuf) {
        self.notes_open = Some(path);
    }

    pub fn run_summary(&mut self, cx: &mut Context<Self>) {
        if self.summary_busy {
            return;
        }
        let settings = self.settings();
        if !settings.warp_agent.enable_warp_agent {
            self.notify_error("warp-agent integration is disabled in settings");
            cx.notify();
            return;
        }
        let Some(exe) = warp_station_core::exe::locate(&settings.warp_agent.executable) else {
            self.notify_error("warp-agent executable not found");
            cx.notify();
            return;
        };
        let choice = warp_station_core::warp::ModelChoice::from_settings(&settings);
        let folders: Vec<(String, std::path::PathBuf)> = self
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|r| (r.name.clone(), r.folder_path.clone()))
            .collect();
        if folders.is_empty() {
            return;
        }
        let summaries_dir =
            warp_station_core::notes::resolve_store_root(&settings.general.notes_store_path)
                .join("Summaries");
        let day = chrono::Local::now().date_naive() - chrono::Duration::days(1);
        self.summary_busy = true;
        let id = format!("summary-{}", self.save_generation);
        self.save_generation += 1;
        let _ = self.net.send_blocking(NetCommand::WandSummary {
            id,
            folders,
            day,
            choice,
            warp_exe: exe.to_string_lossy().to_string(),
            summaries_dir,
        });
        cx.notify();
    }

    /// Loads one harness's model catalog through the door (cache-backed when
    /// offline; never launches warp-agent). The settings model picker asks
    /// for the harness it is showing.
    pub fn load_models_for(&mut self, harness: String, cx: &mut Context<Self>) {
        if self.warp_ready().is_some() {
            let _ = self.net.send_blocking(NetCommand::LoadModels { harness });
        } else {
            self.apply_catalog(
                &harness,
                warp_station_core::opencode::cached_harness_catalog(&harness),
            );
            cx.notify();
        }
    }

    /// Replaces one harness's slice of the catalog; the other harness's
    /// entries stay so switching the radio never empties a loaded list.
    fn apply_catalog(&mut self, harness: &str, catalog: warp_station_core::opencode::ModelCatalog) {
        if !catalog.harnesses.is_empty() {
            self.harnesses = catalog.harnesses.clone();
        }
        if self.harnesses.is_empty() {
            self.harnesses = warp_station_core::opencode::default_harnesses();
        }
        self.catalog.harnesses = self.harnesses.clone();
        if catalog.models.is_empty() {
            // an offline miss keeps the last known list rather than clearing it
            return;
        }
        self.catalog.models.retain(|model| model.harness != harness);
        for mut model in catalog.models {
            if model.harness.trim().is_empty() {
                model.harness = harness.to_string();
            }
            model.key = model.key.trim().to_string();
            if !model.key.is_empty() && !model.key.starts_with('(') {
                self.catalog.models.push(model);
            }
        }
    }

    /// One prompt-template op from the Settings card; the settled state
    /// returns as [UiEvent::TemplateLoaded].
    pub fn template_io(&mut self, id: &str, op: TemplateOp) {
        let _ = self.net.send_blocking(NetCommand::TemplateIo {
            id: id.to_string(),
            op,
        });
    }

    /// Kicks one wand run for the selected repo on the selected
    /// harness/model/variant — commit generation included; there is no
    /// separate commit model.
    pub fn run_wand(&mut self, kind: WandKind, cx: &mut Context<Self>) {
        if self.wand_busy.is_some() {
            return;
        }
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let settings = self.settings();
        if !settings.warp_agent.enable_warp_agent {
            self.notify_error(
                "Could not generate — warp-agent integration is disabled in settings",
            );
            cx.notify();
            return;
        }
        let Some(exe) = warp_station_core::exe::locate(&settings.warp_agent.executable) else {
            self.notify_error("Could not generate — warp-agent executable not found");
            cx.notify();
            return;
        };
        let choice = warp_station_core::warp::ModelChoice::from_settings(&settings);
        self.wand_busy = Some(kind);
        self.wand_repo = Some(folder.clone());
        self.wand_error = None;
        let id = format!("wand-{}", self.save_generation);
        self.save_generation += 1;
        let _ = self.net.send_blocking(NetCommand::Wand {
            id,
            kind,
            folder: std::path::PathBuf::from(folder),
            choice,
            warp_exe: exe.to_string_lossy().to_string(),
        });
        cx.notify();
    }

    /// True when the AI surface can run (gates every wand button).
    pub fn wand_available(&self) -> bool {
        self.warp_ready().is_some()
    }

    /// Opens a repo as a warp-agent workspace (added there when missing).
    pub fn open_in_warp(&mut self, folder: String, cx: &mut Context<Self>) {
        let Some(exe) = self.warp_ready() else {
            self.notify_error("warp-agent is not available");
            cx.notify();
            return;
        };
        let _ = self.net.send_blocking(NetCommand::WarpOpen {
            folder: std::path::PathBuf::from(folder),
            warp_exe: exe.to_string_lossy().to_string(),
        });
        cx.notify();
    }

    /// Sends one mutation for the selected repo and marks the rail busy; the
    /// net thread settles the reload.
    pub fn run_git(&mut self, action: GitAction, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        self.git_jobs.insert(folder.clone(), action.label());
        let _ = self.net.send_blocking(NetCommand::Git {
            folder: std::path::PathBuf::from(folder),
            action,
        });
        cx.notify();
    }

    /// The selected repo's running job.
    pub fn running_job(&self) -> Option<&'static str> {
        let folder = self.selected_repo.as_ref()?;
        self.git_jobs.get(folder).copied()
    }

    pub fn git_busy(&self) -> bool {
        self.running_job().is_some()
    }

    /// The selected repo's job that just landed.
    pub fn finished_job(&self) -> Option<&'static str> {
        let (folder, label) = self.git_finished.as_ref()?;
        (self.selected_repo.as_ref() == Some(folder)).then_some(*label)
    }

    /// Holds the past tense on the job's button for a moment.
    fn show_finished(&mut self, folder: String, label: &'static str, cx: &mut Context<Self>) {
        self.git_finished = Some((folder, label));
        self.finished_generation += 1;
        let generation = self.finished_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FINISHED_HOLD).await;
            let _ = this.update(cx, |store, cx| {
                if store.finished_generation == generation {
                    store.git_finished = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Adopts scan results into the tracked list (the Add-Repositories
    /// dialog's Add button): new folders append; known folders keep their
    /// tags. Persists immediately and re-probes the new arrivals.
    pub fn register_repos(
        &mut self,
        found: &[warp_station_core::scan::ScannedRepo],
        cx: &mut Context<Self>,
    ) {
        let previous: Vec<RepoEntry> = self.repos.clone();
        let (mut merged, _) = repo_store::merge_scan(&previous, found);
        // merge_scan drops repos absent from `found`; the dialog adds to the
        // EXISTING list, so union instead
        for existing in &previous {
            if !merged
                .iter()
                .any(|m: &RepoEntry| m.folder_path == existing.folder_path)
            {
                merged.push(existing.clone());
            }
        }
        merged.sort_by_key(|a| a.name.to_lowercase());
        let new_folders: Vec<std::path::PathBuf> = merged
            .iter()
            .filter(|m| !previous.iter().any(|p| p.folder_path == m.folder_path))
            .map(|m| m.folder_path.clone())
            .collect();
        self.repos = merged;
        self.reproject();
        // the listing scan rebuilds this list from the persisted scan roots
        // and drops everything else, so the new folders must join the roots
        // (roots are repo folders, like adopt_clone's) or the next rescan —
        // startup, Refresh — erases the add; the cache save below only
        // carries tags and git status across that scan
        if !new_folders.is_empty() {
            self.update_settings(|s| {
                let roots = s.repos.repo_scan_folders.get_or_insert_default();
                for folder in &new_folders {
                    let folder = folder.to_string_lossy().to_string();
                    if !roots
                        .iter()
                        .any(|f| warp_station_core::paths::same_folder_text(f, &folder))
                    {
                        roots.push(folder);
                    }
                }
                roots.sort();
            });
        }
        if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &self.repos) {
            tracing::warn!("could not save repo cache: {err}");
        }
        if !new_folders.is_empty() {
            let _ = self.net.send_blocking(NetCommand::ProbeAll {
                folders: new_folders,
            });
        }
        cx.notify();
    }

    /// Adopts a finished clone: its folder joins the persisted scan roots
    /// (the roots are repo folders — this is how the rescan keeps finding it),
    /// the list re-scans, and the bar opens on the clone's Overview.
    pub fn adopt_clone(&mut self, parent_dir: &str, name: &str, cx: &mut Context<Self>) {
        let folder = std::path::PathBuf::from(parent_dir).join(name);
        if !folder.join(".git").is_dir() {
            tracing::warn!(
                "clone destination missing after clone: {}",
                folder.display()
            );
            self.notify_error("The clone landed but could not be added to the list");
            cx.notify();
            return;
        }
        let folder_str = folder.to_string_lossy().to_string();
        self.update_settings(|s| {
            if let Some(roots) = &mut s.repos.repo_scan_folders {
                if !roots
                    .iter()
                    .any(|f| warp_station_core::paths::same_folder_text(f, &folder_str))
                {
                    roots.push(folder_str.clone());
                    roots.sort();
                }
            } else {
                s.repos.repo_scan_folders = Some(vec![folder_str.clone()]);
            }
        });
        let scanned = vec![warp_station_core::scan::ScannedRepo {
            name: name.to_string(),
            folder_path: folder.clone(),
            solution_path: None,
            tags: Vec::new(),
        }];
        self.register_repos(&scanned, cx);
        self.kick_scan(&self.settings());
        self.select_repo(&folder_str, cx);
    }

    // ---- toasts -------------------------------------------------------------

    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        let id = self.notice_generation;
        self.notice_generation += 1;
        self.notices.push(Notice {
            id,
            kind,
            text: text.into(),
            born: std::time::Instant::now(),
        });
        // a burst of failures must not stack a wall of cards
        if self.notices.len() > 4 {
            self.notices.remove(0);
        }
    }

    pub fn notify_success(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Success, text);
    }

    pub fn notify_info(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Info, text);
    }

    pub fn notify_error(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Error, text);
    }

    /// The toast's close button.
    pub fn dismiss_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.notices.len();
        self.notices.retain(|n| n.id != id);
        if self.notices.len() != before {
            cx.notify();
        }
    }

    /// Once-per-second tick: drops toasts past their kind's ttl, notifying
    /// only when one actually expired (idle windows stay idle).
    pub fn tick_notices(&mut self, cx: &mut Context<Self>) {
        let before = self.notices.len();
        self.notices.retain(|n| n.born.elapsed() < n.kind.ttl());
        if self.notices.len() != before {
            cx.notify();
        }
    }

    // ---- periodic fetch -----------------------------------------------------

    /// The effective auto-fetch interval: WARP_STATION_AUTO_FETCH_SECS=<secs>
    /// (E2E hook, mirrors the other WARP_STATION_* launch hooks) replaces the
    /// minutes setting; None = the feature is off.
    fn auto_fetch_interval(repos: &ReposSettings) -> Option<std::time::Duration> {
        if let Ok(secs) = std::env::var("WARP_STATION_AUTO_FETCH_SECS")
            && let Ok(secs) = secs.parse::<u64>()
        {
            return Some(std::time::Duration::from_secs(secs));
        }
        repos
            .auto_fetch_minutes
            .map(|minutes| std::time::Duration::from_secs(u64::from(minutes) * 60))
    }

    /// Re-arms the periodic fetch from the current settings; call wherever a
    /// settings load or save can change the interval. Fetch-off + focus-gate-on
    /// arms a single immediate pass instead (one fetch, held for focus).
    pub fn refresh_auto_fetch_schedule(&mut self) {
        let repos = &self.settings().repos;
        self.next_auto_fetch = if let Some(interval) = Self::auto_fetch_interval(repos) {
            Some(std::time::Instant::now() + interval)
        } else if repos.auto_fetch_only_focused.unwrap_or(false) {
            Some(std::time::Instant::now())
        } else {
            None
        };
    }

    /// Whether any open window is the focused one. The notes window counts,
    /// so the user reading notes still counts as "looking at Warp Station".
    fn any_window_focused(cx: &mut App) -> bool {
        cx.windows().into_iter().any(|handle| {
            handle
                .update(cx, |_, window, _| window.is_window_active())
                .unwrap_or(false)
        })
    }

    /// The tick's check: when the schedule is due, fetch every repo with a
    /// remote — except the selected repo's folder while a git mutation runs
    /// there (index.lock conflict avoidance) — then re-arm into the future
    /// (an interval) or clear (the launch one-shot).
    pub fn check_auto_fetch(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.next_auto_fetch else {
            return;
        };
        if std::time::Instant::now() < due {
            return;
        }
        // the setting may have been turned off since this was armed
        let interval = Self::auto_fetch_interval(&self.settings().repos);
        let gated = self
            .settings()
            .repos
            .auto_fetch_only_focused
            .unwrap_or(false);
        if interval.is_none() && !gated {
            self.next_auto_fetch = None;
            return;
        }
        // the focus gate holds the pass — and the schedule — until a
        // window regains focus; no re-arm, so focusing fires it right away
        if gated && !Self::any_window_focused(cx) {
            return;
        }
        // the launch one-shot waits for the first scan to land repos;
        // otherwise it is consumed by an empty table and never fetches
        if interval.is_none() && self.repos.is_empty() {
            return;
        }
        let folders: Vec<std::path::PathBuf> = self
            .repos
            .iter()
            .filter(|repo| repo.git_has_remote && !repo.is_archived())
            .map(|repo| repo.folder_path.clone())
            .filter(|folder| {
                !self
                    .git_jobs
                    .contains_key(folder.to_string_lossy().as_ref())
            })
            .collect();
        // an interval re-arms into the future; the one-shot clears instead
        self.next_auto_fetch = interval.map(|i| std::time::Instant::now() + i);
        if !folders.is_empty() {
            let _ = self.net.send_blocking(NetCommand::FetchAll { folders });
        }
    }

    // ---- list shaping ------------------------------------------------------

    /// Clears the tag filter and the group solo filter in one transition
    /// (the toolbar's Clear).
    pub fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.tag_filters.clear();
        self.group_filter = None;
        self.attention_only = false;
        self.reproject();
        cx.notify();
    }

    /// The scope row's "Needs attention" filter: uncommitted work, an open
    /// issue, or commits still to push. Session-only, like the tag chips.
    pub fn set_attention_only(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.attention_only == on {
            return;
        }
        self.attention_only = on;
        self.reproject();
        cx.notify();
    }

    /// A sortable header click: the kit's cycle drives both the mode and the
    /// direction, so both land together.
    pub fn set_sort_direction(
        &mut self,
        mode: RepoSortMode,
        ascending: bool,
        cx: &mut Context<Self>,
    ) {
        self.sort_mode = mode;
        self.sort_ascending = ascending;
        self.update_settings(|s| {
            s.repos.sort_mode = mode;
            s.repos.sort_ascending = Some(ascending);
        });
        self.reproject();
        cx.notify();
    }

    /// A group header's chevron: collapses/expands that folder section and
    /// persists the key (absence-meaningful, like sort_ascending).
    pub fn toggle_group_collapse(&mut self, key: &str, cx: &mut Context<Self>) {
        let keys = self.collapsed_groups.get_or_insert_with(Vec::new);
        match keys.iter().position(|k| k == key) {
            Some(ix) => {
                keys.remove(ix);
            }
            None => keys.push(key.to_string()),
        }
        if keys.is_empty() {
            self.collapsed_groups = None;
        }
        self.update_settings(|s| s.repos.collapsed_groups = self.collapsed_groups.clone());
        self.reproject();
        cx.notify();
    }

    /// A group header's solo filter: shows only that group's rows; pressing
    /// the same header's icon again clears it.
    pub fn solo_group(&mut self, key: &str, cx: &mut Context<Self>) {
        self.group_filter = if self.group_filter.as_deref() == Some(key) {
            None
        } else {
            Some(key.to_string())
        };
        self.reproject();
        cx.notify();
    }

    /// The Settings save path: flips grouping and re-projects. No rescan —
    /// the repos themselves did not change, only their presentation.
    pub fn set_group_by_root(&mut self, on: bool) {
        if self.group_by_root == on {
            return;
        }
        self.group_by_root = on;
        self.reproject();
    }

    /// The Settings save path: moves the group headers one folder further
    /// up (or down) and re-projects. No rescan, like the toggle —
    /// presentation only, so stale persisted collapse keys (from another
    /// depth) simply stop matching and their groups start expanded.
    pub fn set_group_depth(&mut self, depth: usize) {
        let depth = depth.clamp(1, 4);
        if self.group_depth == depth {
            return;
        }
        self.group_depth = depth;
        self.reproject();
    }

    /// Checks/unchecks one tag-filter chip (OR semantics across the set).
    pub fn clear_tag_filters(&mut self, cx: &mut Context<Self>) {
        self.tag_filters.clear();
        self.reproject();
        cx.notify();
    }

    pub fn toggle_tag_filter(&mut self, tag: &str, cx: &mut Context<Self>) {
        match self
            .tag_filters
            .iter()
            .position(|t| t.eq_ignore_ascii_case(tag))
        {
            Some(ix) => {
                self.tag_filters.remove(ix);
            }
            None => self.tag_filters.push(tag.to_string()),
        }
        self.reproject();
        cx.notify();
    }

    /// Selects the repo and asks for its workspace. Selecting the selected
    /// repo changes nothing, so it keeps what the rail already loaded.
    pub fn select_repo(&mut self, folder: &str, cx: &mut Context<Self>) {
        if self.selected_repo.as_deref() == Some(folder) {
            return;
        }
        self.selected_repo = Some(folder.to_string());
        self.update_settings(|s| s.repos.selected_repo = Some(folder.to_string()));
        self.reset_repo_workspace();
        self.load_repo_workspace(folder);
        cx.notify();
    }

    /// Asks the net thread for what the rail and the workspace views show for
    /// the newly selected repo: its changes, and what GitHub says about it.
    fn load_repo_workspace(&self, folder: &str) {
        let path = std::path::PathBuf::from(folder);
        let _ = self.net.send_blocking(NetCommand::LoadChanges {
            folder: path.clone(),
        });
        let settings = self.settings();
        if settings.repos.enable_git_hub
            && let Some(gh) = warp_station_core::exe::locate(&settings.repos.git_hub_executable)
        {
            let _ = self.net.send_blocking(NetCommand::GitHubDetails {
                folder: path,
                gh_exe: gh.to_string_lossy().to_string(),
            });
        }
    }

    /// The selected repo's GitHub page, from whichever pass answered first:
    /// the details request or the background activity pass.
    pub fn selected_github_url(&self) -> Option<String> {
        self.github_details
            .as_ref()
            .and_then(|details| details.url.clone())
            .or_else(|| {
                let folder = self.selected_repo.as_ref()?;
                self.github_by_folder.get(folder)?.url.clone()
            })
    }

    /// Shows `tab` in the detail pane, and records it as the current tab's
    /// resting view so a restart opens the pane where the user left it.
    pub fn open_detail_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        self.detail_tab = tab;
        match &self.active_repo_tab {
            Some(folder) => {
                if let Some(entry) = self.repo_tabs.iter_mut().find(|t| &t.folder == folder) {
                    entry.tab = tab;
                }
            }
            None => self.home_tab = tab,
        }
        self.open_pull = None;
        self.save_repo_tabs();
        cx.notify();
    }

    fn gh_exe(&self) -> Option<String> {
        let settings = self.settings();
        if !settings.repos.enable_git_hub {
            return None;
        }
        warp_station_core::exe::locate(&settings.repos.git_hub_executable)
            .map(|gh| gh.to_string_lossy().to_string())
    }

    /// Asks once for the selected repo's closed list; a tab calls this as it draws.
    pub fn load_closed(&mut self, pull_requests: bool) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        if !self
            .closed_requested
            .insert((folder.clone(), pull_requests))
        {
            return;
        }
        if let Some(gh_exe) = self.gh_exe() {
            let _ = self.net.send_blocking(NetCommand::GitHubClosed {
                folder: std::path::PathBuf::from(folder),
                gh_exe,
                pull_requests,
            });
        }
    }

    /// Appearance applies at once, without Save. The desktop's own wish stands
    /// when the switch goes off.
    pub fn set_reduce_motion(&mut self, on: bool, cx: &mut Context<Self>) {
        self.update_settings(|s| s.general.reduce_motion = on);
        warpui_kit_core::motion::set_reduced(
            on || warpui_kit_core::motion::system_prefers_reduced(),
        );
        cx.notify();
    }

    pub fn issue_sort(&self) -> warp_station_core::github::IssueSort {
        warp_station_core::github::IssueSort::from_key(self.settings().repos.issue_sort.as_deref())
    }

    pub fn set_issue_sort(
        &mut self,
        sort: warp_station_core::github::IssueSort,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(|s| s.repos.issue_sort = Some(sort.key().to_string()));
        cx.notify();
    }

    /// Turns the detail pane into `item`, a row of the Pull requests tab. The
    /// row's own fields carry the head until the full view answers.
    pub fn open_pull_request(
        &mut self,
        item: warp_station_core::github::GitHubItem,
        cx: &mut Context<Self>,
    ) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        self.open_pull = Some(OpenPull {
            folder,
            item,
            pull: None,
            failed: false,
            merging: false,
            merge_armed: false,
        });
        self.reload_open_pull();
        cx.notify();
    }

    fn reload_open_pull(&mut self) {
        let Some(open) = &self.open_pull else { return };
        if let Some(gh_exe) = self.gh_exe() {
            let _ = self.net.send_blocking(NetCommand::GitHubPull {
                folder: std::path::PathBuf::from(&open.folder),
                gh_exe,
                number: open.item.number,
            });
        }
    }

    /// Back, or Escape. Returns whether a pull request was open.
    pub fn close_pull_request(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.open_pull.take().is_some();
        if open {
            cx.notify();
        }
        open
    }

    /// Merging can't be taken back, so the first press only arms the button.
    pub fn press_merge(&mut self, cx: &mut Context<Self>) {
        let Some(gh_exe) = self.gh_exe() else { return };
        let Some(open) = self.open_pull.as_mut() else {
            return;
        };
        if open.merging || !open.pull.as_ref().is_some_and(|p| p.ready_to_merge()) {
            return;
        }
        if !open.merge_armed {
            open.merge_armed = true;
            cx.notify();
            return;
        }
        open.merge_armed = false;
        open.merging = true;
        let _ = self.net.send_blocking(NetCommand::GitHubMerge {
            folder: std::path::PathBuf::from(&open.folder),
            gh_exe,
            number: open.item.number,
        });
        cx.notify();
    }

    /// The detail tabs these settings and this repo allow: an integration that
    /// is off takes its tabs along, and Pipelines needs an Azure project.
    pub fn detail_tabs(&self) -> Vec<WorkspaceTab> {
        let settings = self.settings();
        let azure = settings.repos.enable_azure_dev_ops
            && self
                .selected_repo
                .as_ref()
                .and_then(|folder| self.azure_by_folder.get(folder))
                .is_some_and(|snapshot| snapshot.available);
        WorkspaceTab::ALL
            .into_iter()
            .filter(|tab| match tab {
                WorkspaceTab::PullRequests | WorkspaceTab::Issues => settings.repos.enable_git_hub,
                WorkspaceTab::Pipelines => azure,
                WorkspaceTab::Overview
                | WorkspaceTab::Changes
                | WorkspaceTab::History
                | WorkspaceTab::Readme => true,
            })
            .collect()
    }

    /// The tab to draw: the one asked for, or Overview once it is unavailable.
    pub fn active_detail_tab(&self) -> WorkspaceTab {
        if self.detail_tabs().contains(&self.detail_tab) {
            self.detail_tab
        } else {
            WorkspaceTab::Overview
        }
    }

    /// Gives a repository its own tab, or goes to the one it has.
    pub fn open_repo_tab(&mut self, folder: &str, cx: &mut Context<Self>) {
        if !self.repo_tabs.iter().any(|tab| tab.folder == folder) {
            let tab = if self.selected_repo.as_deref() == Some(folder) {
                self.detail_tab
            } else {
                WorkspaceTab::Overview
            };
            self.repo_tabs.push(RepoTab {
                folder: folder.to_string(),
                tab,
            });
            self.save_repo_tabs();
        }
        self.show_repo_tab(Some(folder.to_string()), cx);
    }

    /// Persists the whole tab arrangement: the strip, each tab's resting
    /// detail tab, the on-screen tab and what the Repositories tab was
    /// showing. One write per arrangement change.
    fn save_repo_tabs(&self) {
        let folders: Vec<String> = self
            .repo_tabs
            .iter()
            .map(|tab| tab.folder.clone())
            .chain(self.pending_tabs.iter().cloned())
            .collect();
        let views: Vec<warp_station_core::settings::RepoTabView> = self
            .repo_tabs
            .iter()
            .map(|tab| warp_station_core::settings::RepoTabView {
                folder: tab.folder.clone(),
                tab: tab.tab.label().to_string(),
            })
            .collect();
        let home = self
            .home_repo
            .as_ref()
            .map(|folder| warp_station_core::settings::RepoTabView {
                folder: folder.clone(),
                tab: self.home_tab.label().to_string(),
            });
        self.update_settings(|s| {
            s.repos.repo_tabs = Some(folders);
            s.repos.tab_views = (!views.is_empty()).then_some(views);
            s.repos.open_tab = self.active_repo_tab.clone();
            s.repos.home_view = home;
        });
    }

    /// Brings last run's tabs into the strip as their repos land, each with
    /// the detail tab it was left on.
    fn settle_pending_tabs(&mut self) {
        if self.pending_tabs.is_empty() {
            return;
        }
        let (found, waiting): (Vec<String>, Vec<String>) = std::mem::take(&mut self.pending_tabs)
            .into_iter()
            .partition(|folder| {
                self.repos
                    .iter()
                    .any(|repo| repo.folder_path.to_string_lossy() == folder.as_str())
            });
        self.repo_tabs.extend(found.into_iter().map(|folder| {
            let tab = self.pending_views.remove(&folder).unwrap_or_default();
            RepoTab { folder, tab }
        }));
        // once a scan has settled, what is still missing has left for good —
        // and the strip (with its resting views) is final enough to write
        let settled = !self.scanning && !self.repos.is_empty();
        if settled {
            self.pending_views.clear();
            self.save_repo_tabs();
        } else {
            self.pending_tabs = waiting;
        }
    }

    pub fn list_width(&self) -> f32 {
        let saved = self
            .list_width
            .or(self.settings().repos.list_width)
            .unwrap_or_else(|| warpui_kit_core::shell::scaled(warpui_kit_core::shell::LIST_WIDTH));
        warpui_kit_core::shell::clamp_list_width(saved)
    }

    /// A drag step. The settings file waits for [`Self::save_list_width`].
    pub fn set_list_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.list_width = Some(warpui_kit_core::shell::clamp_list_width(width));
        cx.notify();
    }

    pub fn save_list_width(&self) {
        if let Some(width) = self.list_width {
            self.update_settings(|s| s.repos.list_width = Some(width));
        }
    }

    /// Moves to a repo tab, or to the Repositories tab with `None`. Each keeps
    /// its own detail tab. The arrangement writes back, so a restart lands on
    /// the tab the user left on screen.
    pub fn show_repo_tab(&mut self, target: Option<String>, cx: &mut Context<Self>) {
        let tab = self.detail_tab;
        match &self.active_repo_tab {
            Some(folder) => {
                if let Some(entry) = self.repo_tabs.iter_mut().find(|t| &t.folder == folder) {
                    entry.tab = tab;
                }
            }
            None => {
                self.home_repo = self.selected_repo.clone();
                self.home_tab = tab;
            }
        }
        self.load_view(target, cx);
        self.save_repo_tabs();
    }

    fn load_view(&mut self, target: Option<String>, cx: &mut Context<Self>) {
        let (folder, tab) = match &target {
            Some(folder) => {
                let entry = self.repo_tabs.iter().find(|tab| &tab.folder == folder);
                (
                    Some(folder.clone()),
                    entry.map_or_else(WorkspaceTab::default, |e| e.tab),
                )
            }
            None => (self.home_repo.clone(), self.home_tab),
        };
        self.active_repo_tab = target;
        self.detail_tab = tab;
        if let Some(folder) = folder {
            self.select_repo(&folder, cx);
        }
        cx.notify();
    }

    /// Closes a repo tab. The active one hands over to its neighbour, or to
    /// Repositories when none is left. The arrangement writes back after the
    /// handover, so the persisted open tab is the one now on screen.
    pub fn close_repo_tab(&mut self, folder: &str, cx: &mut Context<Self>) {
        let Some(ix) = self.repo_tabs.iter().position(|tab| tab.folder == folder) else {
            return;
        };
        self.repo_tabs.remove(ix);
        if self.active_repo_tab.as_deref() == Some(folder) {
            let next = self
                .repo_tabs
                .get(ix)
                .or_else(|| ix.checked_sub(1).and_then(|prev| self.repo_tabs.get(prev)))
                .map(|tab| tab.folder.clone());
            self.load_view(next, cx);
        } else {
            cx.notify();
        }
        self.save_repo_tabs();
    }

    /// Steps along the strip, Repositories included, wrapping at the ends.
    pub fn cycle_repo_tab(&mut self, delta: i32, cx: &mut Context<Self>) {
        let count = self.repo_tabs.len() as i32 + 1;
        let current = self
            .active_repo_tab
            .as_ref()
            .and_then(|folder| self.repo_tabs.iter().position(|tab| &tab.folder == folder))
            .map_or(0, |ix| ix as i32 + 1);
        self.show_repo_tab_at((current + delta).rem_euclid(count) as usize, cx);
    }

    /// Goes to a strip position: 0 is Repositories.
    pub fn show_repo_tab_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let target = match index.checked_sub(1) {
            None => None,
            Some(ix) => match self.repo_tabs.get(ix) {
                Some(tab) => Some(tab.folder.clone()),
                None => return,
            },
        };
        self.show_repo_tab(target, cx);
    }

    /// A press on a list row: a repo with a tab goes there from inside a tab,
    /// anything else previews in the Repositories tab.
    pub fn pick_repo(&mut self, folder: &str, cx: &mut Context<Self>) {
        if self.active_repo_tab.is_some() {
            if self.repo_tabs.iter().any(|tab| tab.folder == folder) {
                self.show_repo_tab(Some(folder.to_string()), cx);
                return;
            }
            self.show_repo_tab(None, cx);
        }
        self.select_repo(folder, cx);
    }

    /// What is running right now, for the title bar.
    pub fn activity(&self) -> Option<String> {
        let others = usize::from(self.summary_busy)
            + usize::from(self.wand_busy.is_some())
            + usize::from(self.scanning);
        let running = self.git_jobs.len() + others;
        if running > 1 {
            return Some(format!("{running} running"));
        }
        if let Some((folder, label)) = self.git_jobs.iter().next() {
            let name = self
                .repos
                .iter()
                .find(|repo| repo.folder_path.to_string_lossy() == folder.as_str())
                .map_or(folder.as_str(), |repo| repo.name.as_str());
            let word = crate::net::git_working_word(label).trim_end_matches('…');
            return Some(format!("{word} {name}"));
        }
        if self.summary_busy {
            return Some("Summarizing".to_string());
        }
        if self.wand_busy.is_some() {
            return Some("Generating".to_string());
        }
        self.scanning.then(|| "Refreshing".to_string())
    }

    /// Drops the selection. The page's next pass picks a row again, so this is
    /// how the selected repo leaves the rail.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected_repo = None;
        self.update_settings(|s| s.repos.selected_repo = None);
        self.reset_repo_workspace();
        cx.notify();
    }

    fn reset_repo_workspace(&mut self) {
        self.set_changes(None);
        self.changes_loaded_for = None;
        self.branches.clear();
        self.recent_commits.clear();
        self.set_history(Vec::new());
        self.unpushed = None;
        self.commit_details = None;
        self.commit_file_patch = None;
        self.github_details = None;
        self.open_pull = None;
        self.commit_message_draft = None;
        self.commit_message_clear = false;
        self.readme_draft = None;
        self.wand_error = None;
        self.wand_repo = None;
    }

    pub fn selected_entry(&self) -> Option<&RepoEntry> {
        self.selected_repo.as_ref().and_then(|folder| {
            self.repos
                .iter()
                .find(|r| r.folder_path.to_string_lossy() == *folder)
        })
    }

    /// Replaces the whole list (test setup; later also the Add-Repositories
    /// dialog's adoption path).
    pub fn replace_repos(&mut self, repos: Vec<RepoEntry>) {
        self.repos = repos;
        self.reproject();
    }

    /// Move the selection one row down (`delta = 1`) or up (`-1`) through the
    /// projection: the split screen's footer advertises ↑↓ as the row keymap.
    /// Steps clamp at the ends of the list, where the row stays selected.
    pub fn step_selection(&mut self, delta: i32, cx: &mut Context<Self>) {
        let rows: Vec<usize> = self
            .projected
            .iter()
            .filter_map(|row| row.repo_index())
            .collect();
        if rows.is_empty() {
            return;
        }
        let current = self
            .selected_repo
            .as_deref()
            .and_then(|folder| {
                rows.iter().position(|ix| {
                    self.repos
                        .get(*ix)
                        .is_some_and(|repo| repo.folder_path.to_string_lossy() == folder)
                })
            })
            .unwrap_or(0);
        let next = (current as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
        let Some(repo) = rows.get(next).and_then(|ix| self.repos.get(*ix)) else {
            return;
        };
        let folder = repo.folder_path.to_string_lossy().to_string();
        self.select_repo(&folder, cx);
    }

    fn first_row_folder(&self) -> Option<String> {
        self.projected
            .iter()
            .find_map(|row| row.repo_index())
            .and_then(|ix| self.repos.get(ix))
            .map(|repo| repo.folder_path.to_string_lossy().to_string())
    }

    /// The rail describes one repo and loads its sections per selection, so a
    /// row is always selected: the first one stands in until the user picks
    /// another, and again when the selected repo leaves the list.
    pub fn ensure_row_selected(&mut self, cx: &mut Context<Self>) {
        self.settle_pending_tabs();
        // a repo that left the list takes its tab along
        let gone: Vec<String> = self
            .repo_tabs
            .iter()
            .filter(|tab| {
                !self
                    .repos
                    .iter()
                    .any(|repo| repo.folder_path.to_string_lossy() == tab.folder.as_str())
            })
            .map(|tab| tab.folder.clone())
            .collect();
        for folder in gone {
            self.close_repo_tab(&folder, cx);
        }
        // last run's on-screen tab lands as its repo arrives
        if let Some(folder) = self.pending_open_tab.clone()
            && self.repo_tabs.iter().any(|tab| tab.folder == folder)
        {
            self.pending_open_tab = None;
            self.pending_selection = None;
            self.load_view(Some(folder), cx);
            return;
        }
        // so does last run's selected row; one the scan no longer finds falls
        // back to the first row below
        if let Some(folder) = self.pending_selection.clone() {
            if self
                .repos
                .iter()
                .any(|repo| repo.folder_path.to_string_lossy() == folder.as_str())
            {
                self.pending_selection = None;
                self.select_repo(&folder, cx);
                return;
            }
            if !self.scanning && !self.repos.is_empty() {
                self.pending_selection = None;
            }
        }
        // a tab holds its repo whatever the list is filtered to
        if self.active_repo_tab.is_some() {
            return;
        }
        if self.selected_entry().is_none()
            && let Some(folder) = self.first_row_folder()
        {
            self.select_repo(&folder, cx);
        }
    }

    fn reproject(&mut self) {
        // locals first: the grouping borrows them, not the store, so the
        // projection result can be assigned back without borrow fights
        let app_settings = self.settings();
        let solo = self.group_filter.clone();
        let collapsed: std::collections::HashSet<String> =
            self.collapsed_groups.iter().flatten().cloned().collect();
        let grouping = self.group_by_root.then(|| projection::Grouping {
            roots: app_settings
                .repos
                .repo_scan_folders
                .as_deref()
                .unwrap_or(&[]),
            depth: self.group_depth,
            collapsed: &collapsed,
            solo: solo.as_deref(),
        });
        self.projected = projection::project_scoped(
            &self.repos,
            &self.tag_filters,
            self.sort_mode,
            self.sort_ascending,
            self.show_archived,
            self.attention_only,
            grouping.as_ref(),
        );
        self.tags_cache = projection::all_tags(&self.repos);
    }

    /// Every distinct tag across the list, favorites and platform leading —
    /// the filter chip row. Cached from the last reproject.
    pub fn all_tags(&self) -> Vec<String> {
        self.tags_cache.clone()
    }

    // ---- tag mutations (write-behind) ---------------------------------------

    pub fn toggle_favorite(&mut self, folder: &str, cx: &mut Context<Self>) {
        let presses = self.star_pressed.as_ref().map_or(0, |(_, n)| n + 1);
        self.star_pressed = Some((folder.to_string(), presses));
        self.toggle_repo_tag_inner(folder, repo_store::FAVORITES_TAG, cx);
    }

    pub fn toggle_repo_tag(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        if tag.eq_ignore_ascii_case(repo_store::FAVORITES_TAG)
            || tag.eq_ignore_ascii_case(warp_station_core::scan::PLATFORM_TAG)
            || tag.eq_ignore_ascii_case(repo_store::ARCHIVED_TAG)
        {
            return;
        }
        self.toggle_repo_tag_inner(folder, tag, cx);
    }

    /// The bar header's "Add tag": case-insensitive duplicate check, then the
    /// normal tag toggle (which pushes when absent).
    pub fn add_repo_tag(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return;
        }
        let duplicate = self
            .repos
            .iter()
            .find(|r| r.folder_path.to_string_lossy() == folder)
            .is_some_and(|r| r.tags.iter().any(|t| t.eq_ignore_ascii_case(trimmed)));
        if duplicate {
            self.notify_info(format!("Tag \"{trimmed}\" already exists"));
            cx.notify();
            return;
        }
        self.toggle_repo_tag(folder, trimmed, cx);
    }

    /// The bar rail's refresh: re-probe the repo, reload its changes and an
    /// open history view.
    pub fn refresh_panel(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let path = std::path::PathBuf::from(&folder);
        let _ = self.net.send_blocking(NetCommand::ProbeAll {
            folders: vec![path.clone()],
        });
        let _ = self.net.send_blocking(NetCommand::LoadChanges {
            folder: path.clone(),
        });
        self.closed_requested.retain(|(key, _)| key != &folder);
        self.kick_github_pass(std::slice::from_ref(&path));
        self.reload_open_pull();
        if !self.history.is_empty() {
            let _ = self
                .net
                .send_blocking(NetCommand::LoadHistory { folder: path });
        }
        cx.notify();
    }

    /// The bar kebab's "Remove from repositories": drops the repo's folder
    /// from the persisted scan roots (disk untouched), rescans so the row
    /// leaves the list, and closes the bar.
    pub fn remove_from_repositories(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let name = self
            .repos
            .iter()
            .find(|r| r.folder_path.to_string_lossy() == folder)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| folder.clone());
        self.update_settings(|s| {
            if let Some(roots) = &mut s.repos.repo_scan_folders {
                roots.retain(|f| !warp_station_core::paths::same_folder_text(f, &folder));
            }
        });
        self.notify_success(format!("Removed \"{name}\" from repositories"));
        self.clear_selection(cx);
        self.kick_scan(&self.settings());
    }

    fn toggle_repo_tag_inner(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .repos
            .iter_mut()
            .find(|r| r.folder_path.to_string_lossy() == folder)
        else {
            return;
        };
        let existing = entry.tags.iter().position(|t| t.eq_ignore_ascii_case(tag));
        match existing {
            Some(ix) => {
                entry.tags.remove(ix);
            }
            None => {
                entry.tags.push(tag.to_string());
            }
        }
        self.reproject();
        self.persist_tags_soon(cx);
    }

    /// Write-behind cache save: a burst of star/tag/archive clicks collapses
    /// into one full-file write after a short debounce.
    fn persist_tags_soon(&mut self, cx: &mut Context<Self>) {
        self.save_generation += 1;
        let generation = self.save_generation;
        let repos = self.repos.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |store: &mut Store, _| {
                if store.save_generation == generation {
                    if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &repos) {
                        tracing::warn!("could not save repo cache: {err}");
                    }
                    store.save_generation = 0;
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The toolbar's Archive chip: flips whether archived rows join the
    /// projection (sorted below the live ones). The choice persists — the
    /// next launch opens on the same view of the list.
    pub fn toggle_show_archived(&mut self, cx: &mut Context<Self>) {
        self.show_archived = !self.show_archived;
        // absence IS the off state, so only the opt-in is written
        let on = self.show_archived;
        self.update_settings(|s| s.repos.show_archived = on.then_some(true));
        self.reproject();
        cx.notify();
    }

    /// Adds or removes the archived marker. Archiving also closes the bar
    /// when it points at the repo (its row is leaving the visible list);
    /// restoring re-probes the folder so the row comes back with fresh
    /// status. Persistence rides the same debounced save as tags — the
    /// marker IS a tag, so rescan merges carry it for free.
    pub fn set_repo_archived(&mut self, folder: &str, archived: bool, cx: &mut Context<Self>) {
        let Some(entry) = self
            .repos
            .iter_mut()
            .find(|r| r.folder_path.to_string_lossy() == folder)
        else {
            return;
        };
        let has = entry.is_archived();
        if has == archived {
            return;
        }
        if archived {
            entry.tags.push(repo_store::ARCHIVED_TAG.to_string());
        } else {
            let ix = entry
                .tags
                .iter()
                .position(|t| t.eq_ignore_ascii_case(repo_store::ARCHIVED_TAG))
                .expect("archived flag just checked");
            entry.tags.remove(ix);
        }
        self.reproject();
        if archived {
            if self.selected_repo.as_deref() == Some(folder) {
                self.clear_selection(cx);
            }
        } else {
            let _ = self.net.send_blocking(NetCommand::ProbeAll {
                folders: vec![std::path::PathBuf::from(folder)],
            });
        }
        self.persist_tags_soon(cx);
    }
}

#[derive(Clone, Debug)]
pub enum GlobalSearchHit {
    Repo {
        name: String,
        folder: String,
    },
    Note {
        title: String,
        excerpt: String,
        path: std::path::PathBuf,
    },
}

impl Store {
    /// One global search: repo name/path matches plus capped note-content
    /// hits across every tracked repo's notes root (6 repos, 12 notes,
    /// 40 files scanned per repo). The note walk reads files, so the work is
    /// debounced and runs off the UI thread; a newer keystroke wins.
    pub fn run_global_search(&mut self, term: &str, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        let term = term.trim().to_string();
        if term.is_empty() {
            self.global_search.clear();
            self.global_search_pending = false;
            self.code_search_hits.clear();
            self.code_searching = false;
            cx.notify();
            return;
        }
        // captured now, before the debounce: the background task must not
        // re-read the store, and archived repos stay out of both hit sets
        let repos: Vec<(String, String)> = self
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|repo| {
                (
                    repo.name.clone(),
                    repo.folder_path.to_string_lossy().to_string(),
                )
            })
            .collect();
        let notes_root =
            warp_station_core::notes::resolve_store_root(&self.settings().general.notes_store_path);
        self.global_search_pending = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(180))
                .await;
            let term_for_search = term.clone();
            let hits = cx
                .background_executor()
                .spawn(async move { global_search_hits(&repos, &notes_root, &term_for_search) })
                .await;
            let _ = this.update(cx, |store: &mut Store, cx| {
                if store.search_generation != generation {
                    return;
                }
                store.global_search = hits;
                store.global_search_pending = false;
                // the code sweep rides the same debounce: it dispatches once
                // the repo/note half settled, for this exact term
                store.dispatch_code_search(&term);
                cx.notify();
            });
        })
        .detach();
    }

    /// The code-search half of the global search: one ripgrep sweep over
    /// every non-archived repo folder, run on the net thread. Terms under
    /// three characters and the disabled setting skip it (a short term
    /// matches too much code to be useful); a missing rg binary warns once
    /// per settled term, like the gh pass. Each dispatch bumps the
    /// generation its [UiEvent::CodeSearchSettled] must answer with. The
    /// caller's `cx.notify()` covers the state changes made here.
    fn dispatch_code_search(&mut self, term: &str) {
        self.code_search_hits.clear();
        let settings = self.settings();
        if term.chars().count() < 3 || settings.repos.enable_code_search == Some(false) {
            self.code_searching = false;
            return;
        }
        let rg_setting = settings
            .repos
            .code_search_executable
            .clone()
            .unwrap_or_else(|| "rg".into());
        let Some(rg_exe) = warp_station_core::exe::locate(&rg_setting) else {
            self.code_searching = false;
            tracing::warn!("code search enabled but ripgrep could not be located ({rg_setting})");
            return;
        };
        // captured now: the net thread holds no repo state, and archived
        // repos stay out of the sweep (same capture-time filter as the hits)
        let folders: Vec<std::path::PathBuf> = self
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|repo| repo.folder_path.clone())
            .collect();
        if folders.is_empty() {
            self.code_searching = false;
            return;
        }
        self.code_search_generation += 1;
        let generation = self.code_search_generation;
        self.code_searching = true;
        let _ = self.net.send_blocking(NetCommand::CodeSearch {
            generation,
            term: term.to_string(),
            folders,
            rg_exe,
        });
    }
}

fn global_search_hits(
    repos: &[(String, String)],
    notes_root: &std::path::Path,
    term: &str,
) -> Vec<GlobalSearchHit> {
    const REPO_CAP: usize = 6;
    const NOTE_CAP: usize = 12;
    let needle = term.to_lowercase();
    let mut hits: Vec<GlobalSearchHit> = repos
        .iter()
        .filter(|(name, folder)| {
            name.to_lowercase().contains(&needle) || folder.to_lowercase().contains(&needle)
        })
        .take(REPO_CAP)
        .map(|(name, folder)| GlobalSearchHit::Repo {
            name: name.clone(),
            folder: folder.clone(),
        })
        .collect();

    // note content: the settings' store root, per-repo subfolders
    for (name, _) in repos {
        if hits.len() >= REPO_CAP + NOTE_CAP {
            break;
        }
        let notes_root = warp_station_core::notes::repo_notes_root(notes_root, name);
        for hit in warp_station_core::notes::search_capped(&notes_root, term, 40) {
            hits.push(GlobalSearchHit::Note {
                title: hit.title,
                excerpt: hit.excerpt,
                path: hit.path,
            });
            if hits.len() >= REPO_CAP + NOTE_CAP {
                break;
            }
        }
    }
    hits
}

/// Capitalizes a git action label for toast texts ("stage all" → "Stage all").
fn capitalized_label(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A toast's severity: which glyph + color it shows and how long it stays up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Success,
    Info,
    Error,
}

impl NoticeKind {
    /// Errors read longer; confirmations clear in the classic few seconds.
    pub fn ttl(self) -> std::time::Duration {
        match self {
            NoticeKind::Error => std::time::Duration::from_secs(8),
            NoticeKind::Info | NoticeKind::Success => std::time::Duration::from_secs(4),
        }
    }

    /// The copy helpers answer fixed static strings ("Password copied…",
    /// "No password stored", "Clipboard copy failed…"); classify those.
    pub fn for_copy_message(message: &str) -> Self {
        if message.starts_with("Clipboard copy failed") {
            NoticeKind::Error
        } else if message == "No password stored" {
            NoticeKind::Info
        } else {
            NoticeKind::Success
        }
    }
}

/// One toast in the window's corner stack.
#[derive(Clone, Debug)]
pub struct Notice {
    pub id: u64,
    pub kind: NoticeKind,
    pub text: String,
    born: std::time::Instant,
}

/// The detail pane's tabs, in the strip's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkspaceTab {
    #[default]
    Overview,
    Changes,
    History,
    PullRequests,
    Issues,
    Pipelines,
    Readme,
}

/// A pull request open in the detail pane. It stays on `item`, the list's row,
/// until `pull`, the full view, answers.
#[derive(Clone, Debug)]
pub struct OpenPull {
    pub folder: String,
    pub item: warp_station_core::github::GitHubItem,
    pub pull: Option<warp_station_core::github::GitHubPull>,
    pub failed: bool,
    pub merging: bool,
    pub merge_armed: bool,
}

/// A repository opened in a tab of its own, and the detail tab it was left on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoTab {
    pub folder: String,
    pub tab: WorkspaceTab,
}

impl WorkspaceTab {
    pub const ALL: [WorkspaceTab; 7] = [
        WorkspaceTab::Overview,
        WorkspaceTab::Changes,
        WorkspaceTab::History,
        WorkspaceTab::PullRequests,
        WorkspaceTab::Issues,
        WorkspaceTab::Pipelines,
        WorkspaceTab::Readme,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WorkspaceTab::Overview => "Overview",
            WorkspaceTab::Changes => "Changes",
            WorkspaceTab::History => "History",
            WorkspaceTab::PullRequests => "Pull requests",
            WorkspaceTab::Issues => "Issues",
            WorkspaceTab::Pipelines => "Pipelines",
            WorkspaceTab::Readme => "README",
        }
    }

    /// The tab's strip element, for its id and the probes' selectors.
    pub fn selector(self) -> &'static str {
        match self {
            WorkspaceTab::Overview => "detail-tab-overview",
            WorkspaceTab::Changes => "detail-tab-changes",
            WorkspaceTab::History => "detail-tab-history",
            WorkspaceTab::PullRequests => "detail-tab-pulls",
            WorkspaceTab::Issues => "detail-tab-issues",
            WorkspaceTab::Pipelines => "detail-tab-pipelines",
            WorkspaceTab::Readme => "detail-tab-readme",
        }
    }

    /// Case-insensitive label lookup.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tab| tab.label().eq_ignore_ascii_case(text.trim()))
    }
}

/// The AI surface's own switch: warp-agent alone. Kept apart from the OpenCode
/// CLI switch, which only arms the repo row's launch button — the runs
/// themselves happen in warp-agent, never in the opencode binary.
fn ai_enabled(settings: &AppSettings) -> bool {
    settings.warp_agent.enable_warp_agent
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two switches are independent: turning the OpenCode CLI off must
    /// leave the AI surface alone, and warp-agent off must alone turn it off.
    #[test]
    fn ai_surface_follows_warp_agent_only() {
        let mut settings = AppSettings::default();
        settings.opencode.enable_opencode = false;
        settings.warp_agent.enable_warp_agent = true;
        assert!(
            ai_enabled(&settings),
            "the OpenCode CLI switch must not gate AI features"
        );

        settings.opencode.enable_opencode = true;
        settings.warp_agent.enable_warp_agent = false;
        assert!(
            !ai_enabled(&settings),
            "warp-agent off turns the AI surface off"
        );
    }
}
