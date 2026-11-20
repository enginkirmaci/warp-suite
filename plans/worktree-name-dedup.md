# Worktree names: generated at send, never duplicated

Status: naming moved into the send path (done). The composer no longer asks for
or shows a branch name; the first send derives it from the prompt's first line.
The kit already resolves sequential collisions with `-N` suffixes. Open: close
the concurrent-create race, and pick a suffix policy past the current cap.

## Where the name comes from

- One owner for the text: `warpui-kit-worktree::branch_name` (`agent/<slug>`).
  The app calls it once, from `worktree_prepare`, with the draft's first input
  line; the toggle only flips a boolean.
- One owner for uniqueness: `warpui-kit-worktree::unique_branch` — it checks
  both `refs/heads/<branch>` and `<repo>/.worktrees/<slug>`, and `create`
  returns the branch it actually checked out. The app stores that returned
  name, so the sidebar header, the restore path and the send all agree without
  re-deriving anything from the slug.
- The name is fixed by git at creation. A chat title that changes later never
  renames the branch; the branch header in the sidebar is the user-visible
  name from then on.

## What is covered today

- Same prompt, same repository, one after another: the second chat gets
  `agent/<slug>-2`, the third `-3`, and so on (`tests/repository.rs`).
- A leftover folder for a deleted branch counts as taken, so creation cannot
  fail on a path `unique_branch` thought was free.
- A failed create leaves the send in the composer with an error note; the user
  can send again, and the next attempt re-runs the same derivation.

## Gaps

1. **Concurrent creates.** Two drafts with the same prompt in one repository
   can both see `<slug>` free, then race `git worktree add`; one fails. That is
   not a duplicate, but it is a spurious creation failure. The app currently
   starts each `CreateWorktree` on an independent blocking task.
2. **Suffix order is not chat order.** The chat that loses the race or sends
   second gets `-2`, regardless of tile order — acceptable, but worth stating.
3. **The 99 cap** returns "no free worktree name" instead of trying a
   non-numeric suffix.
4. **Truncation collisions.** Titles longer than `MAX_SLUG` fold to the same
   48-byte prefix and therefore rely on suffixes more often.
5. **External actors.** A person running `git worktree add` with the same
   branch between the check and the create still collides.

## Proposed shape

Recommended: serialize per repository in the app and keep the kit as the only
name authority.

- App: one worktree-creation queue keyed by repository root on the net thread,
  so two creates for one repo never overlap. Different repositories still run
  in parallel. No new kit API and no change to the `-N` names.
- Kit: bounded retry inside `create` (for example three attempts) when git
  refuses because the branch or path appeared after `unique_branch`; each retry
  recomputes `unique_branch`. This covers external actors, not just our own
  queue.
- Past the cap, either fail with the current message (simplest, keeps names
  readable) or switch the suffix to a short hash of the chat's tile key
  (unbounded, still deterministic). Decide before touching `unique_branch`.

Rejected: random suffix on every create. It makes the common case unreadable
(`agent/fix-login-x7f2`) to solve a race that serialization fixes.

## Decisions to settle

- Retry count in the kit, and whether an exhausted retry reports the last git
  error or the naming error.
- Cap behavior: keep `-99` + error, or fall back to a hash suffix.
- Whether the sidebar branch header should append the chat count or stay the
  bare branch name when several chats ever share a checkout (they currently
  do not: one chat per worktree).

## Acceptance

- Probe: two drafts in one repository, same prompt, both sends before either
  `WorktreeCreated` lands — both end with distinct branches and folders, and
  neither shows a creation error.
- Probe: sequential same-prompt sends get `<slug>` then `<slug>-2`, and the
  app's stored branch equals the kit's returned branch.
- Probe: the composer renders no name field; automatic naming happens on
  `send_from_tile`, not on the toggle.
- Kit test: a branch created behind `unique_branch`'s back still yields a free
  name after retry.
- Update the worktree bullets in `apps/warp-agent/AGENTS.md` and the
  `warpui-kit-worktree` crate docs once the retry lands.
