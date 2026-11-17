# Worktrees live in their repository

_Status: accepted._

A worktree is a second working directory of one repository, so it lives inside
it — `<parent repo>/.worktrees/<slug>`, one flat level — and both apps model it
as a child of that repo: Warp Station lists it under the repo and never as a
scanned repo of its own, warp-agent hides its workspace row and shows its chats
under the parent's, and the kit writes `.worktrees/` into the repo's
`.git/info/exclude` so the parent's own changes stay clean. The deciding factor
was that `git worktree list` stays the single source of truth: a checkout a
plain `git worktree add` would have produced is exactly one the apps can find,
and nothing is hidden in an app-owned namespace.

Considered options: an app-owned `~/.warp/worktrees/<repo>/<branch>` (clean
repos, but checkouts on another filesystem and invisible to anyone driving git
by hand), sibling directories `<repo>-<branch>` (litters the parent of every
checkout and breaks the "scan root holds repos" mental model), and a committed
`.gitignore` entry instead of `info/exclude` (forces a diff into the user's
repo for an app-private convention).

Consequences: Station's identity stays the parent folder, so tabs, the flow
cache and every per-folder map are unaffected by a worktree; removal is a
Station action and never follows a chat deletion; two apps share one `git`
CLI wrapper (the kit crate) rather than one of them growing git support.
