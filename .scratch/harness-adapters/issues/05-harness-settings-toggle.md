# 05: Harness settings map, enable and disable

**What to build:** Settings lists every registered harness with an enable
switch, an executable and a default selection; disabling one stops a managed
harness or disconnects an attached one, and its chats become read-only. The
composer and the door only offer enabled harnesses. Existing settings, chats
and model caches migrate without loss.

**Blocked by:** 03, 04

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Settings is generated from the registry; each harness has enabled, executable and default selection.
- [ ] `opencode` and `dsh` start enabled and `opencode-v2` starts disabled; the old executable and default fields migrate unchanged.
- [ ] Disabling a managed harness stops its processes (asking first if a turn is running); disabling an attached harness disconnects and leaves the shared service running.
- [ ] Open chats on a disabled harness stay readable, refuse sends with a note, and become sendable again when re-enabled.
- [ ] The last enabled harness cannot be disabled; the default falls back to an enabled harness.
- [ ] The composer pill and the door list only enabled harnesses; cached models stay on disk but out of pickers.
