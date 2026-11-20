# 04: dsh adapter

**What to build:** dsh runs through the harness interface with today's
behavior: one managed `dsh web` process, launch-token and cookie auth,
WebSocket streams, event folding, and the dsh title/permission/question/command
behavior. Existing dsh chats and tests do not change.

**Blocked by:** 03

**Status:** ready-for-agent

**Category:** enhancement

- [ ] dsh starts and stops through the adapter; the singleton and its token/cookie flow are preserved.
- [ ] dsh events fold into the typed stream; titles, permissions, questions, commands and diffs behave as today.
- [ ] Existing dsh tests, chat behavior and probes stay green.
- [ ] The descriptor reports dsh's capabilities, including the operations it does not support.
- [ ] The adapter is reached by the stable `dsh` id; the old `"dsh"` workspace sentinel does not leak into the host.
