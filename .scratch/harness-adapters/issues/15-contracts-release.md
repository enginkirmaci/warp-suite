# 15: Contracts, kit tag and suite release

**What to build:** The contracts, pins and release window land: the settings
schema, the suite's cross-app contract, both apps' agent instructions, the kit
tag carrying the door protocol constants, both app pins and the suite tags. Both
apps and the kit are green on the suite gates.

**Blocked by:** 13, 14

**Status:** ready-for-agent

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
