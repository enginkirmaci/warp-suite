# Collect the per-repo docs into `docs/`

_Status: exploration done, decisions open. Nothing moved yet._

Every app repository carries some mix of `docs/`, `plans/` and `GLOSSARY.md` at
its root, and the three are governed by different things — one by a skill, one
by a skill's setup step, one by nothing but convention. This records what
actually governs each, which of them warp-flow creates (none), and what moving
them under `docs/` would cost.

## What governs each file

| Artifact | Written by | Skill-driven? | warp-flow creates it? |
| --- | --- | --- | --- |
| `GLOSSARY.md` | `domain-modeling` | **yes** — hardcoded at the repo root | no |
| `docs/adr/NNNN-*.md` | `domain-modeling` | **yes** — hardcoded as `docs/adr/` | no |
| `docs/agents/*.md` | `setup-matt-pocock-skills` | **yes** — the skill's Section C | no |
| `plans/<topic>.md` | a person, following `AGENTS.md` | **no skill at all** | no |
| `AGENTS.md` | a person | no | no |
| `README.md` | a person | no | no |
| `.scratch/<slug>/**` | the warp-flow engine | no | **yes** |
| `.flow/*.toml` | the warp-flow app | no | **yes** |

The two rows that matter:

**`plans/` is not skill-driven.** No skill in the installed set mentions
`plans/` — not `domain-modeling`, not `research`, not `wayfinder`, not
`grilling`. It is a pure `AGENTS.md` convention, repeated near-verbatim in
warp-agent, warp-notes, warp-android-scribe, warp-flow and warp-suite ("Design
plans live in `plans/`: a plan is deleted when its work lands"). Moving it is
therefore free: one sentence per `AGENTS.md` and a `git mv`.

**warp-flow creates none of them.** The engine and the app write only under
`.scratch/<slug>/` (`spec.md`, `run.md`, `issues/`, `questions/`, `repairs/`,
`sessions/`, the pinned `workflow.toml`) and `.flow/` (the gates and the
template). Every `docs/`, `plans/` and `GLOSSARY.md` in every repository was
written by a person or by a skill, never by a run. So this move cannot break a
run, and no engine change is involved.

## What is actually on disk

Seventeen repositories; seven carry anything.

| Repository | `docs/` | `plans/` | `GLOSSARY.md` | `docs/adr/` | `docs/agents/` |
| --- | --- | --- | --- | --- | --- |
| warp-suite | yes | yes (5) | yes | yes | yes |
| warp-flow | yes | yes (1) | yes | yes | no |
| warp-agent | yes | yes (1) | no | yes | no |
| warp-notes | no | yes (1) | no | no | no |
| warp-android-scribe | no | yes (2) | no | no | no |
| warp-asistant | yes | no | no | no | no |
| lumux | yes | no | no | no | no |
| the other ten | no | no | no | no | no |

Two shapes are already close to the target and worth knowing before planning:

- **lumux** already keeps its plan at `docs/plans/restructuring.md` — the only
  repository that does.
- **warp-asistant**'s `docs/` is not documentation: `architecture.md` plus four
  exported diagrams (`.png`/`.svg`) and an `.obsidian/` vault config. It is a
  drawing folder, and moving it would be a rename with no gain.

## The blocker: `domain-modeling` hardcodes the root

The skill's own file structure, verbatim:

```
/
├── GLOSSARY.md
├── docs/
│   └── adr/
```

and: *"If no `GLOSSARY.md` exists, create one when the first term is resolved."*

So `docs/adr/` is already where the skill expects it — moving ADRs is a no-op.
`GLOSSARY.md` is the one file that fights the skill: move it to
`docs/GLOSSARY.md` and the next `domain-modeling` session, finding no root
glossary, creates a second one at the root. That is the whole risk, and it is
one file.

The skill does have a sanctioned override point: `docs/agents/domain.md` is the
"consumer rules" file that `setup-matt-pocock-skills` writes, and it is where
the repo states its layout. warp-suite's copy already says *"Single-context:
`GLOSSARY.md` and `docs/adr/` at the repo root."* — so the file that would have
to change is the file that exists to be changed. The skill's own `SKILL.md`
still assumes the root, though, so the override is a convention the agent
follows, not something the skill enforces.

## The target

```text
/
├── docs/
│   ├── GLOSSARY.md          ← the one contested move
│   ├── adr/                 ← already here
│   ├── agents/              ← already here (warp-suite only)
│   └── plans/               ← free to move
├── AGENTS.md                ← stays: it is the entry point
└── README.md                ← stays: it is the front door
```

`AGENTS.md` and `README.md` stay at the root on purpose: both are read before
anything else, and burying the entry point costs more than the tidiness gains.

## Decisions to settle

1. **Does `GLOSSARY.md` move?** Moving it is tidier and costs one convention
   note; keeping it is what the skill assumes and costs nothing. The middle
   path — move it, and make `docs/agents/domain.md` say so in every repo that
   has one — is what this plan recommends, but it is a real trade.
2. **Does `docs/agents/` spread to the apps?** Today only warp-suite has it.
   If the apps get one, each needs a `domain.md` to carry the layout note.
3. **What happens to `warp-asistant/docs/`?** It is diagrams, not docs. Leave
   it, or rename it to something honest.
4. **Do the ten empty repositories get a `docs/`?** No — create lazily, the way
   the skill does. An empty folder is noise.

## The work, once the decisions are made

Per repository, in this order:

1. `git mv plans docs/plans` where `plans/` exists (warp-suite, warp-flow,
   warp-agent, warp-notes, warp-android-scribe). lumux is already there.
2. `git mv GLOSSARY.md docs/GLOSSARY.md` where it exists (warp-suite,
   warp-flow) — only if decision 1 says move.
3. Rewrite the `plans/` sentence in each `AGENTS.md` to `docs/plans/`, and the
   `GLOSSARY.md` references in `README.md`, `AGENTS.md` and `docs/agents/`.
4. Update `docs/agents/doc-layout.md` and `docs/agents/domain.md` in
   warp-suite — the two files that state the layout.
5. Fix the relative links that break: `plans/README.md` → `../AGENTS.md`
   becomes `../../AGENTS.md`, and every `../docs/adr/` from a plan becomes
   `../adr/`.

Step 5 is the one that bites: a `git mv` of a folder silently breaks every
relative link inside it, and nothing in CI checks markdown links today.

## Open question for the skills

The clean fix is upstream: `domain-modeling` could read the layout from
`docs/agents/domain.md` instead of hardcoding the root. Until it does, every
repository that moves its glossary is one skill run away from growing a second
one. Worth raising before the move, not after.
