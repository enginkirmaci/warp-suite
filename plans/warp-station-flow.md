# The push → pull request → pipeline flow

Status: 2026-11-13 — implemented. Decisions: screen advance only (Create and
Merge stay one click each), GitHub and Azure both covered, and a watched run
keeps polling after the Pipelines tab is left behind with a switch to turn
that off. The "What else" list below is the unstarted backlog.

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
- `Store::start_new_pull()` switches to Pull requests and opens the in-pane form
  on GitHub. Azure has no in-app form, so the button opens the project's
  `pullrequestcreate` page (the same place the list's **New** goes).
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
- The toast is not clickable yet; opening the run stays a Pipelines action.

## 4. Workflow settings

New `workflow` section in `~/.warp/station/settings.json`:

| Key | Default | Meaning |
|---|---|---|
| `auto_advance` | false | master: the two screen advances below |
| `open_pull_request_after_push` | true | after a successful push/publish, open the Create PR step (GitHub's in-pane form; Azure's create page stays a click) |
| `open_pipelines_after_merge` | true | after a merge, run the Pipelines watch |
| `watch_in_background` | true | keep polling the watched run after leaving Pipelines |

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
  otherwise GitHub owns Create (in-app form) and Azure provides the page.
- **A pull request already open for the branch** (snapshot stale): the button
  says View; the form still refuses a duplicate with `gh`'s own message.
- **Azure's create page**: the auto-advance only opens the in-app GitHub form;
  launching a browser without a click is more than the switch promises, so the
  Azure button is shown and its page waits for the press.
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

Ordered by value against cost.

1. **A flow strip instead of three one-off buttons.** After a push, the Changes
   head shows the whole next path — `Create PR → Merge → Pipeline → Result` —
   with the current step filled and the completed ones as links. It explains
   itself, survives a restart, and makes the automation one visible state
   rather than three separate reveals.
2. **A watched-runs list, not one run.** The merge might start several
   workflows on the target branch. `Vec<Watch>` with per-branch notifications
   ("2 of 3 passed") would report the whole merge gate, at the cost of more list
   diffing and noise. Start with one run; add a "watch all runs on branch"
   switch if the user asks.
3. **Make the pipeline toast actionable.** A toast with an "Open run" action
   (and a desktop notification click) would remove the last hunt. The toast
   component has no action slot today; this is a kit change, not an app one.
4. **Auto-fill the form without opening the PR.** On the post-push advance,
   prefill the title with the tip commit's subject and the body with the
   commit bodies; the user still reviews and presses Create. That is the safest
   "less clicking" step and uses data the History pass has.
5. **Auto-create only for a draft.** `gh pr create --fill --draft` behind a
   separate switch lets the team triage drafts; the user asked for no-click, but
   a draft is reversible where a real PR is social.
6. **Persist the flow step per branch.** Today the View-vs-Create verb is
   derived from the live snapshot. A small per-branch record ("PR opened",
   "merged at", "run id") would make the step survive a restart and would let
   the strip say "Pipeline 2m ago" without another `gh` call.
7. **A branch filter that persists across a repo switch is a trap** — the
   current session-only focus is deliberate. If it should survive, it belongs
   in `settings.json` with a timestamp, so a stale focus cannot hide every run
   on the next launch.
8. **Azure in-app PR creation.** The POST is straightforward but needs a
   project/repository id, a default reviewer and a title/body form; the browser
   page is the honest shortcut until the form exists on both platforms.
9. **Notifications for multi-stage pipelines.** The run's state is the whole
   pipeline; stage or job failures are already in the detail. A notification per
   stage would be noise.
10. **Keyboard reach.** The two new verbs should also answer a shortcut once the
    app has a command surface; today the toolbar is mouse-first.

## Verification

- `cargo test -p warp-station -p warp-station-core` (the probes: `follow_probe`
  gains the notification and background-watch cases; `changes_probe` and
  `settings_probe` the new controls).
- `cargo fmt --all --check`, `cargo clippy --all-targets -p warp-station -p warp-station-core -- -D warnings`.
- `changes_probe` reads this repository's own history, so run it from the full
  clone.
- Headless draw of the Workflow section and the Pipelines branch chip; no
  network in probes (events are injected exactly as the net thread sends them).
