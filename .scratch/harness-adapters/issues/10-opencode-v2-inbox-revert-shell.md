# 10: opencode v2 inbox, revert, shell and background sessions

**What to build:** The v2-only surfaces work: inbox list/update/cancel, revert
stage/commit/clear, a shell command in the session directory, and background
sessions that keep working while the tile is not focused.

**Blocked by:** 07

**Status:** ready-for-agent

**Category:** enhancement

- [x] Inbox items are listed, steered or queued, updated and cancelled.
- [x] Revert can be staged, committed and cleared, and the file effect is visible.
- [x] A shell command runs in the session directory and its output and state are visible.
- [x] A background session continues while the tile is not focused and reports its state.
- [x] All four are capability-gated and tested against the fake v2 server.

## Progress

Adapter side landed in warp-agent `4ecc999` ("feat(agent): expose opencode v2 inbox, revert and shell controls", version 0.1.27).

- Inbox: `GET /api/session/{id}/inbox` parses the four item kinds (user/synthetic/compaction/move) and queue/steer delivery; `PATCH .../inbox/{item}` changes delivery and `DELETE .../inbox/{item}` cancels.
- Revert: `POST .../revert/stage` sends `{messageID}`, `POST .../revert/commit` commits and `DELETE .../revert` clears. The file effect is read back through the existing `diff` call.
- Shell: `POST .../shell` sends `{command}`; `session.shell.ended` becomes a `CommandOutput` carrying the captured output (or a typed failure for timeout/killed).
- Background: `POST .../background` marks the session's blocking work as background; the session status events already report its state.
- The HTTP client now treats `204 No Content` as an empty success, which the inbox/revert/shell/background endpoints use. The fake-server test drives all four surfaces and asserts the posted bodies/paths; the SSE test also drives a shell-ended output event.
- Gates: 122 lib tests plus probes, fmt and clippy green.
- Remaining: the inbox bar, revert card, shell output card and background badge consume these values in ticket 14; capability-driven hiding is part of the same wiring. The adapter contract is ready for ticket 11.
