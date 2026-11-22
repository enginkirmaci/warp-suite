# warp-flow: the warp-suite-v3 shell and run surfaces

Status: in progress (2026-11-22). Lead owns `src/ui/window.rs`; the surfaces are
split one file per agent. Design source: OpenDesign `warp-suite-v3`
(`DESIGN.md` + `source/flow.css` + `source/flow.js`), extracted to
`.scratch/warp-flow-v3/design-spec.md`; the app as built is inventoried in
`.scratch/warp-flow-v3/inventory.md`.

## The gap

warp-flow still carries the pre-v3 shape: a 248 rail that holds the
repositories, the runs **and** the screen nav, a six-column kanban board as the
main surface, and a 460 detail pane beside it. The design's one shell is

```
title bar 46
rail 76          | list 432                        | detail (rest)
Runs, Repos      | Repositories (All Repos,        | Runs: 320 run column + the run
About, Settings  | Favorites, Repositories)        | Repos: the repository detail
```

and everything a person does with a run happens in that detail's run pane, in
four tabs: **Run, Task & diff, Artifacts, Workflow**. The kanban board does not
disappear as a feature: it becomes the run column, "the board read downwards" —
same six states, same order, same keyboard cursor, same reveal.

## Rules this pass follows

1. **Nothing is removed.** Every existing behaviour keeps a home; the board's
   state grouping becomes the run column's lane bands, and the rail's screen
   nav becomes the run's tab strip.
2. **Selectors are contracts.** Every debug selector and `*_for_probe`
   accessor keeps working across the restructure, so the 13 probes keep testing
   behaviour rather than a layout. New surfaces add selectors, they do not
   rename the old ones (`flow-board` still names the run column's scroller,
   `flow-column-<state>` still names a lane, `flow-nav-<key>` still names a tab).
3. **No colour, size or radius literals in app code.** Measures come from
   `warpui_kit_core::shell` or `theme::`, ink from the palette readers.
4. **No kit change.** `warpui-kit-core` is pinned by tag; the About screen that
   would need `about::AppId::Flow` is deferred (see Open decisions).

## Measures (design → app)

| Part | Value | Where |
|---|---|---|
| title bar | 46 | `shell::TITLE_BAR_HEIGHT` |
| rail | 76 | `shell::RAIL_WIDTH` (replaces `theme::RAIL_WIDTH` 248) |
| list | 432, 320–560 | `shell::LIST_WIDTH` + `theme::LIST_WIDTH` (remembered in settings) |
| run column | 320 | `theme::RUN_COLUMN_WIDTH` |
| run column row | 64 | `theme::RUN_ROW_HEIGHT` |
| lane band | 32 | `theme::LANE_HEIGHT` |
| run strip | 44 | `theme::RUN_STRIP_HEIGHT` |
| run tab | 42 | `shell::DETAIL_TAB_HEIGHT` |
| step band | 44 | `theme::STEP_HEIGHT` |
| per-task row | 36 | `theme::TASK_ROW_HEIGHT` |
| attempt row | 44 | `theme::ATTEMPT_HEIGHT` |
| changes rail | 320 (220 under 1500 window) | `theme::CHANGES_RAIL_WIDTH` |
| artifacts rail | 240 | `theme::ARTIFACTS_RAIL_WIDTH` |
| reading measure | 760 | `shell::READING_WIDTH` |
| control / field | 34 / 36 | `shell::control_height()`, `shell::field_height()` |

Type: `font_screen_title` 22/600 for the run title, `font_section` 16/600 for
cards, `font_name` 15/600 for rows, `font_body` 14, `font_meta` 13,
`font_mono_meta` 13 for ids/paths, `font_micro` 12 as the floor (no all-caps
labels any more).

## Module split

| File | Owns |
|---|---|
| `src/ui/window.rs` | state, scan/host adoption, selection, keyboard, render composition, probes |
| `src/ui/chrome.rs` | title bar (mark, app links, activity chip, list toggle, text size, palette) and the 76 rail (Runs, Repos, About, Settings) |
| `src/ui/repos.rs` | the list pane: All Repos row, Favorites band, Repositories band, 64 rows with stats + star + Open in Station |
| `src/ui/runs.rs` | the 320 run column: scope head + Refresh, summary line, filter menu, lane bands, 64 run rows, empty states |
| `src/ui/detail.rs` | the run pane's head (title, state pill, primary action, ⋯ menu), the four-tab strip, the 44 run strip, and the tab body host |
| `src/ui/run.rs` | the Run tab body: question/blocked/paused cards, Workflow step bands, Tasks card, Attempt history, Engine log |
| `src/ui/screens/*` | Artifacts (240 rail + document), Diff (320 changes rail + diff), Workflow (revision + editor), Gates (editor), Settings |
| `src/ui/about.rs` | deferred, see Open decisions |

New state in `FlowApp`: `section` (`Runs` | `Repos`), `tab` (`RunTab`),
`filter` (`RunFilter`), `list_hidden`, `list_width`, `favorites`. Favorites and
the list width go to `~/.warp/flow/settings.json` (schema 2, additive).

## Phases

1. **Shell** (lead): module split, state, chrome, repos list, run column, run
   pane head/tabs/strip, tab routing; old screens keep rendering inside their
   tab; every selector preserved. Gate: `cargo build -p warp-flow`.
2. **Surfaces** (parallel, one file each): Run tab body (agent), Artifacts
   (agent), Diff/Task & diff (agent), Workflow + Gates (agent), Settings (agent).
3. **Probes** (agent): update shell-shape assertions, add selectors for the new
   controls (star, filter menu, Refresh, ⋯ menu, tab strip, primary action), keep
   every behaviour assertion.
4. **Gates** (lead): `cargo fmt`, `cargo clippy -p warp-flow -p warp-flow-core
   --all-targets -- -D warnings`, `cargo test -p warp-flow -p warp-flow-core`,
   kit gate untouched.
5. **Docs** (lead): `AGENTS.md` shape section, app `README.md`, this plan
   deleted or folded.

## Open decisions

- **About.** `warpui_kit_core::about` has no `AppId::Flow`, so a faithful About
  screen needs a kit addition plus a kit tag before warp-flow's pin can move.
  Deferred unless the owner asks; Settings carries the build line meanwhile.
- **Kanban vs run column.** The design replaces the board with the run column;
  the lane bands keep the states, the order and the keyboard. If the owner wants
  the 2-D board back it becomes a view toggle inside the run column, never a
  second detail surface.
- **Paused.** `Column` folds `paused` into "Waiting on you"; the design has a
  quiet Paused lane of its own. Kept folded (the state word on the run strip
  says `paused`), because `run.md`'s paused status is a person's to resume and
  "Waiting on you" is exactly that.

## What landed (2026-11-22)

- **Shell**: `src/ui/chrome.rs` (46 title bar with the mark, the two sibling
  launches, the activity chip, the list toggle, the shared text-size and palette
  controls and the window buttons; 76 rail with Runs, Repos, About, Settings and
  the waiting-count pip), `src/ui/repos.rs` (the 432 list: All Repos, Favorites,
  Repositories, 64 rows with the star, the newest run's age, the stats line in
  the entity hues and Open in Station on hover), `src/ui/runs.rs` (the 320 run
  column: scope head with Refresh, summary, filter menu, lane bands in pipeline
  order, 64 run rows, the driving spinner where the age sits, the keyboard
  cursor and its reveal) and `src/ui/detail.rs` (run head with the state pill,
  the actions, the close, the four tabs, and the 44 strip; the repository head
  with Gates and Workflow).
- **State**: `Section`, `RunTab`, `RepoTab`, `RunFilter`, `list_hidden`,
  `list_width`, `favorites`; `Settings` gained `favorites` and `list_width`;
  `Run` gained `updated` (from `run.md`, else the folder) and `Column::Merged`
  reads `Landed`.
- **About** (`src/ui/about.rs`): identity, Build card, the family's installed
  versions and Legal, from the kit's copy table and the platform's sibling scan.
- **Icons**: 59 assets generated from the design's own Lucide geometry, so shared
  concepts stay byte-identical to the other apps.
- **Probes**: updated deliberately where the shell moved — the run opens on the
  row click (the preview pane's Open run is gone), `flow-nav-board` is
  `flow-rail-runs`, the detail pane's Spec/Tasks sections are the Artifacts tab,
  empty lanes are asserted absent, the run column is asserted fixed-320 with no
  sideways scroll, and `ScreenRun` carries `read_only`.

**Known deltas from the spec** (deliberate, recorded here so they are not
rediscovered): the scope title is 16 rather than 17 (the kit has no 17 step);
Settings keeps its one flat page instead of a list of groups; the gates and
workflow editors still own their run's copy, so the Repos section drives them
from the repository in focus through one of its runs; the run row's ⋯ menu is
not built yet (the run head carries the actions); Artifacts keeps its editor
(the owner's rule: no existing feature is removed) although the design would
make the folder read-only.

## Surfaces (agents, same pass)

- **Run tab** (`run.rs`): a 760 column of cards — the question card
  (`changes` border, full-width options, the written-answer field, **Answer** as
  the run's one filled primary, Abandon arming in place), the blocked card in
  `danger` with the failing line and its retry budget, the paused note, the
  "not started" issue card, the Workflow card (44 bands + 36 task rows with
  22-high chips), the Tasks card, Attempt history (44 rows, number chip,
  `attempt n of 3`, View log) and the Engine log and Sessions cards. Every
  spinner carries a word. The head's cluster follows the design's one-primary
  table (Start run / Resume / Retry gate filled; Pause plain; a waiting run has
  none, because the card's Answer is; a landed run has none).
- **Task & diff** (`diff.rs`): 320 changes rail (220 under 1500, folder suffix
  dropped), Conflicts first then Changed, 34 rows; the task card, the read-only
  gates card (38 rows from `gates::read_gates` joined with the run's last
  outcome), the conflict bar, the path bar with Unified|Split and Wrap, the kit
  diff at 13/21 and a conflict block with ours/theirs tags.
- **Artifacts** (`artifacts.rs`): 240 file rail, 58 rows, the document at 760
  with its name, path, meaning and Copy; the editor and its atomic save are
  kept (the design would make the folder read-only — the owner's no-removal rule
  wins), with the read-only reason spelled out when the engine drives.
- **Gates** (`gates.rs`): 320 work list of gates and one 4-field editor; Run
  once, Save as the one filled primary and a trash that arms on the first click.
- **Workflow** (`workflow.rs`): 320 step list + editor (kind seg, id, skill,
  gate, produces, depends on) and the Revisions card; the run's tab is strictly
  read-only and names where the template is edited.
- **Settings**: the family's flat row page, capped at the reading measure.

Known follow-ups this pass did not close: the run row's ⋯ menu (the head holds
the actions), the list pane's drag-resize (the width and `Ctrl+\` are there),
`about::AppId::Flow` in the kit plus a kit tag before warp-flow's pin moves, a
repository-level `workflow.toml` the engine reads (`prepare.rs` still reads the
issue's copy), and a `Screens` navigation seam so the read-only Workflow tab and
the conflict bar can link to Repos › Workflow / the run's question.

## Follow-up pass (same day): the four items the owner asked for

1. **The title bar carries no appearance knobs.** Text size and palette moved to
   `Settings` as their own rows (`settings-row-text-size`, `settings-row-palette`,
   the kit's own controls), so the bar is only what the window does: mark,
   siblings, activity, list toggle, window buttons.
2. **About lives in the kit.** `warpui_kit_core::about` gained `AppId::Flow`, its
   `SUITE` entry and the two-tone chevron marks; warp-flow's About section is the
   kit's `about::list` + `about::detail`.
3. **The workflow template belongs to the repository.** `prepare` reads the
   issue's copy first and falls back to `<repo>/workflow.toml`; `Prepared::
   pin_workflow` copies the template into the issue folder at run start, so a
   run keeps the revision it started on; `store` reads the same fallback and
   `Run::workflow_path` names the file; `Screens::show_repo` gives the Repos
   section's two tabs a repository payload with no run involved, so a repository
   with no runs still has gates and a template to edit.
4. **The run row has its ⋯ menu** (copy id, copy folder, open folder, open in
   Station, the one state action, armed abandon).
