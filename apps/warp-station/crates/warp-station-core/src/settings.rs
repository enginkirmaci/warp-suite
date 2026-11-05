//! The settings store: schema byte-compatible with the settings.json already
//! on disk (PascalCase keys, same defaults, enum-as-number sort mode) plus
//! the single-owner service with the legacy-key migration and the atomic
//! write.

use crate::jsonio;
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct AppSettings {
    #[serde(default)]
    pub nuget_local: NugetLocalSettings,
    #[serde(default)]
    pub repos: ReposSettings,
    #[serde(default)]
    pub clipboard_password: ClipboardPasswordSettings,
    #[serde(default)]
    pub snap_it: SnapItSettings,
    #[serde(default)]
    pub keep_awake: KeepAwakeSettings,
    #[serde(default, rename = "OpenCode")]
    pub opencode: OpenCodeSettings,
    #[serde(default, rename = "DeepSeek")]
    pub deepseek: DeepSeekSettings,
    #[serde(default, rename = "WarpAgent")]
    pub warp_agent: WarpAgentSettings,
    #[serde(default, rename = "WarpNotes")]
    pub warp_notes: WarpNotesSettings,
    #[serde(default)]
    pub general: GeneralSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct NugetLocalSettings {
    pub enable_nuget: bool,
    pub watch_folder: Option<String>,
    pub nuget_package_filter: String,
    pub file_copy_delay_ms: u32,
    pub count_reset_interval_seconds: u32,
}

impl Default for NugetLocalSettings {
    fn default() -> Self {
        Self {
            enable_nuget: true,
            watch_folder: None,
            nuget_package_filter: "*.nupkg".into(),
            file_copy_delay_ms: 2000,
            count_reset_interval_seconds: 60,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ReposSettings {
    pub repo_scan_folders: Option<Vec<String>>,
    pub git_folder_pattern: String,
    pub solution_file_pattern: String,
    pub platform_folder_name: String,
    #[serde(rename = "VSCodeExecutable")]
    pub vscode_executable: String,
    #[serde(rename = "VSCodeProfile")]
    pub vscode_profile: String,
    pub terminal_executable: String,
    pub ide_executable: String,
    pub opencode_executable: String,
    pub deepseek_executable: String,
    pub enable_terminal: bool,
    #[serde(rename = "EnableVSCode")]
    pub enable_vscode: bool,
    pub enable_visual_studio: bool,
    pub enable_git_hub: bool,
    pub git_hub_executable: String,
    pub enable_azure_dev_ops: bool,
    pub azure_dev_ops_pat: String,
    pub azure_dev_ops_url: String,
    pub excluded_folders: Option<Vec<String>>,
    pub sort_mode: RepoSortMode,
    /// None = the mode's own default (Name ascending, the rest best-first);
    /// legacy settings files predate the field, so absence stays meaningful.
    pub sort_ascending: Option<bool>,
    /// Background `git fetch --prune` interval; None = off. Absence-meaningful
    /// like sort_ascending.
    pub auto_fetch_minutes: Option<u16>,
    /// The periodic fetch runs only while some Warp Station window is focused;
    /// with auto_fetch_minutes off, the gate instead arms a single fetch at
    /// launch. None/false = fetch regardless. Absence-meaningful like
    /// auto_fetch_minutes.
    pub auto_fetch_only_focused: Option<bool>,
    /// Background list-refresh interval: the repository page's Refresh button
    /// (fetch + rescan + status probe) on a timer. None = off. Absence-
    /// meaningful like auto_fetch_minutes.
    pub auto_refresh_minutes: Option<u16>,
    /// Regaining focus after the app was unfocused runs one fetch + refresh
    /// and restarts both periodic timers. None/false = off.
    /// Absence-meaningful like auto_fetch_minutes.
    pub sync_on_focus: Option<bool>,
    /// Collapsible folder groups in the repos table; None/false = the flat
    /// list. Absence-meaningful like sort_ascending.
    pub group_repos_by_root: Option<bool>,
    /// Folder levels the group headers count up from the scan roots' shared
    /// folder (1 = that folder itself, 2 = its parent); None = 1.
    /// Absence-meaningful like sort_ascending.
    pub group_root_depth: Option<u8>,
    /// Collapsed group keys (folder paths, or "other"); None = all
    /// expanded. Absence-meaningful like sort_ascending.
    pub collapsed_groups: Option<Vec<String>>,
    /// Cross-repo ripgrep search in the titlebar popup; None = enabled —
    /// the absence-tolerant opt-out, like auto_fetch_minutes.
    pub enable_code_search: Option<bool>,
    /// The ripgrep binary the code search runs through; None = "rg".
    pub code_search_executable: Option<String>,
    pub max_scan_depth: i32,
    pub branch_name_prefix: String,
    /// The Issues tab's sort: "updated", "newest", "oldest" or "comments"; None
    /// = updated.
    pub issue_sort: Option<String>,
    /// The repository tabs, left to right: folder paths. None = no tabs.
    pub repo_tabs: Option<Vec<String>>,
    /// The list pane's width in layout pixels; None = the shell's default.
    pub list_width: Option<f32>,
    /// The toolbar's Archive toggle; None = off. Absence-meaningful like
    /// sort_ascending.
    pub show_archived: Option<bool>,
    /// The repo row the list last showed; None = the first row stands in.
    pub selected_repo: Option<String>,
    /// The strip's on-screen tab (a repo folder); None = the Repositories tab.
    pub open_tab: Option<String>,
    /// Each repo tab's resting detail tab, matched to the strip by folder;
    /// None = every tab opens on Overview.
    pub tab_views: Option<Vec<RepoTabView>>,
    /// What the Repositories tab was showing when a repo tab took over;
    /// None = the defaults.
    pub home_view: Option<RepoTabView>,
}

/// One user-arranged view that survives a restart: a repo folder and the
/// detail tab it was left on. `tab` carries the WorkspaceTab label so the
/// file stays readable.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RepoTabView {
    pub folder: String,
    pub tab: String,
}

impl Default for ReposSettings {
    fn default() -> Self {
        Self {
            repo_scan_folders: None,
            git_folder_pattern: "*.git".into(),
            solution_file_pattern: "*.sln,*.slnx".into(),
            platform_folder_name: "platform".into(),
            vscode_executable: "code".into(),
            vscode_profile: String::new(),
            terminal_executable: "wt".into(),
            ide_executable: String::new(),
            opencode_executable: "opencode".into(),
            deepseek_executable: "dsh".into(),
            enable_terminal: true,
            enable_vscode: true,
            enable_visual_studio: true,
            enable_git_hub: true,
            git_hub_executable: "gh".into(),
            enable_azure_dev_ops: true,
            azure_dev_ops_pat: String::new(),
            azure_dev_ops_url: String::new(),
            excluded_folders: None,
            sort_mode: RepoSortMode::Name,
            sort_ascending: None,
            auto_fetch_minutes: None,
            auto_fetch_only_focused: None,
            auto_refresh_minutes: None,
            sync_on_focus: None,
            group_repos_by_root: None,
            group_root_depth: None,
            collapsed_groups: None,
            enable_code_search: None,
            code_search_executable: None,
            max_scan_depth: 3,
            branch_name_prefix: "users/enginkirmaci".into(),
            issue_sort: None,
            repo_tabs: None,
            list_width: None,
            show_archived: None,
            selected_repo: None,
            open_tab: None,
            tab_views: None,
            home_view: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RepoSortMode {
    /// Alphabetical by repo name (the historical default).
    #[default]
    Name = 0,
    /// Most recent commit first; repos without a known commit date last.
    LastActivity = 1,
    /// Most pending work first: changes, then to-push, then to-pull counts.
    Changes = 2,
    /// Most open GitHub pull requests + issues first (the GitHub column's
    /// combined read).
    GitHub = 3,
    /// Alphabetical by branch name; repos without a known branch last.
    Branch = 5,
    /// Repos with something waiting on the developer first, newest commit
    /// first on either side of that line.
    Attention = 6,
}

impl RepoSortMode {
    /// The order a column starts in: Name and Branch read naturally
    /// ascending, the other modes lead with their "best" row (newest / most
    /// changes / most remote activity), which is a descending read.
    pub fn default_ascending(self) -> bool {
        matches!(self, RepoSortMode::Name | RepoSortMode::Branch)
    }
}

// the settings file stores the enum as its number; the retired Issues slot
// (4) falls back to Name
impl Serialize for RepoSortMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for RepoSortMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match u8::deserialize(deserializer)? {
            1 => RepoSortMode::LastActivity,
            2 => RepoSortMode::Changes,
            3 => RepoSortMode::GitHub,
            5 => RepoSortMode::Branch,
            6 => RepoSortMode::Attention,
            _ => RepoSortMode::Name,
        })
    }
}

/// The harness a fresh settings file selects. warp-agent reports the real
/// list over its door; this keeps the model choice usable before the first
/// reply.
pub const DEFAULT_HARNESS: &str = "opencode";

fn default_harness() -> String {
    DEFAULT_HARNESS.into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct OpenCodeSettings {
    #[serde(rename = "EnableOpenCode")]
    pub enable_opencode: bool,
    /// The one model every AI run uses (the wands, commit messages and the
    /// daily summary). The JSON key stays `DefaultModel` so existing files
    /// load unchanged; the UI calls it "Model".
    #[serde(rename = "DefaultModel")]
    pub model: String,
}

/// The DeepSeek CLI quick-launch: only arms the repo row's launch button —
/// the runs themselves happen in warp-agent, never in the dsh binary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct DeepSeekSettings {
    #[serde(rename = "EnableDeepSeek")]
    pub enable_deepseek: bool,
}

/// The warp-agent integration: Warp Station' AI surface runs in warp-agent over
/// its door, so these toggle the whole AI feature set and name its binary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct WarpAgentSettings {
    #[serde(rename = "EnableWarpAgent")]
    pub enable_warp_agent: bool,
    pub executable: String,
    /// The warp-agent harness id ("opencode", "dsh", …). The door owns the
    /// list; an unknown id falls back to the default.
    #[serde(default = "default_harness")]
    pub harness: String,
    /// The variant for the selected model; None = the harness's default.
    pub variant: Option<String>,
}

impl WarpAgentSettings {
    /// The selected harness, never empty: the door treats an absent harness
    /// as "use warp-agent's default", but settings must always show a pick.
    pub fn selected_harness(&self) -> &str {
        let harness = self.harness.trim();
        if harness.is_empty() {
            DEFAULT_HARNESS
        } else {
            harness
        }
    }
}

impl Default for WarpAgentSettings {
    fn default() -> Self {
        Self {
            enable_warp_agent: true,
            executable: "warp-agent".into(),
            harness: default_harness(),
            variant: None,
        }
    }
}

/// The warp-notes integration: the Notes surfaces launch the external app
/// over the shared ~/notes store, so these toggle the feature set and
/// name its binary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct WarpNotesSettings {
    #[serde(rename = "EnableWarpNotes")]
    pub enable_warp_notes: bool,
    pub executable: String,
}

impl Default for WarpNotesSettings {
    fn default() -> Self {
        Self {
            enable_warp_notes: true,
            executable: "warp-notes".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct ClipboardPasswordSettings {
    pub encrypted_password: Option<String>,
    pub enable_clipboard_password: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct SnapItSettings {
    /// Spawns the SnapIt companion at app launch.
    pub auto_start: bool,
    /// The companion binary to launch; empty = the packaged Tools.SnapIt.exe
    /// next to the app. A bare name resolves through PATH and the user
    /// install dirs, like the warp-agent executable.
    pub executable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
#[derive(Default)]
pub struct KeepAwakeSettings {
    /// Restores the keep-awake execution state at app launch.
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct GeneralSettings {
    pub start_minimized: bool,
    pub start_at_boot: bool,
    pub notes_store_path: String,
    pub show_tooltips: bool,
    /// "compact" | "normal" | "large" text-size variant; None = normal.
    pub font_scale: Option<String>,
    /// Turns the family's motion off. The busy words stay.
    pub reduce_motion: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            start_minimized: false,
            start_at_boot: false,
            notes_store_path: String::new(),
            show_tooltips: true,
            font_scale: None,
            reduce_motion: false,
        }
    }
}

/// Single owner of the settings graph: reads hand out full clones, writes are
/// serialized and persisted atomically. An unreadable file falls back to
/// defaults instead of failing startup.
pub struct SettingsService {
    path: PathBuf,
    state: Mutex<AppSettings>,
}

impl SettingsService {
    pub fn open() -> Self {
        let service = Self::at(AppPaths::settings_file());
        // one-time: the notes store moved out of `~/.warp/notes` to `~/notes`,
        // so an explicitly saved old default would now point at the notes
        // app's own settings folder
        let legacy = AppPaths::legacy_notes_dir().to_string_lossy().to_string();
        let mut current = service.get();
        if clear_legacy_notes_path(&mut current, &legacy) {
            let _ = service.update(|settings| {
                clear_legacy_notes_path(settings, &legacy);
            });
        }
        service
    }

    pub fn at(path: PathBuf) -> Self {
        let settings = match std::fs::read_to_string(&path) {
            Ok(json) => Self::parse(&json).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        };
        Self {
            path,
            state: Mutex::new(settings),
        }
    }

    fn parse(json: &str) -> anyhow::Result<AppSettings> {
        let migrated = migrate_legacy_enable_keys(json);
        Ok(serde_json::from_str(&migrated)?)
    }

    pub fn get(&self) -> AppSettings {
        self.state.lock().unwrap().clone()
    }

    /// Apply `mutate` to a clone of the current settings and persist the whole
    /// graph as one transition, so concurrent section updates stack instead of
    /// racing.
    pub fn update(&self, mutate: impl FnOnce(&mut AppSettings)) -> anyhow::Result<AppSettings> {
        let (updated, path) = {
            let mut guard = self.state.lock().unwrap();
            let mut next = guard.clone();
            mutate(&mut next);
            *guard = next.clone();
            (next, self.path.clone())
        };
        jsonio::write_json_atomically(&path, &updated)?;
        Ok(updated)
    }
}

/// The notes store moved out of `~/.warp/notes`; a settings file that still
/// names the old default would point at the notes app's settings folder.
/// Clears it back to "use the default" and reports whether it changed.
fn clear_legacy_notes_path(settings: &mut AppSettings, legacy: &str) -> bool {
    if settings.general.notes_store_path.trim() == legacy {
        settings.general.notes_store_path.clear();
        true
    } else {
        false
    }
}

/// Maps the legacy per-tool toggle keys to the uniform Enable<Tool> scheme,
/// each only when the new key is absent: Repos.ShowGitHubColumn →
/// Repos.EnableGitHub, Repos.ShowAzureDevOpsColumn → Repos.EnableAzureDevOps,
/// OpenCode.Enabled → OpenCode.EnableOpenCode, and the INVERTED
/// ClipboardPassword.HideFromGui → ClipboardPassword.EnableClipboardPassword.
fn migrate_legacy_enable_keys(json: &str) -> String {
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(json) else {
        return json.to_string();
    };
    let mut changed = false;
    let mut copy_bool = |section: &str, legacy: &str, new: &str, inverted: bool| {
        let Some(obj) = root.get_mut(section).and_then(|v| v.as_object_mut()) else {
            return;
        };
        let Some(legacy_value) = obj.get(legacy).and_then(|v| v.as_bool()) else {
            return;
        };
        if obj.contains_key(new) {
            return;
        }
        obj.insert(
            new.to_string(),
            serde_json::Value::Bool(if inverted {
                !legacy_value
            } else {
                legacy_value
            }),
        );
        changed = true;
    };
    copy_bool("Repos", "ShowGitHubColumn", "EnableGitHub", false);
    copy_bool("Repos", "ShowAzureDevOpsColumn", "EnableAzureDevOps", false);
    copy_bool("OpenCode", "Enabled", "EnableOpenCode", false);
    copy_bool(
        "ClipboardPassword",
        "HideFromGui",
        "EnableClipboardPassword",
        true,
    );
    if changed {
        root.to_string()
    } else {
        json.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_legacy_file_shape() {
        let json = r#"{
            "Repos": {"RepoScanFolders": ["/a", "/b"], "SortMode": 2, "MaxScanDepth": 5,
                      "EnableGitHub": true, "AzureDevOpsPat": "pat"},
            "General": {"ShowTooltips": false},
            "OpenCode": {"EnableOpenCode": true, "DefaultModel": "x", "CommitModel": "legacy"}
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(
            s.repos.repo_scan_folders.as_deref(),
            Some(&["/a".to_string(), "/b".to_string()][..])
        );
        assert_eq!(s.repos.sort_mode, RepoSortMode::Changes);
        assert_eq!(s.repos.max_scan_depth, 5);
        assert!(!s.general.show_tooltips);
        assert!(s.opencode.enable_opencode);
        assert_eq!(s.opencode.model, "x");
        assert_eq!(s.warp_agent.selected_harness(), DEFAULT_HARNESS);
        assert_eq!(s.warp_agent.variant, None);
        // defaults fill absent sections
        assert!(s.nuget_local.enable_nuget);
        assert!(!s.keep_awake.enabled);
        assert_eq!(s.repos.branch_name_prefix, "users/enginkirmaci");
    }

    #[test]
    fn legacy_enable_keys_migrate_once() {
        let json =
            r#"{"OpenCode": {"Enabled": true}, "ClipboardPassword": {"HideFromGui": false}}"#;
        let s: AppSettings = serde_json::from_str(&migrate_legacy_enable_keys(json)).unwrap();
        assert!(s.opencode.enable_opencode);
        // inverted legacy key: HideFromGui=false → enabled
        assert!(s.clipboard_password.enable_clipboard_password);
        // an explicit new key wins over the legacy one
        let json2 = r#"{"OpenCode": {"Enabled": true, "EnableOpenCode": false}}"#;
        let s2: AppSettings = serde_json::from_str(&migrate_legacy_enable_keys(json2)).unwrap();
        assert!(!s2.opencode.enable_opencode);
    }

    #[test]
    fn sort_modes_survive_the_settings_file() {
        for mode in [
            RepoSortMode::Name,
            RepoSortMode::LastActivity,
            RepoSortMode::Changes,
            RepoSortMode::GitHub,
            RepoSortMode::Branch,
            RepoSortMode::Attention,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(serde_json::from_str::<RepoSortMode>(&json).unwrap(), mode);
        }
    }

    #[test]
    fn round_trips_pascal_case() {
        let s = AppSettings::default();
        let json = serde_json::to_string_pretty(&s).unwrap();
        assert!(json.contains("\"RepoScanFolders\""));
        assert!(json.contains("\"MaxScanDepth\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.sort_mode, s.repos.sort_mode);
    }

    #[test]
    fn auto_fetch_minutes_round_trips_and_absent_means_off() {
        // absence (legacy files) = off
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.auto_fetch_minutes, None);
        assert_eq!(legacy.repos.auto_fetch_only_focused, None);
        // an explicit interval persists and reads back
        let on: AppSettings =
            serde_json::from_str(r#"{"Repos": {"AutoFetchMinutes": 30}}"#).unwrap();
        assert_eq!(on.repos.auto_fetch_minutes, Some(30));
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"AutoFetchMinutes\": 30"));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.auto_fetch_minutes, Some(30));
    }

    #[test]
    fn auto_refresh_minutes_round_trips_and_absent_means_off() {
        // absence (legacy files) = off
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.auto_refresh_minutes, None);
        // an explicit interval persists and reads back
        let on: AppSettings =
            serde_json::from_str(r#"{"Repos": {"AutoRefreshMinutes": 15}}"#).unwrap();
        assert_eq!(on.repos.auto_refresh_minutes, Some(15));
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"AutoRefreshMinutes\": 15"));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.auto_refresh_minutes, Some(15));
    }

    #[test]
    fn sync_on_focus_round_trips_and_absent_means_off() {
        // absence (legacy files) = off
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.sync_on_focus, None);
        // the opt-in persists and reads back
        let on: AppSettings = serde_json::from_str(r#"{"Repos": {"SyncOnFocus": true}}"#).unwrap();
        assert_eq!(on.repos.sync_on_focus, Some(true));
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"SyncOnFocus\": true"));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.sync_on_focus, Some(true));
    }

    #[test]
    fn auto_fetch_focus_gate_round_trips_and_absent_means_fetch_always() {
        // absence (legacy files) = fetch regardless of focus
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.auto_fetch_only_focused, None);
        let gated: AppSettings =
            serde_json::from_str(r#"{"Repos": {"AutoFetchOnlyFocused": true}}"#).unwrap();
        assert_eq!(gated.repos.auto_fetch_only_focused, Some(true));
        let json = serde_json::to_string_pretty(&gated).unwrap();
        assert!(json.contains("\"AutoFetchOnlyFocused\": true"));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.auto_fetch_only_focused, Some(true));
    }

    #[test]
    fn group_repos_keys_round_trip_and_absent_means_flat() {
        // absence (legacy files) = flat list, nothing collapsed
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.group_repos_by_root, None);
        assert_eq!(legacy.repos.group_root_depth, None);
        assert_eq!(legacy.repos.collapsed_groups, None);
        // an explicit grouping toggle and collapsed keys persist and read back
        let on: AppSettings = serde_json::from_str(
            r#"{"Repos": {"GroupReposByRoot": true, "GroupRootDepth": 2, "CollapsedGroups": ["/work/rust", "other"]}}"#,
        )
        .unwrap();
        assert_eq!(on.repos.group_repos_by_root, Some(true));
        assert_eq!(on.repos.group_root_depth, Some(2));
        assert_eq!(
            on.repos.collapsed_groups,
            Some(vec!["/work/rust".to_string(), "other".to_string()])
        );
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"GroupReposByRoot\": true"));
        assert!(json.contains("\"GroupRootDepth\": 2"));
        assert!(json.contains("\"CollapsedGroups\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.group_repos_by_root, Some(true));
        assert_eq!(back.repos.group_root_depth, Some(2));
        assert_eq!(back.repos.collapsed_groups, on.repos.collapsed_groups);
    }

    #[test]
    fn code_search_keys_round_trip_and_absent_means_enabled() {
        // absence (legacy files) = the feature is on, through the default rg
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.enable_code_search, None);
        assert_ne!(legacy.repos.enable_code_search, Some(false));
        assert_eq!(legacy.repos.code_search_executable.as_deref(), None);
        assert_eq!(
            legacy
                .repos
                .code_search_executable
                .clone()
                .unwrap_or_else(|| "rg".into()),
            "rg"
        );
        // an explicit opt-out and a custom binary persist and read back
        let off: AppSettings = serde_json::from_str(
            r#"{"Repos": {"EnableCodeSearch": false, "CodeSearchExecutable": "rgz"}}"#,
        )
        .unwrap();
        assert_eq!(off.repos.enable_code_search, Some(false));
        assert_eq!(off.repos.code_search_executable.as_deref(), Some("rgz"));
        let json = serde_json::to_string_pretty(&off).unwrap();
        assert!(json.contains("\"EnableCodeSearch\": false"));
        assert!(json.contains("\"CodeSearchExecutable\": \"rgz\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.enable_code_search, Some(false));
        assert_eq!(back.repos.code_search_executable.as_deref(), Some("rgz"));
    }

    #[test]
    fn snap_it_executable_round_trips_and_absent_means_packaged_companion() {
        // absence (legacy files) = the packaged side-by-side companion
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.snap_it.executable, "");
        let custom: AppSettings =
            serde_json::from_str(r#"{"SnapIt": {"Executable": "snapit"}}"#).unwrap();
        assert_eq!(custom.snap_it.executable, "snapit");
        let json = serde_json::to_string_pretty(&custom).unwrap();
        assert!(json.contains("\"Executable\": \"snapit\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.snap_it.executable, "snapit");
    }

    #[test]
    fn deepseek_keys_round_trip_and_absent_means_off() {
        // absence (legacy files) = the button is off, through the default binary
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.deepseek.enable_deepseek);
        assert_eq!(legacy.repos.deepseek_executable, "dsh");
        let on: AppSettings = serde_json::from_str(
            r#"{"DeepSeek": {"EnableDeepSeek": true}, "Repos": {"DeepseekExecutable": "dshz"}}"#,
        )
        .unwrap();
        assert!(on.deepseek.enable_deepseek);
        assert_eq!(on.repos.deepseek_executable, "dshz");
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"EnableDeepSeek\": true"));
        assert!(json.contains("\"DeepseekExecutable\": \"dshz\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(back.deepseek.enable_deepseek);
        assert_eq!(back.repos.deepseek_executable, "dshz");
    }

    #[test]
    fn warp_agent_defaults_on_with_executable() {
        // absent section = enabled with the default binary name
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(s.warp_agent.enable_warp_agent);
        assert_eq!(s.warp_agent.executable, "warp-agent");
        assert_eq!(s.warp_agent.selected_harness(), DEFAULT_HARNESS);
        assert_eq!(s.warp_agent.variant, None);
        let configured: AppSettings =
            serde_json::from_str(r#"{"WarpAgent": {"Harness": "dsh", "Variant": "max"}}"#).unwrap();
        assert_eq!(configured.warp_agent.selected_harness(), "dsh");
        assert_eq!(configured.warp_agent.variant.as_deref(), Some("max"));
        // an explicit off round-trips
        let off: AppSettings = serde_json::from_str(
            r#"{"WarpAgent": {"EnableWarpAgent": false, "Executable": "wa"}}"#,
        )
        .unwrap();
        assert!(!off.warp_agent.enable_warp_agent);
        assert_eq!(off.warp_agent.executable, "wa");
    }

    #[test]
    fn ui_state_keys_round_trip_and_absent_means_defaults() {
        // legacy files predate every UI-state key: absence reads as the
        // launch defaults (no archive toggle, first row, Repositories tab)
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.show_archived, None);
        assert_eq!(legacy.repos.selected_repo, None);
        assert_eq!(legacy.repos.open_tab, None);
        assert_eq!(legacy.repos.tab_views, None);
        assert_eq!(legacy.repos.home_view, None);

        let arranged: AppSettings = serde_json::from_str(
            r#"{
                "Repos": {
                    "ShowArchived": true,
                    "SelectedRepo": "/work/alpha",
                    "OpenTab": "/work/beta",
                    "TabViews": [{"Folder": "/work/beta", "Tab": "Changes"}],
                    "HomeView": {"Folder": "/work/alpha", "Tab": "History"}
                }
            }"#,
        )
        .unwrap();
        assert_eq!(arranged.repos.show_archived, Some(true));
        assert_eq!(arranged.repos.selected_repo.as_deref(), Some("/work/alpha"));
        assert_eq!(arranged.repos.open_tab.as_deref(), Some("/work/beta"));
        assert_eq!(arranged.repos.tab_views.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(
            arranged.repos.home_view.as_ref().map(|v| v.tab.as_str()),
            Some("History")
        );

        // and the whole graph serializes back under the same PascalCase keys
        let json = serde_json::to_string_pretty(&arranged).unwrap();
        assert!(json.contains("\"ShowArchived\": true"));
        assert!(json.contains("\"SelectedRepo\": \"/work/alpha\""));
        assert!(json.contains("\"OpenTab\""));
        assert!(json.contains("\"TabViews\""));
        assert!(json.contains("\"HomeView\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos, arranged.repos);
    }

    #[test]
    fn a_saved_legacy_notes_path_clears_but_a_custom_one_stays() {
        let legacy = "/home/e/.warp/notes";
        let mut stale = AppSettings::default();
        stale.general.notes_store_path = legacy.into();
        assert!(clear_legacy_notes_path(&mut stale, legacy));
        assert!(stale.general.notes_store_path.is_empty());
        assert!(
            !clear_legacy_notes_path(&mut stale, legacy),
            "cleared once, then a no-op"
        );

        let mut custom = AppSettings::default();
        custom.general.notes_store_path = "/data/notes".into();
        assert!(!clear_legacy_notes_path(&mut custom, legacy));
        assert_eq!(custom.general.notes_store_path, "/data/notes");
    }
}
