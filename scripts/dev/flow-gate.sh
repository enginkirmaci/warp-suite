#!/bin/sh
# The cargo runner warp-flow's gates use: one checkout, one target directory,
# and a lockfile the gate puts back.
#
# A task worktree is a full checkout, so its own `.cargo/config.toml` would
# send cargo to `<worktree>/.build/target` and compile the whole gpui graph
# once per task. The main checkout's target directory (`.build/target`, a
# machine-local symlink into the build cache) is the one every app build here
# already fills, so a gate reuses it instead. `git rev-parse
# --git-common-dir` names the main checkout from both places, so the same
# command works in the checkout and in `.worktrees/<slug>`.
#
# A worktree also has no `apps/` checkouts, and the committed Cargo.lock
# carries the app members of a linked checkout. Cargo prunes or extends that
# lock as the member set changes; a gate must leave the checkout byte-for-byte
# clean, because the merge queue rebases this worktree right after the gate
# steps and git refuses a rebase over uncommitted changes. The lock is copied
# back after the command, whatever it did. A task that really changes
# dependencies commits its own lock, and the gate then measures that tree.
#
# A window launched from the desktop may not have rustup on PATH: its bin comes
# first, and the rust-toolchain.toml pin still picks the toolchain.
#
# usage: scripts/dev/flow-gate.sh <cargo args...>
set -eu

PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
export PATH

root=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/.build/target}"
export CARGO_TARGET_DIR

# No lockfile is not this runner's problem to invent one for: let cargo say so.
if [ ! -f Cargo.lock ]; then
    exec cargo "$@"
fi

lock=$(mktemp "${TMPDIR:-/tmp}/warp-flow-gate-lock.XXXXXX")
cp Cargo.lock "$lock"
status=0
cargo "$@" || status=$?
cp "$lock" Cargo.lock
rm -f "$lock"
exit "$status"
