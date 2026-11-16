# ADRs

Architecture decision records for warp-suite: one decision per file, named
`NNNN-<slug>.md` (`0001-one-workspace-one-lockfile.md`), numbered in order and
never renumbered.

Write one when a choice is hard to reverse and someone later will ask why — the
one-workspace/one-lockfile split, the pinned `gpui` fork, the local-markdown
ticket tracker. `/domain-modeling` writes them as decisions land, usually from
`/grill-with-docs` or `/improve-codebase-architecture`.

An accepted ADR is not edited: supersede it with a new one that says so, and
leave the old record in place.

The rules agents follow when reading these live in
[`../agents/domain.md`](../agents/domain.md).
