# 08: opencode v2 permissions and forms

**What to build:** v2 approval prompts and forms work in the chat:
once/always/reject permissions, question cards for forms, multi-field answers,
and the existing auto-allow behavior where it applies.

**Blocked by:** 07

**Status:** ready-for-agent

**Category:** enhancement

- [x] A v2 permission prompt shows once/always/reject and replies with the chosen decision.
- [ ] Auto-allow answers permissions per the existing settings; with auto-allow off the manual prompt stays.
- [x] A v2 form renders as a question card; a multi-field form can be answered and settles.
- [x] Permission and form events are scoped to the right session and clear when answered or cancelled.
- [x] Fake-server tests cover asked/replied/cancelled and invalid-answer failures.

## Progress

Adapter side landed in warp-agent `a64da14` ("feat(agent): surface opencode v2 approvals and forms", version 0.1.25).

- `permissions(session)` reads `/api/session/{id}/permission`; `reply_permission` posts once/always/reject to `/permission/{request}/reply`. `permission.asked` and `permission.replied` translate into the typed `PermissionAsked`/`PermissionReplied` events with the session and decision.
- `forms(session)` reads `/api/session/{id}/form` and parses all six field kinds, required flags and option labels; `reply_form` posts the multi-field `FormAnswer` map to `/form/{id}/reply`. `form.created` becomes `QuestionAsked`, `form.replied`/`form.cancelled` become `QuestionResolved` with the answer summary.
- Fake-server coverage: a permission and a two-field form round-trip with the request bodies asserted (`"reply":"once"`, `"answer"` with both fields), an invalid form id returns a typed `Failed` error, and the SSE test now drives asked/replied/cancelled alongside the streaming events.
- Gates: 120 lib tests plus probes, fmt and clippy green.
- Remaining: the existing auto-allow setting must answer v2 permissions at the app/store layer (the adapter deliberately has no settings); the approval/question cards consume these typed events in ticket 14, when v2 is wired into the store and tile. The adapter contract is ready for ticket 09.
