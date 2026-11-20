# 01: Harness registry, trait and host scaffold

**What to build:** One app-level `Harness` interface, a compile-time registry of
harness descriptors, and a host that resolves a harness by its stable id. The
app can register a fake adapter, ask what a harness supports, and receive a
typed `Unsupported` answer for an operation the adapter does not implement. The
existing opencode and dsh paths keep running untouched, so nothing in the UI
changes yet.

**Blocked by:** None (can start immediately)

**Status:** resolved

**Category:** enhancement

- [x] A `Harness` trait covers the spec's app-level operations: lifecycle, catalogs, sessions, turns, approvals, diffs, the v2-only operations and one event stream.
- [x] The registry resolves the stable ids `opencode`, `opencode-v2` and `dsh`; an unknown id is a typed error.
- [x] A descriptor carries the display label, default executable, connection kind and capability set; capabilities are data, not a user switch.
- [x] A fake adapter registers and every method returns either a typed result or `Unsupported`.
- [x] The old opencode and dsh paths still build, run and pass their tests; no behavior change is visible.

## Comments

Implemented in warp-agent `668dcb7` ("feat(agent): scaffold the harness interface and registry", version 0.1.18).

- New `harness` module: the wide `Harness` trait (app-level operations), stable `HarnessId` strings, capability/connection descriptors, the compiled-in `HarnessRegistry` with `opencode` / `opencode-v2` / `dsh`, and a `HarnessHost` that owns instances and shuts them down.
- A fake adapter crosses the trait in the unit tests; typed `Unsupported`, unknown/duplicate ids and the host shutdown are covered.
- The old opencode and dsh paths are untouched. Linked-suite gates pass: `cargo fmt -p warp-agent`, `cargo clippy -p warp-agent --all-targets -- -D warnings`, and `cargo test -p warp-agent` (93 lib tests plus the probes).
- The app's standalone `cargo clippy --locked` gate still fails on pre-existing kit-pin drift (`about::brand_tile`); it is unrelated to this ticket and belongs to the release-pin work in ticket 15.
