# 13: Warp Station harness client and AI card

**What to build:** Warp Station drives the new door in the same window: the AI
card lists registry harnesses and v2 models/variants, a run can start on v2, and
a permission or form that blocks a Station-started run is shown and answerable.
Existing v1/dsh wand runs do not change.

**Blocked by:** 12, 08

**Status:** ready-for-agent

**Category:** enhancement

- [x] Station's client speaks `harness` and `subscribe` and still speaks the existing commands.
- [ ] The AI card lists the registry's enabled harnesses and the selected harness's models and variants; a disabled or unavailable harness is explained.
- [ ] A Station-started run that hits a permission or form shows the blocking card and can answer it; with auto-allow on, the run proceeds without a card.
- [x] Existing v1/dsh wand runs and their tests are unchanged.
- [ ] Station's settings accept the new harness id and check the door protocol version.
- [x] Station tests pass against a fake door, and the change lands together with 12.

## Progress

Client side landed with ticket 12 in Station `390a86a` ("feat(station): speak the door harness namespace and subscriptions", version 0.1.90).

- `warp-station-core/src/warp.rs` gains `DoorHarness` (id, label, enabled, available, state, connection, capabilities), `Client::harnesses()` over the door's `harness metadata`, `Client::subscribe()`/`unsubscribe()` over filtered subscriptions, and `Client::protocol_version()`.
- The fake-door test now asserts protocol 3, parses two registry harnesses (including an unavailable `opencode-v2`), and opens/unsubscribes a filtered subscription. Existing v1/dsh model-catalog and run tests are unchanged.
- `WarpAgentSettings` already accepts an arbitrary harness id (v2 included); the settings UI will call the protocol/harness metadata once the store supplies v2 in the model catalog (ticket 14).
- Remaining: the AI card consuming `DoorHarness`/subscriptions for its picker and the blocking permission/form card for a Station-started run. Those need the door to associate run ids with harness events, which is ticket 14's host wiring; the client surface is ready.
