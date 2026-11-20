# 12: Door harness namespace, subscribe and metadata

**What to build:** The door exposes the harness operations under one `harness`
command and a `subscribe` event stream; the existing commands keep their shape;
the protocol version moves. Warp-agent's side is complete and tested over a
real loopback connection.

**Blocked by:** 05

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Existing commands (`ping`, `open-workspace`, `new-chat`, `run-prompt`, `cancel`, `models`, `status`) keep their shape and pass their tests.
- [ ] `harness` dispatches the app-level methods and returns typed results or errors: unknown method, disabled harness, unavailable harness, unsupported operation.
- [ ] `subscribe` pushes filtered harness events with subscription ids; overflow and close are explicit.
- [ ] Harness metadata lists enabled harnesses with availability and capabilities; `status` reports the same; disabled harnesses are rejected.
- [ ] `ping` reports the new protocol version; the token remains the only gate; auto-allow is untouched.
- [ ] Door tests cover the wire frames, error envelopes and token rejection.
