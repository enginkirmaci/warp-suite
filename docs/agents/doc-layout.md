# Where a document lives

warp-suite is the kit and release repository, so a document belongs here only
when it is about the suite itself or about an interface two repositories must
change together. Everything else lives with the app it describes.

## The rule

Keep it in warp-suite when it is about:

- the kits under `kits/`, the one workspace, the lockfile or the toolchain pin;
- the pin table (`suite.toml`), the build, deploy or release scripts;
- an interface two repositories must change in the same commit — the two doors,
  the notes store, SnapIt settings, the `~/.warp` layout, the Android bridge,
  version facts. The full text is
  [`README.md#cross-app-contracts`](../../README.md#cross-app-contracts).

Move it to the app's repository when it is about one app — its UI, its modules,
its roadmap, or a decision only it implements — even when the text names
siblings.

## What that means in practice

| Kind | Suite | App repository |
|---|---|---|
| plan, proposal, migration checklist | `plans/<topic>.md` (suite-level work) | `plans/<topic>.md` |
| ADR | `docs/adr/` — the contract decisions | `docs/adr/` — the app's own, family numbering |
| glossary | `GLOSSARY.md` — family terms | `GLOSSARY.md` — the app's vocabulary |
| agent instructions | `AGENTS.md` plus `docs/agents/` | `AGENTS.md` plus the app's `README.md` |
| design system | `docs/agents/design-system.md` | uses the kit; adds nothing |

The app's files win when the work is in the app; this repository's `AGENTS.md`
stays the entry point for the shared rules.

Two files look like app config but are warp-flow's, not the suite's: the gates
and the workflow template. They live in `.flow/` at the root of every
repository warp-flow drives, and the suite is one such repository:

```text
.flow/flow.toml       the gates and the merge policy
.flow/workflow.toml   the workflow template a run copies
```

`warp-flow-core`'s `layout` module owns both paths
(warp-flow's ADR-0013), so neither the engine nor the app builds one by hand.

## Landing a move

Move both sides in the same window, app repository first, then the suite commit
that removes its copy — so no revision points at a file that is not there. A
plan is deleted when its work lands; keep the text recoverable from history
rather than archiving it.
