# apps/

Local-only symlinks to the sibling app checkouts, created by
`scripts/dev/link-apps.sh`:

```
apps/warp-station -> ../../warp-station
apps/warp-agent   -> ../../warp-agent
apps/warp-notes   -> ../../warp-notes
apps/warp-snapit  -> ../../warp-snapit
```

The directory itself only carries this file; the links are gitignored. With
them in place the root workspace's `apps/*` member glob builds every app against
the local `kits/` (the `[patch]` table in `Cargo.toml` swaps the git kit
source for the local path).
