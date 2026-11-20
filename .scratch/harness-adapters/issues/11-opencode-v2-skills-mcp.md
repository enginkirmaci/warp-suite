# 11: opencode v2 skill activation and MCP management

**What to build:** Skill activation and MCP add/connect/disconnect/remove work
on v2; the other experimental v2 operations stay hidden.

**Blocked by:** 06

**Status:** ready-for-agent

**Category:** enhancement

- [x] Skills list and activate, and the activation is visible in the chat state.
- [x] MCP servers list with status; add, connect, disconnect and remove work and surface errors.
- [ ] Capability flags hide both surfaces when absent; no other experimental v2 operation is exposed.
- [x] Fake-server tests cover success and error paths.

## Progress

Adapter side landed in warp-agent `9a9cb6d` ("feat(agent): activate opencode v2 skills and manage MCP servers", version 0.1.28).

- `activate_skill` posts `{id}` to `/api/experimental/session/{id}/skill`; the skill catalog itself came from ticket 06. The adapter exposes no other experimental v2 operation.
- `mcp_servers` reads `/api/mcp` and maps the five status object shapes (connected/pending/disabled/failed/needs_auth). `add_mcp` converts the shared `McpConfig` to a v2 local (`{type, command: [...]}` plus `environment`) or remote (`{type, url}`) config and PUTs `/api/experimental/mcp/{name}`; connect/disconnect POST the subpaths and remove DELETEs the server.
- Fake-server coverage: the catalog with two statuses, the local config body (command array + environment), connect/disconnect/remove paths, skill activation body, and a 500 from the fake server surfacing as a typed `Failed` error.
- Gates: 123 lib tests plus probes, fmt and clippy green.
- Remaining: the skills bar and MCP manager consume these values in ticket 14, and capability flags (`skill_activation`, `mcp_management` in `OPENCODE_V2`, false in `OPENCODE_V1`/`DSH`) drive their visibility there. The adapter contract is complete for tickets 06–11; the next app-side step is the door namespace (12) and then the store/host wiring (14).
