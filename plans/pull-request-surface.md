# The pull-request surface

Status: built. Phases 1–5 landed (core data, net/store, the workbench, the
pull request's own three tabs and its verbs, the embedded New pull request
form), plus the Issues workbench, the Files rail stacking, the Azure
pull-request detail and the Pipelines run detail, and the Pipelines workbench
now lists GitHub Actions and Azure Pipelines runs in one list (the shared run
vocabulary lives in `activity.rs`), and the Pull requests and Issues tabs list
both platforms' rows in one work list each — the union that
`plans/azure-github-activity-merge.md` owned is in, so the Azure item cards are
gone and an Azure pull request opens in the merged list's column. The design
source is the OpenDesign project
`warp-suite-v3` — `DESIGN.md` §5 "Work lists", "Pull requests tab", "Pull
request", "New pull request", "Issues tab" — and the prototype's `prList`,
`prDetail`, `newPrView`, `issueDetail`, `runDetail` and `pipelines`.

## 1. What the design asks for, and what the app had

The design puts a pull request **inside** the Pull requests tab, as one split:
a 320 work list, a hairline, then the selected pull request. The shipped app
replaced the whole detail pane with the pull request and had no list beside it.

| Design | Before | Now |
|---|---|---|
| 320 work list + hairline + detail | none — the pane was the PR | workbench split inside the tab |
| Rows 58 high, `raised` + 3 px `accent` edge when selected | ~40 px rows, hover only | 58 px, selection edge |
| First row selected on wide panes | nothing selected | first row auto-selected above 900 px of pane |
| Below 900 px: one pane + Back | always the whole pane | one pane + Back, `Esc` |
| Title row: state pill, Approve split, one filled primary split, quiet open-in-browser | state pill, one Merge button, a text "Open on GitHub" | split action group, review menu, quiet icon |
| Subtitle: author, `repo #n`, head → base, check summary, `+adds −dels` | same minus the source tag | same + `GitHub` tag |
| 44 status strip on `base`: approvals, comments, updated; auto-complete badge and reviewer stack on the right | approvals, conflicts, base, updated | the design's facts |
| Tabs Overview / Files / Commits | none | three tabs |
| Overview: Policies card, Description, Comments + composer; Reviewers side card | a thread and a checks card | the design's two columns |
| Files: changed-file rail + the Changes diff viewer | none | rail + diff, Unified/Split/Wrap |
| Commits: day-grouped list, selected commit's files, diff, Copy SHA | none | the History viewer inside the PR |
| New pull request in the pane | opens the browser | the form in the detail column |
| Issues: the same split, with a detail showing state, labels, assignee, description and activity, `Open on GitHub` as the one primary | one full-width list, rows opened on GitHub | workbench + issue detail |
| Files rail under 900 px of column | a 240 px column at every width | a 180-high band above the diff |

## 2. Decisions

- **D1 — the pull request lives in the tab, not over it.** `store.open_pull` stops
  meaning "takes over the detail pane" and starts meaning "selected in the Pull
  requests tab". `detail.rs` no longer intercepts the pane; the tab's body
  renders the workbench. Back/Escape only exist below 900 px of pane, exactly as
  the design says, so a wide pane keeps its selection.
- **D2 — selection is implicit.** No separate selection field: the list's first
  row is the selection when nothing else is picked (`currentPull` in the
  prototype). The view asks the store for the full view of that row, so the
  detail column is never empty on a wide pane.
- **D3 — one `gh` call per thing, lazily.** `gh pr view` gains `files`, `commits`,
  `reviewRequests` and `autoMergeRequest` — cheap, and enough for the rail's
  counts and the tabs' labels. The whole-PR patch (`gh pr diff`) is fetched when
  Files is first opened, and one commit's files (`gh api
  repos/{owner}/{repo}/commits/{sha}`) when that commit is selected. A PR that
  is never opened past Overview costs one process.
- **D4 — every verb is a `gh pr` subcommand.** Approve and request changes are
  `gh pr review`, comments are `gh pr comment`, draft/ready is `gh pr ready
  [--undo]`, close/reopen are `gh pr close` / `gh pr reopen`, merge keeps
  `gh pr merge --merge`. GitHub has no auto-complete through `gh pr` (it is a
  GraphQL mutation), so the primary menu shows no auto-complete item on GitHub;
  the strip shows the badge when `autoMergeRequest` is already set.
  `Request changes` needs a body on GitHub, so it posts `Requested changes.`
  until a review composer exists.
- **D5 — the file rail's status letter comes from the patch.** `gh pr view`'s
  `files` carry no status, so the letter is read from the split patch
  (`new file` → A, `deleted file` → D, `rename from` → R, else M) and defaults
  to M until the patch lands.
- **D6 — Abandon is a dialog, at the page level.** The design gives it a
  confirmation; `Store` holds `confirm_abandon` and the page draws the card over
  the scrim, like Add Repositories. Escape and the card's Cancel both clear it.
- **D7 — the diff viewer is shared, not copied.** `render_diff_with_scroll`, the
  wrap/unified/split toggles, the pinned horizontal bars and the empty state move
  to `ui/diff_controls.rs`; Changes and the pull request's Files and Commits tabs
  draw the same controls and persist the same `DiffViewMode` / wrap settings.
- **D8 — merge keeps its second press.** The design gives Merge as a plain
  primary; the shipped app arms it first, and a merge can't be undone from the
  app, so the arm stays. Every other verb is one press.
- **D9 — the title keeps a floor.** The design's title row puts the title, the
  state pill and the action group on one line and clips the title. In the
  workbench the PR column is ~670 px on a 1500 px window, where that leaves a
  four-word stub, so the title has a 220 px floor and the action group wraps
  under it (the prototype's own `.detail-title` is `flex-wrap: wrap`).
- **D10 — the form's preview is local git.** Source and base are free text, as
  the design draws them; the Files and Commits previews come from
  `git::read::compare` (three-dot, so a moved base does not read as removals)
  rather than from GitHub, so a branch does not have to be pushed to be
  previewed. The compare is debounced in the net intake, one read per burst of
  typing. A pair that is not a pair yet says so instead of spinning, and a
  compare that found nothing says that.
- **D11 — the form is its own view.** `NewPullView` sits beside
  `PullRequestView` in the workbench's detail column and is picked by
  `store.new_pull`. Opening it leaves `open_pull` alone, which is what makes
  Cancel restore the selection. Creating clears the form, selects the new pull
  request, switches the list to Open and puts the body on Overview.
- **D12 — a required review blocks the merge.** `ready_to_merge` now also
  refuses `reviewDecision == REVIEW_REQUIRED`, and the policy card says "Review
  required before merging" — the design's "completion control stays disabled
  until they are approved" without inventing a reviewer policy of our own.

- **D13 — both tabs are one workbench.** `ItemListView` already served the two
  tabs; it now draws the split for both, with the detail column picked by the
  tab (`PullRequestView`/`NewPullView` on one, `IssueDetailView` on the other).
  The first row is auto-selected only when nothing is selected: the prototype
  falls back to `pool[0]` on every render, which in the app would be one
  `gh issue view` per keystroke in the filter.
- **D14 — one `gh issue view` per selected issue.** The row carries the counts;
  the detail asks for the words (state, labels, assignees, body, thread) once,
  on selection, and keeps the answer until another row is picked.
- **D15 — the rail reads its column, not the pane.** The workbench hands the
  detail column its width (`pane − 320`, or the pane when stacked), and the
  Files and Commits rails become a 180-high band above the diff under 900 px.
  The workbench's column is under that on any window narrower than ~1728 px
  (`window − rail − list − 320`), so this is the common case, not the edge.

- **D16 — an Azure pull request opens from the Pipelines tab.** *Superseded:
  the union list landed, so an Azure row sits in the Pull requests list and its
  pane is that tab's detail column.* The original shape was the card under the
  runs, opening the detail over the run detail; the pane itself (head, strip,
  tabs, verbs) did not change when it moved. The pane is the same shape as D1–D9, with Azure's own data and
  verbs behind it: state pill Active/Completed/Abandoned, review split
  Approve/Decline (a vote), primary Complete / Set auto-complete /
  Auto-complete set / Reopen, Abandon through the page's card.
- **D17 — Azure hands out no patch with its change list.** A file's diff is a
  second call per file (`items?path=…&includeContent=true` on both branches)
  and a whole-file unified diff built in core: one hunk with three lines of
  context, capped at 1200 lines a side, and an empty diff when the two texts
  match. The Files rail says "Reading the file…" until that second call
  answers, and "No textual changes" when the file is binary or empty — it never
  pretends to a patch it does not have. Line counts are therefore absent from
  the Azure subtitle and the rail, which is a deliberate difference from the
  GitHub pane.
- **D18 — a verb never guesses what the server did.** A vote, complete, abandon
  or auto-complete sends one REST request, and its answer reloads the record
  rather than patching the local copy: the head's buttons stay dead while
  `acting`, and the pane re-reads the pull request on success. A failure shows
  Azure's own message (the JSON envelope's `message`), not a generic toast.
- **D19 — completing arms on the first press.** D8's idiom, for the same
  reason: a completion cannot be taken back from the app. Abandon keeps the
  page's confirmation card, its own state (`confirm_azure_abandon`), and
  Azure's word for the outcome ("It moves to Abandoned. You can reopen it
  later.").
- **D20 — the Pipelines tab is the design's workbench too.** A 320 run list
  (state glyph, pipeline and run number, branch, state word, age, `Azure` tag),
  a hairline, then the selected run: head with the state pill and one `Open
  run` primary, subtitle with branch, short commit and tag, a 44 strip with
  state, start, duration and trigger, one card per stage with job rows, and the
  log's tail in a card. The first run is selected above 900 px of pane; below
  it the list and the run take turns with Back/Escape, exactly like D1–D3 and
  D13.
- **D21 — the run's stage and log data is two calls, asked once.** `builds/{id}/timeline`
  carries the stage and job records; `builds/{id}/logs` lists the entries and
  the last one's text is the excerpt (capped at 8 kB, cut on a line boundary).
  A timeline groups jobs under their stage by walking up the parent chain, so a
  multi-config phase in between still files the job under its stage; tasks are
  not job rows. The detail asks once per run and keeps the answer.
- **D22 — a repository with no runs still gets the tab.** It shows the run
  column's "Nothing has run yet" and nothing else: the project's pull requests
  and work items moved to the merged lists when the union landed, so the
  Pipelines tab draws runs only.

## 3. What landed

- `warp-station-core/src/github.rs`: `GitHubFilePatch`, `GitHubCommit`,
  `PullAction` (argv + toast word), `pull_diff`, `commit_files`, `act_on_pull`,
  `split_unified_diff`, and the new `GitHubPull` fields. Unit tests for each.
- `warp-station-core/src/rel_time.rs`: `day_label` for the Commits tab.
- `net.rs`: `GitHubPullDiff`, `GitHubPullCommitFiles`, `GitHubPullAction` and
  their events; the pull view rides the boxed `GitHubPullLoaded`.
- `store/mod.rs`: `pull_diff`, `pull_commit_files`, `pull_acting`,
  `pull_comment_sent`, `confirm_abandon`, `pane_width`, and the verb/abandon
  helpers.
- `ui/pull_request.rs`: `PullRequestView` — head, status strip, the three tabs,
  the policy and reviewer cards, the comments composer and the two menus.
- `ui/pr_issues.rs`: the workbench, its rows, the auto-selection and Back.
- `ui/diff_controls.rs`: the shared diff chrome, also used by `changes.rs`.
- `ui/dialogs.rs`, `ui/repos.rs`, `ui/detail.rs`: the abandon card, the pane
  width, and the tab's Escape order.
- `tests/split_probe.rs`: nine probes — the workbench beside the list, the
  narrow pane's Back, the three tab bodies, the verbs, the composer, and the
  form's four (opens in the pane, previews the branches, refuses what gh would
  refuse, creates and selects, takes the narrow pane).
- `ui/new_pull.rs`: `NewPullView` — the branch row with its swap, the three
  tabs, the fields, the error banner and the Create split.
- `warp-station-core`: `git::read::compare` (with its three tests),
  `github::create_pull` / `NewPullRequest`, and `ComparePreview` /
  `CompareCommit` in the model.
- `WARP_STATION_NEW_PULL=1`, a capture hook for the form, documented beside the
  other hooks.
- `warp-station-core/src/github.rs`: `GitHubIssue` with `issue_view` and its
  parser (`state,author,labels,assignees,createdAt,updatedAt,body,comments`),
  and two tests for the shape and for a bare payload.
- `ui/issue_detail.rs`: `IssueDetailView` — title row with Back and the one
  filled primary, the subtitle, the `base` status strip, the description card
  and the comment thread.
- `ui/pr_issues.rs`: one workbench for both tabs, `render_issue_row`, the issue
  selection's Escape/Open-Closed handling; the old full-width issue list and its
  row shape are gone.
- `ui/pull_request.rs`: `column_width`, and the Files and Commits rails drawn as
  a band under 900 px of column.
- `net.rs` / `store/mod.rs`: `GitHubIssue` + `GitHubIssueLoaded`, `OpenIssue`,
  `open_issue`, `close_issue`, `reload_open_issue`.
- `tests/split_probe.rs`: three probes — the Issues workbench and its lazy ask,
  the narrow issue pane's Back, and the rail stacking at 1920 px and 1500 px.

- `warp-station-core/src/azure.rs`: `PipelineRun`'s `source_version`, `reason`,
  `start`, `finish` with `state_word`, `trigger_word` and `short_source`;
  `RunJob`, `RunStage`, `RunDetail`, `run_detail`, `parse_timeline`,
  `duration_between`; `AzurePull`, `AzureReviewer`, `AzurePolicy`,
  `AzureThread`, `AzureCommit`, `AzureFile`, `AzurePullDetail`,
  `AzurePullAction`, `pull_detail`, `pull_files`, `pull_commits`,
  `pull_file_diff`, `pull_action`, the parsers and `unified_diff` (the
  whole-file diff). Nineteen unit tests.
- `net.rs`: `AzureRunDetail` and the four `AzurePull*` commands with their
  events, all on the read intake so a detail never queues behind a pass.
- `store/mod.rs`: `OpenRun`, `open_run` / `close_run` / `load_run_detail`;
  `OpenAzurePull`, `open_azure_pull` / `close_azure_pull`, the lazy
  `load_azure_pull` / `load_azure_pull_files` / `load_azure_pull_commits` /
  `load_azure_pull_diff`, `act_on_azure_pull`, and the
  `ask_abandon_azure` / `cancel_abandon_azure` / `confirm_abandon_azure` set.
- `ui/azure_tab.rs`: the Pipelines workbench — the run list, its rows, the run
  detail with its stage cards and log card, and the project's item groups.
- `ui/azure_pull.rs`: `AzurePullView` — head, strip, Overview (policies,
  description, comments, reviewers), Files (rail + diff, stacked under 900 px),
  Commits, and the two menus.
- `ui/dialogs.rs`, `ui/repos.rs`, `ui/detail.rs`: the Azure abandon card, its
  Escape order, the tab's dismiss and the view's construction.
- `tests/azure_probe.rs`: seven probes — the run workbench and its lazy ask,
  a repository without runs, the Azure head/Overview, the Files tab's two asks,
  every verb, completing in two presses, and the narrow pane's Back.

## 4. Open work

1. **The Azure work item's detail.** Its row is in the merged Issues list, but
   a work item has no pane of its own yet: the row opens on Azure DevOps. The
   design's issue detail wants state, type, assignee, description and activity
   for it, which is one more REST read (`wit/workitems/{id}` plus its comments).
   The other two questions are settled: the union list landed
   (`plans/azure-github-activity-merge.md`) and the Azure pull request opens in
   the merged list's column.
2. **GitHub Actions runs — done.** `github::refresh` lists ten runs with the
   same pass, `activity.rs` makes them one vocabulary with Azure's, and the
   workbench interleaves the two newest-first one row at a time; an Actions run
   opens its own detail from `gh run view`. A GitHub-only repository gets the
   tab (`Store::detail_tabs`). Reported in that plan's D2/D5.
3. **Azure's `+adds −dels`.** Azure serves no line counts with its change list;
   the counts the GitHub subtitle carries are absent on Azure (D17). Filling
   them means diffing every file on open, which is the one thing the pane
   deliberately does not do up front.
4. Fold what is durable into `apps/warp-station/AGENTS.md` or delete this plan
   once 1 and 3 are decided.
