# The pull-request surface

Status: in progress. Phases 1–5 landed (core data, net/store, the workbench, the
pull request's own three tabs and its verbs, the embedded New pull request
form), plus the Issues workbench and the Files rail stacking. Open: the Azure
pull-request detail, blocked on the merged list another plan owns, and the
Pipelines detail, which has no data to draw. The design source is the OpenDesign
project `warp-suite-v3` — `DESIGN.md` §5 "Work lists", "Pull requests tab",
"Pull request", "New pull request", "Issues tab" — and the prototype's
`prList`, `prDetail`, `newPrView` and `issueDetail`.

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

## 4. Open work

1. **Azure pull requests — reported, not built.** The design wants an Azure
   pull request in the same pane, and the app cannot reach one: the Pull
   requests list carries GitHub rows only, so nothing selects an Azure PR. The
   union list (the `!214` reference and the `Source` tag) belongs to
   `plans/azure-github-activity-merge.md`, which is another session's untracked
   work; this plan does not own it and does not implement it.

   What the client needs once the list exists, per pull request:
   - **The record:** `GET {base}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}?api-version=7.1`
     for state (`active`/`completed`/`abandoned`), title, `sourceRefName` →
     `targetRefName`, `isDraft`, `mergeStatus`, `createdBy`, `creationDate`,
     `closedDate`, `lastMergeSourceCommit`.
   - **Policies:** `.../pullrequests/{id}/evaluations?api-version=7.1` for the
     checklist — required reviewers, build validation, merge conflicts — which
     is the Azure spelling of the policy card's rows.
   - **Reviewers:** `.../pullrequests/{id}/reviewers` for the vote
     (`10` approved, `5` approved with suggestions, `0` no vote, `-5` waiting,
     `-10` rejected) and the required/optional flag.
   - **Files and commits:** `.../pullrequests/{id}/iterations` then
     `.../iterations/{n}/changes` for the changed files, and
     `.../pullrequests/{id}/commits` for the commit list; Azure returns no patch
     text with the change list, so a file's diff is a second call per file
     (`.../items?path=…&includeContent=true` on both sides) or a `git diff`
     against the fetched refs.
   - **Threads:** `.../pullrequests/{id}/threads` for comments, each with its
     `status` (`active`, `fixed`, `wontFix`, `closed`) — the "comments must be
     resolved" policy row reads from here.
   - **Verbs:** reviewer vote `PUT .../reviewers/{reviewerId}`;
     `PATCH .../pullrequests/{id}` with `status: "completed"` +
     `lastMergeSourceCommit` + `completionOptions` to complete,
     `status: "abandoned"` to abandon, `status: "active"` to reopen; and
     `PUT .../pullrequests/{id}/autocomplete` to arm auto-complete. Approve and
     decline are votes, not `gh pr review`.
   - **The pane:** the design's Azure primary is `Set auto-complete` /
     `Complete` / `Auto-complete set`, the state pill reads Active/Completed/
     Abandoned, and the review menu is Approve/Decline rather than
     Approve/Request changes. Everything else — the workbench, the strip, the
     cards — is the GitHub view's shape with a second data source behind it.

2. **Pipelines — reported, not built.** Its design detail (stages with job rows,
   a log excerpt, the run's trigger and duration, one `Open run` primary) needs
   data neither snapshot carries: the Azure snapshot has runs but no stages,
   jobs or logs, and the GitHub snapshot has no Actions runs at all — `gh run
   list` is a different plan's work (`plans/azure-github-activity-merge.md`,
   D2). Until that lands, the tab keeps the shape it has rather than a detail
   that would be empty.

3. Delete this plan when 1–2 land, or fold the durable parts into
   `apps/warp-station/AGENTS.md`.
