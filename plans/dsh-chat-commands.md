# Plan: dsh chat commands — `/compact` from the first second

Status: implemented in this tree, 2026-11-11; gates green, awaiting the
owner's review and commit. The reported chat is the **dsh** harness; the
opencode counterpart is
[`opencode-compact-command.md`](opencode-compact-command.md).

## 1. What the app does today

- The dsh slash menu is filled from `skills/list` only (`dsh.rs::skills`).
  That call needs a followed session (`any_session()`), so a fresh chat lists
  nothing until its first send creates one.
- dsh has a separate **command** registry. `commands/list` returns the builtin
  commands for an agent: `compact`, `export`, `feedback`, `goal`,
  `permission`, `plan`. The app never asks for it, so `/compact` is never in
  the menu even after the first answer.
- Slash text sent to dsh goes out as a normal prompt (`dsh::send_message`);
  dsh's command registry is not reached. A typed `/compact` therefore does
  not compact the chat; `commands/execute` is the endpoint that does.
- There is no loading state in the menu: before `CommandsLoaded` it says
  `no server commands`.

A probe of `dsh web` (same RPC envelope the app uses) confirmed both facts:
`commands/list` with a real session returns `compact, export, feedback, goal,
permission, plan` and errors for a non-existent session; `skills/list` returns
the workspace skills. `commands/execute` on an empty session answers
`No compactable history yet.` (success) and `/compact now` answers
`Usage: /compact (no arguments)` (error). Both are scoped to an agent
(session id), not the server.

## 2. Target

- A dsh chat's slash menu lists the server's commands (including `compact`)
  and its skills from the **first** `/`, not after the first answer.
- `/compact` executes through dsh's `commands/execute` and its result lands as
  a transcript note. It works before any history by returning dsh's own
  "No compactable history yet." text instead of starting a model turn.
- No empty chats appear just because a chat was opened: the dsh session is
  created lazily when the slash menu is first opened, and the same session is
  reused by the first send.
- A starting server shows `loading commands…`, never a false "no commands".

## 3. Discovery

`dsh.rs` gains `commands(handle, session_id)`:

- `commands/list` with the **flat** args `{ "agentId": <session> }` (the
  gateway rejects the nested `request` shape that `skills/list` uses).
  Parse `name`, `description`, `input.hint`; source `"command"`.
- `skills/list` with `{ "request": { "sessionId": <session> } }`; source
  `"skill"`.
- Merge commands first (dsh returns them name-sorted), then skills sorted by
  name; dedupe by name with the command winning.
- If `commands/list` fails (older dsh), log and still return the skills rather
  than losing the whole menu.

`ApiRequest::ListDshCommands { session_id: Option<String> }` carries the
tile's session. `dsh::run_request` answers with `UiEvent::CommandsLoaded`
under the dsh key. The opencode `ListCommands` path is untouched.

## 4. The session before the first send

A dsh draft has no session, and both RPCs need one. On the first `/` in a dsh
draft:

1. `refresh_commands` marks the dsh key loading and asks for a session.
2. If the shared `dsh web` is not up, `ensure_dsh` starts it and the tile
   waits in `pending_command_tiles`; `on_dsh_started` drains that set.
3. `ApiRequest::CreateDshCommandSession { tile_key, folder }` creates the
   session, and `UiEvent::DshCommandSessionCreated { tile_key, result }`
   adopts it into the tile (session id, title, workspace/session maps, sidebar
   entry) without a pending send.
4. The store then requests `ListDshCommands { session_id: Some(id) }`.
5. The first real send finds `tile.session_id` set and sends normally, so the
   eager session is not wasted.

The session is created only when the user opens `/`, not when the tile is
created, so switching harness before asking for commands leaves no orphan.

## 5. Execution

- `send_core` checks the matched `CommandInfo.source`. For a dsh
  `"command"`, it calls `ApiRequest::ExecuteDshCommand { session_id, line }`
  instead of `SendMessage`, and adds no optimistic user bubble (dsh commands
  are log-only, they never become model messages).
- `commands/execute` takes the flat args
  `{ "agentId", "line", "submittedAttachments": [] }` and returns
  `{ commandId, result: { kind: "success"|"error", text? } }`.
- `UiEvent::DshCommandExecuted { tile_key/session_id, result }` becomes a
  `SystemNote`: success text as-is, error text with `is_error`.
- `compact` with no history is not special-cased: dsh's handler returns
  `No compactable history yet.` and the note shows it. A busy turn returns
  dsh's busy message.
- Skills keep the current prompt path; only `source == "command"` bypasses
  `send_message`.
- If the user sends `/compact` before the menu's session exists, the normal
  draft-create path sets the session, then `send_core` runs the command after
  the create; `compact` is also accepted when the list has not arrived yet so
  the command never falls through to the model.

## 6. Loading and refresh

- `commands_loading: HashSet<String>` and `commands_loaded: HashSet<String>`
  keyed by net key.
- `refresh_commands(tile_key, cx)` marks loading, keeps the old list, and
  starts the dsh session when needed.
- `CommandsLoaded` stores, clears loading, marks loaded.
- Request again on `session.idle` for dsh (cheap; skills/commands can change)
  and after a command session is created.
- Menu rows: `loading commands…` while loading and empty; `no server commands`
  only after a loaded empty answer.

## 7. Tests

- dsh unit test with a fake HTTP server: `commands()` sends the flat
  `agentId`, merges commands + skills in the right order and keeps skills when
  `commands/list` fails.
- Store test: a dsh draft reports `loading commands…` on the first `/`, not
  "no server commands"; a `CommandsLoaded` reply with compact makes
  `tile_commands` return it.
- Store test: `/compact` on a dsh tile goes to `ExecuteDshCommand` (not
  `SendMessage`), adds no user message, and the result note is written.
- Gates: `cargo test -p warp-agent`, `cargo fmt --all`, clippy.

## 8. Out of scope

- opencode's `/compact` (client-side TUI command; separate change).
- dsh command attachments (`submittedAttachments` stays empty; `compact`,
  `plan` and `goal` text input work).
- Rendering a dedicated compaction divider from dsh's `surfaceOp replace`;
  the transcript already prunes the replaced range.

## 9. Verification

- `cargo test -p warp-agent`: 60 lib tests plus every existing probe pass;
  `cargo clippy --locked --all-targets -p warp-agent -- -D warnings` and
  `cargo fmt --all` are clean.
- New store test `store::command_flow_tests::a_dsh_draft_opens_its_catalog_and_runs_compact`:
  first `/` starts dsh, asks for a session, lists the catalog, marks loading
  until it lands, and sends `/compact` through `commands/execute` with no
  model message.
- New dsh tests with a fake HTTP server lock the flat `agentId` args, the
  commands + skills merge, and the error text (`command_error_text_reaches_the_caller`).
- Live `dsh web` probe: `commands/list` returned `compact, export, feedback,
  goal, permission, plan`; `commands/execute /compact` answered
  `No compactable history yet.` and `/compact now` answered
  `Usage: /compact (no arguments)`.
- App smoke (debug build, hidden workspace 99, scratch HOME with the owner's
  dsh credentials): the door opened a dsh chat with `/compact` and auto-send;
  the app created a dsh session and ran the command, with no model turn.
  The hidden window does not redraw, so the menu itself is covered by the
  headless store test rather than a screenshot. The scratch dsh profile lists
  `deepseek-official/deepseek-flash` and `deepseek-v4-pro`;
  `deepseek-v4.1-flash` is an `opencode-go` model, not a dsh one.
