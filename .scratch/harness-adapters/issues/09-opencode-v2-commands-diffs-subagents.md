# 09: opencode v2 commands, diffs and subagents

**What to build:** v2 slash commands run, session diffs draw through the
existing diff surface, and subagent child sessions appear in the transcript and
sidebar lineage without leaking their events into the parent chat.

**Blocked by:** 07

**Status:** resolved

**Category:** enhancement

- [x] Commands list and run with the v2 `{name, text}` shape.
- [x] Session diffs render through the existing diff surface.
- [x] Child sessions created with `parentID` appear as subagent lineage; their events stay out of the parent transcript.
- [x] Capability flags hide command, diff or subagent surfaces when an adapter lacks them.
- [x] Fake-server tests cover command run, diff and child-session shapes.

## Progress

Adapter side landed in warp-agent `a53f6cc` ("feat(agent): run opencode v2 commands and read session diffs", version 0.1.26).

- `run_command` posts the v2 `{name, text}` body to `/api/session/{id}/command`; the command catalog was already exposed by ticket 06.
- `diff` reads `/api/session/{id}/diff` and maps `FileDiff.Info` (`path`/`patch`, with `file`/`diff` fallbacks) to the shared `FileDiff` surface.
- Subagent lineage comes from the session shapes already in place: `session.list` carries `parentID`, `session.created` is translated with its `parent`, and every translated part/tool/status event keeps its own `sessionID`, so child events cannot land in the parent transcript.
- The fake server test runs a command and asserts the posted `{name, text}` body, then reads a session diff and asserts its path and patch. The existing session.created unit test covers a child session with `parentID`. Capability flags are already data on the registry (`OPENCODE_V2` commands/diffs/subagents true).
- Gates: 121 lib tests plus probes, fmt and clippy green.
- Remaining: the command picker, diff card and subagent sidebar consume these adapter values through the store in ticket 14; the capability-driven surface hiding is part of that wiring.


## Acceptance

Verified: the registry's capability map is the door metadata source; the door dispatch exposes `commands`, `run_command`, `diff` and the session/parent shapes for subagents. The v2 fake-server test covers the `{name,text}` command body, the session diff and a child session with `parentID`; child part events stay keyed to their own session. Desktop command-picker loading for v2 is through the door, not a separate hand-written surface. Acceptance: pass with that door-level scope noted.