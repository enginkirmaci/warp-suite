#!/usr/bin/env bash
# Builds every app under apps/ (release) and stages a self-contained Linux
# bundle under dist/:
#
#   dist/warp-suite-<version>-linux/
#     bin/                    the built binaries
#     applications/           desktop entries with Exec=__WARP_SUITE_BIN__/<app>
#     icons/hicolor/...       per-app hicolor icons (raster + scalable)
#     REVISIONS.txt           suite commit per app
#     install-root.sh         copied from scripts/, run after extraction
#
# The same layout the suite-release workflow publishes; run this to build
# locally without CI.
# Usage: bundle-linux.sh [--version <v>]   (default: git describe, else "dev")
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

version="dev"
while [ $# -gt 0 ]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done
if [ "$version" = "dev" ] && git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    version="$(git -C "$root" describe --tags --always 2>/dev/null || echo dev)"
fi

command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq required (cargo target dir discovery)" >&2; exit 1; }

if ! pkg-config --exists wayland-client xkbcommon 2>/dev/null; then
    echo "warning: wayland-client/xkbcommon dev packages missing - the build will likely fail" >&2
    echo "  Arch:   sudo pacman -S wayland libxkbcommon vulkan-headers fontconfig" >&2
    echo "  Ubuntu: sudo apt install libwayland-dev libxkbcommon-dev libvulkan-dev libfontconfig1-dev" >&2
fi

build_dir="$root/.build"
dist_root="$root/dist"
bundle="$dist_root/warp-suite-$version-linux"
rm -rf "$bundle"
mkdir -p "$bundle/bin" "$bundle/applications" "$bundle/icons/hicolor"

# One shared target dir for all apps: warp-station and warp-agent share most
# of the GPUI dependency tree, so repeated builds reuse artifacts.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$build_dir/target}"

suite_sha=""
if git -C "$root" rev-parse HEAD >/dev/null 2>&1; then
    suite_sha="$(git -C "$root" rev-parse HEAD)"
    [ -z "$(git -C "$root" status --porcelain -- apps warpui-kits 2>/dev/null)" ] || suite_sha="$suite_sha-dirty"
fi

: > "$bundle/REVISIONS.txt"
while read -r app; do
    app_dir="$root/apps/$app"
    build_args="$(python3 "$here/manifest.py" get "$app" linux_build 2>/dev/null || true)"
    icon_prefix="$(python3 "$here/manifest.py" get "$app" icon_prefix 2>/dev/null || echo "$app")"
    svg_rel="$(python3 "$here/manifest.py" get "$app" scalable_svg 2>/dev/null || true)"
    linux_skip="$(python3 "$here/manifest.py" get "$app" linux_skip 2>/dev/null || true)"

    # Windows-only apps (linux_skip = true) ship through the suite-release
    # Windows job only: no Linux build, so they get no REVISIONS.txt entry,
    # binary, desktop entry or icons in this bundle.
    if [ "$linux_skip" = "true" ]; then
        echo "== $app: skipped on Linux (Windows-only)"
        continue
    fi

    echo "== $app"
    # shellcheck disable=SC2086
    (cd "$app_dir" && cargo build --release $build_args)
    target_dir="$(cd "$app_dir" && cargo metadata --no-deps --format-version 1 | jq -r '.target_directory')"
    install -m755 "$target_dir/release/$app" "$bundle/bin/$app"
    echo "$app ${suite_sha:-unknown}" >> "$bundle/REVISIONS.txt"

    desktop_src="$app_dir/packaging/linux/$app.desktop"
    [ -f "$desktop_src" ] || { echo "missing $desktop_src" >&2; exit 1; }
    sed 's|^Exec=.*|Exec=__WARP_SUITE_BIN__/'"$app"'|' "$desktop_src" \
        > "$bundle/applications/$app.desktop"

    for f in "$app_dir/packaging/linux/icons/$icon_prefix"-*.png; do
        [ -e "$f" ] || continue
        size="$(basename "$f" .png)"; size="${size#"$icon_prefix"-}"
        dir="$bundle/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        cp "$f" "$dir/$icon_prefix.png"
    done
    if [ -n "$svg_rel" ]; then
        install -Dm644 "$app_dir/$svg_rel" \
            "$bundle/icons/hicolor/scalable/apps/$icon_prefix.svg"
    fi
done < <(python3 "$here/manifest.py" list)

# The kit crates are part of this repo (warpui-kits/), so the suite commit is
# their provenance: record it against each crate in the bundle.
if [ -n "$suite_sha" ]; then
    {
        echo
        echo "# warpui-kit crates from this suite checkout (warpui-kits/)"
        for kit_toml in "$root"/warpui-kits/*/Cargo.toml; do
            echo "lib $(basename "$(dirname "$kit_toml")") $suite_sha"
        done
    } >> "$bundle/REVISIONS.txt"
fi

cp "$here/install-root.sh" "$bundle/"
tar -czf "$dist_root/warp-suite-$version-linux.tar.gz" -C "$dist_root" "warp-suite-$version-linux"
echo "bundle staged in $bundle"
echo "tarball: $dist_root/warp-suite-$version-linux.tar.gz"
