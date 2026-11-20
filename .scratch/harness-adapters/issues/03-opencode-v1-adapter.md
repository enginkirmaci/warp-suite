# 03: opencode v1 adapter

**What to build:** opencode v1 runs through the harness interface with today's
behavior: one managed server per workspace, the same readiness and reconnect
behavior, catalogs, sessions, prompt streaming, permissions and questions,
slash commands and diffs. Existing v1 chats and probes do not change.

**Blocked by:** 01, 02

**Status:** resolved

**Category:** enhancement

- [x] v1 starts and stops through the adapter; attach-to-healthy, the readiness wait and the event pump keep their behavior.
- [x] Models, agents, commands, session list/create/fork/delete, prompt streaming, permissions/questions and diffs reach the store through the typed stream.
- [x] Chat behavior and every existing v1 probe stay green.
- [x] The host sees v1's health and capability state through the interface.
- [x] The adapter is reached by the stable `opencode` id; no new v1-specific variant appears on the shared store or door surface.

## Comments

Implemented in warp-agent `ed03e1f` ("feat(agent): run opencode v1 through the harness adapter", version 0.1.20).

- `OpenCodeV1Harness` implements the wide trait over the existing `ServerProcess`/`ApiClient`: attach-to-healthy, the 25 s readiness wait, the SSE pump with the reconnect ladder, a 10 s health poll and the idle stop. Catalogs (providers, models, agents, commands, skills), sessions (list/get/create/rename/delete), prompt and command sends, interrupt, compaction, permissions, questions-as-forms, diffs and MCP read go through the adapter.
- The pump translates opencode v1 `/event` frames into the typed vocabulary (`session.*`, `message.updated`, `message.part.updated/removed`, permission and question asks/replies, `session.error`), with the text/reasoning/tool/file part payloads and token/cost metadata. Pending permissions and questions are tracked per session so the list methods answer from the stream.
- Instance scope is now a descriptor fact: `InstanceScope::Workspace` for v1, `App` for dsh/v2. `HarnessHost` keys instances by `(id, workspace)` and exposes `instance_at` / `with_instance_mut_at`; a second v1 folder is a second instance, a duplicate folder is a typed error. The v1 factory is on the `opencode` descriptor.
- Sends are fire-and-forget on the adapter runtime (v1's POST resolves only when the turn ends), so an approval or interrupt can still reach the instance during a turn. A missing executable reports a typed start failure and leaves health `Unavailable`; the health poll now watches the child handle and reports an early exit.
- Review fixes: `HarnessKind::from_wire` replaced the duplicate id match; providers come from `/config/providers` (display names preserved); compact refuses without a model like the app; `translate_event` returns `Option`; permission/question replies no longer require a session id; MCP management stays `Unsupported` while `mcp_read` is advertised; the diff copy uses the same filediff shape.
- A store-seam test translates real v1 frames and feeds `Store::ingest_harness_event`, proving prompt streaming, session/title and idle reach the tile. Linked-suite gates pass: `cargo fmt --all --check`, `cargo clippy -p warp-agent --all-targets -- -D warnings`, `cargo test -p warp-agent` (101 lib tests plus the probes).
- Known follow-ups in their own tickets: the host-owned runtime and non-blocking boot land with the settings/host ticket 05 (the adapter is self-contained until then); the shared `oc.rs` pump extraction and the old path's removal are ticket 14; the fake v1 HTTP/SSE conformance suite is part of ticket 15. v1 has no server-side fork: the descriptor reports `fork: false` and `fork_session` returns `Unsupported`; the app's existing seed-the-composer fork remains the UI flow.
