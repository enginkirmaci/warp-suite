# 02: Typed harness event stream and store ingestion

**What to build:** A single typed event stream per harness instance that the
store can consume to drive a chat, and an ingestion path that keeps the
existing transcript, tool-call, permission and question state working. A fake
harness can take a prompt, stream text, raise a permission prompt and go idle
without any network or child process. The old event path still feeds the app
until the contract ticket.

**Blocked by:** 01

**Status:** ready-for-agent

**Category:** enhancement

- [ ] `HarnessEvent` covers server/health, session status/idle, session changes, message/part snapshots, tool calls, permissions, questions/forms, catalogs and structured errors.
- [ ] The store consumes the typed stream into the existing tile and transcript state; a fake harness drives a full chat: prompt, streamed text, a tool call, a permission prompt, idle.
- [ ] The transcript stays part-shaped and virtualized; the existing transcript and layout tests pass unchanged.
- [ ] Events are routed to the right session; no event leaks across sessions.
- [ ] The old event path and the typed path can coexist behind a switch until the contract ticket.
