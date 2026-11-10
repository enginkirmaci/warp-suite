# Azure DevOps × GitHub: one activity surface

Status: proposal. No Rust change has landed. The OpenDesign prototype
(`warp-suite-v3`) has been updated to this shape so the plan can be reviewed as
pixels. Decisions D1–D6 below are proposals; settle them before phase 1.

## 1. What changes

The three activity tabs stop meaning "GitHub" or "Azure DevOps" and start
meaning the activity itself:

| Surface | Before | After |
|---|---|---|
| Pipelines | Azure project: recent Azure runs **plus** the project's active PRs and work items as cards | Recent runs only, from Azure Pipelines **and** GitHub Actions |
| Pull requests | GitHub pulls only | GitHub pulls and Azure pull requests in one list |
| Issues | GitHub issues only | GitHub issues and Azure work items in one list |
| Repo row / tab strip | GitHub counts, plus separate Azure `PR n` / `WI n` chips | One PR total and one issue total across both sources; one pipeline-state glyph |

A repository is normally on one platform; the union also covers a migration
window where both remotes resolve. The tabs are shown by capability, not by
platform, so an Azure-only repository gets Pull requests and Issues and a
GitHub repository gets Pipelines.

Out of scope for this plan: Azure pull-request detail/merge inside the pane,
GitHub Actions log tails, service hooks/webhooks, and a combined check
dashboard. Azure PRs open on Azure DevOps; GitHub PRs keep the in-app pane.

## 2. Decisions

- **D1 — the source is a row fact.** Every merged row carries a quiet
  `GitHub` or `Azure` tag next to its metadata. The platform also decides the
  reference prefix (`#482`, Azure PR `!214`, work item `#3381`) and where the
  row opens. Always showing the tag is cheaper to read than inferring it from
  the repo, and it makes a mixed list unambiguous. The tag is a compact variant
  of the existing `tag` component (`text-3` on the raised surface); it
  introduces no colour.
- **D2 — Actions runs join the existing pass.** `github::refresh` gains one
  `gh run list --limit 10` call, joined with the existing PR/issue list calls.
  The full pass runs every 15 minutes, and one more `gh` process per repo is
  cheaper than a second scheduler and a second cache. If a large repo list
  makes the pass visibly slower, the fallback is a lazy `gh run list` on the
  Pipelines tab; the data type stays the same.
- **D3 — Azure PR rows open the browser.** Only GitHub items can enter the
  in-app pull-request pane because that pane reads `gh pr view` and merges
  through `gh`. Phase 1 keeps that boundary. An Azure PR detail is a later
  feature, not a blocker for the merged list.
- **D4 — Azure closed lists are on demand.** The Open/Closed toggle must mean
  the same thing for both sources. Azure gets a closed fetch that mirrors the
  GitHub one: pull requests by `completed`/`abandoned`, work items whose state
  is `Done`/`Closed`/`Removed`. The cache key becomes
  `(folder, kind, source)`; a missing answer is retryable, not empty.
- **D5 — Pipelines is one runs card.** The toolbar carries the latest run's
  state (with its source), and the body is one full-width "Recent runs" card,
  newest first. The two Azure item cards are deleted. A repository that resolves
  both sources shows one interleaved list with a source tag per row; no source
  filter is needed while a repository normally resolves one remote.
- **D6 — totals live on the repo entry.** `RepoEntry` gains
  `azure_pull_count` and `azure_work_item_count` (serde defaults, written when
  an Azure snapshot settles). The rail, projection, list summary, tab strip
  and Overview sum the GitHub and Azure fields. Snapshot lengths remain the
  source of truth for the open lists; the entry fields exist so sorting and
  persistence do not need to walk both maps on every frame.

## 3. Core data

### 3.1 `activity.rs` (new, headless)

One vocabulary for the three surfaces:

- `ActivitySource { GitHub, Azure }` with `label()` (`"GitHub"`, `"Azure"`) and
  the open-in-browser wording.
- `ActivityKind { PullRequest, Issue }` with the prefix rule: GitHub both `#`,
  Azure `PullRequest` `!`, Azure `Issue` `#`.
- `ActivityItem { source, kind, number, title, url, author, labels, state,
  is_draft, review_decision, updated_at, created_at, comments, head_ref,
  base_ref, work_item_type, assigned_to }`. `reference()` and `matches()` live
  here. GitHub and Azure constructors keep the mapping in one place.
- `ActivityRun { source, id, name, run_label, branch, state, url, at }` with
  `is_failed()`, `is_running()`, `is_succeeded()`, `display_branch()`.
- `ActivityCounts { pulls, issues, runs }` for the rail and tab strip.
- `IssueSort` moves here from `github.rs`; it sorts `ActivityItem` by updated,
  created, comments. GitHub's parser re-exports or uses the shared type.

No network, no UI and no gpui in this module.

### 3.2 `github.rs`

- `GitHubRun { id, name, display_title, branch, status, conclusion, url,
  created_at }`; `conclusion` may be absent while queued or running.
- `GitHubSnapshot.runs: Vec<GitHubRun>`.
- `refresh` runs `gh run list --limit 10 --json
  databaseId,workflowName,displayTitle,headBranch,status,conclusion,url,createdAt`
  in the same `tokio::join!` as the two list calls. Parse defensively like the
  other gh payloads.
- The parsers already tolerate shape drift; a missing runs payload is an empty
  list, not `available: false`.
- `IssueSort` is removed from here once `activity.rs` owns it; existing imports
  move with it.

### 3.3 `azure.rs`

- `AzureItem` gains the fields the merged rows need: `source_ref_name`,
  `target_ref_name` (PRs), `work_item_type`, `assigned_to`, `changed_at`
  (work items), and `is_draft`/`state` stay. The PR fetch requests
  `sourceRefName,targetRefName`; the WIQL selects
  `System.WorkItemType,System.AssignedTo,System.ChangedDate`.
- `PipelineRun` gains `at` from `queueTime`/`finishTime`, so the merged run list
  can sort across sources.
- The snapshot keeps the web base for building `new_pull_request_url()` and
  `new_work_item_url()`; the create buttons use them.
- `closed_items(folder, pat, custom_base, pull_requests) -> Option<Vec<AzureItem>>`
  mirrors `github::closed_items`; `None` is a failed call.
- Keep one `refresh` path and one 401 rule; do not add a second Azure client.

### 3.4 `repo_store.rs`

- Add `azure_pull_count: u32` and `azure_work_item_count: u32` with
  `#[serde(default)]` to `RepoEntry`, the cache view and the round-trip
  conversion.
- Existing `github_pr_count` / `github_issue_count` keep their names; totals
  are computed at the call site so no migration rewrites cached files.

## 4. Net and store

- **GitHub pass**: one extra `gh run list` per repo inside the existing
  two-way throttled pass. `GitHubSettled` stores the runs with the snapshot and
  updates the GitHub counts as it does today.
- **Azure pass**: unchanged shape. `AzureSettled` updates
  `azure_pull_count`/`azure_work_item_count` and persists the repo cache when
  they change, exactly like `GitHubSettled`.
- **Closed lists**: generalise the GitHub-only `GitHubClosedLoaded` event and
  cache key to `(folder, kind, source)`, or add one Azure event beside it if the
  generalisation spreads too far. The tab must be able to say "one source
  answered, the other is still loading".
- **New item commands**: `AzureClosed { folder, pat, custom_base, pull_requests }`
  next to `GitHubClosed`.
- **Store helpers**: `activity_counts(folder)`, `has_github(folder)`,
  `has_azure(folder)` and `latest_run(folder)` read both snapshot maps. The
  selected repo's counts feed the tab strip; the rail reads the entry fields.
- **Refresh panel**: one press re-asks both passes for the selected repo and
  clears the closed caches for it, so the merged list cannot show one fresh
  source beside a stale one.

## 5. UI

### 5.1 Detail tab strip (`detail.rs`)

- Pull requests and Issues are available when GitHub or Azure is enabled and
  has a snapshot (loading still shows the tab for GitHub, as today).
- Pipelines is available for GitHub too; it no longer waits for an Azure
  project.
- Counts: Pull requests = `github_pr_count + azure_pull_count`; Issues =
  `github_issue_count + azure_work_item_count`; Pipelines = the loaded run
  count (no persisted field).

### 5.2 `pr_issues.rs` → `activity_list.rs`

- One `ActivityListView` serves both tabs; `pull_requests: bool` stays the only
  difference.
- Open rows are the union of the GitHub open list and the Azure active/open
  snapshot, in the source's own newest-first order. A stable interleave is
  enough; no cross-platform sort is implied.
- Closed rows read the generalized closed cache; `load_closed` asks the
  enabled sources.
- Loading, failed and unavailable states are per source: if GitHub is still
  loading and Azure already answered, show the Azure rows now and a quiet
  "GitHub is loading" note, not a full-page spinner.
- Rows use `ActivityItem`: leading glyph by kind, platform reference and title,
  then head → base (PRs) or type · state · assignee/labels (issues), then a
  source tag. The right cluster keeps author/assignee, age and the PR state
  pill.
- A click on a GitHub PR row still opens the in-app pane; every other row opens
  its URL. The external-link affordance and the `a11y` label say the platform.
- Issues keeps the filter and Sort menu; both operate on the union. `Most
  comments` treats a work item as 0 comments.
- The New verb is per source: `New pull request` / `New issue` on GitHub,
  `New pull request` / `New work item` on Azure; hidden when that source has no
  web base.

### 5.3 `azure_tab.rs` → `pipelines_tab.rs`

- `PipelinesView` reads the GitHub snapshot's runs and the Azure snapshot's
  pipelines.
- Toolbar: latest run's state and name, plus its source tag; Refresh unchanged.
- Body: one full-width card of `ActivityRun` rows. Each row: state glyph,
  workflow/pipeline name and run/build label, then branch · state word · source
  tag · age, and an open-in-new control.
- The failed tint stays for either source; a running row spins.
- Empty states: no runs loaded yet, no runs at all, and one source unavailable
  (the other's runs still show).
- The `Pipelines` / `Pull requests` / `Work items` cards and `items_card` /
  `item_row` are deleted.

### 5.4 Repo row and list summary (`repos_list.rs`, `repos.rs`)

- `stats` takes both source switches and sums the entry counts; the separate
  Azure `PR n` / `WI n` chip goes away.
- One pipeline glyph remains, from the newest run across GitHub Actions and
  Azure Pipelines. It keeps a debug selector (`repo-pipeline-{ix}`) so probes
  can see it; the old `repo-azure-{ix}` selector is retired.
- The list head's summary sums both issue counts.

### 5.5 Overview (`overview.rs`)

- "Needs you" considers the union: the first open PR (GitHub first, then
  Azure), the first open issue/work item (same order), and the newest failed
  run from either source. The row's subtitle names the source.
- An Azure PR/issue row's Open action goes to the corresponding list tab (the
  in-app detail does not exist yet); a GitHub PR row keeps `open_pull_request`.
- The pipeline row's debug selector becomes `insp-pipeline-row`, with
  `insp-pipeline-open` for its link; keep the old selectors as aliases for one
  release if a probe depends on them.

### 5.6 Settings copy

The GitHub and Azure rows stay separate switches. The section subtitle changes
from "Notes, GitHub and Azure DevOps" only if the settings page needs the
clarification; no schema change and no new toggle.

## 6. Tests

Core:

- `activity`: reference prefixes, source labels, filter matching, sort order for
  an interleaved list.
- `github`: runs parse both a completed and a running payload; a missing
  `conclusion` is running; an empty runs list does not clear `available`.
- `azure`: PR branches and draft state parse; work-item type/assignee/date
  parse; closed WIQL and closed PR statuses produce the expected items; 401 is
  still `None`/unavailable; the new-item URLs carry the project and repository.
- `repo_store`: the two Azure counts survive a cache round trip and default to
  zero for older files.

Headless probes:

- `azure_work_items_join_the_issues_list`: seed both snapshots; the Issues tab
  shows the GitHub issue and the Azure work item, with the right references and
  source tags.
- `azure_pulls_join_the_pull_requests_list`: the Azure PR row uses `!214` and
  opens the Azure URL; the GitHub row still opens the pane.
- `pipelines_lists_runs_only`: seed an Azure snapshot with runs, PRs and work
  items; the Pipelines pane shows the run rows and no `azure-PR-*` /
  `azure-WI-*` cards.
- `pipelines_is_available_for_github_repos`: a GitHub-only repo gets the tab,
  and the Actions run lands after `GitHubSettled`.
- `counts_sum_both_sources`: rail meta and tab strip show the summed totals; the
  old `repo-azure-*` chip is gone.
- `closed_toggle_asks_both_sources`: Open/Closed swaps both lists and one
  failed source offers Retry without hiding the other's rows.
- Update `split_probe.rs` expectations that assume Azure PR/WI cards or the
  `repo-azure-*` selector.

Run `cargo fmt --all`, then
`cargo test -p warp-station -p warp-station-core`; clippy for the two crates
before calling it done.

## 7. OpenDesign

Project `warp-suite-v3` is the design source. This pass updated:

- `source/app.src.html`: mock Azure PRs/work items and GitHub Actions runs;
  merged Issues and Pull requests lists with source tags; Pipelines as one runs
  card; tab counts and Overview rows updated; the `!r.azure` tab guard removed.
- `source/warp.css`: a compact `.src` tag (12 px, raised surface, `text-3`);
  the existing `.tag` was too tall for a metadata line.
- the built `app.html`: re-bundled from the source (inline CSS, WOFF2 fonts,
  icons); the bundle hashes equal the source inputs.
- `DESIGN.md`: the Pipelines, Issues and Pull requests screen descriptions, the
  repo-row counts and the source-tag rule. `design-system.html` is unchanged
  because the source tag is a variant of `tag`, not a new component.
- `source/design-system.src.html`: unchanged.

The prototype was checked for both samples: `warp-station` (GitHub, Actions),
`warp-asistant` (Azure, Pipelines), and `warpui-kit-picker` (no runs, empty
state). The screenshots live outside the repo; the project is the source of
truth.

## 8. Sequence

1. **Core vocabulary** — `activity.rs`, GitHub runs, Azure fields/closed
   queries, repo counts, unit tests. No UI reads the new types yet.
2. **Wiring** — net commands/events, store maps and helpers, counts and
   persistence. The old views keep compiling.
3. **Pipelines first** — rename `azure_tab.rs`, delete the item cards, render
   the merged runs. This is the smallest visible change and proves the run
   model.
4. **Merged lists** — rename `pr_issues.rs`, render `ActivityItem`, per-source
   closed/loading states, source tags.
5. **Shell surfaces** — rail chip, tab counts, Overview rows, list summary.
6. **Probes and docs** — new probes, fix old selectors, run fmt/test/clippy.
7. **Design sync** — rebuild `app.html` from the updated source, update
   `DESIGN.md`, verify screenshots of both sample repos.
8. **Cleanup** — delete this plan when the Rust work lands; fold the durable
   facts into `apps/warp-station/AGENTS.md` or `README.md`.

## 9. Acceptance

- The Pipelines tab for an Azure repo shows only runs; GitHub-only repos get the
  same tab with Actions runs.
- The Issues tab shows GitHub issues and Azure work items in one list; Pull
  requests does the same for PRs; references and source tags are correct.
- Open/Closed, filter and sort apply to both sources; a failed source is
  retryable and does not erase the other source's rows.
- Rail counts, tab counts and Overview totals include both sources; no separate
  `PR n` / `WI n` Azure chips remain.
- `cargo fmt --all --check`, `cargo test -p warp-station -p warp-station-core`
  and clippy for both crates pass.
- The OpenDesign prototype and `DESIGN.md` show the same shape and the bundle
  is generated from the updated source, not hand-edited.
