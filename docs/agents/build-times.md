# Build times

Where the time in `cargo test` goes, and the rules that keep a loop short.
Measured 2026-11 on the owner's machine (20 cores, 62 GB RAM, zram swap).

## The shape of the cost

- `cargo test -p <app>` builds **one binary per test target**. Warp Station has
  23 probes plus the lib/unit tests, `warp-station-core` has 3: about 26 links
  per run. Every probe statically links the app and its whole gpui graph, so
  each one is ~300 MB with `debug = "line-tables-only"` (release is ~40 MB).
- Any change to an app's `src/` (or to a crate it links) recompiles that crate
  and **relinks every probe**: ~8 GB of writes for Warp Station.
- A package-version change does the same: cargo's unit hash includes the
  package id, so the pre-commit hook's PATCH bump invalidates everything that
  links that crate. That is one full relink per commit, by design; it measures
  26 s on the NVMe.

## The disk was the whole story

The suite's one target dir used to live on `/mnt/disk2` (Samsung 860 QVO, QLC
SATA, 89 % full). Measured there while linking: **1.9 MB/s** for a 64 MB fsync
write, while the NVMe did 5.9 GB/s cached for the same write. CPU sat at 87 %
idle and 11 % iowait, so the work was never the CPU: one relink of the 26
binaries took **15-20 minutes**.

`.build/target` is a **symlink** to `~/.cache/warp-suite/target` on the NVMe
(744 GB free, `~/.cache` on the same btrfs as `/home`). `.build/` is
gitignored, so this is a machine-local fact, not repo config: CI has no
symlink and builds into a fresh `.build/target`. Do not replace the symlink
with a directory, and do not `cargo clean` casually - the warm tree is ~40 GB
and a cold graph build is minutes even on the NVMe.

Measured on the NVMe after the move:

| Command | Time |
|---|---|
| no-op `cargo test -p warp-station --no-run` | 0.2 s |
| no-op `cargo build -p warp-station` | 0.2 s |
| one `[workspace.package]` version bump, then the same command | 26 s (8 crates + 26 links) |
| the full `cargo test -p warp-station -p warp-station-core`, warm | 17 s, ~600 tests |

## The rules

- Build only what you are iterating on: `cargo test -p warp-station --test
  changes_probe`. The package selector alone links all 23.
- The headless crate is cheap and display-free: `cargo test -p
  warp-station-core` (3 probes, no gpui).
- `--no-run` separates compile+link from execution, and prints each built
  executable.
- Cap link parallelism on a link-heavy run: `CARGO_BUILD_JOBS=8`. Twenty
  concurrent linkers at ~1.8 GB RSS each thrash zram; the disk is the
  bottleneck anyway.
- Never pipe a build through `tail`: it holds every line until the process
  exits, so a long run looks hung. `tee` a log and filter with
  `grep --line-buffered` when you need to.
- A no-op rerun is ~0.2 s. If it is not, something is dirty: read the
  `Compiling` lines instead of assuming a cache miss.

## A build script must not declare its own output

`apps/warp-station/build.rs` generates `$OUT_DIR/embedded_assets.rs` and used
to `println!("cargo:rerun-if-changed=<that file>")`. Because the script
rewrites the file every time it runs, that directive made every invocation
re-run the script and recompile the whole crate: **11 s per `cargo test` with
no source change at all**, and the same for `cargo build` and clippy. The
fixed script declares only its real inputs (`assets/`); rustc tracks the
`include!` through dep-info, so a regenerated table still rebuilds the crate.
Any new build script that generates a file follows the same rule: declare the
inputs, never the output.

## What does not help

- Swapping the linker: bfd vs lld measured equal (2.8 s vs 2.9 s on the same
  link) because the link was I/O-bound on the old disk.
- `debug = 0` would roughly halve each probe (384 MB -> 185 MB stripped), but
  the repo keeps `line-tables-only` so panic backtraces keep file:line. The
  disk move bought more than the debuginfo cut would have.
- `sccache` and `cargo-nextest` are not installed here; neither shortens the
  compile+link phase that dominates.
