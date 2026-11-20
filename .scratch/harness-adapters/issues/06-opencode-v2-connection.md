# 06: opencode v2 connection, discovery and catalogs

**What to build:** `opencode-v2` appears when enabled, discovers the user's
shared service, authenticates, starts the service when it is stopped, and
exposes its models, providers, agents, commands, skills and sessions. Missing
CLI, missing registration, stopped service and bad credentials each produce a
clear unavailable state instead of a crash.

**Blocked by:** 01, 02, 05

**Status:** ready-for-agent

**Category:** enhancement

- [x] Discovery reads `service status` and the platform registration file; missing CLI, missing registration, stopped service and bad credentials are distinct unavailable states.
- [x] When stopped and enabled, the adapter starts the service, retries boundedly, confirms `/api/info` and attaches.
- [x] Basic auth uses username `opencode` and the registration password; a 401 re-reads the registration file; no password is written to settings.
- [x] Models, providers, agents, commands, skills and session list/create/fork/delete work and are scoped to the v2 harness.
- [ ] v2 appears or disappears in the picker and settings with its enable and availability state; its sessions carry the correct directory/location.
- [x] Fake v2 server tests cover auth, stopped/start, 401 and the catalog shapes.

## Progress

The adapter landed in warp-agent `deda99f` ("feat(agent): attach opencode v2 through the harness adapter", version 0.1.23).

- `src/harness/opencode_v2.rs` implements the wide `Harness` trait: `opencode service status` + `$XDG_STATE_HOME/opencode/service.json` discovery (HOME/USERPROFILE fallback; `LOCALAPPDATA` on Windows), a bounded `service start` retry loop, `/api/info` confirmation, Basic auth with username `opencode` and the registration password, and a one-shot 401 re-read of the rotated registration file. The service is never stopped; `stop()` only detaches.
- Catalogs: providers, models (disabled models and variants handled), agents, commands, skills; sessions: list/session/create/fork/rename/delete with `location.directory` from `Session.Info`. Chat streaming, approvals, forms, diffs, inbox/revert/shell/MCP return `Unsupported` for the following tickets.
- The registry factory for `opencode-v2` now builds the adapter. Five Unix fake-server tests cover discovery/catalog shapes, a stopped service that is started, missing CLI vs missing registration, bad credentials, and a 401 that re-reads a rotated registration file.
- Full app gates: 117 lib tests plus probes, fmt and clippy green.
- Remaining before this ticket can be resolved: make the app hold the v2 instance through the host so enabling/disabling starts/detaches it, show its availability in Settings, and expose v2 in the composer/picker. Those need the host wiring and the enum removal; they are ticket 14 work (ticket 07 starts consuming the v2 event stream). The adapter itself is ready for ticket 07.
