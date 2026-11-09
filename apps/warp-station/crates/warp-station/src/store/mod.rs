//! The app store: one entity owning all app state; views hold only ephemeral
//! UI state, snapshot `store.read(cx)` at render top and end mutations with
//! `cx.notify()`. The store never holds UI handles.

pub mod projection;

use crate::net::{GitAction, NetCommand, TemplateOp, UiEvent, WandKind, WarpWorkspaceStatus};
use crate::notices::{self, Notices};
use gpui::{App, Context};
use std::sync::Arc;
use warp_station_core::repo_store::{self, RepoEntry};
use warp_station_core::settings::{AppSettings, DiffViewMode, RepoSortMode, SettingsService};

// the toasts' types live beside the notification service; the store re-exports
// them so every existing `crate::store::NoticeKind` call site stays put
pub use crate::notices::{Notice, NoticeKind};

/// How long a button holds its past tense.
const FINISHED_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);

/// The dedicated GitHub/Azure activity cadence: the full-list pass that keeps
/// the rail's PR/issue counts fresh without waiting for a user refresh (the
/// per-refresh passes cover only the eager set below).
const ACTIVITY_REFRESH_SECS: u64 = 15 * 60;
/// How long a repo's GitHub/Azure snapshot stays trusted: opening the repo
/// or a refresh's eager set re-asks once the snapshot is older.
const ACTIVITY_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// How many wand requests may wait behind the one live run.
const WAND_QUEUE_CAP: usize = 8;

/// A wand draft that finished while its repo was off screen; the next
/// selection of that repo drains it into the live draft slot.
#[derive(Clone, Debug)]
pub struct WandDraft {
    pub kind: WandKind,
    pub text: String,
}

/// One queued wand request. The folder is captured at the click, so a
/// selection change while it waits never redirects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WandRequest {
    pub kind: WandKind,
    pub folder: String,
    /// A Generate & Commit request: the run's message is committed when it
    /// lands, even if another repo is on screen by then.
    pub auto_commit: bool,
}

/// One wand's user-facing subject, for the queue and ready notices.
fn wand_noun(kind: WandKind) -> &'static str {
    match kind {
        WandKind::Commit => "Commit message",
        WandKind::Readme => "README",
    }
}

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
    /// The closed list is already asked for (in flight or cached). One ask per
    /// (folder, pull_requests); cleared by Refresh, a merge, or Retry.
    closed_requested: std::collections::HashSet<(String, bool)>,
    /// Closed list requests that failed. The tab shows an error + Retry
    /// instead of a "Loading…" that never ends (audit A2).
    pub closed_failed: std::collections::HashSet<(String, bool)>,

    // ---- wand (warp-agent) state ----
    pub wand_busy: Option<WandKind>,
    /// The live run's id, for the progress chip's Cancel.
    pub wand_id: Option<String>,
    /// The live run commits its draft when it lands (Generate & Commit).
    wand_auto_commit: bool,
    /// Auto-committed messages by folder, kept until each commit lands so a
    /// failed commit hands the message back as a draft.
    wand_commits: std::collections::HashMap<String, String>,
    /// The repo the running wand was kicked for. A selection change keeps it
    /// so the title bar can name the run and Cancel stays reachable; its
    /// draft lands live only while that repo is selected and is otherwise
    /// kept in [`Self::wand_drafts`].
    pub wand_repo: Option<String>,
    pub wand_error: Option<String>,
    /// The commit-message wand's draft, drained into the Changes commit box.
    pub commit_message_draft: Option<String>,
    /// Set when a commit succeeded: the view drains it to empty the box.
    pub commit_message_clear: bool,
    /// The README wand's draft, rendered by the Overview's README card with
    /// Save/Discard.
    pub readme_draft: Option<String>,
    /// Finished drafts whose repo was off screen, keyed by folder: selecting
    /// the repo drains its entry into the live draft.
    pub wand_drafts: std::collections::HashMap<String, WandDraft>,
    /// Requests waiting for the one wand slot, oldest first; the live run's
    /// end starts the next.
    pub wand_queue: std::collections::VecDeque<WandRequest>,
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
    /// The notification service: the toast stack (newest last, capped; the
    /// window overlay renders it and the per-second tick expires it) plus the
    /// desktop-mirror routing. `notify_*` calls into it.
    pub notices: Notices,
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
    /// When the next full-list GitHub/Azure activity pass fires: the rail's
    /// PR/issue counts cadence, deliberately slower than the fetch/refresh
    /// clocks it no longer rides on. Armed by
    /// [Self::refresh_activity_schedule], checked on the same tick.
    pub next_activity_refresh: Option<std::time::Instant>,
    /// The running fetch pass's streaming progress, straight from the
    /// per-repo settle events: the Refresh button's Fetching k/n stage.
    pub fetch_progress: Option<(u32, u32)>,
    /// The queued fetch pass's per-folder behind counts, read at queue time:
    /// [Self::summarize_fetch_pass] compares the final counts against this
    /// once the pass's last repo lands.
    fetch_baseline: Option<std::collections::HashMap<String, u32>>,
    /// The folders the fetch pass covers for the refresh in flight: the
    /// post-scan probe skips exactly these, since the pass re-probes each of
    /// its own as the fetch lands.
    fetch_probe_coverage: Option<std::collections::HashSet<String>>,
    /// When each repo's last GitHub/Azure snapshot landed, for the open and
    /// eager passes' freshness gate ([Self::maybe_activity_kick]).
    activity_landed_at: std::collections::HashMap<String, std::time::Instant>,
    /// The harness a user-driven catalog refresh is waiting on; the answer's
    /// `fresh` flag decides whether it becomes "updated" or a cache notice.
    pub models_refresh_requested: Option<String>,
    /// Last focus state the tick observed, for the notification gate; None
    /// before the first observation reads as visible.
    window_focused: Option<bool>,
    /// A second launch asked this instance to come forward; the window clears
    /// it from its store observer (the single-instance door).
    focus_requested: bool,

    /// Write-behind cache saves: bumped on every cache-relevant mutation
    /// (tags, favorites and the status/activity counts); the deferred flush
    /// only writes when its generation is still current. Kept separate from
    /// [`Self::run_id_seq`]: a wand or summary run must not invalidate a
    /// pending cache save (the bug this split fixes).
    cache_generation: u64,
    /// Monotonic id source for wand/summary runs, so two runs in one session
    /// never share an id without touching the cache-save gate.
    run_id_seq: u64,
    /// Debounce sequence for persisting the selected row: arrow-key browsing
    /// updates the in-memory selection at once but writes settings once.
    selection_generation: u64,
    /// Latest NuGet watch service snapshot (watching, count, activity log).
    pub nuget: Arc<warp_station_core::nuget::NugetSnapshot>,
    /// Distinct tags across the list (filter chip row), recomputed on
    /// reproject so the toolbar does not rescan every render.
    tags_cache: Vec<String>,
    /// Bumped whenever the repo list (or the selection contract) changes in a
    /// way `ensure_row_selected` must react to. The repos page observes this
    /// instead of running an O(n) pass on every store notification (audit C5).
    pub list_version: u64,
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
            closed_failed: std::collections::HashSet::new(),
            wand_busy: None,
            wand_id: None,
            wand_auto_commit: false,
            wand_commits: std::collections::HashMap::new(),
            wand_repo: None,
            wand_error: None,
            commit_message_draft: None,
            commit_message_clear: false,
            readme_draft: None,
            wand_drafts: std::collections::HashMap::new(),
            wand_queue: std::collections::VecDeque::new(),
            harnesses,
            catalog,
            template_loaded: None,
            summary_busy: false,
            warp_connected: false,
            warp_running: std::collections::HashMap::new(),
            notices: Notices::default(),
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
            next_activity_refresh: None,
            fetch_progress: None,
            fetch_baseline: None,
            fetch_probe_coverage: None,
            activity_landed_at: std::collections::HashMap::new(),
            models_refresh_requested: None,
            window_focused: None,
            focus_requested: false,
            cache_generation: 0,
            run_id_seq: 0,
            selection_generation: 0,
            nuget: Arc::new(Default::default()),
            tags_cache: Vec::new(),
            list_version: 0,
            search_generation: 0,
            global_search_pending: false,
            code_search_hits: Vec::new(),
            code_search_generation: 0,
            code_searching: false,
        };

        // User tags moved from the repo cache into settings. Lift the old
        // cache's tags once, then let settings be the source of truth; the
        // cache write below drops the duplicated tag list.
        let cached_tags: std::collections::BTreeMap<String, Vec<String>> = store
            .repos
            .iter()
            .filter(|repo| !repo.tags.is_empty())
            .map(|repo| {
                (
                    repo.folder_path.to_string_lossy().to_string(),
                    user_tags(&repo.tags),
                )
            })
            .filter(|(_, tags)| !tags.is_empty())
            .collect();
        let settings_tags = store.settings.get().repos.repo_tags;
        let mut lifted_cache_tags = false;
        if settings_tags.is_empty() {
            if !cached_tags.is_empty() {
                let _ = store
                    .settings
                    .update(|settings| settings.repos.repo_tags = cached_tags);
                lifted_cache_tags = true;
            }
        } else {
            for repo in &mut store.repos {
                repo.tags = settings_tags
                    .get(&repo.folder_path.to_string_lossy().to_string())
                    .cloned()
                    .unwrap_or_default();
            }
        }
        if lifted_cache_tags {
            store.save_cache_now();
        }
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
        // the launch just queued the full GitHub/Azure pass below, so the
        // activity clock starts a whole cadence out
        store.next_activity_refresh =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(ACTIVITY_REFRESH_SECS));

        // Cache first for instant labels. The scan is a filesystem walk
        // (milliseconds) and goes out before the network-bound activity
        // pass: its settle probes the merged list, so the launch never
        // waits for `gh` to learn its own repos. Archived repos stay out of
        // the activity pass; the scan still gets the full list so their
        // tags survive the merge.
        let folders: Vec<std::path::PathBuf> = store
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|r| r.folder_path.clone())
            .collect();
        store.scanning = true;
        let _ = store.net.send_blocking(NetCommand::Scan {
            settings: app_settings.repos.clone(),
        });
        store.kick_github_pass(&folders);
        store.kick_azure_pass(&folders);

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

    /// The Changes pane's persisted diff layout.
    pub fn diff_view(&self) -> DiffViewMode {
        self.settings().changes.diff_view
    }

    /// Persist the Changes pane's diff layout; the view repaints itself.
    pub fn set_diff_view(&mut self, mode: DiffViewMode) {
        self.update_settings(|s| s.changes.diff_view = mode);
    }

    /// Whether the Changes diff wraps long rows.
    pub fn diff_wrap(&self) -> bool {
        self.settings().changes.diff_wrap
    }

    /// Persist the word-wrap choice; the view repaints itself.
    pub fn set_diff_wrap(&mut self, wrap: bool) {
        self.update_settings(|s| s.changes.diff_wrap = wrap);
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
        });
    }

    /// A full refresh — the repository page's Refresh button. The order is
    /// local-first: the scan (a filesystem walk, milliseconds) refreshes the
    /// list at once, then the much slower fetch pass rides behind it,
    /// re-probing each repo as its own fetch lands (per-repo badges). The
    /// eager set (favorites + the selected repo) re-asks even when its
    /// snapshots are still inside [ACTIVITY_TTL]: an explicit ask outranks
    /// the clock. While a scan is already in flight that pass is this ask:
    /// nothing is queued behind it.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_inner(cx, true);
    }

    /// The refresh body: scan first (local), fetch queued after it.
    /// `force_activity` re-asks the eager set even inside its TTL.
    fn refresh_inner(&mut self, cx: &mut Context<Self>, force_activity: bool) {
        if self.scanning {
            cx.notify();
            return;
        }
        let settings = self.settings();
        let folders = self.fetchable_folders();
        self.kick_scan(&settings);
        self.queue_fetch(&folders);
        self.kick_eager_activity(force_activity);
        cx.notify();
    }

    /// Queues the fetch pass and records which folders it re-probes itself,
    /// so the scan's settle only probes the complement. An empty list is no
    /// coverage and no pass.
    fn queue_fetch(&mut self, folders: &[std::path::PathBuf]) {
        if folders.is_empty() {
            self.fetch_probe_coverage = None;
            return;
        }
        self.fetch_probe_coverage = Some(
            folders
                .iter()
                .map(|f| f.to_string_lossy().to_string())
                .collect(),
        );
        // the wake-up baseline: the behind counts the pass set out from
        self.fetch_baseline = Some(
            folders
                .iter()
                .filter_map(|folder| {
                    self.repos
                        .iter()
                        .find(|repo| &repo.folder_path == folder)
                        .map(|repo| (folder.to_string_lossy().to_string(), repo.git_to_pull_count))
                })
                .collect(),
        );
        // a new pass resets the previous one's progress
        self.fetch_progress = None;
        self.net
            .send_blocking(NetCommand::FetchAll {
                folders: folders.to_vec(),
            })
            .ok();
    }

    pub fn on_net_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        match event {
            UiEvent::ScanSettled { scanned } => {
                // Merge into the live list, not into the snapshot the scan
                // was queued with: a status probe or a GitHub snapshot can
                // land while the scan waits behind a fetch or a `gh` sweep,
                // and replaying the older snapshot would blank the counts
                // they just filled.
                let (repos, _removed) = repo_store::merge_scan(&self.repos, &scanned);
                self.repos = repos;
                self.save_cache_now();
                self.list_version += 1;
                self.scanning = false;
                self.reproject();
                // the fetch pass re-probes its own folders as each fetch
                // lands, so the settle probe covers only the complement —
                // the no-remote repos after a refresh, every repo after a
                // scan-only kick (no fetch pass queued)
                let coverage = self.fetch_probe_coverage.take();
                let folders: Vec<std::path::PathBuf> = self
                    .repos
                    .iter()
                    .filter(|repo| !repo.is_archived())
                    .map(|r| r.folder_path.clone())
                    .filter(|folder| {
                        coverage
                            .as_ref()
                            .map(|cover| !cover.contains(&folder.to_string_lossy().to_string()))
                            .unwrap_or(true)
                    })
                    .collect();
                if !folders.is_empty() {
                    let _ = self.net.send_blocking(NetCommand::ProbeAll { folders });
                }
                // the GitHub/Azure sweeps moved to the activity schedule and
                // the eager set; a scan settling no longer sweeps the list
                cx.notify();
            }
            UiEvent::StatusSettled {
                probes,
                after_fetch,
                progress,
            } => {
                let by_folder = self.repo_index_by_folder();
                let mut applied = false;
                for probe in &probes {
                    // a user git action owns the truth until its own probe
                    // lands: now that passes run off the command loop, a
                    // background probe can have read mid-mutation (audit B1)
                    if self.git_jobs.contains_key(&probe.folder_path) {
                        continue;
                    }
                    if let Some(&ix) = by_folder.get(&probe.folder_path) {
                        repo_store::apply_status_probe(&mut self.repos[ix], probe);
                        applied = true;
                    }
                }
                // the cache carries the last known counts: keep it in step
                // with the probes so the next launch paints them at once
                if applied {
                    self.persist_cache_soon(cx);
                }
                // a fetch pass streams one repo at a time: the settled badge
                // goes up as each fetch lands, and the toolbar tracks k/n
                if after_fetch && let Some((done, total)) = progress {
                    self.fetch_progress = Some((done, total));
                    // the pass's wake-up summary is a pass aggregate: only
                    // computed once the last repo's probe landed, compared
                    // against the behind counts read at queue time
                    if done >= total {
                        self.fetch_progress = None;
                        self.summarize_fetch_pass();
                    }
                }
                // a fetch pass report ends here: the selected repo's open
                // workspace re-reads so the rail's file list and branches
                // show the post-fetch state without a manual reselect
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
                    // the freshness gate for the eager and open passes: a
                    // failed probe must not count as a fresh answer
                    if snapshot.available {
                        self.activity_landed_at
                            .insert(folder.clone(), std::time::Instant::now());
                    }
                    self.azure_by_folder.insert(folder, Arc::new(snapshot));
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::GitHubSettled { items } => {
                let by_folder = self.repo_index_by_folder();
                let mut counts_changed = false;
                for (folder, snapshot) in items {
                    // a failed probe is not a fresh answer: the rail keeps
                    // its last known counts, and the eager/open kick may
                    // retry at once instead of waiting out the TTL
                    if snapshot.available {
                        self.activity_landed_at
                            .insert(folder.clone(), std::time::Instant::now());
                    }
                    if snapshot.available
                        && let Some(&ix) = by_folder.get(&folder)
                    {
                        let entry = &mut self.repos[ix];
                        let pulls = snapshot.pulls.len() as u32;
                        let issues = snapshot.issues.len() as u32;
                        if entry.github_pr_count != pulls || entry.github_issue_count != issues {
                            entry.github_pr_count = pulls;
                            entry.github_issue_count = issues;
                            counts_changed = true;
                        }
                    }
                    // the tab still records the failure, so it can offer a
                    // retry instead of showing stale pulls as current
                    self.github_by_folder.insert(folder, Arc::new(snapshot));
                }
                if counts_changed {
                    self.persist_cache_soon(cx);
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
                let key = (folder, pull_requests);
                match items {
                    Some(items) => {
                        self.closed_failed.remove(&key);
                        self.github_closed.insert(key, Arc::new(items));
                    }
                    None => {
                        // keep the guard so the tab does not re-ask every
                        // frame; the tab renders an error with Retry instead
                        self.closed_failed.insert(key);
                        self.notify_error("GitHub did not answer with the closed list");
                    }
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
                        let key = (folder, true);
                        self.github_closed.remove(&key);
                        self.closed_requested.remove(&key);
                        self.closed_failed.remove(&key);
                    }
                    Err(err) if err.is_empty() => {
                        self.notify_error(format!("Could not merge #{number}"));
                    }
                    Err(err) => self.notify_error(err),
                }
                cx.notify();
            }
            UiEvent::ModelsLoaded {
                harness,
                catalog,
                fresh,
            } => {
                self.apply_catalog(&harness, catalog, fresh);
                if self.models_refresh_requested.as_deref() == Some(harness.as_str()) {
                    self.models_refresh_requested = None;
                    if fresh {
                        self.notify_success("Models updated");
                    } else {
                        self.notify_error("Could not refresh models — showing the cached catalog");
                    }
                }
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
            UiEvent::FocusRequested => {
                self.focus_requested = true;
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
                // a run kicked for repo A must never land its draft in
                // another repo's box: the repo keeps it until it is selected
                let repo = self.wand_repo.clone();
                let for_selected = repo.as_deref() == self.selected_repo.as_deref();
                let name = repo.as_deref().map(|folder| self.repo_name(folder));
                let noun = wand_noun(kind);
                let for_name = name
                    .as_deref()
                    .map(|name| format!(" for {name}"))
                    .unwrap_or_default();
                self.wand_busy = None;
                self.wand_id = None;
                self.wand_repo = None;
                let cancelled = matches!(&result, Err(err) if err == "cancelled");
                // an auto-commit consumes the message: it never lands in a box
                let auto_commit = std::mem::take(&mut self.wand_auto_commit);
                match (&result, kind) {
                    (Ok(text), kind) => {
                        let text = text.trim().to_string();
                        if auto_commit && kind == WandKind::Commit && !text.is_empty() {
                            if let Some(folder) = repo {
                                self.commit_wand_message(folder, text);
                            }
                        } else if for_selected {
                            self.set_wand_draft(kind, text);
                        } else if let (Some(folder), Some(name)) = (repo, name) {
                            self.wand_drafts.insert(folder, WandDraft { kind, text });
                            // the repo is off screen, so the box the draft
                            // waits for cannot show it: say it is ready
                            self.notify_info(format!("{noun} ready for {name}"));
                        }
                    }
                    (Err(err), WandKind::Commit) => {
                        if cancelled {
                            self.notify_info("Commit message generation cancelled");
                        } else {
                            tracing::warn!("wand failed: {err}");
                            if for_selected {
                                self.wand_error = Some(err.clone());
                            }
                            self.notify_error(format!(
                                "Could not generate a commit message{for_name}"
                            ));
                        }
                    }
                    (Err(err), WandKind::Readme) => {
                        if cancelled {
                            self.notify_info("README generation cancelled");
                        } else {
                            tracing::warn!("wand failed: {err}");
                            if for_selected {
                                self.wand_error = Some(err.clone());
                            }
                            self.notify_error(format!("Could not generate a README{for_name}"));
                        }
                    }
                }
                cx.notify();
                // the slot is free: a request that waited behind this run
                // starts now
                self.start_next_wand(cx);
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
                // the commit a Generate & Commit run fired, if this is its
                // answer: its message must not be counted as a typed one
                let wand_message = if label == "commit" {
                    self.wand_commits.remove(&folder)
                } else {
                    None
                };
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
                    // nothing was committed: keep the generated message for
                    // its repo instead of losing it
                    if let Some(message) = wand_message {
                        if self.selected_repo.as_deref() == Some(folder.as_str()) {
                            // the box is on screen: hand it straight back
                            self.commit_message_draft = Some(message);
                        } else {
                            self.wand_drafts.insert(
                                folder.clone(),
                                WandDraft {
                                    kind: WandKind::Commit,
                                    text: message,
                                },
                            );
                        }
                        self.notify_info(format!("Commit message ready for {name}"));
                    }
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
                    if label == "commit" {
                        // a store-fired commit never filled the box, so it
                        // must not empty a draft the user typed meanwhile
                        if wand_message.is_none()
                            && self.selected_repo.as_deref() == Some(folder.as_str())
                        {
                            self.commit_message_clear = true;
                        }
                        // the commit used the message: an older kept draft
                        // for the same repo is stale
                        if wand_message.is_some()
                            && self
                                .wand_drafts
                                .get(&folder)
                                .is_some_and(|draft| draft.kind == WandKind::Commit)
                        {
                            self.wand_drafts.remove(&folder);
                        }
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
        if !settings.repos.enable_github || folders.is_empty() {
            return;
        }
        let Some(gh_path) = warp_station_core::exe::locate(&settings.repos.github_executable)
        else {
            tracing::warn!(
                "GitHub column enabled but the gh CLI could not be located ({})",
                settings.repos.github_executable
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
        if !settings.repos.enable_azure_devops || folders.is_empty() {
            return;
        }
        let Some(pat) = warp_station_core::azure::resolve_pat(&settings.repos.azure_devops_pat)
        else {
            tracing::warn!(
                "Azure DevOps column enabled but no personal access token is configured; set one in Repos settings (or the AZURE_DEVOPS_PAT environment variable)"
            );
            return;
        };
        let _ = self.net.send_blocking(NetCommand::AzurePass {
            folders: folders.to_vec(),
            pat,
            custom_base: settings.repos.azure_devops_url.clone(),
        });
    }

    /// The eager GitHub/Azure set of a refresh: favorites and the selected
    /// repo, non-archived. The integration gates live in the kickers, so a
    /// disabled column means this sends nothing.
    fn eager_activity_folders(&self) -> Vec<std::path::PathBuf> {
        let selected = self.selected_repo.clone();
        self.repos
            .iter()
            .filter(|repo| {
                !repo.is_archived()
                    && (repo.is_favorite()
                        || selected.as_deref() == Some(repo.folder_path.to_string_lossy().as_ref()))
            })
            .map(|repo| repo.folder_path.clone())
            .collect()
    }

    /// Kicks the GitHub/Azure passes for `folders`, skipping the snapshots
    /// that landed within [ACTIVITY_TTL] unless `force`. Freshness is tracked
    /// per folder in one map for both integrations: the two sweeps answer at
    /// the same cadence, so a shared age is honest enough.
    fn maybe_activity_kick(&mut self, folders: &[std::path::PathBuf], force: bool) {
        if folders.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let due: Vec<std::path::PathBuf> = folders
            .iter()
            .filter(|folder| {
                force
                    || self
                        .activity_landed_at
                        .get(&folder.to_string_lossy().to_string())
                        .map(|landed| now.duration_since(*landed) >= ACTIVITY_TTL)
                        .unwrap_or(true)
            })
            .cloned()
            .collect();
        if due.is_empty() {
            return;
        }
        self.kick_github_pass(&due);
        self.kick_azure_pass(&due);
    }

    /// Kicks the eager activity set (favorites + the selected repo) through
    /// the freshness gate; the Refresh button forces it, the open/select paths
    /// do not. Public so a probe can exercise the gate without the scan/fetch
    /// side effects of a full refresh.
    pub fn kick_eager_activity(&mut self, force: bool) {
        let folders = self.eager_activity_folders();
        self.maybe_activity_kick(&folders, force);
    }

    /// A repo just opened: its GitHub/Azure snapshots re-ask when stale — the
    /// "lazy on open" leg of the activity plan.
    fn kick_open_activity(&mut self, folder: &str) {
        let folders = vec![std::path::PathBuf::from(folder)];
        self.maybe_activity_kick(&folders, false);
    }

    /// The activity clock: a slower, dedicated cadence for the full-list
    /// GitHub/Azure sweeps (non-archived) that keep the rail's PR/issue
    /// counts fresh independently of the per-refresh eager set. The launch
    /// arm sits a whole cadence out — the startup queues the full pass.
    pub fn refresh_activity_schedule(&mut self) {
        self.next_activity_refresh =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(ACTIVITY_REFRESH_SECS));
    }

    /// The tick's activity check: when due, run the full-list GitHub/Azure
    /// passes and re-arm a whole cadence out. The first scan and status pass
    /// must land first (the same hold a refresh's fetch pass uses; an empty
    /// list has nothing to sweep).
    pub fn check_activity_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.next_activity_refresh else {
            return;
        };
        if std::time::Instant::now() < due {
            return;
        }
        if self.repos.is_empty() || self.repos.iter().all(|repo| !repo.git_status_loaded) {
            return;
        }
        let folders: Vec<std::path::PathBuf> = self
            .repos
            .iter()
            .filter(|repo| !repo.is_archived())
            .map(|r| r.folder_path.clone())
            .collect();
        if !folders.is_empty() {
            self.kick_github_pass(&folders);
            self.kick_azure_pass(&folders);
        }
        self.refresh_activity_schedule();
        cx.notify();
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
        let id = self.next_run_id("summary");
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
    /// for the harness it is showing; `refresh` asks the door to re-read the
    /// backend instead of answering from its in-memory list.
    pub fn load_models_for(&mut self, harness: String, refresh: bool, cx: &mut Context<Self>) {
        // a load for another harness retires the previous refresh's pending
        // toast: the answer would otherwise report a catalog the page left
        if self
            .models_refresh_requested
            .as_deref()
            .is_some_and(|pending| pending != harness)
        {
            self.models_refresh_requested = None;
        }
        if refresh {
            self.models_refresh_requested = Some(harness.clone());
        }
        if self.warp_ready().is_some() {
            let _ = self
                .net
                .send_blocking(NetCommand::LoadModels { harness, refresh });
        } else {
            if refresh {
                self.models_refresh_requested = None;
                self.notify_error("Could not refresh models — warp-agent is disabled or missing");
            }
            self.apply_catalog(
                &harness,
                warp_station_core::opencode::cached_harness_catalog(&harness),
                false,
            );
            cx.notify();
        }
    }

    /// Replaces one harness's slice of the catalog; the other harness's
    /// entries stay so switching the radio never empties a loaded list.
    /// A `fresh` empty answer clears the harness's slice (the backend really
    /// has no models now); an offline miss keeps the last known list.
    fn apply_catalog(
        &mut self,
        harness: &str,
        catalog: warp_station_core::opencode::ModelCatalog,
        fresh: bool,
    ) {
        if !catalog.harnesses.is_empty() {
            self.harnesses = catalog.harnesses.clone();
        }
        if self.harnesses.is_empty() {
            self.harnesses = warp_station_core::opencode::default_harnesses();
        }
        self.catalog.harnesses = self.harnesses.clone();
        if catalog.models.is_empty() {
            if fresh {
                self.catalog.models.retain(|model| model.harness != harness);
            }
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
    /// separate commit model. While another run is live the request queues
    /// for its repo and starts when the slot frees. `auto_commit` commits the
    /// drafted message when the run lands (Generate & Commit).
    pub fn run_wand(&mut self, kind: WandKind, auto_commit: bool, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let auto_commit = auto_commit && kind == WandKind::Commit;
        // the gate is checked at the click so a disabled or missing
        // warp-agent reports now; a queued run re-checks when it starts
        let (exe, choice) = match self.wand_launch() {
            Ok(launch) => launch,
            Err(message) => {
                self.notify_error(message);
                cx.notify();
                return;
            }
        };
        if self.wand_busy.is_some() {
            self.enqueue_wand(kind, folder, auto_commit);
        } else {
            self.start_wand(kind, folder, exe, choice, auto_commit);
        }
        cx.notify();
    }

    /// Whether this repo's request of `kind` is waiting for the slot.
    pub fn wand_queued(&self, kind: WandKind, folder: &str) -> bool {
        self.wand_queue
            .iter()
            .any(|queued| queued.kind == kind && queued.folder == folder)
    }

    /// Queues one request for its repo; the live run's end starts it.
    fn enqueue_wand(&mut self, kind: WandKind, folder: String, auto_commit: bool) {
        if self.wand_busy == Some(kind) && self.wand_repo.as_deref() == Some(folder.as_str()) {
            // this exact run is already live; the click would repeat it
            return;
        }
        if self
            .wand_queue
            .iter()
            .any(|queued| queued.kind == kind && queued.folder == folder)
        {
            return;
        }
        if self.wand_queue.len() >= WAND_QUEUE_CAP {
            self.notify_info(format!(
                "The generation queue is full ({WAND_QUEUE_CAP}); wait for one to finish"
            ));
            return;
        }
        let name = self.repo_name(&folder);
        self.wand_queue.push_back(WandRequest {
            kind,
            folder,
            auto_commit,
        });
        self.notify_info(format!("{} queued for {name}", wand_noun(kind)));
    }

    /// Drops one queued request (the queued chip's Cancel).
    pub fn dequeue_wand(&mut self, kind: WandKind, folder: &str, cx: &mut Context<Self>) {
        let before = self.wand_queue.len();
        self.wand_queue
            .retain(|queued| !(queued.kind == kind && queued.folder == folder));
        if self.wand_queue.len() != before {
            self.notify_info("Queued generation cancelled");
        }
        cx.notify();
    }

    /// Stops everything: the live run and every queued request (the title
    /// bar's stop). The per-item Cancel drops one queue entry instead.
    pub fn cancel_all_wands(&mut self, cx: &mut Context<Self>) {
        let queued = self.wand_queue.len();
        self.wand_queue.clear();
        self.cancel_wand(cx);
        if queued > 0 {
            self.notify_info(match queued {
                1 => "1 queued generation cleared".to_string(),
                count => format!("{count} queued generations cleared"),
            });
        }
        cx.notify();
    }

    /// Stops the live wand run; a request queued behind it starts next. The
    /// run still answers with WandDone, so the busy state clears through the
    /// one path and a late draft can never land.
    pub fn cancel_wand(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.wand_id.clone() else {
            return;
        };
        let _ = self.net.send_blocking(NetCommand::CancelWand { id });
        cx.notify();
    }

    /// Starts queued requests until one takes the slot (or none is left);
    /// a request whose repo left the list is dropped.
    fn start_next_wand(&mut self, cx: &mut Context<Self>) {
        while self.wand_busy.is_none() {
            let Some(next) = self.wand_queue.pop_front() else {
                break;
            };
            if !self
                .repos
                .iter()
                .any(|repo| repo.folder_path.to_string_lossy() == next.folder)
            {
                continue;
            }
            match self.wand_launch() {
                Ok((exe, choice)) => {
                    self.start_wand(next.kind, next.folder, exe, choice, next.auto_commit)
                }
                Err(message) => {
                    // the same gate blocks every request behind this one:
                    // report once instead of once per entry
                    self.notify_error(message);
                    self.wand_queue.clear();
                    break;
                }
            }
        }
        cx.notify();
    }

    /// What a run needs, or why it cannot start. Checked at the click and
    /// again when a queued request takes the slot.
    fn wand_launch(
        &self,
    ) -> Result<(std::path::PathBuf, warp_station_core::warp::ModelChoice), String> {
        let settings = self.settings();
        if !settings.warp_agent.enable_warp_agent {
            return Err(
                "Could not generate — warp-agent integration is disabled in settings".to_string(),
            );
        }
        let Some(exe) = warp_station_core::exe::locate(&settings.warp_agent.executable) else {
            return Err("Could not generate — warp-agent executable not found".to_string());
        };
        Ok((
            exe,
            warp_station_core::warp::ModelChoice::from_settings(&settings),
        ))
    }

    /// Takes the slot for one run; the caller checked the gate.
    fn start_wand(
        &mut self,
        kind: WandKind,
        folder: String,
        exe: std::path::PathBuf,
        choice: warp_station_core::warp::ModelChoice,
        auto_commit: bool,
    ) {
        self.wand_busy = Some(kind);
        self.wand_repo = Some(folder.clone());
        self.wand_auto_commit = auto_commit;
        self.wand_error = None;
        let id = self.next_run_id("wand");
        self.wand_id = Some(id.clone());
        let _ = self.net.send_blocking(NetCommand::Wand {
            id,
            kind,
            folder: std::path::PathBuf::from(folder),
            choice,
            warp_exe: exe.to_string_lossy().to_string(),
        });
    }

    /// Fires the commit a Generate & Commit run asked for, in its own repo —
    /// the user may be looking at another one by now. The message is kept
    /// until the commit answers so a failure can hand it back.
    fn commit_wand_message(&mut self, folder: String, message: String) {
        self.wand_commits.insert(folder.clone(), message.clone());
        self.git_jobs.insert(folder.clone(), "commit");
        let _ = self.net.send_blocking(NetCommand::Git {
            folder: std::path::PathBuf::from(folder),
            action: GitAction::Commit(message),
        });
    }

    /// Puts one finished draft in the slot its view drains.
    fn set_wand_draft(&mut self, kind: WandKind, text: String) {
        match kind {
            WandKind::Commit => self.commit_message_draft = Some(text),
            WandKind::Readme => self.readme_draft = Some(text),
        }
    }

    /// Hands a repo the draft that finished while it was off screen.
    fn take_pending_wand_draft(&mut self, folder: &str) {
        let Some(draft) = self.wand_drafts.remove(folder) else {
            return;
        };
        self.set_wand_draft(draft.kind, draft.text);
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
        self.list_version += 1;
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
        self.save_cache_now();
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

    /// The one event walk: the toast lands as always, and the notification
    /// service decides whether the text also reaches the desktop (the
    /// settings' rules — unfocused gate, categories, dedup — all live there).
    /// The publish itself runs on a detached thread, so nothing here waits —
    /// including no context: this never touches a window, only the desktop.
    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        let settings = self.settings().notifications;
        // the tick-tracked focus: `None` before the first tick means the
        // window state is unobserved and reads as visible
        let focused = self.window_focused;
        if let Some(desktop) = self.notices.push(kind, text, focused, &settings) {
            notices::publish_desktop(desktop);
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
        if self.notices.dismiss(id) {
            cx.notify();
        }
    }

    /// Once-per-second tick: drops toasts past their kind's ttl, notifying
    /// only when one actually expired (idle windows stay idle).
    pub fn tick_notices(&mut self, cx: &mut Context<Self>) {
        if self.notices.tick() {
            cx.notify();
        }
    }

    /// The pass's result → the desktop: names what came down only when the
    /// pass actually pushed something further behind, and lets the settings
    /// and the dedup window answer whether the desktop should ring at all.
    /// The wake-up aggregate of a finished fetch pass, desktop-only: repos
    /// that went further behind than [Self::queue_fetch] found them, in list
    /// order. The per-repo settle events arrive one at a time, so the
    /// summary waits for the pass's last repo and reads the final counts
    /// against the queued baseline. Nothing went further behind → no toast,
    /// so a quiet remote pings once, a busy one once per change.
    fn summarize_fetch_pass(&mut self) {
        let Some(baseline) = self.fetch_baseline.take() else {
            return;
        };
        if let Some(text) = self.fetch_summary_text(&baseline) {
            self.notify_fetch_summary(text);
        }
    }

    /// The pass's text: repos whose behind count now exceeds the queued
    /// baseline, named in list order. Archived repos stay out even when a
    /// user archived one mid-pass — their folder can be in the queued
    /// baseline, and a folder the desktop sees as hibernating has no wake-up.
    fn fetch_summary_text(
        &self,
        previous: &std::collections::HashMap<String, u32>,
    ) -> Option<String> {
        let changed: Vec<(&RepoEntry, u32)> = self
            .repos
            .iter()
            .filter_map(|entry| {
                let before = previous.get(entry.folder_path.to_string_lossy().as_ref())?;
                let behind = entry.git_to_pull_count;
                let moved_further = !entry.is_archived() && behind > 0 && behind > *before;
                moved_further.then_some((entry, behind))
            })
            .collect();
        match changed.len() {
            0 => None,
            1 => {
                let (entry, behind) = changed[0];
                Some(format!(
                    "{} is {} commit{} behind",
                    entry.name,
                    behind,
                    if behind == 1 { "" } else { "s" }
                ))
            }
            _ => {
                let names = changed
                    .iter()
                    .map(|(entry, behind)| format!("{} ({})", entry.name, behind))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!("{} repos behind: {names}", changed.len()))
            }
        }
    }

    /// A fetch pass's aggregate, desktop-only: the store's background events
    /// are silent in-app (their results land in the repos list, visible on
    /// return), so the service gets no toast to mirror and routes straight to
    /// the desktop gate — same unfocused rule, same dedup as everything else.
    pub fn notify_fetch_summary(&mut self, text: impl Into<String>) {
        let settings = self.settings().notifications;
        let focused = self.window_focused;
        if let Some(desktop) = self.notices.push_desktop_only(text, focused, &settings) {
            notices::publish_desktop(desktop);
        }
    }

    // ---- focus and the fetch pass ------------------------------------------

    /// Whether any open window is the focused one. The notes window counts,
    /// so the user reading notes still counts as "looking at Warp Station".
    fn any_window_focused(cx: &mut App) -> bool {
        cx.windows().into_iter().any(|handle| {
            handle
                .update(cx, |_, window, _| window.is_window_active())
                .unwrap_or(false)
        })
    }

    /// The repos a fetch pass may touch: a remote is configured, the repo is
    /// not archived, and no git mutation is running in its folder (index.lock
    /// conflict avoidance).
    fn fetchable_folders(&self) -> Vec<std::path::PathBuf> {
        self.repos
            .iter()
            .filter(|repo| repo.git_has_remote && !repo.is_archived())
            .map(|repo| repo.folder_path.clone())
            .filter(|folder| {
                !self
                    .git_jobs
                    .contains_key(folder.to_string_lossy().as_ref())
            })
            .collect()
    }

    /// The tick's focus poll: keeps `window_focused` current for the
    /// notification gate (desktop toasts only while the app is unfocused).
    /// Any open Warp Station window counts as focused, so moving between the
    /// main window and Notes does not look like a focus loss.
    pub fn check_window_focus(&mut self, cx: &mut App) {
        self.window_focused = Some(Self::any_window_focused(cx));
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
        self.persist_selection_soon(Some(folder.to_string()), cx);
        self.reset_repo_workspace();
        // a draft that finished while this repo was off screen belongs to it
        self.take_pending_wand_draft(folder);
        self.load_repo_workspace(folder);
        // the just-opened repo's GitHub/Azure snapshots re-ask once stale —
        // the rail's counts for it may date back an activity cadence
        self.kick_open_activity(folder);
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
        if settings.repos.enable_github
            && let Some(gh) = warp_station_core::exe::locate(&settings.repos.github_executable)
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
        if !settings.repos.enable_github {
            return None;
        }
        warp_station_core::exe::locate(&settings.repos.github_executable)
            .map(|gh| gh.to_string_lossy().to_string())
    }

    /// Asks once for the selected repo's closed list. The tab calls this as
    /// it draws; the two guard sets make that a no-op after the first ask and
    /// after a failure, so drawing cannot re-request (audit A2).
    pub fn load_closed(&mut self, pull_requests: bool) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let key = (folder.clone(), pull_requests);
        // an in-flight/cached ask, or a failure waiting for the user's Retry:
        // either way this call is a no-op, so the tab may ask as it draws
        if self.closed_requested.contains(&key) || self.closed_failed.contains(&key) {
            return;
        }
        // no gh: the same failure surface the tab shows for a failed request
        let Some(gh_exe) = self.gh_exe() else {
            self.closed_failed.insert(key);
            return;
        };
        self.closed_failed.remove(&key);
        self.closed_requested.insert(key);
        let _ = self.net.send_blocking(NetCommand::GitHubClosed {
            folder: std::path::PathBuf::from(folder),
            gh_exe,
            pull_requests,
        });
    }

    /// The closed tab's Retry: forget both the ask guard and the failure, then
    /// ask again for the selected repo.
    pub fn retry_closed(&mut self, pull_requests: bool) {
        let Some(folder) = self.selected_repo.clone() else {
            return;
        };
        let key = (folder, pull_requests);
        self.closed_requested.remove(&key);
        self.closed_failed.remove(&key);
        self.load_closed(pull_requests);
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
        let azure = settings.repos.enable_azure_devops
            && self
                .selected_repo
                .as_ref()
                .and_then(|folder| self.azure_by_folder.get(folder))
                .is_some_and(|snapshot| snapshot.available);
        WorkspaceTab::ALL
            .into_iter()
            .filter(|tab| match tab {
                WorkspaceTab::PullRequests | WorkspaceTab::Issues => settings.repos.enable_github,
                WorkspaceTab::Pipelines => azure,
                WorkspaceTab::Overview | WorkspaceTab::Changes | WorkspaceTab::History => true,
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

    /// Gives a repository its own tab, or goes to the one it has. A row's
    /// double click, its middle button and its "Open in tab" verb land here;
    /// a single press stays a preview in the Repositories tab.
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
            let word = crate::net::git_working_word(label).trim_end_matches('…');
            return Some(format!("{word} {}", self.repo_name(folder)));
        }
        if self.summary_busy {
            return Some("Summarizing".to_string());
        }
        if let Some(folder) = self.wand_repo.as_deref() {
            // name the repo: the run outlives the selection, so its chip is
            // the one place that still says whose generation is live
            return Some(format!("Generating {}", self.repo_name(folder)));
        }
        if self.wand_busy.is_some() {
            return Some("Generating".to_string());
        }
        if self.scanning {
            return Some("Refreshing".to_string());
        }
        // the user-visible fetch pass streams k/n after the scan settles
        self.fetch_progress
            .map(|(done, total)| format!("Fetching {done}/{total}"))
    }

    /// A repo's display name, falling back to its folder when the list no
    /// longer carries it. The title-bar queue panel uses it too.
    pub fn repo_name(&self, folder: &str) -> String {
        self.repos
            .iter()
            .find(|repo| repo.folder_path.to_string_lossy() == folder)
            .map_or_else(|| folder.to_string(), |repo| repo.name.clone())
    }

    /// The repository page's Refresh button state: "Refreshing" while the
    /// scan runs, then "Fetching k/n" while the pass drains (per repo, as
    /// each fetch lands), then the resting label. The counter never sleeps
    /// on a bulk timer pass — those are silent.
    pub fn refresh_label(&self) -> String {
        if self.scanning {
            "Refreshing".to_string()
        } else if let Some((done, total)) = self.fetch_progress {
            format!("Fetching {done}/{total}")
        } else {
            "Refresh".to_string()
        }
    }

    /// Takes a pending "bring the window forward" request from the
    /// single-instance door; the window activates itself and clears it.
    pub fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    /// Drops the selection. The page's next pass picks a row again, so this is
    /// how the selected repo leaves the rail.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected_repo = None;
        // the page's next pass picks a row again; count it as a list change
        self.list_version += 1;
        self.persist_selection_soon(None, cx);
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
    }

    /// Folder text → index, so one status/GitHub pass does not scan the whole
    /// list per entry (n² `to_string_lossy` allocations).
    fn repo_index_by_folder(&self) -> std::collections::HashMap<String, usize> {
        self.repos
            .iter()
            .enumerate()
            .map(|(ix, repo)| (repo.folder_path.to_string_lossy().to_string(), ix))
            .collect()
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
        self.list_version += 1;
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
        self.closed_failed.retain(|(key, _)| key != &folder);
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
        // a draft kept for the removed repo can never be selected again
        self.wand_drafts.remove(&folder);
        self.wand_queue.retain(|queued| queued.folder != folder);
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
        self.persist_cache_soon(cx);
    }

    /// Persists the selected row after a short idle: holding an arrow key
    /// must not rewrite the whole settings file once per row. Only the newest
    /// timer's value lands.
    fn persist_selection_soon(&mut self, folder: Option<String>, cx: &mut Context<Self>) {
        self.selection_generation += 1;
        let generation = self.selection_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let _ = this.update(cx, |store: &mut Store, _| {
                if store.selection_generation == generation {
                    store.update_settings(|s| s.repos.selected_repo = folder.clone());
                }
            });
        })
        .detach();
    }

    /// The next wand/summary id. Deliberately does not touch
    /// [`Self::cache_generation`]: allocating a run id used to invalidate the
    /// debounced tag/star cache write (audit A4).
    fn next_run_id(&mut self, prefix: &str) -> String {
        self.run_id_seq += 1;
        format!("{prefix}-{}", self.run_id_seq)
    }

    /// Write-behind cache save: a burst of star/tag/archive clicks or a
    /// stream of per-repo probe/GitHub settles collapses into one full-file
    /// write after a short debounce.
    fn persist_cache_soon(&mut self, cx: &mut Context<Self>) {
        self.cache_generation += 1;
        let generation = self.cache_generation;
        let repos = self.repos.clone();
        let settings = self.settings.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |store: &mut Store, _| {
                if store.cache_generation == generation {
                    persist_repo_state(&settings, &repos);
                    store.cache_generation = 0;
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Immediate cache write for an event that replaces the list (a scan
    /// settle, a registration): the list is new, and bumping the generation
    /// invalidates a pending debounced write so it cannot replay an older
    /// list over this one.
    fn save_cache_now(&mut self) {
        self.cache_generation += 1;
        persist_repo_state(&self.settings, &self.repos);
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
        self.persist_cache_soon(cx);
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
    pub const ALL: [WorkspaceTab; 6] = [
        WorkspaceTab::Overview,
        WorkspaceTab::Changes,
        WorkspaceTab::History,
        WorkspaceTab::PullRequests,
        WorkspaceTab::Issues,
        WorkspaceTab::Pipelines,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WorkspaceTab::Overview => "Overview",
            WorkspaceTab::Changes => "Changes",
            WorkspaceTab::History => "History",
            WorkspaceTab::PullRequests => "Pull requests",
            WorkspaceTab::Issues => "Issues",
            WorkspaceTab::Pipelines => "Pipelines",
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

/// Persist the repo list's user tags and regenerable status as one step:
/// settings owns the tags, the cache owns the status.
fn persist_repo_state(settings: &SettingsService, repos: &[RepoEntry]) {
    let tags: std::collections::BTreeMap<String, Vec<String>> = repos
        .iter()
        .filter(|repo| !repo.tags.is_empty())
        .map(|repo| {
            (
                repo.folder_path.to_string_lossy().to_string(),
                user_tags(&repo.tags),
            )
        })
        .filter(|(_, tags)| !tags.is_empty())
        .collect();
    if settings.get().repos.repo_tags != tags
        && let Err(err) = settings.update(|settings| settings.repos.repo_tags = tags)
    {
        tracing::warn!("could not save repo tags: {err}");
    }
    if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), repos) {
        tracing::warn!("could not save repo cache: {err}");
    }
}

fn user_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|tag| !tag.eq_ignore_ascii_case(warp_station_core::scan::PLATFORM_TAG))
        .cloned()
        .collect()
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

    #[test]
    fn the_scan_platform_tag_never_reaches_settings() {
        assert_eq!(
            user_tags(&["platform".into(), "favorites".into(), "work".into()]),
            vec!["favorites".to_string(), "work".to_string()]
        );
    }

    /// Allocating a run id must not move the cache-save generation: both
    /// shared one counter until the audit, so starting a wand within the
    /// 400 ms tag-save window silently dropped the favorite/tag write.
    #[test]
    fn run_ids_do_not_touch_the_cache_save_gate() {
        let (tx, _rx) = async_channel::unbounded();
        let scratch =
            std::env::temp_dir().join(format!("warp-station-run-id-{}.json", std::process::id()));
        std::fs::write(&scratch, "{}").expect("scratch settings");
        let mut store = Store::with_settings(tx, Arc::new(SettingsService::at(scratch.clone())));
        let before = store.cache_generation;
        assert_eq!(store.next_run_id("wand"), "wand-1");
        assert_eq!(store.next_run_id("summary"), "summary-2");
        assert_eq!(
            store.cache_generation, before,
            "a run id must not invalidate a pending cache save"
        );
        let _ = std::fs::remove_file(&scratch);
    }

    /// The single-instance door's focus request is a one-shot latch: the
    /// window takes it once and a later notification does not re-activate.
    #[test]
    fn a_focus_request_is_taken_once() {
        let (tx, _rx) = async_channel::unbounded();
        let scratch =
            std::env::temp_dir().join(format!("warp-station-focus-{}.json", std::process::id()));
        std::fs::write(&scratch, "{}").expect("scratch settings");
        let mut store = Store::with_settings(tx, Arc::new(SettingsService::at(scratch.clone())));
        assert!(!store.take_focus_request());
        store.focus_requested = true;
        assert!(store.take_focus_request());
        assert!(!store.take_focus_request(), "the request is one-shot");
        let _ = std::fs::remove_file(&scratch);
    }

    /// The fetch summary's wake-up math over the store's real shape: queue
    /// (a baseline read) → probe apply → text. Only repos this pass pushed
    /// further behind are named — a remote that did not move, or one that
    /// stayed behind by the count the pass already knew, stays silent (a
    /// quiet remote must not ring the desktop every interval).
    #[test]
    fn the_fetch_summary_names_only_what_got_further_behind() {
        let (tx, _rx) = async_channel::unbounded();
        let scratch = std::env::temp_dir().join(format!(
            "warp-station-fetch-summary-{}.json",
            std::process::id()
        ));
        std::fs::write(&scratch, "{}").expect("scratch settings");
        let mut store = Store::with_settings(tx, Arc::new(SettingsService::at(scratch.clone())));

        let entry = |name: &str, folder: &str, behind: u32| repo_store::RepoEntry {
            name: name.into(),
            folder_path: std::path::PathBuf::from(folder),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: behind,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: true,
            git_branch_published: true,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
        };
        store.repos = vec![
            entry("alpha", "/probe/alpha", 0),
            entry("bend", "/probe/bend", 1),
        ];
        let by_folder = std::collections::HashMap::from([
            ("/probe/alpha".to_string(), 0usize),
            ("/probe/bend".to_string(), 1usize),
        ]);
        let probe = |folder: &str, behind: u32| repo_store::StatusProbe {
            folder_path: folder.into(),
            behind_count: behind,
            has_remote: true,
            ..Default::default()
        };
        let apply = |store: &mut Store, probes: &[repo_store::StatusProbe]| {
            for probe in probes {
                if let Some(&ix) = by_folder.get(&probe.folder_path) {
                    repo_store::apply_status_probe(&mut store.repos[ix], probe);
                }
            }
        };
        // the queue-time baseline the pass sets out from (the queue captures
        // it itself; the test reads the same pre-pass counts to hand over)
        let baseline_for = |store: &Store, folders: &[&str]| {
            folders
                .iter()
                .filter_map(|folder| {
                    store.repos.iter().find_map(|entry| {
                        (entry.folder_path == std::path::PathBuf::from(folder))
                            .then(|| (folder.to_string(), entry.git_to_pull_count))
                    })
                })
                .collect::<std::collections::HashMap<String, u32>>()
        };

        // pass one: alpha fell 2 behind, bend's count sits where it was
        let probes = vec![probe("/probe/alpha", 2), probe("/probe/bend", 1)];
        let previous = baseline_for(&store, &["/probe/alpha", "/probe/bend"]);
        apply(&mut store, &probes);
        assert_eq!(
            store.fetch_summary_text(&previous),
            Some("alpha is 2 commits behind".into()),
            "one repo moved: the summary names it alone"
        );

        // pass two: nothing moved further; the same counts must stay silent
        // even though bend is still 1 behind the remote
        let before = baseline_for(&store, &["/probe/alpha", "/probe/bend"]);
        assert_eq!(
            store.fetch_summary_text(&before),
            None,
            "a quiet pass repeats nothing, whatever the standing behind counts"
        );

        // pass three: bend also catches new commits — both names, in list order
        let probes = vec![probe("/probe/alpha", 4), probe("/probe/bend", 3)];
        let previous = baseline_for(&store, &["/probe/alpha", "/probe/bend"]);
        apply(&mut store, &probes);
        assert_eq!(
            store.fetch_summary_text(&previous),
            Some("2 repos behind: alpha (4), bend (3)".into())
        );

        // archived repos never make the summary, even when the pass queued
        // before they were archived: the ghost folder stays in the baseline,
        // and the pass's probe finds it two behind against a one-behind queue
        let mut archived = entry("ghost", "/probe/ghost", 1);
        archived.tags = vec![repo_store::ARCHIVED_TAG.into()];
        store.repos.push(archived);
        let previous: std::collections::HashMap<String, u32> = std::collections::HashMap::from([
            ("/probe/alpha".to_string(), 4u32),
            ("/probe/bend".to_string(), 3u32),
            ("/probe/ghost".to_string(), 1u32),
        ]);
        let probe = probe("/probe/ghost", 2);
        let ghost_ix = store
            .repos
            .iter()
            .position(|entry| entry.name == "ghost")
            .expect("the ghost entry is in the list for this scenario");
        repo_store::apply_status_probe(&mut store.repos[ghost_ix], &probe);
        // the archived entry read back: named in the list, behind 2 now
        assert_eq!(store.repos[ghost_ix].git_to_pull_count, 2);
        assert_eq!(
            store.fetch_summary_text(&previous),
            None,
            "an archived repo alone never rings the desktop, whatever it fell behind by"
        );
        let _ = std::fs::remove_file(&scratch);
    }

    /// Unfocused alone is not the desktop's rule: the settings' master switch
    /// and per-category toggles decide. This store-side seam only mirrors —
    /// the gate math itself lives in core, already tested there.
    #[test]
    fn the_desktop_mirror_reads_the_store_settings() {
        let (tx, _rx) = async_channel::unbounded();
        let scratch = std::env::temp_dir().join(format!(
            "warp-station-notify-settings-{}.json",
            std::process::id()
        ));
        std::fs::write(&scratch, "{}").expect("scratch settings");
        let store = Store::with_settings(tx, Arc::new(SettingsService::at(scratch.clone())));
        // defaults: on and unfocused-only
        assert!(store.settings().notifications.enabled);

        store.update_settings(|s| {
            s.notifications.enabled = false;
        });
        assert!(!store.settings().notifications.enabled);
        let _ = std::fs::remove_file(&scratch);
    }
}
