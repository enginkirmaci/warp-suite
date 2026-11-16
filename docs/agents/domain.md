# Domain docs

How the engineering skills consume this repo's domain documentation when
exploring the codebase. warp-suite is **single-context**: one glossary and one
ADR folder, both at the root.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root.
- **`docs/adr/`**: read the ADRs that touch the area you are about to work in.

If a file does not exist yet, **proceed silently**. Don't flag its absence and
don't suggest creating one upfront. `/domain-modeling` (reached via
`/grill-with-docs` and `/improve-codebase-architecture`) adds terms and ADRs
lazily, when a decision or a term actually gets resolved.

## File structure

Single-context (this repo):

```
/
├── GLOSSARY.md
├── docs/adr/
│   └── 0001-<decision-slug>.md
├── kits/                 the six warpui-kit-* crates, workspace members
└── apps/<name>           symlinks to the sibling app checkouts (gitignored)
```

The checkouts under `apps/` are their own repositories with their own glossary,
ADRs and `AGENTS.md`. This file governs warp-suite only; when the work is inside
an app, that app's `AGENTS.md` and its domain docs win.

## Use the glossary's vocabulary

When your output names a domain concept — an issue title, a refactor proposal, a
hypothesis, a test name — use the term as `GLOSSARY.md` defines it. Don't drift
to a synonym the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that is a signal: either you
are inventing language the project doesn't use (reconsider), or there is a real
gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than
silently overriding:

> _Contradicts ADR-0007 (one workspace, one lockfile), but worth reopening
> because…_
