//! The settings store: a versioned snake_case `settings.json`, the one-time
//! importer for the pre-versioning PascalCase file, and the single-owner
//! service that persists the whole graph atomically.

use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use warpui_kit_platform::config::{Store, rename_legacy_keys};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AppSettings {
    #[serde(default)]
    pub nuget_local: NugetLocalSettings,
    #[serde(default)]
    pub repos: ReposSettings,
    #[serde(default)]
    pub clipboard_password: ClipboardPasswordSettings,
    #[serde(default)]
    pub snapit: SnapitSettings,
    #[serde(default)]
    pub keep_awake: KeepAwakeSettings,
    #[serde(default)]
    pub opencode: OpencodeSettings,
    #[serde(default)]
    pub deepseek: DeepseekSettings,
    #[serde(default)]
    pub warp_agent: WarpAgentSettings,
    #[serde(default)]
    pub warp_notes: WarpNotesSettings,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub changes: ChangesSettings,
    #[serde(default)]
    pub notifications: NotificationsSettings,
}

/// What a desktop notification may interrupt the desktop with. The section is
/// new (files before it carry no `Notifications` key at all), so absence reads
/// as every default: mirroring toasts while Warp Station is unfocused, with
/// errors, action confirmations and the retired fetch summaries allowed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct NotificationsSettings {
    /// The master switch: off silences the desktop channel entirely (the
    /// in-app toasts are unaffected).
    pub enabled: bool,
    /// Only copy the toasts to the desktop while no Warp Station window has
    /// focus — anything on screen is already visible in its own toast.
    pub only_when_unfocused: bool,
    /// Mirrors error toasts ("Commit failed for alpha: …").
    pub errors: bool,
    /// Mirrors confirmation toasts: merges, commits, agent handoffs,
    /// generation drafts, password copies.
    pub action_confirmations: bool,
    /// Retired with the bulk fetch pass. Kept in the schema so an existing
    /// settings file with the key still parses and a save never drops it;
    /// nothing emits the category any more.
    pub fetch_summaries: bool,
}

impl Default for NotificationsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            only_when_unfocused: true,
            errors: true,
            action_confirmations: true,
            fetch_summaries: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
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
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct ReposSettings {
    pub repo_scan_folders: Option<Vec<String>>,
    /// User tags keyed by repo folder. The cache never carries them, so a
    /// deleted cache cannot lose a favorite or an archived marker.
    #[serde(default)]
    pub repo_tags: BTreeMap<String, Vec<String>>,
    pub git_folder_pattern: String,
    pub solution_file_pattern: String,
    pub platform_folder_name: String,
    pub vscode_executable: String,
    pub vscode_profile: String,
    pub terminal_executable: String,
    pub ide_executable: String,
    pub opencode_executable: String,
    pub deepseek_executable: String,
    pub enable_terminal: bool,
    pub enable_vscode: bool,
    pub enable_visual_studio: bool,
    pub enable_github: bool,
    pub github_executable: String,
    pub enable_azure_devops: bool,
    pub azure_devops_pat: String,
    pub azure_devops_url: String,
    pub excluded_folders: Option<Vec<String>>,
    pub sort_mode: RepoSortMode,
    /// None = the mode's own default (Name ascending, the rest best-first);
    /// legacy settings files predate the field, so absence stays meaningful.
    pub sort_ascending: Option<bool>,
    /// Collapsible folder groups in the repos table; None/false = the flat
    /// list. Absence-meaningful like sort_ascending.
    pub group_repos_by_root: Option<bool>,
    /// Folder levels above each repo the group headers count from (1 = the
    /// folder containing it, 2 = its parent); None = 1.
    /// Absence-meaningful like sort_ascending.
    pub group_root_depth: Option<u8>,
    /// Rescan the list when the app's window regains focus; absent reads as
    /// on, the requested default.
    pub auto_refresh_on_focus: bool,
    /// Minutes between automatic scans while the window is focused; None = 0
    /// = off. Absence-meaningful like sort_ascending.
    pub auto_refresh_minutes: Option<u32>,
    /// Collapsed group keys (folder paths, or "other"); None = all
    /// expanded. Absence-meaningful like sort_ascending.
    pub collapsed_groups: Option<Vec<String>>,
    /// Cross-repo ripgrep search in the titlebar popup; None = enabled —
    /// the absence-tolerant opt-out, like sort_ascending.
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
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RepoTabView {
    pub folder: String,
    pub tab: String,
}

impl Default for ReposSettings {
    fn default() -> Self {
        Self {
            repo_scan_folders: None,
            repo_tags: BTreeMap::new(),
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
            enable_github: true,
            github_executable: "gh".into(),
            enable_azure_devops: true,
            azure_devops_pat: String::new(),
            azure_devops_url: String::new(),
            excluded_folders: None,
            sort_mode: RepoSortMode::Name,
            sort_ascending: None,
            group_repos_by_root: None,
            group_root_depth: None,
            auto_refresh_on_focus: true,
            auto_refresh_minutes: None,
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
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
#[derive(Default)]
pub struct OpencodeSettings {
    pub enable_opencode: bool,
    /// The one model every AI run uses (the wands, commit messages and the
    /// daily summary). The UI calls it "Model"; the file keeps the
    /// `default_model` spelling.
    #[serde(rename = "default_model")]
    pub model: String,
}

/// The DeepSeek CLI quick-launch: only arms the repo row's launch button —
/// the runs themselves happen in warp-agent, never in the dsh binary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
#[derive(Default)]
pub struct DeepseekSettings {
    pub enable_deepseek: bool,
}

/// The warp-agent integration: Warp Station' AI surface runs in warp-agent over
/// its door, so these toggle the whole AI feature set and name its binary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct WarpAgentSettings {
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
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct WarpNotesSettings {
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
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
#[derive(Default)]
pub struct ClipboardPasswordSettings {
    pub encrypted_password: Option<String>,
    pub enable_clipboard_password: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
#[derive(Default)]
pub struct SnapitSettings {
    /// Spawns the SnapIt companion at app launch.
    pub auto_start: bool,
    /// The companion binary to launch; empty = the packaged warp-snapit.exe
    /// next to the app. A bare name resolves through PATH and the user
    /// install dirs, like the warp-agent executable.
    pub executable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
#[derive(Default)]
pub struct KeepAwakeSettings {
    /// Restores the keep-awake execution state at app launch.
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct GeneralSettings {
    pub start_minimized: bool,
    pub start_at_boot: bool,
    pub notes_store_path: String,
    pub show_tooltips: bool,
    /// "compact" | "normal" | "large" text-size variant; None = normal.
    pub text_size: Option<String>,
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
            text_size: None,
            reduce_motion: false,
        }
    }
}

/// The Changes pane's view state that outlives one window — today only the
/// diff layout. The section is additive: an absent key reads unified.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default, deny_unknown_fields)]
pub struct ChangesSettings {
    pub diff_view: DiffViewMode,
    /// Wrap long diff rows instead of scrolling horizontally.
    pub diff_wrap: bool,
}

impl Default for ChangesSettings {
    fn default() -> Self {
        Self {
            diff_view: DiffViewMode::Unified,
            diff_wrap: false,
        }
    }
}

/// How the Changes diff lays its rows out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffViewMode {
    #[default]
    Unified,
    Split,
}

// a hand-edited or newer value must not invalidate the whole settings file
impl<'de> Deserialize<'de> for DiffViewMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "split" => Self::Split,
            _ => Self::Unified,
        })
    }
}

/// Single owner of the settings graph: reads hand out full clones, writes are
/// serialized and persisted atomically. An unreadable file falls back to
/// defaults instead of failing startup.
pub struct SettingsService {
    store: Store<AppSettings>,
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
        Self::load(path, true)
    }

    /// A read-only view for the net thread and the clipboard ping: it reads
    /// the same file but never rewrites an unversioned one.
    pub fn open_readonly() -> Self {
        let service = Self::load(AppPaths::settings_file(), false);
        let legacy = AppPaths::legacy_notes_dir().to_string_lossy().to_string();
        let mut current = service.get();
        if clear_legacy_notes_path(&mut current, &legacy) {
            *service.state.lock().unwrap() = current;
        }
        service
    }

    pub fn at_readonly(path: PathBuf) -> Self {
        Self::load(path, false)
    }

    fn load(path: PathBuf, persist_import: bool) -> Self {
        let store = Store::<AppSettings>::new(path.clone());
        let settings = match store.load() {
            Ok(settings) => settings,
            Err(err) if err.needs_import() => {
                let imported = import_legacy(&path);
                if persist_import && let Some(settings) = &imported {
                    let _ = store.save(settings);
                }
                imported.unwrap_or_default()
            }
            Err(err) => {
                tracing::warn!("could not read {}: {err}", path.display());
                AppSettings::default()
            }
        };
        Self {
            store,
            state: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> AppSettings {
        self.state.lock().unwrap().clone()
    }

    /// Apply `mutate` to a clone of the current settings and persist the whole
    /// graph as one transition, so concurrent section updates stack instead of
    /// racing.
    pub fn update(&self, mutate: impl FnOnce(&mut AppSettings)) -> anyhow::Result<AppSettings> {
        let updated = {
            let mut guard = self.state.lock().unwrap();
            let mut next = guard.clone();
            mutate(&mut next);
            *guard = next.clone();
            next
        };
        self.store.save(&updated)?;
        Ok(updated)
    }
}

/// The pre-versioning file: PascalCase keys, the old per-tool toggles, and a
/// few fields the app has since retired. Translate once; the live type is
/// strict.
fn import_legacy(path: &Path) -> Option<AppSettings> {
    import_legacy_json(&std::fs::read_to_string(path).ok()?)
}

fn import_legacy_json(text: &str) -> Option<AppSettings> {
    let mut root: serde_json::Value = serde_json::from_str(text).ok()?;
    rename_legacy_keys(&mut root, &[]).ok()?;

    copy_section_bool(
        &mut root,
        "repos",
        "show_github_column",
        "enable_github",
        false,
    );
    copy_section_bool(
        &mut root,
        "repos",
        "show_azure_devops_column",
        "enable_azure_devops",
        false,
    );
    copy_section_bool(&mut root, "opencode", "enabled", "enable_opencode", false);
    copy_section_bool(
        &mut root,
        "clipboard_password",
        "hide_from_gui",
        "enable_clipboard_password",
        true,
    );
    move_section_key(&mut root, "general", "font_scale", "text_size");
    remove_section_keys(
        &mut root,
        "repos",
        &[
            "auto_fetch_minutes",
            "auto_fetch_only_focused",
            "auto_refresh_minutes",
            "sync_on_focus",
        ],
    );
    remove_section_keys(&mut root, "opencode", &["commit_model"]);

    serde_json::from_value(root).ok()
}

/// Copy a legacy bool to its new key unless the new key is already there,
/// then remove the legacy spelling.
fn copy_section_bool(
    root: &mut serde_json::Value,
    section: &str,
    old: &str,
    new: &str,
    inverted: bool,
) {
    let Some(object) = root
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let Some(value) = object.get(old).and_then(serde_json::Value::as_bool) else {
        return;
    };
    if !object.contains_key(new) {
        object.insert(
            new.to_string(),
            serde_json::Value::Bool(if inverted { !value } else { value }),
        );
    }
    object.remove(old);
}

fn move_section_key(root: &mut serde_json::Value, section: &str, old: &str, new: &str) {
    let Some(object) = root
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let Some(value) = object.remove(old) else {
        return;
    };
    if !object.contains_key(new) {
        object.insert(new.to_string(), value);
    }
}

fn remove_section_keys(root: &mut serde_json::Value, section: &str, keys: &[&str]) {
    let Some(object) = root
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for key in keys {
        object.remove(*key);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn imported(json: &str) -> AppSettings {
        import_legacy_json(json).expect("legacy settings import")
    }

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "warp-station-settings-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn reads_the_legacy_file_shape() {
        let s = imported(
            r#"{
            "Repos": {"RepoScanFolders": ["/a", "/b"], "SortMode": 2, "MaxScanDepth": 5,
                      "EnableGitHub": true, "AzureDevOpsPat": "pat"},
            "General": {"ShowTooltips": false},
            "OpenCode": {"EnableOpenCode": true, "DefaultModel": "x", "CommitModel": "legacy"}
        }"#,
        );
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
        assert!(s.nuget_local.enable_nuget);
        assert!(!s.keep_awake.enabled);
        assert_eq!(s.repos.branch_name_prefix, "users/enginkirmaci");
    }

    #[test]
    fn legacy_enable_keys_migrate_once() {
        let s = imported(
            r#"{"OpenCode": {"Enabled": true}, "ClipboardPassword": {"HideFromGui": false}}"#,
        );
        assert!(s.opencode.enable_opencode);
        assert!(s.clipboard_password.enable_clipboard_password);

        let s = imported(r#"{"OpenCode": {"Enabled": true, "EnableOpenCode": false}}"#);
        assert!(!s.opencode.enable_opencode, "the explicit new key wins");
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
    fn round_trips_snake_case() {
        let s = AppSettings::default();
        let json = serde_json::to_string_pretty(&s).unwrap();
        assert!(json.contains("\"repo_scan_folders\""));
        assert!(json.contains("\"max_scan_depth\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.sort_mode, s.repos.sort_mode);
    }

    #[test]
    fn diff_view_reads_and_falls_back() {
        let split: AppSettings =
            serde_json::from_str(r#"{"changes":{"diff_view":"split"}}"#).unwrap();
        assert_eq!(split.changes.diff_view, DiffViewMode::Split);

        let missing: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.changes.diff_view, DiffViewMode::Unified);
        assert!(!missing.changes.diff_wrap);

        let wrapped: AppSettings =
            serde_json::from_str(r#"{"changes":{"diff_wrap":true}}"#).unwrap();
        assert!(wrapped.changes.diff_wrap);

        let unknown: AppSettings =
            serde_json::from_str(r#"{"changes":{"diff_view":"sideways"}}"#).unwrap();
        assert_eq!(
            unknown.changes.diff_view,
            DiffViewMode::Unified,
            "a stale string falls back instead of failing the page"
        );

        let json = serde_json::to_string(&split).unwrap();
        assert!(json.contains(r#""diff_view":"split""#), "{json}");
        assert!(json.contains(r#""diff_wrap":false"#), "{json}");
    }

    #[test]
    fn retired_schedule_keys_load_and_are_not_written_back() {
        let legacy = imported(
            r#"{"Repos": {
                "AutoFetchMinutes": 15,
                "AutoFetchOnlyFocused": true,
                "AutoRefreshMinutes": 30,
                "SyncOnFocus": true,
                "BranchNamePrefix": "users/x"
            }}"#,
        );
        assert_eq!(legacy.repos.branch_name_prefix, "users/x");
        let json = serde_json::to_string_pretty(&legacy).unwrap();
        for key in [
            "auto_fetch_minutes",
            "auto_fetch_only_focused",
            "sync_on_focus",
        ] {
            assert!(!json.contains(key), "{key} must not be written back");
        }
        // the new snake_case auto-refresh key is live now: it serializes even
        // when unset, and the retired PascalCase spelling above never feeds it
        assert!(json.contains("\"auto_refresh_minutes\": null"));
    }

    #[test]
    fn group_repos_keys_round_trip_and_absent_means_flat() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.group_repos_by_root, None);
        assert_eq!(legacy.repos.group_root_depth, None);
        assert_eq!(legacy.repos.collapsed_groups, None);

        let on: AppSettings = serde_json::from_str(
            r#"{"repos": {"group_repos_by_root": true, "group_root_depth": 2, "collapsed_groups": ["/work/rust", "other"]}}"#,
        )
        .unwrap();
        assert_eq!(on.repos.group_repos_by_root, Some(true));
        assert_eq!(on.repos.group_root_depth, Some(2));
        assert_eq!(
            on.repos.collapsed_groups,
            Some(vec!["/work/rust".to_string(), "other".to_string()])
        );
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"group_repos_by_root\": true"));
        assert!(json.contains("\"group_root_depth\": 2"));
        assert!(json.contains("\"collapsed_groups\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos, on.repos);
    }

    #[test]
    fn code_search_keys_round_trip_and_absent_means_enabled() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.enable_code_search, None);
        assert_ne!(legacy.repos.enable_code_search, Some(false));
        assert_eq!(legacy.repos.code_search_executable.as_deref(), None);

        let off: AppSettings = serde_json::from_str(
            r#"{"repos": {"enable_code_search": false, "code_search_executable": "rgz"}}"#,
        )
        .unwrap();
        assert_eq!(off.repos.enable_code_search, Some(false));
        assert_eq!(off.repos.code_search_executable.as_deref(), Some("rgz"));
        let json = serde_json::to_string_pretty(&off).unwrap();
        assert!(json.contains("\"enable_code_search\": false"));
        assert!(json.contains("\"code_search_executable\": \"rgz\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.enable_code_search, Some(false));
        assert_eq!(back.repos.code_search_executable.as_deref(), Some("rgz"));
    }

    #[test]
    fn snapit_executable_round_trips_and_absent_means_packaged_companion() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.snapit.executable, "");
        let custom = imported(r#"{"SnapIt": {"Executable": "snapit"}}"#);
        assert_eq!(custom.snapit.executable, "snapit");
        let json = serde_json::to_string_pretty(&custom).unwrap();
        assert!(json.contains("\"snapit\""));
        assert!(json.contains("\"executable\": \"snapit\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.snapit.executable, "snapit");
    }

    #[test]
    fn deepseek_keys_round_trip_and_absent_means_off() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.deepseek.enable_deepseek);
        assert_eq!(legacy.repos.deepseek_executable, "dsh");
        let on = imported(
            r#"{"DeepSeek": {"EnableDeepSeek": true}, "Repos": {"DeepseekExecutable": "dshz"}}"#,
        );
        assert!(on.deepseek.enable_deepseek);
        assert_eq!(on.repos.deepseek_executable, "dshz");
        let json = serde_json::to_string_pretty(&on).unwrap();
        assert!(json.contains("\"enable_deepseek\": true"));
        assert!(json.contains("\"deepseek_executable\": \"dshz\""));
    }

    #[test]
    fn warp_agent_defaults_on_with_executable() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(s.warp_agent.enable_warp_agent);
        assert_eq!(s.warp_agent.executable, "warp-agent");
        assert_eq!(s.warp_agent.selected_harness(), DEFAULT_HARNESS);
        assert_eq!(s.warp_agent.variant, None);

        let configured = imported(r#"{"WarpAgent": {"Harness": "dsh", "Variant": "max"}}"#);
        assert_eq!(configured.warp_agent.selected_harness(), "dsh");
        assert_eq!(configured.warp_agent.variant.as_deref(), Some("max"));

        let off = imported(r#"{"WarpAgent": {"EnableWarpAgent": false, "Executable": "wa"}}"#);
        assert!(!off.warp_agent.enable_warp_agent);
        assert_eq!(off.warp_agent.executable, "wa");
    }

    #[test]
    fn ui_state_keys_round_trip_and_absent_means_defaults() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.repos.show_archived, None);
        assert_eq!(legacy.repos.selected_repo, None);
        assert_eq!(legacy.repos.open_tab, None);
        assert_eq!(legacy.repos.tab_views, None);
        assert_eq!(legacy.repos.home_view, None);

        let arranged = imported(
            r#"{
                "Repos": {
                    "ShowArchived": true,
                    "SelectedRepo": "/work/alpha",
                    "OpenTab": "/work/beta",
                    "TabViews": [{"Folder": "/work/beta", "Tab": "Changes"}],
                    "HomeView": {"Folder": "/work/alpha", "Tab": "History"}
                }
            }"#,
        );
        assert_eq!(arranged.repos.show_archived, Some(true));
        assert_eq!(arranged.repos.selected_repo.as_deref(), Some("/work/alpha"));
        assert_eq!(arranged.repos.open_tab.as_deref(), Some("/work/beta"));
        assert_eq!(arranged.repos.tab_views.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(
            arranged.repos.home_view.as_ref().map(|v| v.tab.as_str()),
            Some("History")
        );

        let json = serde_json::to_string_pretty(&arranged).unwrap();
        assert!(json.contains("\"show_archived\": true"));
        assert!(json.contains("\"selected_repo\": \"/work/alpha\""));
        assert!(json.contains("\"open_tab\""));
        assert!(json.contains("\"tab_views\""));
        assert!(json.contains("\"home_view\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos, arranged.repos);
    }

    #[test]
    fn notifications_round_trip_and_absent_means_on_unfocused() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(legacy.notifications.enabled);
        assert!(legacy.notifications.only_when_unfocused);
        assert!(legacy.notifications.errors);
        assert!(legacy.notifications.action_confirmations);
        assert!(legacy.notifications.fetch_summaries);

        let tuned = imported(
            r#"{"Notifications": {"Enabled": false, "OnlyWhenUnfocused": false,
                "Errors": false, "ActionConfirmations": false, "FetchSummaries": false}}"#,
        );
        assert!(!tuned.notifications.enabled);
        assert!(!tuned.notifications.only_when_unfocused);
        assert!(!tuned.notifications.errors);
        assert!(!tuned.notifications.action_confirmations);
        assert!(!tuned.notifications.fetch_summaries);

        let json = serde_json::to_string_pretty(&tuned).unwrap();
        assert!(json.contains("\"notifications\""));
        assert!(json.contains("\"only_when_unfocused\": false"));
    }

    #[test]
    fn auto_refresh_keys_round_trip_and_absent_means_on() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(
            legacy.repos.auto_refresh_on_focus,
            "an absent focus switch means the requested on default"
        );
        assert_eq!(
            legacy.repos.auto_refresh_minutes, None,
            "the periodic scan is off until it gets a minute value"
        );

        let tuned: AppSettings = serde_json::from_str(
            r#"{"repos": {"auto_refresh_on_focus": false, "auto_refresh_minutes": 15}}"#,
        )
        .unwrap();
        assert!(!tuned.repos.auto_refresh_on_focus);
        assert_eq!(tuned.repos.auto_refresh_minutes, Some(15));

        let json = serde_json::to_string_pretty(&tuned).unwrap();
        assert!(json.contains("\"auto_refresh_on_focus\": false"));
        assert!(json.contains("\"auto_refresh_minutes\": 15"));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.auto_refresh_minutes, Some(15));
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

    #[test]
    fn the_service_imports_once_and_writes_a_versioned_file() {
        let path = scratch("import");
        std::fs::write(
            &path,
            r#"{"Repos": {"EnableGitHub": false, "GitHubExecutable": "gh-old"},
                "General": {"FontScale": "large"},
                "OpenCode": {"DefaultModel": "x"}}"#,
        )
        .unwrap();

        let service = SettingsService::at(path.clone());
        assert!(!service.get().repos.enable_github);
        assert_eq!(service.get().repos.github_executable, "gh-old");
        assert_eq!(service.get().general.text_size.as_deref(), Some("large"));
        assert_eq!(service.get().opencode.model, "x");

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["schema_version"], 1);
        assert_eq!(saved["repos"]["enable_github"], false);
        assert_eq!(saved["general"]["text_size"], "large");
        assert!(saved.get("Repos").is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
