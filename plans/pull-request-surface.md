# The pull-request surface

Status: in progress. Phases 1–5 landed: core data, net/store, the workbench, the
pull request's own three tabs and its verbs, and the embedded New pull request
form. Open: the Azure pull-request detail, the Issues and Pipelines workbench,
and the Files rail stacking on a narrow column. The design source is the
OpenDesign project `warp-suite-v3` — `DESIGN.md` §5 "Work lists", "Pull requests
tab", "Pull request", "New pull request" — and the prototype's `prList`,
`prDetail` and `newPrView`.

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

## 4. Open work

1. **Azure pull requests.** `azure.rs` has list items only — no policies,
   reviewers, threads or merge. The design wants them in the same pane; that is
   a second client, not a port. The list, the source tag and the row reference
   (`!214`) are also still GitHub-only here.
2. **Issues and Pipelines workbench.** The design gives all four tabs the same
   split; only Pull requests has it after this pass.
3. **The Files rail on a narrow PR column.** The design stacks the rail above the
   diff under 900 px; the workbench's PR column is under that on a 1500 px
   window and the rail stays beside the diff.
4. Delete this plan when 1–3 land, or fold the durable parts into
   `apps/warp-station/AGENTS.md`.
