# apps/

Local-only symlinks to the sibling app checkouts, created by
`scripts/dev/link-apps.sh`:

```
apps/warp-station  -> ../../warp-station
apps/warp-agent    -> ../../warp-agent
apps/warp-flow     -> ../../warp-flow
apps/warp-notes    -> ../../warp-notes
apps/warp-snapit   -> ../../warp-snapit
apps/warp-pomodoro -> ../../warp-pomodoro
```

The directory itself only carries this file; the links are gitignored. With
them in place the root workspace's `apps/*` member glob builds every app against
the local `kits/` (the `[patch]` table in `Cargo.toml` swaps the git kit
source for the local path).

Every app is a link. `link_apps.py` takes the app names from `suite.toml`, so a
new app's link appears once its tag is pinned there; `--check` reports the
links, `--dry-run` prints what it would create. The app's own repository holds
its code, `AGENTS.md`, `plans/` and packaging.
