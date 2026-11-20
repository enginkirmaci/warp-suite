# 14: Remove the old harness special cases

**What to build:** The old harness enum, the dsh workspace sentinel, the
per-kind network state and every shared dsh-named request/event variant are
gone; the app runs entirely through the registry and adapters with no behavior
change.

**Blocked by:** 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

**Status:** ready-for-agent

**Category:** enhancement

- [ ] No old harness enum, dsh sentinel, per-kind client field or dsh-named shared request/event variant remains.
- [ ] The store, UI and door iterate the registry; unknown ids are typed errors.
- [ ] Every existing probe and unit test is green; chat, v1, dsh, v2, settings, door and transcript behavior is unchanged.
- [ ] No `if harness == ...` branch remains outside the adapters.
- [ ] The v2 protocol is known only to its adapter.
