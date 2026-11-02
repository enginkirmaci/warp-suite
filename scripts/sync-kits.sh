#!/usr/bin/env bash
# Refreshes vendor/warpui-kit-* from the sibling checkouts of the kit crates.
#
# The apps depend on the kit crates as path siblings (`warpui-kit-core = { path
# = "../warpui-kit-core" }`), and the kit repos are not published yet, so the
# suite carries a vendored snapshot it can stage beside the app clones in CI and
# in bundle-linux.sh. Run this after changing any kit crate and before pinning
# a suite release; the app pins alone do not describe the libraries.
#
# Usage: scripts/sync-kits.sh [--kits-root <dir>]   (default: the sibling folder)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

kits_root="$(cd "$root/.." && pwd)"
while [ $# -gt 0 ]; do
    case "$1" in
        --kits-root) kits_root="$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

kits=(warpui-kit-core warpui-kit-diff warpui-kit-markdown warpui-kit-picker)
{
    echo "# Source commits of the vendored warpui-kit crates (scripts/sync-kits.sh writes this)."
} > "$root/vendor/REVISIONS.txt"

for kit in "${kits[@]}"; do
    src="$kits_root/$kit"
    [ -d "$src/src" ] || { echo "missing kit checkout: $src" >&2; exit 1; }
    rsync -a --delete \
        --exclude target --exclude .git --exclude Cargo.lock \
        "$src/" "$root/vendor/$kit/"
    echo "$kit $(git -C "$src" rev-parse HEAD) $(git -C "$src" rev-parse --abbrev-ref HEAD)" \
        >> "$root/vendor/REVISIONS.txt"
    echo "synced $kit @ $(git -C "$src" rev-parse --short HEAD)"
done
