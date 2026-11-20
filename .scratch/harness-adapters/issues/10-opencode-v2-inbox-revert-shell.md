# 10: opencode v2 inbox, revert, shell and background sessions

**What to build:** The v2-only surfaces work: inbox list/update/cancel, revert
stage/commit/clear, a shell command in the session directory, and background
sessions that keep working while the tile is not focused.

**Blocked by:** 07

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Inbox items are listed, steered or queued, updated and cancelled.
- [ ] Revert can be staged, committed and cleared, and the file effect is visible.
- [ ] A shell command runs in the session directory and its output and state are visible.
- [ ] A background session continues while the tile is not focused and reports its state.
- [ ] All four are capability-gated and tested against the fake v2 server.
