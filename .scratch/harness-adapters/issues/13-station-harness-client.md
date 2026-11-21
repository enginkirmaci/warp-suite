# 13: Warp Station harness client and AI card

**What to build:** Warp Station drives the new door in the same window: the AI
card lists registry harnesses and v2 models/variants, a run can start on v2, and
a permission or form that blocks a Station-started run is shown and answerable.
Existing v1/dsh wand runs do not change.

**Blocked by:** 12, 08

**Status:** resolved

**Category:** enhancement

- [x] Station's client speaks `harness` and `subscribe` and still speaks the existing commands.
- [x] The AI card lists the registry's enabled harnesses and the selected harness's models and variants; a disabled or unavailable harness is explained.
- [x] A Station-started run that hits a permission or form shows the blocking card and can answer it; with auto-allow on, the run proceeds without a card.
- [x] Existing v1/dsh wand runs and their tests are unchanged.
- [x] Station's settings accept the new harness id and check the door protocol version.
- [x] Station tests pass against a fake door, and the change lands together with 12.

## Progress

Client side landed with ticket 12 in Station `390a86a` ("feat(station): speak the door harness namespace and subscriptions", version 0.1.90).

- `warp-station-core/src/warp.rs` gains `DoorHarness` (id, label, enabled, available, state, connection, capabilities), `Client::harnesses()` over the door's `harness metadata`, `Client::subscribe()`/`unsubscribe()` over filtered subscriptions, and `Client::protocol_version()`.
- The fake-door test now asserts protocol 3, parses two registry harnesses (including an unavailable `opencode-v2`), and opens/unsubscribes a filtered subscription. Existing v1/dsh model-catalog and run tests are unchanged.
- `WarpAgentSettings` already accepts an arbitrary harness id (v2 included); the settings UI will call the protocol/harness metadata once the store supplies v2 in the model catalog (ticket 14).
- Remaining: the AI card consuming `DoorHarness`/subscriptions for its picker and the blocking permission/form card for a Station-started run. Those need the door to associate run ids with harness events, which is ticket 14's host wiring; the client surface is ready.


Additional client-side progress: Warp Station `9bfd62a` ("feat(station): require the harness-aware door protocol", version 0.1.91) makes `Client::models` call `protocol_version` first and refuse a door older than protocol 3 with a clear update message; `WarpAgentSettings` already stores an arbitrary harness id (v2 included). The fake-door tests still pass.

The remaining ticket-13 work is the visual AI card: consuming `DoorHarness`/subscriptions to show availability and the blocking permission/form card for a Station-started run. The door now dispatches the full harness surface (`23d13e0`), so the client calls exist; the card and run↔event association remain.

## Card progress

Warp Station `497ee91` (0.1.92) and `a1f2d39` (0.1.93):
- The settings AI card loads `harness metadata` (`LoadHarnesses` → `UiEvent::HarnessesLoaded`) and labels an unavailable harness with its door state; the model/variant pickers continue to come from the door catalog.
- A wand run opens a filtered door subscription (fire-and-forget subscribe before the prompt) and forwards `permission-asked`/`question-asked` frames to `UiEvent::WandApproval`. The AI card renders the blocking permission (action, message, resources) with Allow once / Always / Reject / Dismiss; a reply opens a fresh door connection and calls `harness reply_permission` with the run's harness/session/request. Auto-allow still answers before the card when warp-agent's setting is on.
- Test: a fake door pushes a permission frame before the run answer; `run_prompt_with_harness_events` exposes it while still returning the final text.
- Remaining: multi-field form answering in the Station card (the event arrives, the card has no inputs/answers yet), and a run↔event association beyond the harness filter (the current subscription is harness-wide while the run is active).

## Resolution

Station `419b59d` (0.1.94) completes the blocking card: `run_prompt_with_harness_events` opens a filtered subscription before the prompt (fire-and-forget, so an old door never delays the run), forwards permission/form frames while the run waits, and the AI card renders permission actions (Allow once/Always/Reject) and a multi-field form card (text/number/integer/boolean/multiselect inputs, comma-separated multiselect, per-field required label). Submitting sends `harness reply_permission`/`reply_form` over a fresh door connection; auto-allow still answers before the card when warp-agent is configured that way. The card inputs persist per approval/field across repaints. The fake-door test drives a permission frame before the run's final answer; the settings card lists registry harnesses/models/variants with the door's availability state, and `Client::models` checks protocol 3. Gate: 215 Station-core tests + 33 probe tests and 162 warp-agent tests + probes, clippy/fmt green.

Caveat recorded honestly: the subscription filters by harness while a run is active (the door's harness events do not carry a Station run id yet); a form field the user leaves blank is sent as an empty string rather than omitted/validated server-side.