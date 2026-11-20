# 06: opencode v2 connection, discovery and catalogs

**What to build:** `opencode-v2` appears when enabled, discovers the user's
shared service, authenticates, starts the service when it is stopped, and
exposes its models, providers, agents, commands, skills and sessions. Missing
CLI, missing registration, stopped service and bad credentials each produce a
clear unavailable state instead of a crash.

**Blocked by:** 01, 02, 05

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Discovery reads `service status` and the platform registration file; missing CLI, missing registration, stopped service and bad credentials are distinct unavailable states.
- [ ] When stopped and enabled, the adapter starts the service, retries boundedly, confirms `/api/info` and attaches.
- [ ] Basic auth uses username `opencode` and the registration password; a 401 re-reads the registration file; no password is written to settings.
- [ ] Models, providers, agents, commands, skills and session list/create/fork/delete work and are scoped to the v2 harness.
- [ ] v2 appears or disappears in the picker and settings with its enable and availability state; its sessions carry the correct directory/location.
- [ ] Fake v2 server tests cover auth, stopped/start, 401 and the catalog shapes.
