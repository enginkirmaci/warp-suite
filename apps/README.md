# apps/

Local-only symlinks to the sibling app checkouts, created by
`scripts/dev/link-apps.sh`:

```
apps/warp-station -> ../../warp-station
apps/warp-agent   -> ../../warp-agent
apps/warp-notes   -> ../../warp-notes
apps/warp-snapit  -> ../../warp-snapit
apps/warp-pomodoro -> ../../warp-pomodoro
```

The directory itself only carries this file; the links are gitignored. With
them in place the root workspace's `apps/*` member glob builds every app against
the local `kits/` (the `[patch]` table in `Cargo.toml` swaps the git kit
source for the local path).

`apps/warp-flow` is the one real directory here, not a link: it holds that app's
own git repository inside the suite on purpose. `apps/*` is gitignored, so this
history never sees it, and the member glob, the shared `.build/target` and the
kit `[patch]` table treat it exactly like a link. `link_apps.py` never replaces
a real directory, so nothing can turn it into one.
