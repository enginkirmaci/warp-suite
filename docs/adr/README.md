# ADRs

Architecture decision records for warp-suite: one decision per file, named
`NNNN-<slug>.md` (`0006-the-door-speaks-a-harness-namespace.md`), numbered in
order and never renumbered.

This folder holds the family's **contract** decisions — the ones two
repositories must change together: the worktree rule, the door protocol, the
door token, and warp-flow's own door. A decision inside one app lives in that
app's `docs/adr/` and keeps the family's numbering, so the series is shared even
though the folders are not.

Write one when a choice is hard to reverse and someone later will ask why — a
cross-repo interface, the pinned `gpui` fork, the local-markdown tracker.
`/domain-modeling` writes them as decisions land, usually from `/grill-with-docs`
or `/improve-codebase-architecture`.

An accepted ADR is not edited: supersede it with a new one that says so, and
leave the old record in place.

The rules agents follow when reading these live in
[`../agents/domain.md`](../agents/domain.md).
