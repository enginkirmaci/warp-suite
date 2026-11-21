# 14: Remove the old harness special cases

**What to build:** The old harness enum, the dsh workspace sentinel, the
per-kind network state and every shared dsh-named request/event variant are
gone; the app runs entirely through the registry and adapters with no behavior
change.

**Blocked by:** 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

**Status:** resolved

## Progress

The functional v2 bridge landed, but the enum/special-case removal is still open.

- warp-agent `edfd652` (0.1.30) and `e542d62` (0.1.31): `HarnessKind::OpenCodeV2` joins the existing enum as a bridge; `opencode-v2` is a valid wire id/settings key/picker choice. The store owns an attached `OpenCodeV2Harness`, starts it on first use, pumps its typed events into `ingest_harness_event`, loads its model catalog and routes a v2 tile's prompt through `create_session`/`send_prompt`. V2 permission replies go through the adapter instead of the old net path; `HarnessKind::ALL` is three ids and the door/models payload includes v2.
- The settings page can select v2 as a default, the composer pill cycles to it when enabled, and disabling v2 detaches the adapter without touching the shared service.
- Still open for this ticket: the enum itself, the dsh workspace sentinel, the per-kind old net branches (`oc.rs` now rejects v2 with explicit arms), the old request/event variants and the v1/dsh adapters being the real chat path. The typed store seam (`harness_ingest`) is switched on by `ensure_v2`; v1/dsh still use the old net client. The acceptance evidence cannot be claimed until that sweep lands.

A second sweep landed in warp-agent `da42a46` ("refactor(agent): replace the harness enum with registry ids", version 0.1.32): the old `HarnessKind` enum, `DSH_SERVER_KEY` and dsh-named shared request/event variants are gone; `HarnessId` strings flow from settings through the store, UI and door; the registry carries launch specs and scope/connection predicates; settings/UI/door iterate descriptors. Inventory before the sweep: 232 `HarnessKind` references across 19 files, 56 sentinel references, ~72 per-kind comparisons. After: zero, with 129 lib tests and all probes green.

Still open: v1/dsh chat/session/streaming/approval paths are still on the legacy net thread (`src/oc.rs`/`src/dsh.rs`, `WorkspaceNet`, `ApiRequest`/`UiEvent`), selected by descriptor predicates. That routing is the last piece of ticket 14.


## Remaining work

- Replace `HarnessKind`/`DSH_SERVER_KEY` and the old `ApiRequest`/`UiEvent` variants with registry ids and the host; move v1/dsh chat/session/streaming/approval flows onto their adapters.
- Keep every probe green while the bridge is removed.


**Category:** enhancement

- [x] No old harness enum, dsh sentinel, per-kind client field or dsh-named shared request/event variant remains.
- [x] The store, UI and door iterate the registry; unknown ids are typed errors.
- [x] Every existing probe and unit test is green; chat, v1, dsh, v2, settings, door and transcript behavior is unchanged.
- [x] No `if harness == ...` branch remains outside the adapters.
- [x] The v2 protocol is known only to its adapter.


## Final sweep

The routing landed in warp-agent `de7b959` ("refactor(agent): route managed harness chats through their adapters", version 0.1.33).

- The net thread now owns `HarnessHost` instances keyed by `(HarnessId, Option<folder>)`, created through the descriptor factory. It starts/stops instances, pumps each adapter's typed `HarnessEvent` stream, maps connection/health/exit facts to store notifications and dispatches every `ApiRequest` through the `Harness` trait. The `WorkspaceNet` per-kind `api`/`shared` fields and the direct `ApiClient`/`DshHandle` routing are gone; those clients remain only as transport inside their adapters.
- The store consumes typed events for every harness (`UiEvent::HarnessEvent` → `ingest_harness_event_from`) with the instance folder tagging the event. Typed vocabulary gained prompt attachments/commands, transcript history, todos, reconnect and exit facts; the trait gained `transcript`, `find_files`, `forget_session`, session-scoped catalogs, and a bridge module builds the wire JSON once.
- Inventory: 232 `HarnessKind` references and 56 sentinel references before the sweep → 0; no `if harness == <id>` branch outside adapters (only descriptor scope/connection facts). v2 protocol code exists only in `harness/opencode_v2.rs`.
- Gates: `cargo fmt --all --check`, `cargo clippy -p warp-agent --all-targets -- -D warnings`, `cargo test -p warp-agent` → **142 lib tests + all 15 probe binaries, 0 failures**.
- Live smoke (removed from the suite): real `opencode serve`/`dsh web` start → typed events → session list/create → transcript → command catalog → find files → delete → stop. Not exercised against a live model: prompt streaming, approval/question round-trips and GPUI-visible flows; those rely on the adapter unit/fake-server tests.
- The door's full trait dispatch and the Station AI card remain tickets 12/13, not this ticket.