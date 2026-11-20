# 02: Typed harness event stream and store ingestion

**What to build:** A single typed event stream per harness instance that the
store can consume to drive a chat, and an ingestion path that keeps the
existing transcript, tool-call, permission and question state working. A fake
harness can take a prompt, stream text, raise a permission prompt and go idle
without any network or child process. The old event path still feeds the app
until the contract ticket.

**Blocked by:** 01

**Status:** resolved

**Category:** enhancement

- [x] `HarnessEvent` covers server/health, session status/idle, session changes, message/part snapshots, tool calls, permissions, questions/forms, catalogs and structured errors.
- [x] The store consumes the typed stream into the existing tile and transcript state; a fake harness drives a full chat: prompt, streamed text, a tool call, a permission prompt, idle.
- [x] The transcript stays part-shaped and virtualized; the existing transcript and layout tests pass unchanged.
- [x] Events are routed to the right session; no event leaks across sessions.
- [x] The old event path and the typed path can coexist behind a switch until the contract ticket.

## Comments

Implemented in warp-agent `b4ea143` ("feat(agent): ingest typed harness events into the store", version 0.1.19).

- `HarnessEvent` now carries the typed vocabulary: connection/health, `SessionCreated` / `SessionChanged` / `SessionDeleted` / `SessionStatus` (idle, busy, retry), message and part snapshots, tool calls (state, input, title, metadata), files, permissions and replies, questions/forms, model/agent/command/skill catalogs, and structured errors with a `retryable` flag.
- `PartPayload` makes the part variant decide which payload exists (`Text`, `Reasoning`, `Tool`, `StepStart`, `StepFinish`, `File`); message snapshots carry token totals and cost so the header meta and token pace still render.
- `store::harness_events` bridges typed events into the existing stream handlers behind `Store::set_harness_ingest` (off by default). The old net path stays authoritative; the test enables the typed path and then proves the old path still updates the same tile.
- The public `harness::test_support::FakeHarness` takes a prompt and replays a scripted chat: two text snapshots (streaming update), a tool call with a diff, a question, a permission, model/command/skill catalogs and idle. The store test seeds two sessions and asserts the second transcript stays empty.
- Review fixes: session lineage is preserved on a title-only update; server-level errors announce instead of disappearing; skills land with source `skill`; catalogs carry the workspace directory; `HarnessKind::from_wire` replaces the duplicate id match; the test-only `HomeGuard` moved to shared `test_support`.
- Linked-suite gates pass: `cargo fmt --all --check`, `cargo clippy -p warp-agent --all-targets -- -D warnings`, and `cargo test -p warp-agent` (94 lib tests plus the probes). The bridge is the expand-phase seam; ticket 14 removes it when the adapters own the stream.
