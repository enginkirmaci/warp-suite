# warp-flow

_Status: design closed; S0, S1 and S2 done. Six grill rounds; the build order
below is accepted. The checkout lives at `apps/warp-flow`; next is S3, the app. Prototype: `.scratch/warp-flow/prototype-screens.html` (throwaway,
fake data)._

Takes a request — a feature, a bug, a change — and drives it to merged code:
distil it into an issue, split it into tasks, write each task in its own
worktree, review it, run the repository's gates, and merge it into the main
branch. Every step that needs judgement is a harness session; the engine itself
never calls a model. When the run needs a person it writes a question and
waits.

**Vocabulary** lives in [`GLOSSARY.md`](../GLOSSARY.md#warp-flow): request,
issue, task, blocker, step, workflow, revision, run, attempt, gate, skill,
artifact, question, approval, repair.

## Decisions

1. **A new app, not a mode of warp-agent.** `warp-flow-core` holds the engine;
   `warp-flow` is a desktop app in the suite, with its own UI and its own door
   ([ADR-0012](../docs/adr/0012-warp-flow-publishes-a-door-of-its-own.md)).
2. **The engine never calls a model**
   ([ADR-0008](../docs/adr/0008-the-engine-never-calls-a-model.md)). Every
   thinking step is a harness session driven over warp-agent's door
   ([ADR-0011](../docs/adr/0011-warp-flow-reaches-harnesses-through-the-door.md)).
3. **The issue folder is the protocol**
   ([ADR-0009](../docs/adr/0009-the-issue-folder-is-the-protocol.md)):
   `.scratch/<feature-slug>/` holds `spec.md`, `issues/<NN>-<slug>.md`,
   `workflow.toml`, `run.md`, `questions/<NN>.md` and `repairs/<NN>.md`. The
   engine advances by parsing them; there is no database.
4. **Workflows are declarative and revisable**
   ([ADR-0010](../docs/adr/0010-workflows-are-declarative-and-revisable.md)).
   A run copies the default graph into the issue folder and reads it at each
   step boundary, so a change — by a person or by the engine — lands on the next
   step and never inside a running session. Every change appends a revision.
5. **Tasks are a dependency graph.** `split` writes `Blocked by:` lines, as the
   tracker already does. A task starts only once every blocker has **merged**,
   because its worktree branches from `main`. Ready tasks run in parallel up to
   a per-repo budget (2 today); the merge queue is strictly serial.
6. **A lane is serial.** `code → review → gate` per task: review may ask for
   changes, so the gate must measure the code that will actually merge.
   Parallelism is across ready tasks, not inside one.
7. **Gates are discovered, then approved.** An agent reads the repository
   (markers, build files, CI if there is any) and proposes commands; a person
   approves them; the result is written to `flow.toml` and committed. A gate is
   a shell command, a working directory, a timeout and an exit-code meaning —
   nothing language-specific. No `ci.yml` is required.
8. **Tests are written by the code step.** The gate still runs the
   repository's own suite; agent-written tests are evidence, not a replacement.
   An existing test may change only with a reason tied to the spec, and deleting
   one, skipping one or weakening an assertion needs a person's answer.
9. **Merge is local and serial.** Rebase onto `main`, run the gates on the
   rebased result, `--no-ff` merge, push only if `flow.toml` says so. A conflict
   is repaired on the task branch (never on `main`) for at most two attempts,
   then it becomes a question. No force-push, ever.
10. **Repair is bounded.** Three attempts per step. A repair may fix code, a
    malformed artifact, or insert a graph node; it may never weaken a test,
    change a gate, or edit the spec or its acceptance criteria. Repeating the
    same failed strategy stops the run instead of burning attempts.
11. **Questions are first class.** `approve` (yes/no), `select`, `multi`,
    `text` — asked by a step or by the harness agent mid-session, written to
    `questions/NN.md`, answered from either UI, written by warp-flow alone. A
    run paused on a question does not spend its wall-clock budget.
12. **Harness permissions are policy, not a person.** Inside a task worktree,
    edits and commands are allowed; writes outside it, network access, `push`
    and force operations become a question. Every automatic answer is written
    to `run.md`.
13. **The issue folder is committed to `main`**, and only that folder. Three
    points: `flow: issue <slug>` at run start, `flow: record <slug>` before a
    merge attempt, and `flow: resolve task NN` after a task lands. The commits
    run only while `main` is checked out, with hooks and signing off, so the
    target repository's own version hook never fires on the engine's writes and
    no other path is ever staged. Without them the merge queue would refuse its
    own run: writing `run.md` is what makes the main checkout dirty. A dirty
    tree outside the issue folder is still a refusal to start.
14. **Artifacts outlive the run.** A merged issue folder stays with
    `Status: merged`; an explicit `flow gc` archives old ones. Nothing is
    deleted automatically.
15. **Dogfood order:** a warp-suite kit crate first (`cargo test -p …`, headless
    and fast), then warp-notes, then warp-flow itself.
16. **The checkout lives inside the suite — no symlink.** `apps/warp-flow` is
    a real directory holding the app's own git repository (its `.git` lives
    there too); `apps/*` is gitignored, so the suite's history never sees it,
    while the workspace glob picks it up and the shared `.build/target` and the
    kit `[patch]` table apply. `link_apps.py` never replaces a real directory,
    so a symlink can never appear later.


## The door contract (measured in S0)

The spike in `.scratch/warp-flow/spike/` ran the real door end to end twice; these
are the rules a Rust client must follow, not guesses:

- Auth is one line (`{"auth": token}` → `{"ok": true}`), protocol 3, token read
  before port; a wrong token closes the connection.
- **Subscribe before create.** `harness create` never returns the session id —
  `id` is the reserved framing field and the payload's `id` is dropped — so the
  id is learned from the first `session-changed` push that names the new title,
  then the broad subscription is narrowed to that session. `dsh` announces
  creation as `session-changed`, not `session-created`.
- **`part` frames are snapshots, not deltas** (replace the text, never append),
  and the prompt itself echoes as a user `part`; the assistant text comes from
  `harness transcript` after `idle`, filtered to `role=assistant`.
- **Never call `open-workspace`.** `harness start` + `harness create` with
  `directory=<worktree>` works on a plain directory, needs no git repository, and
  adds no row to the owner's workspace list; `open-workspace` would add one per
  task worktree, permanently, and no door verb removes it.
- The door does no validation: `create` with a missing directory **creates it**.
  The engine validates the worktree itself before it asks for a session.
- Only a managed harness streams. `dsh` is managed and app-scoped; `opencode`
  v1 is workspace-scoped (and disabled here), and `opencode-v2` is attached with
  no event pump on this path. Budgets are wide: a one-word prompt took 4.2 s and
  15.6 s on two runs, so a turn caps at 90 s and a timeout sends
  `harness interrupt`.
- Pushes are live only — no replay, `subscribe-overflow` means lost events — and
  some statuses are broadcast twice, so frames are deduped.
- `permission-asked` / `question-asked` pushes and `reply_permission` /
  `reply_form` methods exist, and `dsh` advertises both capabilities, so the
  engine can answer permissions by policy and escalate questions.
- There is no system-prompt channel: a skill rides in the prompt text.

Two small fixes are worth asking warp-agent for: return the new session id under
a non-reserved key (e.g. `session`) in the `create` reply, and give attached
harnesses the same event pump — neither blocks S2.


## The gate file (`flow.toml`)

Every repository warp-flow drives carries one at its root, written once by the
gate-discovery step and approved by a person:

```toml
schema = 1

[merge]
mode = "local"      # local | pr
push = false

[[gate]]
name = "test"
command = "cargo test -p warp-notes"
cwd = "."           # relative to the worktree root
timeout_secs = 900  # default 900
```

A workflow's gate step names one of these by `name`. Nothing in the file is
language-specific: a gate is a shell command, a directory, a timeout and an exit
code, so a repository in any language is covered by the same shape.

## The graph

`analyze → split → approve-plan → [ready tasks: code → review → gate] → join →
merge → approve-merge`, with two node kinds the engine may insert: `research`
(when `analyze` finds an open question) and `repair` (after a failed attempt).
A run's state lives in `run.md`; the live view is the graph itself.

## The UI

Rail of repositories → kanban board over the run lifecycle (Triage, Ready,
Running, Waiting on you, Blocked, Merged) → run screen with the workflow DAG,
the task dependency strip, step and attempt history, the harness session, and
the question cards. Plus artifacts (markdown with revision diffs), task & diff,
the workflow editor (diagram + forms + revisions), repos & gates, and settings.
warp-agent gets a Flow page that reads the same door and answers the same
questions.

The design language is the suite's: the five `warpui-kit-core` palettes, Inter
and JetBrains Mono, the 12/13/14/15/16/20/22 type steps, radii 6/8/10, one
filled primary per surface.

## Build order (accepted)

- **S0 — door round trip (done).** A throwaway client proved the seam; its
  measured rules are [above](#the-door-contract-measured-in-s0).
- **S1 — core (done).** `warp-flow-core`: the issue folder protocol, the graph
  interpreter, the task scheduler, gates and a fake harness. 44 tests, no
  model, no network. Two rules it settled: a **question step is an approval**
  until the schema grows `select`/`multi`/`text`, and **only the artifact
  resolves a task** — the engine books per-task steps but never writes
  `resolved` itself (the merge step in S2 does).
- **S2 — the full chain, CLI only.** In two halves that landed together:
  **S2a (done)** the door client and `DoorHarness` — subscribe before create,
  the session id from a push, the transcript as the answer, `interrupt` on a
  timeout; **S2b (done)** per-task worktrees through the kit and the merge
  queue — dirty `main` refused, rebase, gates on the rebased tree, `--no-ff`,
  conflict aborts, no force-push. **S2c (done)** the CLI (`run`, `answer`,
  `status`, `--dry-run`), `flow.toml` loading, a `merge` step kind, resuming
  from `run.md`, and an end-to-end probe that drives a real throwaway
  repository to merged, resolved tasks.
- **S3 — the app shell.** Repo rail, kanban board, run screen, waiting-on-you,
  against the live engine.
- **S4 — the warp-agent Flow page.** Reads the flow door, answers questions.
- **S5 — the rest of the screens.** Artifacts, task & diff, workflow editor,
  repos & gates, settings.
- **S6 — release.** The `release` workflow, packaging assets, and a
  `suite.toml` pin once a published tag exists.

## Skills (v1)

Seven documents: `analyze`, `analyze-gates`, `split`, `code`, `review`,
`merge`, `repair`. `research` and `prototype` are v2.

## Next

S2 — the full chain on a real repository, CLI only: the door client (built on
the measured contract above), per-task worktrees, real gates from `flow.toml`,
answering questions and resuming from `run.md`, and the merge queue. S1 left
these deliberately: concurrent ready tasks within the per-repo budget,
re-reading `workflow.toml` at each step boundary, richer question kinds, repairs
that inject a graph node, the Windows gate, and repairing a malformed artifact
discovered mid-run.
