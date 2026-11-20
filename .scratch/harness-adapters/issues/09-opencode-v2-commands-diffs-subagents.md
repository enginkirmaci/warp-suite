# 09: opencode v2 commands, diffs and subagents

**What to build:** v2 slash commands run, session diffs draw through the
existing diff surface, and subagent child sessions appear in the transcript and
sidebar lineage without leaking their events into the parent chat.

**Blocked by:** 07

**Status:** ready-for-agent

**Category:** enhancement

- [ ] Commands list and run with the v2 `{name, text}` shape.
- [ ] Session diffs render through the existing diff surface.
- [ ] Child sessions created with `parentID` appear as subagent lineage; their events stay out of the parent transcript.
- [ ] Capability flags hide command, diff or subagent surfaces when an adapter lacks them.
- [ ] Fake-server tests cover command run, diff and child-session shapes.
