# warp-suite glossary

The domain vocabulary this repo runs on, seeded from `README.md` and
`AGENTS.md`. It is a starting point, not a finished lexicon: `/domain-modeling`
(reached via `/grill-with-docs`) sharpens it as decisions land, and a term earns
a row only once the project actually uses it.

Agents write issue titles, refactor proposals and test names in these words. The
consumer rules are in [`docs/agents/domain.md`](docs/agents/domain.md).

| Term | What it means here |
| --- | --- |
| **suite** | This repository: the kit sources plus the release pin table. It is not a second source of truth for app code. |
| **app** | One product of the family — warp-station, warp-agent, warp-snapit, warp-notes, warp-pomodoro, warp-android-scribe. Each lives in its own repository and is linked locally at `apps/<name>`. |
| **kit** | One of the six shared `warpui-kit-*` crates under `kits/`, members of the root workspace: the app-facing building blocks for GPUI, markdown, diffs, pickers, platform paths and build facts. |
| **linked workspace** | The root cargo workspace while `apps/*` symlinks exist — kits plus whatever apps are linked. CI builds it kit-only. |
| **pin table** | `suite.toml`: the newest published kit tag and one tag per app. A suite release names exactly those tags. |
| **bootstrap** | `warpui_kit_core::bootstrap::init`, the one startup sequence every app runs before its first window (text size, palette, scale, motion). |
| **ramp** | The current palette, read through the kit helpers. The only source of colours and accents; app code holds no colour, font family or font size literal. |
| **door** | A token-authenticated loopback NDJSON interface an app publishes (`ipc.port` + `ipc.token` under `~/.warp/<app>/`) so its siblings can drive it. warp-agent's and warp-pomodoro's doors are contracts. |
| **harness** | A coding-agent runtime the app can run a chat on, named by a stable wire id (`opencode`, `opencode-v2`, `dsh`). The app reaches it through an adapter, never through its own protocol. |
| **adapter** | The app-side implementation for one harness protocol: process lifecycle, transport and translation into the app's event stream. |
| **connection** | How the app reaches a harness: it spawns and owns a process (*managed*), or attaches to a service someone else runs (*attached*). |
| **probe** | A headless UI integration test at `tests/<name>_probe.rs` that builds the real views through the `lib.rs` boundary. |
| **capture** | A screenshot of a hidden window, taken without pulling the owner's screen; the recipe is in `docs/agents/testing-and-captures.md`. |
| **store** | The notes store (default `~/notes`): one root, repo notes under `projects/<repo>`. User content, not app data. |
| **per-user layout** | The `~/.warp` tree: one folder per app, the shared `ui.json`, one `logs/`. Owned by `warpui_kit_platform::paths`. |
| **plan** | A working design file at `plans/<topic>.md`, deleted once the work lands. |
| **bridge** | The warp-android-scribe service on the NAS: turns the phone's `/transcribe` request into Superwhisper or Hex calls. |
| **version facts** | Each repository's `version.toml` line plus the values stamped into the binary (About, `--version`, the Windows file version, `/health`). |
| **rail** | The fixed-width file column beside a diff pane — the Changes file list, a History commit's files, and a pull request's Files tab. A rail is a focusable list with its own cursor. |
| **file tree** | Changed files grouped by the folders their paths share, folding and counting per folder. Synthesized from the changed paths only: it is a view of the change, not a repository explorer, and never lists an untouched file. |
| **worktree** | A second working directory of a repository, checked out on its own branch, so work can happen without moving the repository's main checkout. A chat's worktree is that chat's directory. Both apps create them by the same rule. |
| **parent repo** | The repository a worktree was branched from: Station's repo row, and the folder a chat's worktree toggle branches off. Its branch is what the worktree starts from. |

## warp-flow

The workflow engine that turns a request into merged code. Its artifacts live in
the tracker under `.scratch/`, its steps run as harness sessions, and every
decision it makes is written down before the next step reads it.

| Term | What it means here |
| --- | --- |
| **warp-flow** | The workflow engine app, and `warp-flow-core` its library half. It drives harness sessions, writes artifacts into the issue folder, and has no model of its own. A desktop app with its own UI for runs and approvals. |
| **request** | The user's raw ask in free text — a feature, a bug, a change. What warp-flow is handed before anything is distilled. |
| **issue** | A request distilled into a tracker record under `.scratch/<feature-slug>/` with a `Status:` line; the unit one run works on. |
| **task** | One numbered ticket under `.scratch/<feature-slug>/issues/`; the unit that gets its own worktree and branch. |
| **blocker** | A task that must be merged before another may start, carried as the `Blocked by:` line the tracker already writes. |
| **step** | One node of a workflow: analyze, split, test-first, code, review, test, merge. |
| **workflow** | The declarative graph of steps, their transitions and gates, held as TOML and revised as it changes. |
| **revision** | One recorded change to a workflow's graph, with who, when and why. Revisions append; an earlier one is never rewritten. |
| **run** | One execution of a workflow for one issue, its state held in `run.md`. |
| **attempt** | One try of one step inside a run; a retry or a repair opens a new attempt. |
| **gate** | A check a step's result must pass: a shell command, its working directory, a timeout, and what its exit code means. |
| **skill** | The markdown document a step hands to its harness session, telling it how to perform that step. |
| **artifact** | A markdown file a step writes inside the issue's folder: its result and the next step's input. |
| **question** | Something the run needs a person to answer before it continues: an approval, one choice, several choices, or free text. It is written into the issue folder, and the run pauses until the answer is written back. |
| **approval** | A question whose answer is yes or no — merge or not, plan or not. |
| **repair** | A bounded reaction to a failed attempt — fix the code, the artifact or the graph, then try again. |

## Not in this glossary

- Ticket and triage vocabulary (`needs-triage`, `ready-for-agent`, …) belongs to
  [`docs/agents/triage-labels.md`](docs/agents/triage-labels.md).
- OS, GPUI, cargo and GitHub terms are used in their ordinary sense.
