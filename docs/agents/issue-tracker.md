# Issue tracker: local markdown

Issues, tickets and specs for warp-suite live as markdown files in `.scratch/`.
Nothing is filed on GitHub: the repo's own Issues stay empty and `gh issue` is
not part of the workflow.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The spec is `.scratch/<feature-slug>/spec.md`
- Tickets are one file per ticket at
  `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01` in
  dependency order (blockers first) — never one combined tickets file.
- A ticket's state is a `Status:` line near the top; the role strings are in
  [`triage-labels.md`](triage-labels.md). Its category is a `Category:` line.
- `Blocked by: 01, 03` near the top lists the tickets that must be resolved
  first. A ticket is unblocked when every file it names says `Status: resolved`.
- Conversation history appends to the bottom of the file under `## Comments`.

## Working with tickets

- **Create**: write the file, making the directory if it is not there yet.
- **Read**: read the file at the path the user named.
- **List open work**: scan `.scratch/*/issues/` for files whose `Status:` is not
  `resolved`.
- **Comment**: append under `## Comments`.
- **Close**: set the `Status:` line, then append the reason under `## Comments`.

`.scratch/` behaves like [`plans/`](../../plans/): a working area that rides in
the tree while the work is live and is deleted once it lands. It is not
gitignored today, so those files reach the public history; add `/.scratch/` to
`.gitignore` if you would rather keep tickets local.

## Pull requests as a request surface

**PRs as a request surface: no.** A pull request here is code to review, not a
request to triage.

## When a skill says "publish to the issue tracker"

Create a file under `.scratch/<feature-slug>/`, creating the directory if
needed.

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or
the ticket number directly.

## Wayfinding operations

Used by `/wayfinder`. The **map** is one file and each **ticket** is its own
file.

- **Map**: `.scratch/<effort>/map.md`, holding the Notes / Decisions-so-far /
  Fog body.
- **Child ticket**: `.scratch/<effort>/issues/NN-<slug>.md`, numbered from `01`,
  with the question in the body. A `Type:` line records the ticket type
  (`research`/`prototype`/`grilling`/`task`); a `Status:` line records
  `claimed`/`resolved`.
- **Blocking**: the `Blocked by: NN, NN` line above.
- **Frontier**: scan `.scratch/<effort>/issues/` for files that are open,
  unblocked and unclaimed; first by number wins.
- **Claim**: set `Status: claimed` and save before any work.
- **Resolve**: append the answer under `## Answer`, set `Status: resolved`, then
  append a context pointer (gist + link) to the map's Decisions-so-far.
