# The push → pull request → pipeline flow

Status: 2026-11-13 — the follow-up backlog landed: the four-step flow strip
with its per-branch cache, the watch-all merge gate, the actionable pipeline
toast, the form prefill, and Azure's in-app create. Decisions: screen advance
only (Create and Merge stay one click each), GitHub and Azure both covered, a
watched run keeps polling after the Pipelines tab is left behind with a switch
to turn that off, and the branch focus stays session-only. The "What else" list
keeps the remaining ideas (auto-create draft, stage notifications, keyboard
reach).

## Goal

One continuous path through the workbench:

1. commit → push (or publish) → **Create PR** appears in the Changes toolbar;
2. merge on the pull request screen → **Pipelines** appears next to the state;
3. the Pipelines tab opens focused on the merged branch, follows its newest run
   and **notifies** when that run ends;
4. a Workflow settings section can advance the first two screens by itself.

The design keeps every irreversible action a user press. Push stays manual,
Create opens the form (it does not create), Merge keeps its arm-and-confirm, and
the automation only changes which screen is showing.

## 1. Create PR after a push

**Where** — `ui/changes.rs`, the git toolbar after Push/Publish/Sync;
`store/mod.rs` owns the decision.

- `Store::pull_shortcut()` answers `Create { source }` or `Open { source, number }`
  from the selected repository:
  - the branch is local, named, has a remote, is published, and nothing is left
    to push (`git_branch_published && git_to_push_count == 0`);
  - the branch is not the default branch (`github_details.default_branch`, else
    `main`/`master`);
  - an open pull request whose head is that branch turns the verb into
    **View PR #N** instead of **Create PR**;
  - a source is available only when its switch is on and it resolved this
    repository (GitHub: `gh` plus a URL/snapshot; Azure: a PAT plus a snapshot).
- The toolbar never shows the verb while there is nothing sensible to do. It is
  a state, not a one-shot: after a restart a pushed branch with no PR still
  offers it.
- `Store::start_new_pull()` switches to Pull requests and opens the in-pane
  form on either platform. The form's Create writes GitHub through
  `gh pr create` and Azure through `azure::create_pull` (POST
  `…/pullrequests?api-version=7.1`); the optional reviewer is passed to GitHub
  as-is and resolved against the Azure organization's people for Azure.
- `Store::open_pull_shortcut()` opens the existing row: GitHub's Pull requests
  tab, Azure's Pipelines tab (where an Azure pull request lives).

**Does not** create a PR. `gh pr create` needs a title and a body; the form is
where those are reviewed, and `--fill` would silently take a commit message.

## 2. Pipeline button after a merge

**Where** — `ui/pull_request.rs` (`head_actions`, merged/closed state) and
`ui/azure_pull.rs` (completed state).

- GitHub: when the pull request is `MERGED`, a **Pipelines** button sits beside
  the state. Azure: when the pull request is `completed`, the button replaces
  the now-empty primary slot.
- The button calls `Store::watch_pipelines_for(branch)` with the merge target
  (`base_ref_name` / Azure's `target`), not the head branch: the merge commit
  and its run land on the target.
- `watch_pipelines_for` switches to the Pipelines tab, closes the Azure
  pull-request detail (it shares that column), sets a session branch focus,
  and either opens the newest run on that branch **at or after the merge**
  (one minute of clock slack; a row with no timestamp is not excluded) or
  waits for one to appear. An older run on the same branch is not mistaken for
  the merge's run.
- The branch focus is a dismissible chip in the Pipelines toolbar
  (`pipelines-branch`). **Clear** returns the full list; a new watch replaces it.
  It is session-only; a repository switch drops it.

**Waiting for the run.** A merge-triggered run can take seconds to appear. While
the focus has no match, the store polls the lightweight runs list every 15 s for
up to five minutes (`PIPELINE_WATCH` / `PIPELINE_WATCH_MAX`), only while the
Pipelines tab is on screen or background watching is on. The list header says
"Waiting for a run on `<branch>`…" with the existing spinner; the first match is
opened and followed. A merge whose pipeline never starts simply stops waiting
silently — no error toast for an empty pipeline.

## 3. Run-end notification

**Where** — `Store::apply_run_detail` and the notice service.

- The first detail answer that says `Succeeded`, `Failed` or `Skipped` while the
  pane was following a going run raises one toast:
  - `Pipeline <name> <label> passed` (success),
  - `Pipeline <name> <label> failed` (error),
  - `Pipeline <name> <label> was skipped` (info).
- One per run: the follow loop's second (log) answer and any later list refresh
  must not repeat it. `OpenRun::announce_finish` marks a run opened by the
  auto-merge advance, so a fast pipeline that finished before its detail
  landed still notifies once; a run the user opened from the list or the
  button never notifies on open.
- A new desktop category, `notifications.pipelines` (default true), gates the
  desktop copy only. The in-app toast always shows, like every other notice.
- The toast has one **Open run** action: it switches to Pipelines and opens the
  run the toast named. The desktop copy has no click callback on this path.

## 4. Workflow settings

New `workflow` section in `~/.warp/station/settings.json`:

| Key | Default | Meaning |
|---|---|---|
| `auto_advance` | false | master: the two screen advances below |
| `open_pull_request_after_push` | true | after a successful push/publish, open the Create PR step in the pane |
| `open_pipelines_after_merge` | true | after a merge, run the Pipelines watch |
| `watch_in_background` | true | keep polling the watched run after leaving Pipelines |
| `watch_all_runs_on_branch` | false | tally every run the merge starts and toast one gate result |

- The `auto_advance` master gates the two navigation switches. With it off the
  buttons still appear and still work; nothing moves on its own.
- `watch_in_background` is independent and on by default: it changes only a
  run the user (or the merge advance) is already watching, so it never starts a
  poll on its own; the switch is the way to stop it.
- The Settings page gets a **Workflow** section after Git and a **Pipeline
  results** switch under Notifications → What to hear about.
- `docs/agents/settings-schema.md` records the keys and defaults. The app's
  `AGENTS.md` "Pipelines follow" paragraph gains the background exception.

**Push success hook.** `UiEvent::GitDone` with `label` in `push|publish` and no
error, while that folder is selected, calls the advance. It runs before the
workspace reload, so the branch's `git_branch_published` may still be false;
the form does not need it. A push in a repo the user has since left does not
steal the screen.

**Merge success hook.** `UiEvent::GitHubMerged` (Ok) and
`UiEvent::AzurePullActed` (Complete, Ok) capture the target branch before the
record reload clears it. The advance and the button share
`watch_pipelines_for`, so turning the switch on never changes what the button
does.

## Edge cases

- **Detached HEAD / no upstream**: no Create verb.
- **Default branch**: no Create verb; pushing `main` has nothing to open.
- **Both platforms on**: an open GitHub PR on the branch wins the View verb;
  otherwise GitHub owns Create in the pane. An open Azure PR wins when there
  is no GitHub one; Create falls back to Azure's form when GitHub is off.
- **A pull request already open for the branch** (snapshot stale): the button
  says View; the form still refuses a duplicate with `gh`'s own message.
- **Azure's create form**: the same in-pane form. A typed reviewer name or
  sign-in name is matched against the organization's people and sent as the
  identity GUID; no match refuses the create instead of silently dropping it.
- **Repo switched while watching**: the workspace reset drops the run, the
  watch and the branch focus; the notification does not follow across repos.
- **Pipelines tab hidden with background watching on**: only the watched run's
  detail cadence (13 s GitHub / 20 s Azure) and its closing calls continue.
  While a merge watch still has no run, its bounded 15 s list read continues
  too; that is the budget the follow loop documents, with no second detail
  cadence.
- **Rate limit**: the existing `RATE_RESERVE` parking applies to the watch's
  list polls too (they go through `follow_runs`, which reads the budget).
- **Narrow pane (< 900 px)**: the branch focus shows the filtered list; the run
  is not auto-opened, so the notification waits for the user's tap. The merge
  button still lands on the right list.

## What else / better ways

Landed: the four-step flow strip and its `cache/flow.json`; the watch-all
merge gate (`watch_all_runs_on_branch`); the toast's in-app **Open run**
action; the commit-message prefill; Azure's in-pane create form. The branch focus
stays session-only by decision.

Still open:

- **Auto-create only for a draft.** `gh pr create --fill --draft` behind a
  separate switch lets the team triage drafts; the user asked for no-click, but
  a draft is reversible where a real PR is social.
- **Notifications for multi-stage pipelines.** The run's state is the whole
  pipeline; stage or job failures are already in the detail. A notification per
  stage would be noise.
- **Keyboard reach.** The verbs should also answer a shortcut once the app has
  a command surface; today the toolbar is mouse-first.

## Verification

- `cargo test -p warp-station -p warp-station-core` (the probes: `follow_probe`
  gains the prefill, toast-action, watch-all gate and Azure-create cases;
  `changes_probe` the strip; `settings_probe` the watch-all row).
- `cargo fmt --all --check`, `cargo clippy --all-targets -p warp-station -p warp-station-core -- -D warnings`.
- `changes_probe` reads this repository's own history, so run it from the full
  clone.
- Headless draw of the Workflow section and the Pipelines branch chip; no
  network in probes (events are injected exactly as the net thread sends them).
