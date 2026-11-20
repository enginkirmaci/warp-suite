# 04: dsh adapter

**What to build:** dsh runs through the harness interface with today's
behavior: one managed `dsh web` process, launch-token and cookie auth,
WebSocket streams, event folding, and the dsh title/permission/question/command
behavior. Existing dsh chats and tests do not change.

**Blocked by:** 03

**Status:** resolved

**Category:** enhancement

- [x] dsh starts and stops through the adapter; the singleton and its token/cookie flow are preserved.
- [x] dsh events fold into the typed stream; titles, permissions, questions, commands and diffs behave as today.
- [x] Existing dsh tests, chat behavior and probes stay green.
- [x] The descriptor reports dsh's capabilities, including the operations it does not support.
- [x] The adapter is reached by the stable `dsh` id; the old `"dsh"` workspace sentinel does not leak into the host.

## Comments

Implemented in warp-agent `acf485d` ("feat(agent): run dsh through the harness adapter", version 0.1.21).

- `DshHarness` starts one managed `dsh web` via `ServerProcess`, takes the launch token, builds the existing `DshHandle`, and runs the existing `run_pump` over a WebSocket. A forward task translates the pump's opencode-shaped frames to the typed vocabulary; health, child-exit and stream-status changes feed `Health`/`Connected`/`Disconnected`. The adapter is app-wide (`InstanceScope::App`) and the old `"dsh"` net key stays internal to the pump.
- Catalogs: providers from `session/modelCatalog`, models/agents through the existing dsh helpers, commands/skills through `commands/list` + `skills/list`. Sessions list every folder with its `cwd`, and create/rename/archive/cancel use the dsh RPCs; `compact` runs the dsh `/compact` command.
- Permissions/questions are tracked from the typed stream and answered through `answer_approval`/`answer_question`/`reject_question`. Diffs read dsh's `metadata.diffs` hunk lists and turn them into kit patches, not opencode's `filediff` shape. Command results are the typed `CommandOutput` event and land as the store's command note.
- The descriptor reports dsh's real surface: `mcp_read`/`mcp_management` false and `fork: false`. dsh has no server-side fork; the adapter returns `Unsupported` and the app's existing seed-the-composer fork remains the UI flow.
- Review fixes: shared `PendingApprovals`, `form_answers`, `partition_commands` and `filediffs` in `harness/shared.rs`; `dsh::list_sessions` replaces the copied session walk; `CommandOutput` carries a `Result` instead of a `failed` bool; `multiSelect` questions and dsh `error.message` text survive translation; `AGENTS.md` and the harness module docs name the adapters.
- Tests: 107 lib tests plus the probes; `cargo fmt --all --check`, `cargo clippy -p warp-agent --all-targets -- -D warnings`, `cargo test -p warp-agent`.
- Known follow-ups in their own tickets: the host-owned runtime and non-blocking boot are ticket 05; the fake `dsh web` conformance server is the shared suite in ticket 15; the old dsh net path is removed in ticket 14.
