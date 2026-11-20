# 03: opencode v1 adapter

**What to build:** opencode v1 runs through the harness interface with today's
behavior: one managed server per workspace, the same readiness and reconnect
behavior, catalogs, sessions, prompt streaming, permissions and questions,
slash commands and diffs. Existing v1 chats and probes do not change.

**Blocked by:** 01, 02

**Status:** ready-for-agent

**Category:** enhancement

- [ ] v1 starts and stops through the adapter; attach-to-healthy, the readiness wait and the event pump keep their behavior.
- [ ] Models, agents, commands, session list/create/fork/delete, prompt streaming, permissions/questions and diffs reach the store through the typed stream.
- [ ] Chat behavior and every existing v1 probe stay green.
- [ ] The host sees v1's health and capability state through the interface.
- [ ] The adapter is reached by the stable `opencode` id; no new v1-specific variant appears on the shared store or door surface.
