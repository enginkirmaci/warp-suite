# Plan: settings and caches as a greenfield subsystem

Status: proposal, not implemented. Decisions D1-D5 are still open.

## 1. Premise

Design the settings/cache layer as if warp-suite were a new application:
define the contract first, build one store, then port each app to it. The
domain model has no legacy keys. Every persisted name is snake_case, every
file is versioned, and no code normalizes keys at runtime.

The existing `~/.warp` files are an **importer input**, not the design. The
importer lives at the edge, preserves user data, and can be removed after the
upgrade window. This replaces the earlier in-place casing migration; section 5
keeps that plan's file inventory as the importer contract.

## 2. Target design

### 2.1 One store

`warpui-kit-platform::config` — a new module, no gpui, no new crate unless the
API outgrows the module:

- `Store<T>`: one path, `load`, `save`, `update`; atomic replace; per-file
  sidecar lock for shared files; schema-version check.
- Every warp-suite-owned file starts with `"schema_version": 1`. If D1=A, the
  external .NET SnapIt files are adapter-owned and keep their existing format.
- `T: Default + Serialize + DeserializeOwned`; per-app settings/UI/cache
  types live in their app, the plumbing lives in the kit.
- Version newer than the binary: keep the file, report an error, do not
  rewrite. Version older: run the registered migration chain.
- Per-app files are strict (`deny_unknown_fields`) so a typo is a test
  failure. The shared `ui.json` is read-modify-write and preserves unknown
  keys and keys owned by other apps.
- Legacy importer types are permissive and separate from the live types.

`warp-snapit` deliberately has no `warpui-kit-*` dependency. It keeps a small
local store with the same contract; only the plumbing is duplicated, not the
schema.

### 2.2 Naming

- All object keys and string enum values are `snake_case` (D5 for values).
- One canonical vocabulary: `github`, `vscode`, `opencode`, `deepseek`,
  `snapit`, `azure_devops`, `mcp`, `nuget`.
- No abbreviations: `server_idle_stop_minutes` (not `_min`),
  `mcp_disabled_workspaces` (not `_ws`), `text_size` (not `font_scale`),
  `show_tooltips` (not `show_tool_tips`).
- The internal model fixes the .NET misspelling: `deactivated_screens`. Only
  the external adapter keeps `DeactivedScreens`.
- Opaque maps are newtypes: `SessionMap`, `WorkspacePathMap`,
  `MonitorLayoutMap`. Their keys are user data and are never normalized or
  case-folded.
- Rust field names follow the keys, so a few fields rename with the schema
  (`git_hub_*` -> `github_*`, `open_code_*` -> `opencode_*`, `snap_it` ->
  `snapit`). The compiler finds the call sites.

### 2.3 Files

New-app layout:

```text
~/.warp/station/settings.json
~/.warp/station/cache/repos.json
~/.warp/station/cache/models.json
~/.warp/agent/settings.json
~/.warp/agent/ui.json
~/.warp/agent/cache/models.json
~/.warp/notes/settings.json
~/.warp/ui.json
~/.warp/snapit/settings.json              # native
~/.warp/snapit/excluded_applications.json
~/.warp/snapit/layouts/*.json
```

- `cache/` holds only regenerable data under D4. Deleting it must never lose a
  user preference or a repo tag; if D4 is declined, that rule is documented as
  the one cache exception.
- Settings, UI state and caches have separate owners; a cache is never the
  only home of user data.
- File names are lowercase and have no `.cache` suffix.
- If the SnapIt adapter keeps the .NET files, the external names stay
  (`Settings.json`, `ExcludedApplicationSettings.json`) and the adapter is the
  only code that knows the PascalCase format.
- This pass does not split Warp Station's settings and UI state into two
  files; the schema doc records which keys are UI state. That split is a
  separate layout change.

### 2.4 External boundary

- Internal SnapIt model: `SnapitSettings`, `SnapitLayout`,
  `ExcludedApplication`, all snake_case and enum values lowercase under D5.
- If the .NET companion is still a supported reader, a `dotnet` adapter maps
  the internal model to/from `Settings.json`, `ExcludedApplicationSettings.json`
  and `layouts/*.json`. Warp Station reads layouts through the same adapter.
- The adapter must round-trip fields the Rust model does not use today
  (`RunAsAdmin`) instead of dropping them.
- If the .NET companion is retired, the adapter is deleted and the native
  snake files are the only format.

### 2.5 Importer

- One `legacy` module per app, run before the first `Store::load`.
- It reads the old files, maps them to the new schema, writes the new files
  atomically, and lets `schema_version` mark the job done.
- Caches are regenerated, except user data they wrongly own today: under D4,
  Warp Station lifts repo tags/favorites/archived from `repos.cache.json` into
  `settings.json`.
- Importer types are permissive; the live types stay strict. Fixtures are
  hand-written and sanitized; real `~/.warp` data is never committed.

## 3. Decisions

D1. **SnapIt boundary.** Is the .NET SnapIt still a supported reader?
  - A: yes — keep the external PascalCase format and add the adapter.
  - B: no — native snake_case files; delete the adapter.
  Recommendation: B. `AGENTS.md` now calls the .NET exe retired and the Rust
  `warp-snapit` port the suite's zone snapper; the importer still reads old
  .NET files. Choose A only if the .NET reader must stay, because the existing
  docs say the `DeactivedScreens` field stays.

D2. **Naming.** Canonical greenfield names (recommended, section 2.2) or a
mechanical snake_case of the existing Rust fields (`git_hub_*`,
`open_code_*`, `snap_it`)? Canonical means more field renames now and a
cleaner schema later.

D3. **File layout.** Adopt `cache/` and lowercase file names (recommended for
a new app), or keep the existing paths (`repos.cache.json`,
`models.cache.json`, `Settings.json`) to shrink the diff? The store and schema
are the same either way.

D4. **Cache purity.** Move repo tags/favorites/archived out of
`repos.cache.json` into `settings.json` (recommended; makes caches disposable)
or keep them in the cache this pass?

D5. **Enum values.** Convert string enum values too (`"left"`,
`"hold_to_enable"`, `"vertical"`; recommended for a new app), or only object
keys? The external SnapIt adapter translates to PascalCase either way.

## 4. Steps

### Step 0 — Preconditions

Action: note the unrelated uncommitted wand-draft changes in
`apps/warp-station/.../store/mod.rs`, `ui/readme.rs` and `ui/window.rs`; do
not revert them. They touch `store/mod.rs`, which step 4 edits, so reconcile
before that step. Confirm D1-D5; capture a baseline test run.

Check: `git status --short` is clean for the files this plan edits; the
baseline `cargo test` for the touched packages passes or its known failures
are written down.

### Step 1 — Write the schema contract

Action: add `docs/agents/settings-schema.md`: every file, key, type, default,
owner, opaque map, schema version and external adapter. No code.

Check: the doc covers every file in section 2.3 and every field the importer
must preserve; D1-D5 are recorded; review passes.

### Step 2 — Build the store

Action: add `kits/warpui-kit-platform/src/config.rs` with `Store<T>`, version
check, atomic save, lock, strict/preserve-unknown modes, and tests.

Check: `cargo test -p warpui-kit-platform config` passes; tests cover atomic
replace, lock contention, a newer schema version, a migration hook, unknown
fields in both modes, and an opaque-map round-trip.

### Step 3 — Notes and shared `ui.json` pilot

Action: define the notes schema and the shared `ui.json` schema; port
`UiPrefs` and `warpui-kit-core`'s `scale.rs` to the store; add
`schema_version`; keep `ui.json` read-modify-write.

Check: `cargo test -p warp-notes -p warpui-kit-core` passes; a scratch `HOME`
shows `schema_version` and snake keys; a test proves writing `text_size` keeps
a foreign `ui.json` key.

### Step 4 — Warp Station

Action: define `station/settings.json` and `cache/repos.json` /
`cache/models.json`; rename fields to the canonical names; move user tags
under D4; add the legacy importer; update `SettingsService`, `repo_store` and
`paths`.

Check: `cargo test -p warp-station-core -p warp-station` passes; an old
settings+cache fixture imports with tags, favorites, tabs and selected repo
intact; under D4, deleting `cache/` leaves the app usable; the importer is
idempotent.

### Step 5 — Warp Agent

Action: define `agent/settings.json`, `ui.json` and `cache/models.json`;
rename fields; model `SessionMap`, `WorkspacePathMap`; add the importer.

Check: `cargo test -p warp-agent` passes; old pins/favorites/drafts/
harness-by-session survive; session ids, folder paths and draft keys are
byte-identical; a second load does not rewrite.

### Step 6 — SnapIt

Action: if D1=A, implement the internal `SnapitSettings` plus the `dotnet`
adapter and route Warp Station's layout reader through it; if D1=B, implement
the native snake files and the importer. Update the packaging defaults.

Check: `cargo test -p snapit -p warp-station-core` passes; the adapter
round-trips a .NET fixture including `DeactivedScreens`, `ScreensLayouts` and
`RunAsAdmin`; the native path imports an old file; `XButton1` and
`SplitDirection` spellings match D5.

### Step 7 — Remove the old shapes, document, gate

Action: delete the old serializers and migration shims, update
`AGENTS.md`/`README.md`/app docs, add the fixture scanner, and run the full
gate. Keep only the importer and adapter as legacy boundaries.

Check:
- `rg -n 'PascalCase|camelCase' apps kits docs` hits only the SnapIt adapter,
  the importer fixtures, the D5 keys-only enum serializers (if D5 is
  declined) and the schema doc's historical note;
- `cargo fmt --all` then `cargo fmt --all --check` passes;
- clippy with `-D warnings` passes for all kit and app packages;
- `cargo test --locked` passes for those packages;
- a scratch-`HOME` end-to-end test imports every legacy fixture, runs each
  loader twice, and finds only snake_case keys/values outside the adapter;
- the real `~/.warp` is untouched by tests;
- if SnapIt changed, the Windows type-check script is run for `snapit` and
  `warp-station` and reported as a type-check, not a build.

## 5. Importer contract

| Old file | New home | Must preserve |
|---|---|---|
| `station/settings.json` | `station/settings.json` | scan roots, executables, tabs, selected repo, widths, toggles |
| `station/repos.cache.json` | `station/settings.json` (tags, D4) + `station/cache/repos.json` (status) | favorites/archived/user tags; status may regenerate |
| `station/models.cache.json` | `station/cache/models.json` | regenerable; import optional |
| `agent/settings.json` | `agent/settings.json` | executables, harness/model defaults, auto-allow, hours |
| `agent/ui.json` | `agent/ui.json` | pins, favorites, workspaces, sessions, drafts, per-session harness, deleted chats |
| `agent/models.cache.json` | `agent/cache/models.json` | regenerable |
| `notes/settings.json` | `notes/settings.json` | list width, expanded folders, open note, scope |
| `ui.json` | `ui.json` | `text_size`, `palette`; other keys preserved |
| `snapit/Settings.json` | adapter (A) or importer (B) | theme, mouse/hold, screen maps, `DeactivedScreens`, `RunAsAdmin` |
| `snapit/ExcludedApplicationSettings.json` | adapter (A) or importer (B) | exclusion rules |
| `snapit/layouts/*.json` | adapter (A) or importer (B) | geometry, names, GUIDs |
| `snapit/packaging/...` | new defaults | shipped settings/layouts |

## 6. Risks

- **Scope.** Canonical names, `cache/` paths and cache purity are more than a
  casing change. If the goal is a small diff, choose mechanical names and keep
  the old paths; the store still gives the versioning and one-writer shape.
- **External adapter.** The .NET adapter must round-trip unknown fields and
  keep `DeactivedScreens`; a fixture with `RunAsAdmin` is the guard.
- **Importer data loss.** Parse before write, sanitized fixtures, idempotent
  marker, and a pre-migration `~/.warp` copy until the new build is confirmed.
- **Downgrade.** The new schema is one-way. An older build sees defaults and
  can overwrite the file; document it and keep the backup.
- **Dirty tree.** The uncommitted wand-draft changes touch `store/mod.rs`,
  which step 4 edits; step 0 is mandatory.

## 7. Done when

- The schema doc is the source of truth and every persisted key/value is
  snake_case except the explicit SnapIt adapter.
- Every warp-suite-owned file has `schema_version: 1`; caches are disposable
  under D4; shared `ui.json` preserves keys it does not own.
- The importer has moved every legacy fixture without losing user data, and
  the full gate passes.
