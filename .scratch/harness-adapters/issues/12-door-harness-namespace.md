# 12: Door harness namespace, subscribe and metadata

**What to build:** The door exposes the harness operations under one `harness`
command and a `subscribe` event stream; the existing commands keep their shape;
the protocol version moves. Warp-agent's side is complete and tested over a
real loopback connection.

**Blocked by:** 05

**Status:** ready-for-agent

**Category:** enhancement

- [x] Existing commands (`ping`, `open-workspace`, `new-chat`, `run-prompt`, `cancel`, `models`, `status`) keep their shape and pass their tests.
- [ ] `harness` dispatches the app-level methods and returns typed results or errors: unknown method, disabled harness, unavailable harness, unsupported operation.
- [x] `subscribe` pushes filtered harness events with subscription ids; overflow and close are explicit.
- [x] Harness metadata lists enabled harnesses with availability and capabilities; `status` reports the same; disabled harnesses are rejected.
- [x] `ping` reports the new protocol version; the token remains the only gate; auto-allow is untouched.
- [x] Door tests cover the wire frames, error envelopes and token rejection.

## Progress

Protocol side landed in warp-agent `a153367` ("feat(agent): expose the door harness namespace and subscriptions", version 0.1.29), Station client `390a86a` (0.1.90), shared protocol crate `fb63530` (0.2.100).

- The shared door crate moves to protocol 3 and adds the `harness`, `subscribe` and `unsubscribe` command constants.
- `harness` frames are `{id, cmd:"harness", method, params}`; the store dispatches `metadata`/`list-harnesses`, `status`, `models`, `sessions` and `session`, with `unknown:`, `disabled:`, `unavailable:` and `unsupported:` error prefixes. More trait methods return `unsupported` until the store/host wiring ticket; the wire shape is already generic.
- Metadata lists only enabled registry harnesses with `connection`, `available`, `state`, `executable` and the full capability map; `status` adds the same list alongside the workspaces. Disabled harnesses are rejected by `models`. `opencode-v2` currently reports `unavailable` because the store does not own its instance until ticket 14.
- `subscribe` is connection-local: it takes a `subscription` id and optional `harness`/`session`/`run` filters, acks the id, and the connection filters the broadcast `harness-event` frames. Non-matching events are dropped; marked events carry the subscription id. Broadcast lag becomes an explicit `subscribe-overflow` frame and app shutdown sends `subscribe-closed`.
- `Store::ingest_harness_event` now broadcasts each typed event through the same filter, so the adapter event stream becomes a door subscription once the host feeds the store (ticket 14). The typed wire shape covers session/part/tool/permission/form/command/error events.
- `ping` answers 3; the token handshake and auto-allow are untouched. Door tests cover metadata, the disabled error, subscribe/filter/unsubscribe, and the token rejection/roundtrip suite.
- Full app gates: 125 lib tests plus probes, fmt and clippy green; kit platform 43 tests and clippy green; Station core warp tests green.

Remaining before this ticket can be resolved: dispatching the rest of the app-level methods (prompt, cancel, session create/fork/delete, permissions, forms, ...) needs the store/host adapter wiring and the v2 instance; that is ticket 14. The door protocol and filtering are complete and tested.
