# Settings and cache schema

Status: the contract for the versioned snake_case settings layer. The code is
authoritative for defaults; this file records the names, ownership and
compatibility rules that must not drift.

## Store contract

- Every warp-suite-owned JSON file is one object with a top-level
  `"schema_version": 1`.
- `warpui_kit_platform::config::Store<T>` reads/writes a typed document:
  atomic replace, optional sidecar lock, version check. A newer version is
  left untouched; an older or unversioned one is importer input.
- `warpui_kit_platform::config::SharedDoc` owns the shared `~/.warp/ui.json`:
  read-modify-write, unknown keys preserved, sidecar lock.
- Per-app settings are strict (`deny_unknown_fields`). UI-state and cache
  files tolerate unknown keys so a newer build does not strand an older one.
- `KeyMap<V>` is a transparent map whose keys are user data (session ids,
  folder paths, monitor names). No casing pass may rewrite those keys.
- File names are lowercase and have no `.cache` suffix.

## Files

| File | Owner | Shape |
|---|---|---|
| `~/.warp/station/settings.json` | warp-station | versioned settings |
| `~/.warp/station/cache/repos.json` | warp-station | regenerable status |
| `~/.warp/station/cache/models.json` | warp-station | regenerable model catalog |
| `~/.warp/agent/settings.json` | warp-agent | versioned settings |
| `~/.warp/agent/ui.json` | warp-agent | versioned window/session state |
| `~/.warp/agent/cache/models.json` | warp-agent | regenerable model catalog |
| `~/.warp/notes/settings.json` | warp-notes | versioned window state |
| `~/.warp/ui.json` | warpui-kit-core | shared additive map |
| `~/.warp/snapit/settings.json` | warp-snapit | versioned settings |
| `~/.warp/snapit/excluded_applications.json` | warp-snapit | versioned exclusions |
| `~/.warp/snapit/layouts/*.json` | warp-snapit / warp-station | versioned layouts |

## Naming

- Keys and string enum values are `snake_case`.
- One-word brands: `github`, `vscode`, `opencode`, `deepseek`, `snapit`,
  `azure_devops`, `mcp`, `nuget`.
- No abbreviations: `server_idle_stop_minutes`,
  `mcp_disabled_workspaces`, `text_size`, `show_tooltips`.
- `tab` values are the app's display labels (`"Changes"`, `"History"`), not
  enum ids.
- `RepoSortMode` stays numeric; `HarnessKind` stays `"opencode"`/`"dsh"`.

## warp-station settings

Top level: `schema_version`, `nuget_local`, `repos`, `clipboard_password`,
`snapit`, `keep_awake`, `opencode`, `deepseek`, `warp_agent`, `warp_notes`,
`general`, `changes`, `notifications`.

- `nuget_local`: `enable_nuget` (true), `watch_folder` (null),
  `nuget_package_filter` (`*.nupkg`), `file_copy_delay_ms` (2000),
  `count_reset_interval_seconds` (60).
- `repos`: `repo_scan_folders` (null), `repo_tags` ({}), `git_folder_pattern`
  (`*.git`), `solution_file_pattern` (`*.sln,*.slnx`), `platform_folder_name`
  (`platform`), `vscode_executable` (`code`), `vscode_profile` (""),
  `terminal_executable` (`wt`), `ide_executable` (""), `opencode_executable`
  (`opencode`), `deepseek_executable` (`dsh`), `enable_terminal` (true),
  `enable_vscode` (true), `enable_visual_studio` (true), `enable_github`
  (true), `github_executable` (`gh`), `enable_azure_devops` (true),
  `azure_devops_pat` (""), `azure_devops_url` (""), `excluded_folders` (null),
  `sort_mode` (0), `sort_ascending` (null), `group_repos_by_root` (null),
  `group_root_depth` (null), `collapsed_groups` (null),
  `enable_code_search` (null), `code_search_executable` (null),
  `max_scan_depth` (3), `branch_name_prefix`, `issue_sort` (null),
  `repo_tabs` (null), `list_width` (null), `show_archived` (null),
  `selected_repo` (null), `open_tab` (null), `tab_views` (null),
  `home_view` (null).
- `repo_tags` is the one home of user tags (`favorites`, `archived`, user
  tags), keyed by folder path. The cache never writes them.
- `tab_views` / `home_view` entries: `folder`, `tab`.
- `clipboard_password`: `encrypted_password` (null),
  `enable_clipboard_password` (false).
- `snapit`: `auto_start` (false), `executable` ("").
- `keep_awake`: `enabled` (false).
- `opencode`: `enable_opencode` (false), `default_model` ("").
- `deepseek`: `enable_deepseek` (false).
- `warp_agent`: `enable_warp_agent` (true), `executable` (`warp-agent`),
  `harness` (`opencode`), `variant` (null).
- `warp_notes`: `enable_warp_notes` (true), `executable` (`warp-notes`).
- `general`: `start_minimized` (false), `start_at_boot` (false),
  `notes_store_path` (""), `show_tooltips` (true), `text_size` (null),
  `reduce_motion` (false).
- `changes`: `diff_view` (`unified` default, `split`) and `diff_wrap`
  (false). The last layout and word-wrap choices from the diff header; global,
  not per repository. An unknown layout string reads unified.
- `notifications`: `enabled` (true), `only_when_unfocused` (true),
  `errors` (true), `action_confirmations` (true), `fetch_summaries` (true).

## warp-station cache

`schema_version`, `repos[]`: `name`, `folder_path`, `solution_path`,
`git_branch_name`, `git_modified_count`, `git_to_push_count`,
`git_to_pull_count`, `git_has_remote`, `git_branch_published`,
`git_status_loaded`, `github_pr_count`, `github_issue_count`,
`git_last_commit_at`, `git_last_fetch_at`. No tags; deleting the file loses no
user state.

## warp-agent settings

`schema_version`, `opencode_executable` (`opencode`), `dsh_executable`
(`dsh`), `default_harness` (`opencode`), `default_model`, `default_variant`,
`default_variant_highest`, `default_agent`, `auto_allow_always`,
`auto_allow_instant`, `auto_allow_default_min_seconds`,
`auto_allow_default_max_seconds`, `auto_allow_away_min_seconds`,
`auto_allow_away_max_seconds`, `server_idle_stop_minutes`,
`lunch_start_hour`, `lunch_end_hour`, `off_hours_start_hour`,
`workday_start_hour`, `text_size`, `show_tooltips`, `reduce_motion`,
`mcp_disabled`.

## warp-agent UI state

`schema_version`, `pins` (`KeyMap<bool>`), `favorites` (`KeyMap<bool>`),
`workspaces[]`, `active_workspace`, `model`, `variant`, `agent`, `folder`,
`auto_allow_always`, `sidebar_width`, `sidebar_visible`, `drafts`
(`KeyMap<String>`), `mcp_disabled_workspaces` (`Vec<String>`),
`harness_by_session` (`KeyMap<"opencode"|"dsh">`), `deleted_sessions`.

`workspaces[]`: `path`, `sessions`, `active_session`, `open_sessions`,
`collapsed`. Session ids and folder paths inside the maps are opaque.

## Model caches

`schema_version`, `models[]`: `key`, `harness`, `variants`, `context_limit`.
The released flat `["provider/model"]` list is importer input only.

## warp-notes settings

`schema_version`, `list_width`, `reduce_motion`, `list_visible`, `section`,
`settings_section`, `open_note`, `expanded`, `recent`, `pinned`, `notes_scope`.
Paths are relative to the notes store root; `recent` is newest-first and
`pinned` is the quick-open/tree pin list.

## Shared `ui.json`

`schema_version`, `text_size` (`small`/`normal`/`large`/`larger`), `palette`
(`copper`/`plum`/`pine`/`blue`), plus any key another app owns. A writer
changes only its own key and preserves the rest.

## SnapIt

Native SnapIt files are snake_case and versioned like the rest of the suite:

- `settings.json`: `schema_version`, `enable_mouse`, `mouse_button`
  (`left`/`right`/`middle`/`x_button1`/`x_button2`), `hold_key`
  (`control`/`alt`/`shift`/`win`), `hold_key_behaviour`
  (`hold_to_enable`/`hold_to_disable`), `screens_layouts` (monitor → GUID),
  `deactivated_screens`, `run_as_admin`, `theme`, …
- `excluded_applications.json`: `schema_version`, `applications[]` with
  `keyword`, `match_rule` (`contains`/`exact`/`wildcard`), `mouse`.
- `layouts/*.json`: `schema_version`, `guid`, `name`, `size`
  (`"x,y"`), `layout_lines` (`point`, `size`, `split_direction`
  `vertical`/`horizontal`), `layout_overlays`, `area_padding`.

The old .NET-shaped `Settings.json`, `ExcludedApplicationSettings.json` and
PascalCase layouts are importer input. They are read once, rewritten native,
and the old file is retired. Warp Station's layout preview reads both shapes
so it works before warp-snapit has run after an upgrade.
## Import rules

- `SettingsService::at`, `AppSettings::load`, `UiState::load`,
  `UiPrefs::load` import a pre-versioning file once and then write the
  versioned file.
- The importer renames old keys with `config::snake_key`, skips opaque maps,
  applies the explicit field renames (`OpenCode` -> `opencode`,
  `FontScale` -> `text_size`, `McpDisabled` -> `mcp_disabled_workspaces`, …)
  and drops retired fields.
- The live store never normalizes keys. A file with a newer
  `schema_version` is left alone and the app falls back to defaults.
- A cache is regenerable. The only user data an old cache owned was repo tags;
  the station importer lifts them into `repos.repo_tags` before the cache is
  rewritten.
