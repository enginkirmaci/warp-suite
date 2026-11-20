# 15: Contracts, kit tag and suite release

**What to build:** The contracts, pins and release window land: the settings
schema, the suite's cross-app contract, both apps' agent instructions, the kit
tag carrying the door protocol constants, both app pins and the suite tags. Both
apps and the kit are green on the suite gates.

**Blocked by:** 13, 14

**Status:** ready-for-agent

**Category:** enhancement

- [ ] The settings-schema contract documents the harness map, defaults, migration and cache identity.
- [ ] The suite's cross-app contract lists the new door commands and the same-window rule.
- [ ] The kit tag with the door constants and protocol version is published and both apps pin it.
- [ ] `suite.toml` names both app tags and the suite release assembles them.
- [ ] `cargo fmt`, clippy with `-D warnings`, the app and kit tests, and the Windows type-check pass.
