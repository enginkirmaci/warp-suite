# 15: Contracts, kit tag and suite release

**What to build:** The contracts, pins and release window land: the settings
schema, the suite's cross-app contract, both apps' agent instructions, the kit
tag carrying the door protocol constants, both app pins and the suite tags. Both
apps and the kit are green on the suite gates.

**Blocked by:** 13, 14

**Status:** ready-for-human

**Category:** enhancement

- [x] The settings-schema contract documents the harness map, defaults, migration and cache identity.
- [x] The suite's cross-app contract lists the new door commands and the same-window rule.
- [ ] The kit tag with the door constants and protocol version is published and both apps pin it.
- [ ] `suite.toml` names both app tags and the suite release assembles them.
- [ ] `cargo fmt`, clippy with `-D warnings`, the app and kit tests, and the Windows type-check pass.

## Progress

Contract docs landed in suite `4d9a849` ("docs: update the harness and door contracts"):
- `docs/agents/settings-schema.md` now documents the `harnesses` map (`enabled`/`executable`), legacy executable migration and sync, the default fallback rule, the v2 registration-file rule, the three harness ids in `harness_by_session`, and the harness-qualified model cache identity.
- `README.md` documents door protocol 3, the `harness` namespace and typed errors, `subscribe`/`unsubscribe` with filters and explicit overflow/close, the enabled-only metadata/status shape, and the same-window rule remains stated for Warp Station.

Still open: publishing the kit tag carrying the door constants and moving both apps' pins, `suite.toml` app tags and the suite-release assembly, plus the final full gate (including the Windows type-check).


## Release blocker (round 9)

The code and contracts are ready, but the publishable tag step cannot be completed from this checkout: the app pins resolve against the GitHub remote, and a local `kits-v0.2.107` tag is not visible to `cargo` (`failed to find tag ... reference refs/remotes/origin/tags/... not found`). The attempted app pin commit was reverted; no unpublished tag was left behind. The Windows type-check script (`$HOME/.agents/skills/rust/scripts/check-windows.sh`) is not installed here, so that gate was not run.

Remaining for a human/release job: tag the kit commit with the door constants, update both apps' kit pins to that tag, tag the app releases (warp-agent 0.1.34, Warp Station 0.1.91), update `suite.toml`, run the suite release assembly and the Windows type-check, and push the tags so the app CI can resolve them.

## Gate evidence (round 11)

- `cargo fmt --all --check` green; clippy `-D warnings` green on warp-agent, warp-station, warp-station-core and the kit crates.
- warp-agent 162 lib tests + 35 probe tests; warp-station-core 215 tests + 33 probe tests; kit crates all green.
- Versions consistent: suite 0.2.111, warp-agent 0.1.34, warp-station 0.1.94.
- Not run: Windows type-check (`$HOME/.agents/skills/rust/scripts/check-windows.sh` is absent) and suite release assembly (needs published kit/app tags).

This is the same blocker reported in rounds 9 and 10: the app manifests resolve kit tags from the GitHub remote, so a local tag cannot be used; pushing tags/releases is outside this checkout. Ticket status is `ready-for-human`; the remaining checklist items are release operations, not code.