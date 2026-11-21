# apps/

Local-only symlinks to the sibling app checkouts, created by
`scripts/dev/link-apps.sh`:

```
apps/warp-station -> ../../warp-station
apps/warp-agent   -> ../../warp-agent
apps/warp-flow    -> ../../warp-flow
apps/warp-notes   -> ../../warp-notes
apps/warp-snapit  -> ../../warp-snapit
apps/warp-pomodoro -> ../../warp-pomodoro
```

The directory itself only carries this file; the links are gitignored. With
them in place the root workspace's `apps/*` member glob builds every app against
the local `kits/` (the `[patch]` table in `Cargo.toml` swaps the git kit
source for the local path).

Every app is a link now. `warp-flow` was the last real directory here — it was
scaffolded inside the suite while its design was still moving — and it moved to
the sibling checkout when S4 landed. Its link is made by hand until its first
release exists: `link_apps.py` takes the app names from `suite.toml`, and that
table pins a tag, so `suite.toml` gains `warp-flow` in the same change window as
its first release.
