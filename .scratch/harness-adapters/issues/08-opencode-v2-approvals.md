# 08: opencode v2 permissions and forms

**What to build:** v2 approval prompts and forms work in the chat:
once/always/reject permissions, question cards for forms, multi-field answers,
and the existing auto-allow behavior where it applies.

**Blocked by:** 07

**Status:** ready-for-agent

**Category:** enhancement

- [ ] A v2 permission prompt shows once/always/reject and replies with the chosen decision.
- [ ] Auto-allow answers permissions per the existing settings; with auto-allow off the manual prompt stays.
- [ ] A v2 form renders as a question card; a multi-field form can be answered and settles.
- [ ] Permission and form events are scoped to the right session and clear when answered or cancelled.
- [ ] Fake-server tests cover asked/replied/cancelled and invalid-answer failures.
