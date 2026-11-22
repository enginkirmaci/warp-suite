# 03: Document when the Worktrees tab appears

**Status:** ready-for-agent
**Category:** enhancement
**Blocked by:** 02

## What to build

Add the tab's condition to the two places that describe the surface today, so
the docs say why a repository with only its own checkout shows no Worktrees tab.
Documentation only: no source, test or gate file changes.

- In `apps/warp-station/AGENTS.md`, append to the `**Worktrees.**` bullet (after
  "Station's own Changes/History stay on the parent checkout."):
  "The tab is hidden until a listing settles with at least one linked checkout
  (or a failed read); selecting a repository, Refresh and the auto-refresh pass
  check again."
- In `apps/warp-station/README.md`, append to the "A repository's `Worktrees`
  tab lists the linked checkouts…" paragraph: "The tab appears only when the
  listing settles with at least one linked checkout — or when the read failed —
  so a repository with no linked checkout shows no empty Worktrees page;
  selecting the repository, Refresh and the auto-refresh pass check again."
- Keep the wording consistent with 02: an error keeps the tab, a reload keeps a
  drawn tab, and a settled empty answer removes it.

## Where it lives

- `apps/warp-station/AGENTS.md`, the `**Worktrees.**` bullet (~line 72).
- `apps/warp-station/README.md`, the `Worktrees` paragraph (~line 73).

## Done when

- `grep -n "hidden until a listing settles with at least one linked checkout" apps/warp-station/AGENTS.md`
  returns the bullet's line.
- `grep -n "The tab appears only when the listing settles" apps/warp-station/README.md`
  returns the paragraph's line.
- `git diff --name-only` for this task lists exactly those two files.
- Both sentences keep the three triggers (selection, Refresh, auto-refresh) and
  both settle outcomes (a linked checkout or a failed read keeps the tab).
