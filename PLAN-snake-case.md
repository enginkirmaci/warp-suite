# Plan: snake_case settings and caches across warp-suite

Status: proposal, not implemented. Working file; delete it when the work lands.
This is separate from the untracked `PLAN.md` (auto-fetch), which is untouched.

## 1. Goal

Every JSON file warp-suite owns or reads under `~/.warp` (and the notes
settings under `~/.warp/notes`) uses **snake_case object keys**. Existing
PascalCase/camelCase files are migrated once, in place, without losing user
data. New writes are always snake_case.

Non-goals for this pass:

- Rust field names (already snake_case).
- File names: `settings.json`, `repos.cache.json`, `models.cache.json`,
  `Settings.json`, `layouts/*.json` keep their names.
- Opaque map keys: session ids under `Pins`/`Drafts`/`HarnessBySession`,
  folder paths under `Favorites`, monitor names under `ScreensLayouts` keep
  their exact spelling.
- String enum values unless decision D3 says otherwise.
- Third-party state (opencode/dsh session stores, logs, prompt markdown).
- The untracked `PLAN.md` (auto-fetch design) and the user's pre-snake backup
  files such as `~/.warp/settings.json.bak-20261013`.

## 2. Current state (verified on disk and in source)

| File | Owner | Keys today | User data that must survive |
|---|---|---|---|
| `~/.warp/station/settings.json` | warp-station | PascalCase | scan roots, executables, tabs, selected repo, widths, toggles |
| `~/.warp/station/repos.cache.json` | warp-station | PascalCase | tags (`favorites`, `archived`, user tags), last known status/fetch times |
| `~/.warp/station/models.cache.json` | warp-station | snake_case | regenerable |
| `~/.warp/agent/settings.json` | warp-agent | mixed (2 snake, rest PascalCase) | executables, harness/model defaults, auto-allow, hours |
| `~/.warp/agent/ui.json` | warp-agent | PascalCase | pins, favorites, workspaces/sessions, drafts, per-session harness, deleted chats |
| `~/.warp/agent/models.cache.json` | warp-agent | snake_case | regenerable |
| `~/.warp/notes/settings.json` | warp-notes | snake_case | already snake; no migration |
| `~/.warp/ui.json` | warpui-kit-core | snake_case (`text_size`, `palette`) | shared, already done |
| `~/.warp/snapit/Settings.json` | warp-snapit (+ .NET) | PascalCase | mouse/hold/theme, `ScreensLayouts`, `DeactivedScreens` |
| `~/.warp/snapit/ExcludedApplicationSettings.json` | warp-snapit (+ .NET) | PascalCase | exclusion rules |
| `~/.warp/snapit/layouts/*.json` | warp-snapit (+ .NET), read by warp-station | PascalCase | user layout geometry |
| `apps/warp-snapit/packaging/windows/settings/snapit/*.json` | packaging seed | PascalCase | shipped defaults |

Source anchors:

- `apps/warp-station/crates/warp-station-core/src/settings.rs` — `AppSettings`,
  nested sections, `SettingsService::at/parse`, legacy enable-key migration,
  tests.
- `apps/warp-station/crates/warp-station-core/src/repo_store.rs` —
  `CachedRepo`, `RepoCacheFile`, `load_cache`, tests.
- `apps/warp-station/crates/warp-station-core/src/snapit.rs` — layout preview
  parser reading `Size`, `LayoutLines`, `Point`, `Name` by string.
- `apps/warp-agent/src/settings.rs` — `AppSettings`, `UiState`,
  `WorkspaceState`, `read_json`, no tests today.
- `apps/warp-snapit/src/settings.rs` — `Settings`, `Theme`, `Layout`,
  `ExcludedApplicationSettings`, enums, loaders, tests.
- `apps/warp-notes/src/ui_prefs.rs` — already writes snake_case keys.
- `kits/warpui-kit-core/src/scale.rs` — already writes `text_size`/`palette`.
- `kits/warpui-kit-platform/src/io.rs` — atomic JSON replace used by the apps.

## 3. Decisions to confirm before step 1

D1. **Scope.** Recommended: all suite-owned files in the table above,
including agent `ui.json`; SnapIt follows D2. Alternative: only
`settings.json` + `*.cache.json`.

D2. **SnapIt / .NET compatibility.** This is the one real conflict.
`apps/warp-snapit/AGENTS.md` says SnapIt reads the same files as the .NET app
and that changing a field is a cross-app contract change. Converting the keys
means the .NET SnapIt can no longer read new files.
Options:
  - A (matches "all"): convert SnapIt too, keep old files readable during
    migration, accept that the .NET app is read-only for old files.
  - B (preserve the contract): leave `~/.warp/snapit` in PascalCase and
    document it as the one deliberate exception. Station's own `SnapIt`
    section still converts because it lives in `station/settings.json`.
  - C (dual format): write both spellings. Rejected unless required: files are
    no longer purely snake_case.

  Recommendation: B until you confirm the .NET companion is retired; if it
  is retired, A is the clean "all" answer.

D3. **Enum values.** Recommended: keys only. `MouseButton: "Left"`,
`HoldKeyBehaviour: "HoldToEnable"`, `SplitDirection: "Vertical"` stay as
values. Alternative: convert enum values too (`"left"`, `"hold_to_enable"`,
`"vertical"`), which is a second breaking change for old layouts and .NET.

D4. **Old data.** Recommended: a lazy one-time migration (first load after the
upgrade) that preserves every value listed in the table, then rewrites the
file atomically. Caches are not reset. Alternative: reset regenerable caches
(`models.cache.json`), never user settings/tags/pins.

D5. **File names.** Recommended: keep them. Renaming `Settings.json` to
`settings.json` is a separate cross-app change with its own migration.

## 4. Migration design

One shared shape, implemented per app (warp-snapit deliberately has no
`warpui-kit-*` dependency, so it carries a small local copy):

1. Read the file to `serde_json::Value`.
2. Recursively rename object keys with `snake_key()`:
   - brand exceptions before generic splitting:
     `VSCode -> vscode`, `OpenCode -> opencode`, `DeepSeek -> deepseek`;
   - split camel/Pascal boundaries, including acronym runs:
     `GitHubExecutable -> git_hub_executable`,
     `AutoAllowDefaultMinSeconds -> auto_allow_default_min_seconds`,
     `DeactivedScreens -> deactived_screens`.
3. Do **not** recurse into opaque map values: `screens_layouts`, `pins`,
   `favorites`, `drafts`, `harness_by_session`. The container key is renamed;
   the keys inside it are untouched.
4. Apply the explicit field-name exceptions where the Rust field and the
   natural snake key differ. These are not aliases and they stay as
   `#[serde(rename = "...")]`:
   - station `OpenCodeSettings.model` keeps the key `default_model`
     (old `DefaultModel`, UI label "Model");
   - agent `UiState.mcp_disabled_ws` keeps the key `mcp_disabled`
     (old `McpDisabled`; the settings file has its own `mcp_disabled`).
5. Map the already-existing legacy toggles in the same pass (station only):
   `repos.show_git_hub_column -> repos.enable_git_hub`,
   `repos.show_azure_dev_ops_column -> repos.enable_azure_dev_ops`,
   `opencode.enabled -> opencode.enable_opencode`,
   `clipboard_password.hide_from_gui -> !enable_clipboard_password`, each only
   when the new key is absent.
6. Refuse to rewrite a file whose old keys collide (two different old keys
   normalizing to the same new key); log both spellings and keep the file.
   Real files checked so far have no collisions, but the guard makes the
   migration safe for hand-edited files.
7. Deserialize the transformed value into the typed struct **before writing**.
   A file that no longer fits the schema is left untouched and falls back to
   the app's current behavior (defaults / warning / `.json.bad` for agent).
8. If any key changed and step 7 succeeded, write the transformed `Value` back
   atomically. Writing the `Value`, not the typed struct, preserves unknown
   fields (`RunAsAdmin` in SnapIt settings, older runtime fields in
   `repos.cache.json`) until the app's next normal save.
   `RunAsAdmin` is a special case: the Rust `Settings` struct does not model
   it today. If D2 = A, keep it in the JSON (as `run_as_admin`) and either add
   the field to the struct or accept that the next `SettingsStore::update`
   drops it. Do not silently lose a .NET setting.
9. The transform is idempotent: a second load finds no uppercase key and does
   not rewrite. A failed write is logged and does not block startup.

Typed structs switch from `rename_all = "PascalCase"` to
`rename_all = "snake_case"`. `RepoSortMode` stays numeric. Agent
`HarnessKind` stays `lowercase`. No serde aliases: migration is the one
compatibility path, which keeps the structs honest and prevents a silent
"old key still works" state from lingering forever.

Downgrade behavior is explicit: an older build reading a migrated file sees
defaults for unknown snake keys, and its next save can overwrite the migrated
file with the old keys (losing pins/tags the old build never read). Document
this and keep the pre-migration backup; do not dual-write.

## 5. Steps

### Step 0 — Preflight and freeze

Action: confirm D1-D5; land or stash the current uncommitted auto-fetch revert
(`settings.rs`, `main.rs`, `store/mod.rs`, `ui/settings.rs`, the station
probes). Copy the real files from `~/.warp` to a temp directory **outside the
repo** for a before/after comparison; commit only hand-written, sanitized
fixtures (no encrypted password, no real session ids or folder paths). Do not
touch `PLAN.md`.

Check: `git status --short` shows no uncommitted change in a file this plan
edits, and the sanitized fixtures exist with their original keys. If the
auto-fetch work must stay dirty, this plan waits; otherwise the two diffs
collide in `settings.rs` and every probe. Capture a baseline: `cargo test`
for the packages this plan touches passes (or the known failures are written
down) before any edit.

### Step 1 — Add the key normalizer

Action: add `kits/warpui-kit-platform/src/json_case.rs` with `snake_key`,
`migrate_object_keys(&mut Value, skip_keys)`, and unit tests; export it from
the kit. If D2 = A, add the same compact helper inside
`apps/warp-snapit/src/settings.rs` (or a small `json_case` module there) with a
comment explaining why it is not shared.

Check: `cargo test -p warpui-kit-platform json_case` and
`cargo test -p snapit json_case` (if D2 = A) pass. The table covers brand
acronyms, acronym runs, digits, already-snake keys, idempotence, skipped maps,
and a two-old-keys-one-new-key collision.

### Step 2 — Convert warp-station settings

Action: switch `AppSettings` and every nested section to
`rename_all = "snake_case"`; replace the string-based
`migrate_legacy_enable_keys` with the `Value` migration from step 1; make
`SettingsService::at/parse` write the transformed value only after typed parse
succeeds. Preserve the existing "legacy notes path clears once" behavior.
Add a non-migrating `SettingsService::open_readonly()` and use it in
`src/net.rs` and `src/hotkey.rs`, so a periodic status read cannot race the
store's save with a migration write; it normalizes in memory (so old files
still read correctly) but never writes. Only the owning startup load migrates.

Check: `cargo test -p warp-station-core settings::` passes. New tests prove:
old PascalCase + `CommitModel` unknown field loads the same values, the file is
rewritten with snake keys, the unknown field is still present, and a second
load does not rewrite (mtime/content unchanged). A full fixture sets every
`AppSettings` field so a field whose key does not match its Rust name is
caught, not just the fields the real file happens to use.

### Step 3 — Convert warp-station repo cache

Action: switch `CachedRepo`, `CachedTag`, `RepoCacheFile` to snake_case; make
`load_cache` migrate before deserializing and rewrite after success.

Check: `cargo test -p warp-station-core repo_store::` passes. A legacy cache
fixture with `favorites`, `archived`, a user tag, counts and fetch times
round-trips with every value intact and writes `folder_path`,
`git_modified_count`, etc.

### Step 4 — Convert warp-agent settings and UI state

Action: switch `AppSettings`, `UiState`, `WorkspaceState` to snake_case; add
`read_json` migration with the opaque-key skip list; rewrite only after typed
parse. Add the missing unit tests (legacy settings, legacy `ui.json`, pins /
favorites / drafts keys unchanged, idempotence).

Check: `cargo test -p warp-agent settings` passes. A fixture with
`Pins`/`Favorites`/`Drafts`/`HarnessBySession` keeps its opaque inner keys
byte-for-byte while the surrounding keys become `active_workspace`,
`sidebar_width`, `deleted_sessions`, etc. The fixture also proves
`McpDisabled` lands on `mcp_disabled_ws` through the deliberate
`#[serde(rename = "mcp_disabled")]` exception.

### Step 5 — Convert SnapIt (conditional on D2)

Action: if A: switch `Settings`, `Theme`, `Layout*`, `ExcludedApplication*`
to snake_case; if D3 says so, switch the value enums too
(`MouseButton`, `HoldKey`, `HoldKeyBehaviour`, `MatchRule`, and
`geometry.rs` `SplitDirection`) and migrate their string values; migrate
`Settings.json`, `ExcludedApplicationSettings.json`, and every
`layouts/*.json` after a successful parse; rewrite the shipped files under
`apps/warp-snapit/packaging/windows/settings/snapit/`; update
`warp-station`'s preview reader
(`apps/warp-station/crates/warp-station-core/src/snapit.rs`) to read snake
keys and accept the legacy spellings for old layout files in the same change.
If B: no code change here; add the exception to the docs and to step 7's
check.

Check: `cargo test -p snapit` and
`cargo test -p warp-station-core snapit::` pass. Shipped defaults parse with
the new keys; old `Settings.json`/layout fixtures migrate with
`ScreensLayouts` monitor names untouched; enum values match D3;
`XButton1` is checked explicitly if values are converted. If `RunAsAdmin` is
kept, the test asserts it survives a settings rewrite.

### Step 6 — Update probes, fixtures and assertions

Action: replace PascalCase JSON literals in the station probes
(`settings_probe.rs`, `repos_probe.rs`, `split_probe.rs`,
`changes_probe.rs`, `search_probe.rs`, `tools_probe.rs`,
`warp-station-core/src/warp.rs`) with snake_case; change `json.contains`
assertions from `GroupReposByRoot` style to `group_repos_by_root`; keep one
legacy fixture per app that exercises the migration. Update the agent/snapit
tests in the same pass.

Check: `rg -n '"(Repos|General|OpenCode|WarpAgent|GroupReposByRoot|Pins|LayoutLines|DeactivedScreens)"' apps --glob '*.rs'`
returns only explicit `legacy` fixtures and migration code. The station probe
suite and `cargo test -p warp-station -p warp-station-core` pass. A small
fixture scanner (or a `jq` check in step 8) parses every committed settings
JSON and fails on an uppercase key outside the documented opaque maps, so a
single missed `json.contains` assertion cannot hide a PascalCase key.

### Step 7 — Docs and comments

Action: update `AGENTS.md`, `README.md`, `apps/warp-station/AGENTS.md`,
`apps/warp-agent/AGENTS.md`, `apps/warp-snapit/AGENTS.md`,
`apps/warp-notes/AGENTS.md` and the app READMEs where they describe the file
shapes. Replace "PascalCase schema" comments with the snake_case schema plus
the one-time migration and the D2 exception (if any). Document the downgrade
behavior and add a release-note line: the format change is one-way, and the
.NET SnapIt (if D2 = A) must be closed once while the Rust side migrates.

Check: `rg -n 'PascalCase' apps kits docs README.md AGENTS.md` only hits the
migration helper/tests, the SnapIt enum value serializers if D3 stays
keys-only, and historical notes; every settings-file description names the
snake_case keys.

### Step 8 — End-to-end migration test and full gate

Action: add a scratch-`HOME` integration test that copies legacy fixtures for
station settings, station repo cache, agent settings/ui and (if A) SnapIt
settings/excluded/layouts, runs each loader, and asserts values survive and
files are snake_case. Do not run it against the real `~/.warp`.

Check:
- the new test passes and a `jq` scan of the scratch files finds no uppercase
  ASCII in object keys outside the documented opaque maps;
- `cargo fmt --all --check` passes (run `cargo fmt --all` first);
- clippy with `-D warnings` passes for `warpui-kit-platform`,
  `warpui-kit-core`, `warp-station-core`, `warp-station`, `warp-agent`,
  `warp-notes`, `snapit`;
- `cargo test --locked` passes for those packages;
- the already-snake `~/.warp/ui.json` and `~/.warp/notes/settings.json` get a
  regression test (or an assertion in the kit/notes tests) so they stay
  snake_case;
- if SnapIt changed, `$HOME/.agents/skills/rust/scripts/check-windows.sh -p snapit`
  and `-p warp-station` are reported as type-checks, not builds.

The scratch-HOME test sets both `HOME` and `USERPROFILE` (Windows resolves the
data root from the latter) and takes turns behind the suite's existing
process-global env lock.

## 6. Risks and rollback

- **Data loss on migration bug.** Mitigated by parse-before-write, fixtures
  from the real files, and preserving unknown fields. A malformed or
  schema-incompatible file is left untouched.
- **Concurrent .NET SnapIt.** If D2 = A and the .NET app is open while
  warp-snapit migrates, the .NET app can write a PascalCase file back or read
  a snake file as defaults. Decision B avoids this; decision A documents
  "close the .NET app once".
- **Dropped unknown field.** `RunAsAdmin` in `~/.warp/snapit/Settings.json` is
  not modelled by Rust. A raw `Value` rewrite preserves it, but the next
  `SettingsStore::update` drops it. If D2 = A, add the field to `Settings` or
  accept the drop explicitly; do not claim it survives.
- **Downgrade.** Old builds fall back to defaults for migrated files. This is
  a one-way format change; no dual-write.
- **Dirty tree collision.** The current uncommitted auto-fetch revert touches
  the same station files and probes. Step 0 is mandatory.
- **Test churn.** The station probes contain many PascalCase literals; step 6
  is the bulk of the diff. It is mechanical, but it must not mask a migration
  bug: keep at least one legacy fixture per file.

Rollback: revert the commit; the migrated files remain valid for the new
build but not for older builds. Keep a copy of the pre-migration `~/.warp`
tree until the user confirms the new build.

## 7. Open questions

1. D2: is the .NET SnapIt still expected to read these files? If yes, SnapIt
   must stay PascalCase or change to dual-write.
2. D3: do enum *values* count as "snake_case", or only keys?
3. D1: is agent `ui.json` (client-side state, not settings) in scope?
4. Should the old files get a `.pre-snake-case` backup next to them, or is the
   atomic replace plus scratch-fixture tests enough?

## 8. Done when

- A scratch `HOME` seeded with legacy fixtures runs each loader once and every
  file in the table is snake_case, with every user value still present.
- The real `~/.warp` is untouched by tests; it migrates only when the user
  launches the app.
- The full gate in step 8 passes, and `rg`/the fixture scanner finds no
  PascalCase key outside the documented opaque maps and the D2 exception.
