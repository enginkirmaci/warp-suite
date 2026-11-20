# 05: Harness settings map, enable and disable

**What to build:** Settings lists every registered harness with an enable
switch, an executable and a default selection; disabling one stops a managed
harness or disconnects an attached one, and its chats become read-only. The
composer and the door only offer enabled harnesses. Existing settings, chats
and model caches migrate without loss.

**Blocked by:** 03, 04

**Status:** ready-for-agent

**Category:** enhancement

- [x] Settings is generated from the registry; each harness has enabled, executable and default selection.
- [x] `opencode` and `dsh` start enabled and `opencode-v2` starts disabled; the old executable and default fields migrate unchanged.
- [ ] Disabling a managed harness stops its processes (asking first if a turn is running); disabling an attached harness disconnects and leaves the shared service running.
- [x] Open chats on a disabled harness stay readable, refuse sends with a note, and become sendable again when re-enabled.
- [x] The last enabled harness cannot be disabled; the default falls back to an enabled harness.
- [x] The composer pill and the door list only enabled harnesses; cached models stay on disk but out of pickers.

## Progress

Partially implemented in warp-agent `e05e8c4` ("feat(agent): add the harness settings map and enable switch", version 0.1.22).

- `AppSettings` gains `harnesses: BTreeMap<wire id, {enabled, executable}>`; the registry supplies the rows and defaults, and the old `opencode_executable` / `dsh_executable` / `default_harness` fields migrate into the map. Defaults are `opencode` + `dsh` enabled, `opencode-v2` disabled; the migration keeps at least one usable backend and falls the default back to an enabled one.
- The Settings Server section renders a row per registry descriptor: enable switch, executable field and a two-step "click again" confirmation when a turn is running. The composer pill cycles only enabled harnesses; the door's `models` payload lists only enabled harnesses and cached models, and `new-chat`/`run-prompt` reject a disabled harness.
- Disabling `opencode` stops every workspace server; disabling `dsh` stops the shared process. Open chats stay readable; `send_from_tile` refuses with a note until the harness is re-enabled. Re-enabling refreshes that harness's catalog.
- Stores tests cover the default fallback, the last-enabled guard and the read-only send. Gates: 111 lib tests plus probes, fmt and clippy.
- Remaining in ticket 06 (attached-v2 connection): the disable branch for `opencode-v2` must disconnect the attached instance, and a registry-fed default that can name `opencode-v2` replaces the enum when the host owns the runtime. The host-owned runtime and non-blocking boot also land there; the current Settings page keeps the known executable field keys for the three registered ids until the contract ticket replaces the enum.
