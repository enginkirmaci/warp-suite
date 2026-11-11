# Plan: opencode `/compact` from the first second

Status: implemented in this tree, 2026-11-11; gates green, awaiting the
owner's review and commit. Companion to
[`dsh-chat-commands.md`](dsh-chat-commands.md): dsh gets its catalog from
`commands/list`, opencode needs an app-owned command.

## 1. Why

opencode 1.19.5's `GET /command` returns `init`, `review`, config commands,
MCP prompts and skills. `/compact` is a TUI command (`session.compact`); the
TUI runs it as `POST /session/{id}/summarize` with the selected
`providerID`/`modelID`. A probe returned the same 40 commands before and after
a completed turn, with no compact, so no amount of waiting or refreshing can
add it to the server list.

warp-agent used `GET /command` as the whole slash menu, so the menu could
never show compact. While a workspace server was starting, the menu also said
`no server commands` with no loading state.

## 2. Target

- Every opencode chat lists `compact` immediately, before the workspace
  server answers.
- `/compact` runs the real summarize endpoint, not a model prompt; it is
  guarded when the chat has no history, is busy, or has no model.
- The server list still supplies `init`, `review`, skills and MCP prompts; a
  custom server command named `compact` wins over the local one.
- No empty session is created just to tell an untouched draft that there is
  nothing to compact.

## 3. Design

### 3.1 The app-owned command

- `Store::ensure_local_opencode_commands(ws)` inserts
  `compact` (`source = "local"`, description "Compact older conversation
  history") into the display map if the name is absent.
- It runs when the slash menu opens (before `ListCommands` is sent) and again
  after `CommandsLoaded`, so the row is visible during the first load and
  survives the server answer. A server-provided `compact` is kept, not
  duplicated.
- `tile_commands` stays a borrowed slice; the menu needs no per-keystroke
  allocation.

### 3.2 Dispatch

`send_core` parses the slash name before `parse_slash_command`:

- harness opencode and name `compact` → `compact_opencode_session`, never
  `/session/{id}/command`.
- guards, each a `SystemNote`:
  - arguments → `Usage: /compact (no arguments)`
  - no session or no message → `Nothing to compact yet — send a message first.`
  - a running turn → `Wait for the current turn to finish.`
  - no real model → `Pick a model before compacting.`
- the model is the tile's real `selected_model`, else `last_used_model` (the
  provider/model of the last assistant message).
- `send_from_tile` catches an untouched draft before the session-create
  branch, so `/compact` on a brand-new chat creates nothing and only notes
  that there is no history.
- a command adds no optimistic user bubble: opencode's summarize writes its
  own compaction messages.

### 3.3 The wire

- `ApiRequest::CompactSession { session_id, model }` goes to the session's
  workspace server.
- `ApiClient::compact_session` splits `provider/model` and POSTs
  `/session/{id}/summarize` with
  `{ providerID, modelID, auto: false }`, generous timeout.
- `UiEvent::SessionCompacted` → `Context compacted.` on success, an error
  note with the server text on failure.
- The transcript updates from the normal event stream: opencode writes a user
  message whose only part is `compaction` (already invisible in warp-agent)
  and an assistant message with `agent = mode = compaction`,
  `summary = true` (its text renders as the summary).

## 4. Verification

- Store test `an_opencode_chat_offers_compact_and_runs_it`: compact is in the
  menu while `CommandsLoaded` is in flight, appears once after a server list
  without it, `/compact` sends `CompactSession` with the tile's model and
  adds no user message, the success note lands, and an untouched draft creates
  no session.
- Live `opencode serve` probe (1.19.5): `POST /session/{id}/summarize` with
  `providerID`/`modelID` returned `true` and wrote the compaction user +
  assistant pair; `/command` never listed compact.
- `cargo test -p warp-agent` (60 lib tests plus probes), `cargo fmt --all`,
  and clippy with `-D warnings` are clean.

## 5. Notes / follow-ups

- opencode 2.x exposes `POST /api/session/{id}/compact` instead; warp-agent
  pins 1.19.5, so no version fallback is added now.
- A dedicated "Context compacted" divider for the compaction assistant (and
  folding the summary) is a presentation follow-up; the current transcript
  shows the summary as an assistant reply plus the app note.
- The local list deliberately carries only `compact`. The TUI's other
  client-side commands (`undo`, `share`, `export`, …) have no wire equivalent
  here and are not implied by this change.
