# 07: opencode v2 chat streaming

**What to build:** A v2 chat streams end to end: the prompt is accepted into
the inbox, text and reasoning arrive as deltas, tool calls appear, the turn can
be interrupted or compacted, and the transcript shows full-text parts as they
grow.

**Blocked by:** 06

**Status:** ready-for-agent

**Category:** enhancement

- [ ] A prompt is acknowledged by the v2 inbox and the adapter is subscribed before or with the send.
- [ ] Text and reasoning deltas accumulate into full-text parts, in order, per assistant message.
- [ ] Tool-call state, step status, idle and structured errors reach the tile.
- [ ] Interrupt and compact behave correctly and settle the turn.
- [ ] The fake v2 SSE server drives the test; no live service is needed.
