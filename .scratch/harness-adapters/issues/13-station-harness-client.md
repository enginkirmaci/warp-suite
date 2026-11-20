# 13: Warp Station harness client and AI card

**What to build:** Warp Station drives the new door in the same window: the AI
card lists registry harnesses and v2 models/variants, a run can start on v2, and
a permission or form that blocks a Station-started run is shown and answerable.
Existing v1/dsh wand runs do not change.

**Blocked by:** 12, 08

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Station's client speaks `harness` and `subscribe` and still speaks the existing commands.
- [ ] The AI card lists the registry's enabled harnesses and the selected harness's models and variants; a disabled or unavailable harness is explained.
- [ ] A Station-started run that hits a permission or form shows the blocking card and can answer it; with auto-allow on, the run proceeds without a card.
- [ ] Existing v1/dsh wand runs and their tests are unchanged.
- [ ] Station's settings accept the new harness id and check the door protocol version.
- [ ] Station tests pass against a fake door, and the change lands together with 12.
