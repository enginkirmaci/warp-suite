# warpui-kit-worktree

The suite's one implementation of git worktrees: a second working directory of
one repository, checked out on its own branch, so a chat can work without
moving the checkout its owner is looking at.

## Rules

- The folder is `<parent repo>/.worktrees/<slug>`, one flat level, where the
  slug is the branch with its separators flattened (`agent/fix-login` →
  `agent-fix-login`).
- The branch is the caller's choice; a chat derives it from its first prompt
  with `branch_name` at send time. A name that is taken gets `-2`, `-3`, …
  instead of attaching an existing branch.
- `worktrees/` goes into the repository's local `.git/info/exclude` once, so the
  parent's own changes stay clean without touching a tracked file.
- Creation starts at the current HEAD. A repository with no commit yet is
  refused; a detached HEAD is fine.
- Removal refuses a dirty worktree unless the caller forces it, and never
  deletes the branch.

The crate drives the `git` command line (`GIT_TERMINAL_PROMPT=0`, a timeout, no
console window on Windows) and links no `git2`. Both apps call it so they cannot
drift apart on naming, location or cleanup; the decision is recorded in
[`docs/adr/0001-worktrees-live-in-their-repo.md`](../../docs/adr/0001-worktrees-live-in-their-repo.md).

## Usage

```rust
use std::path::Path;
use warpui_kit_worktree as worktree;

fn start_chat(repo: &Path, title: &str) -> anyhow::Result<worktree::Worktree> {
    if !worktree::is_repository(repo) {
        anyhow::bail!("{title} needs a git repository");
    }
    let branch = worktree::branch_name(title);
    worktree::create(repo, &branch)
}

fn the_open_ones(repo: &Path) -> anyhow::Result<Vec<worktree::Worktree>> {
    let mut open: Vec<_> = worktree::list(repo)?
        .into_iter()
        .filter(|entry| !entry.main)
        .collect();
    open.sort_by_key(worktree::Worktree::label);
    Ok(open)
}
```

## Tests

The tests drive real repositories in throwaway folders, because the crate's
contract *is* the git command line:

```sh
cargo test -p warpui-kit-worktree
```
