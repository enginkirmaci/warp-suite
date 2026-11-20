# 07: opencode v2 chat streaming

**What to build:** A v2 chat streams end to end: the prompt is accepted into
the inbox, text and reasoning arrive as deltas, tool calls appear, the turn can
be interrupted or compacted, and the transcript shows full-text parts as they
grow.

**Blocked by:** 06

**Status:** ready-for-agent

**Category:** enhancement

- [x] A prompt is acknowledged by the v2 inbox and the adapter is subscribed before or with the send.
- [x] Text and reasoning deltas accumulate into full-text parts, in order, per assistant message.
- [ ] Tool-call state, step status, idle and structured errors reach the tile.
- [x] Interrupt and compact behave correctly and settle the turn.
- [x] The fake v2 SSE server drives the test; no live service is needed.

## Progress

The adapter side landed in warp-agent `2d19340` ("feat(agent): stream opencode v2 deltas through the harness", version 0.1.24).

- `/api/event` SSE pump starts with `start` and reconnects with the registration password re-read on 401. It parses `data:` frames and reconciles the v2 no-part-id stream into full `PartSnapshot`s: text/reasoning deltas accumulate per `(assistantMessageID, direction)`, tools are keyed by their assistant-scoped id and move Pending → Running → Completed/Failed, step started/ended become step parts, `session.created/deleted/renamed/agent.selected` become session events, and `session.execution.failed`/`session.step.failed`/`session.compaction.failed` become typed errors.
- `send_prompt`, `interrupt` and `compact` use `/api/session/{id}/prompt|interrupt|compact`; the fake SSE server test drives busy → text deltas → reasoning → tool called/success → step started/ended → idle, asserts the accumulated parts and tool output, and covers the interrupted/compacted calls. A second unit test covers structured errors and session.created.
- Gates: 119 lib tests plus probes, fmt and clippy green.
- Remaining before this ticket can be resolved: the store/tile ingestion of v2 `PartSnapshot`s and the v2 session in the composer. The typed store seam exists (`Store::ingest_harness_event` behind `harness_ingest`) but the app still runs chats through the old opencode/dsh paths; switching it on is ticket 14. The adapter event stream is ready for ticket 08 (permissions/forms) and the other v2 surface tickets.
