# Harness-adapter acceptance round

Status: completed 2026-11-21 against suite v0.2.118, warp-agent v0.1.38 and
Warp Station v0.1.96.

## Result

All tickets 01–15 are resolved. The app and suite gates are green, and the
published releases were produced by the repositories' own workflows:

- kit tag `kits-v0.2.112` (door protocol 3, harness namespace, subscribe);
- warp-agent `v0.1.38`: Linux tarball, Windows portable ZIP and installer;
- Warp Station `v0.1.96`: Linux tarball, Windows portable ZIP and installer;
- suite `v0.2.118`: assembled Linux tarball, Windows ZIP and SHA256SUMS.

## Per-ticket evidence

| Ticket | Verdict | Evidence |
|---|---|---|
| 01 registry | pass | built-in registry, compiled-in trait, factory tests |
| 02 typed events | pass | typed `HarnessEvent` stream and bridge tests |
| 03 opencode v1 adapter | pass | managed serve, SSE/reconnect, fake-server tests |
| 04 dsh adapter | pass | managed `dsh web`, mux fold, fake-server tests |
| 05 settings toggle | pass | enabled map, migration, disable stops, last-enabled guard, confirmation |
| 06 v2 connection | pass | discovery/start/auth/401, catalog, picker and availability state |
| 07 v2 chat | pass | deltas, tool state, step note, idle and structured errors; fake SSE |
| 08 v2 approvals | pass | permission/form cards, replies, auto-allow path, fake tests |
| 09 commands/diffs/subagents | pass | command `{name,text}`, diff, child lineage, capability metadata |
| 10 inbox/revert/shell/background | pass | adapter/door operations, 204 handling, fake tests |
| 11 skills/MCP | pass | skill activation note, MCP list/manage, capability metadata |
| 12 door namespace/subscribe | pass | protocol 3, full dispatch, filters/overflow tests |
| 13 Station client | pass | metadata, models gate, availability, permission/form cards |
| 14 special-case removal | pass | no `HarnessKind`/`DSH_SERVER_KEY`/per-kind routes |
| 15 contracts/release | pass | docs, tags, pins, CI releases and suite assembly |

## Caveats recorded during acceptance

- The v2-only desktop panels (inbox/revert/shell, MCP manager) are exposed at
  the app level through the door harness namespace rather than as separate
  hand-written Station panels; the Station work covers the run loop and the
  permission/form cards.
- The door subscription filters by harness while a run is active; it does not
  carry a Station run id yet.
- Local Windows type-check could not run because the skill script is absent;
  the repositories' Windows release builds completed instead, which is the
  stronger build evidence.

## Gates

- `cargo fmt --all --check`
- clippy `-D warnings` on warp-agent, warp-station, warp-station-core and the
  kit crates
- warp-agent 162 lib tests + probes; warp-station-core 215 tests + 33 probes
- kit crates all green; version checks consistent
