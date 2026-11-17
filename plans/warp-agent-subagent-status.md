# Subagent status in warp-agent

**Status: proposal — no code written yet. Four decisions still open (below).**

Reported: warp-agent does not show the status of a subagent *inside* a
subagent. The store already tracks one level of subagent runs, but the model is
flat (child → one parent), the live discovery for dsh is missing, and every
status surface except the chat's own transcript is one level deep.

## Open decisions

1. **Which reading is the bug?** The report fits three cases. This plan fixes
   A and B together because they share the same root cause; C is follow-up.
   - A. Opening a subagent's chat as a tile shows nothing running: it has no
     `Running` chip, no spinner in the sidebar/tile, and is not counted in the
     OS title (`is_busy` only reads `running_by_session`, and subagent
     sessions are never put there).
   - B. A chat's `Tasks & agents` flyout lists only runs whose *direct* parent
     is that chat, so a grandchild run never appears anywhere; and the list is
     not rebuilt when a tile opens, so an already-running child can be missing
     until some event flips.
   - C. Runs that already finished are not kept at all (rows disappear on
     idle), so there is no done/failed status to read afterwards.

   **Recommended:** A + B now, C as a later slice; say if C is the actual
   complaint and the retention design moves into slice 3.

2. **Flyout scope.** Show the chat's **whole live subtree** (pre-order,
   indented) rather than direct children only. A subagent chat's own tile then
   shows its children, and the root chat shows the whole run.
   **Recommended: whole subtree.** The alternative (direct children + a “via
   <parent>” line) keeps the list short but still hides depth ≥ 2 from the
   root chat.

3. **Subagent sessions in the sidebar.** dsh's `session/list` returns
   subagent-origin rows with the parent's `cwd`, so today they can enter
   `ws.state.sessions` and look like ordinary chats. **Recommended:** hide
   `origin: subagent` rows from the sidebar and reach them only through the
   agents tree. Alternative: keep them and mark them as subagent chats.

4. **OS title count.** `2 running — Warp Agent` counts chats, not runs.
   **Recommended:** keep counting root chats, plus a subagent only while its
   own chat tile is open (that chat is what the window shows). Do not inflate
   the count with hidden subagent runs.

## What the app knows today, and where it leaks

All of this is in `apps/warp-agent` and is checked into `main`. Line numbers are
from the current working tree, which also carries another session's worktree
edits; re-read a cited line before cutting a patch.

| Concern | Today | Leak |
|---|---|---|
| Child → parent map | `subagent_by_session` filled from lists and opencode `session.updated` (`src/store/runs.rs:149`, `src/store/sessions.rs:22`) | A dsh `session/title` fold emits `session.updated` with no parent (`src/dsh.rs:1196`), and the store reads “no parent” as “not a subagent”, so any title update wipes the mapping and the row (`src/store/runs.rs:158`). |
| Live discovery | dsh `$events` folds only `api-session/status` and `api-session/error` (`src/dsh.rs:1594`) | `api-session/added` (full `SessionSummary`: `sessionId`, `parentSessionId`, `origin`, `running`) and `api-session/removed` are dropped. A child created and finished inside one parent turn is never seen. |
| List rows | `SessionInfo` carries id/title/timestamps/parent only (`src/oc.rs:48`); dsh's `list_sessions_filtered` reads `parentSessionId` only (`src/dsh.rs:798`) | The list's `running` and `origin` are ignored, so a running subagent discovered by a list gets a mapping but no row until its next status flip. Every list also re-stamps the label to the literal `subagent`, overwriting the agent name from live events (`src/store/runs.rs:163`). |
| Running set | `set_running` refuses anything that is not an app session and never marks subagents (`src/store/runs.rs:90`) | `ChatTile::is_busy` (`src/store/tile.rs:186`), `Store::running_chats` (`src/store/mod.rs:1469`) and the sidebar's Running scope all miss subagent sessions, so an opened subagent chat looks idle while it streams. |
| Tile flyout | `refresh_tile_agents` filters on the direct parent only (`src/store/runs.rs:247`); called from status changes, never from tile open | Depth ≥ 2 is invisible; a running child is missing until the next event. |
| Opening / stopping / attention | `effective_workspace_path` climbs one level (`src/store/mod.rs:1526`); `open_subagent_session` (`src/store/runs.rs:292`), `mark_attention` (`src/store/runs.rs:72`), `drop_subagent_rows` (`src/store/runs.rs:258`) are all one-level; dsh's Abort is a plain `session/cancel` (`src/dsh.rs:664`) | A depth ≥ 2 row can render but clicking it opens nothing. An ask whose own session has no row falls back only one level, and a workspace removal clears one level. Stop may be rejected on a child session: dsh's own client cancels a subagent through `subagents/interruptByUser(childSessionId)`, not `session/cancel`. |

dsh facts this plan relies on (checked against the installed 0.2.2-rc.2):

- `SessionSummary` = `{sessionId, updatedAt, running, blank, parentSessionId?,
  origin?: 'subagent', cwd?, projections?}`. Child sessions are minted with a
  bare UUID id; the parent reference is the parent's `session-<uuid>` id.
  **Treat both as opaque strings, never normalize a prefix.**
- `api-session/added(summary)` is an emit on `$events`; `api-session/status`
  is already mapped.
- `session/list` includes subagent-origin rows (the web client indents them),
  and its `projections.subagentCatalog` names a parent's children with labels
  if the list rows ever prove insufficient.

## Intended shape

One run record per tracked session, two projections:

- **State:** `AgentRun { session_id, parent: Option<String>, label, title,
  started_ms, status }`, plus `running_by_session` as the single authority for
  “this session's agent is working”. `status` is derived: running +
  pending ask → *Needs you*; running → *Running*; otherwise *Idle/Done*.
- **Projection 1 (tree):** `tile_agents` becomes the chat's whole live subtree
  in pre-order, each row carrying `depth`, the parent's title, and a status
  word. Built from the parent map, never filtered by `== direct parent`.
- **Projection 2 (chats):** the sidebar keeps root chats only; a subagent
  session never becomes a `SessionItem` (decision 3).

Status shows wherever a chat is shown, following the design-system motion
rule: a word everywhere, a spinner only beside it, an accessible name and a
tooltip on the animated element, and the same word under reduced motion.

- Flyout row: indent by `depth`, status word (Running / Needs you), agent label
  and title, stop button, `aria_label` naming the agent and its parent.
- Opened subagent chat: its tile head shows the existing `Running` chip, its
  flyout shows *its* children, and the sidebar does not add it as a chat.
- Root chat: the flyout shows the whole run; the summary count is the number
  of live rows.
- Clicking any row opens that session's chat (climbing to the owner workspace
  when the child has no direct workspace entry); Stop aborts that run.

## Slices

Each slice is independently shippable and testable; slices 1 and 2 together
already fix A and most of B.

### 1. Wire model: keep parent, running and origin

- `SessionInfo` gains `running: bool` and `origin: Option<SubagentOrigin>` (or
  `parent_id.is_some()` + origin flag); both harnesses fill them —
  opencode from `parentID` (running comes from events today, so `false` there
  is acceptable if the list does not say), dsh from `parentSessionId`,
  `origin`, `running`.
- `dsh::fold_emit` maps `api-session/added` to an opencode-shaped
  `session.created`/`session.updated` carrying `info.parentID`, `info.title`
  (when present) and the row's `running`; maps `api-session/removed` to a
  “session gone” event that clears rows. dsh's title-only `session.updated`
  must not be able to clear a parent: distinguish *parent unknown*
  (field absent, e.g. a title event) from *explicitly no parent* (a list or
  `added` row). Additive updates only.
- `apply_session_list` seeds `set_subagent_running(child, row.running)` from
  the list instead of waiting for a flip, and no longer overwrites a known
  agent label with `subagent`.
- Keep `refresh_dsh_lists` as is; with `api-session/added` the tree should no
  longer depend on list timing.

Tests: store unit tests fed synthetic dsh folds — a child adopted from an
`added` frame before any list, a title event that must not drop the parent, a
list that seeds a running child, a nested pair (`S → T`).

### 2. One running set, status everywhere

- Let `set_running` mark any known session (app session or tracked subagent);
  the *row* logic, not the running set, decides chat vs flyout row.
- `ChatTile::is_busy` and the tile-head chip then work for an opened subagent
  chat with no extra path.
- `running_chats` (OS title) per decision 4; `update_sidebar_running` no-ops
  for sessions that are not sidebar chats.
- `session.idle` still ends the run and drains queues. The idle path may keep
  refreshing the chat list, but discovery must no longer depend on it.

Tests: a probe asserting the tile head shows `Running` for a running subagent
session and the OS title stays at the root count when a hidden subagent runs.

### 3. Tree instead of a flat list

- Compute depth and descendants from `subagent_by_session`; break cycles
  defensively (a session naming itself or a loop must not hang).
- `refresh_tile_agents` builds each tile's rows from the subtree of the tile's
  session, sorted pre-order (depth, then `started_ms`), and is also called when
  a tile is opened, closed and attached to a session.
- Rows carry `depth`, `parent_title`, `is_subagent` (keep), `status`.
- Cap the visible tree for very wide runs (e.g. first N rows + “+k more”) so a
  big fan-out does not turn the flyout into a scroll wall; the cap is a
  constant beside the flyout.

Tests: pure store test over `S → {T1, T2}`, `T1 → U` asserting pre-order,
depth, titles and that a parent's own row is not duplicated in its child list.

### 4. Any depth is reachable and stoppable

- `effective_workspace_path` climbs to the owner (`owner_session_id` already
  exists) instead of one level.
- `open_subagent_session` opens any tracked depth; a depth ≥ 2 chat loads
  through the owner's server key (dsh: the shared key, so no workspace entry is
  needed; opencode: the owner's folder).
- `mark_attention` and `drop_subagent_rows` use the owner/descendant walk, so
  an ask from depth 2 lights the run that owns it and a removed workspace
  clears every descendant.
- Stop routes by target: a tracked subagent on dsh goes through
  `subagents/interruptByUser(childSessionId)` (owner-only, which this app is),
  with `session/cancel` as the root-chat path; keep one `ApiRequest::Abort`
  and let `dsh.rs` pick the method from the parent map it already mirrors.
  Opencode keeps its own abort.

Tests: a store test opening a depth-2 session (asserts the request went to the
right net key), one marking attention from a depth-2 ask, and a dsh
`run_request` test proving a subagent abort sends `subagents/interruptByUser`
while a root chat still sends `session/cancel`.

### 5. Flyout UI

- Indent the row by `depth` (a small fixed step; no new colour or font size —
  read the kit tokens through `theme`), show the status word per decision 1,
  keep the row's open/stop exclusivity and the existing hover/rounded shape.
- `aria_label` names the agent, its status and its parent; the status spinner
  gets its own accessible name and tooltip.
- Section header summary becomes the live-row count (“3 running”, “1 needs
  you” when any row waits).
- The `Tasks & agents` toggle appears when the subtree or the tasks are
  non-empty (unchanged rule, wider source).

Tests: extend the real-window probe style already used in
`tests/pane_moves_probe.rs`/`tests/sidebar_keys_probe.rs` — seed a root tile
with a two-level tree and assert the flyout shows both rows, indented, with the
status words and the right aria labels.

### 6. Sidebar hygiene (decision 3)

- Do not insert `origin: subagent` sessions into `ws.state.sessions`, and do
  not render them as chats; keep them in the parent map and reachable from the
  tree.
- A subagent session that was already adopted by an older build is dropped
  from the sidebar on the next list without touching its tile, if open.
- The door's `status` payload is unchanged — subagents are not chats and Warp
  Station must see no new rows.

Tests: a list-apply test asserting a subagent row does not enter
`ws.state.sessions` but does enter the tree.

### 7. Docs and close-out

- One `AGENTS.md` bullet in `apps/warp-agent/AGENTS.md` under Transcript or a
  new “Subagents” note: the parent map is additive, dsh's `api-session/added`
  is the live source, the tree is per tile, and a subagent chat's status comes
  from the same running set.
- Delete this plan when the slices land, or fold the surviving rules into the
  app's docs.

## Verification

```sh
cargo test -p warp-agent
cargo build -p warp-agent
cargo fmt --all --check
cargo clippy -p warp-agent --all-targets -- -D warnings
```

Manual check against a real dsh chat: start one and prompt “spawn a subagent
which spawns its own subagent and make both run for a while”. Expected: the
root chat's flyout shows both rows indented with `Running`; the middle
subagent's own chat shows its child and its own `Running` head; closing and
reopening the middle chat keeps the child row; Stop on the grandchild aborts
only it; a permission ask from the grandchild marks the row that owns it.

Do not change the door contract in this work: it stays the same commands and
fields, and Warp Station is untouched.

## Risks and out of scope

- `api-session/added` payload shape: confirmed from the installed host's event
  signature (one `SessionSummary` argument) but not yet captured in this app's
  logs. Capture one real frame before slice 1 lands; if the emit arrives with
  the summary nested differently, fold it in `fold_emit` only.
- dsh child ids are bare UUIDs while parent ids are prefixed; mixing the two
  would silently break the map. Never strip or add `session-`.
- Opencode parity: opencode already sends `parentID` on session events, so the
  tree should work there unchanged; verify one opencode run before shipping,
  and do not add a dsh-only rendering path.
- Subagent cancellation is the least certain endpoint: dsh's own web client
  uses `subagents/interruptByUser` for a child, but `session/cancel` has not
  been tried against a child. Probe both once (send each and read the error);
  keep whichever the host accepts, with the other as a fallback only if the
  failure is a clean `not-found`/`unsupported` code.
- Out of scope: the full dsh Tasks page (graph, workflow runs, agent teams),
  the `subagents/*` remote namespace, persisted run history with outcome text,
  and any change to how a subagent's own transcript renders.
