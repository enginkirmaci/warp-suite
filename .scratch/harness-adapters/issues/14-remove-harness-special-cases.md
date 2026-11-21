# 14: Remove the old harness special cases

**What to build:** The old harness enum, the dsh workspace sentinel, the
per-kind network state and every shared dsh-named request/event variant are
gone; the app runs entirely through the registry and adapters with no behavior
change.

**Blocked by:** 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

**Status:** ready-for-agent

## Progress

The functional v2 bridge landed, but the enum/special-case removal is still open.

- warp-agent `edfd652` (0.1.30) and `e542d62` (0.1.31): `HarnessKind::OpenCodeV2` joins the existing enum as a bridge; `opencode-v2` is a valid wire id/settings key/picker choice. The store owns an attached `OpenCodeV2Harness`, starts it on first use, pumps its typed events into `ingest_harness_event`, loads its model catalog and routes a v2 tile's prompt through `create_session`/`send_prompt`. V2 permission replies go through the adapter instead of the old net path; `HarnessKind::ALL` is three ids and the door/models payload includes v2.
- The settings page can select v2 as a default, the composer pill cycles to it when enabled, and disabling v2 detaches the adapter without touching the shared service.
- Still open for this ticket: the enum itself, the dsh workspace sentinel, the per-kind old net branches (`oc.rs` now rejects v2 with explicit arms), the old request/event variants and the v1/dsh adapters being the real chat path. The typed store seam (`harness_ingest`) is switched on by `ensure_v2`; v1/dsh still use the old net client. The acceptance evidence cannot be claimed until that sweep lands.

## Remaining work

- Replace `HarnessKind`/`DSH_SERVER_KEY` and the old `ApiRequest`/`UiEvent` variants with registry ids and the host; move v1/dsh chat/session/streaming/approval flows onto their adapters.
- Keep every probe green while the bridge is removed.


**Category:** enhancement

- [ ] No old harness enum, dsh sentinel, per-kind client field or dsh-named shared request/event variant remains.
- [ ] The store, UI and door iterate the registry; unknown ids are typed errors.
- [ ] Every existing probe and unit test is green; chat, v1, dsh, v2, settings, door and transcript behavior is unchanged.
- [ ] No `if harness == ...` branch remains outside the adapters.
- [ ] The v2 protocol is known only to its adapter.
