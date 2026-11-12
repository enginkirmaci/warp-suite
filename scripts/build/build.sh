#!/usr/bin/env bash
# Builds the Linux deliverables for every app under apps/ into one
# self-contained bundle:
#
#   dist/linux/warp-suite-<version>-linux/
#     bin/                 the built binaries
#     applications/        desktop entries (Exec=__WARP_SUITE_BIN__/<bin>)
#     icons/hicolor/...    per-app hicolor icons (raster + scalable)
#     REVISIONS.txt        the suite commit, recorded per app and kit crate
#     bundle-install.sh    installs the bundle into ~/.local
#
# The only Linux build path: local runs and .github/workflows/suite-release.yml
# both call this script. --version is a cross-check against version.toml, not
# an override: the release workflow passes the dispatched tag and a mismatch
# stops the build.
#
# Usage: build.sh [--version V] [--app NAME]... [--no-tar]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

version_arg=""
selected=()
make_tar=1
while [ $# -gt 0 ]; do
    case "$1" in
        --version) version_arg="${2:?--version needs a value}"; shift 2 ;;
        --app) selected+=("${2:?--app needs a value}"); shift 2 ;;
        --no-tar) make_tar=0; shift ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq required (cargo metadata parsing)" >&2; exit 1; }

json_get() { printf '%s' "$1" | python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$2"; }

known="$(python3 "$here/../meta/manifest.py" list)"
for a in ${selected[@]+"${selected[@]}"}; do
    printf '%s\n' "$known" | grep -qx "$a" || { echo "apps.toml has no [apps.$a]" >&2; exit 1; }
done

# The suite version, build counter, commit and channel. version.toml is the
# source; a plain `cargo build` reads it through kits/warpui-kit-build too.
facts="$(python3 "$here/../meta/version.py" show --json)"
version="$(json_get "$facts" version)"
if [ -n "$version_arg" ] && [ "$version_arg" != "$version" ]; then
    echo "version mismatch: --version $version_arg but version.toml says $version" >&2
    exit 1
fi

# Every build script run consumes one build number. A CI job passes
# WARP_SUITE_BUILD so the Linux and Windows artifacts of one tag agree.
if [ -z "${WARP_SUITE_BUILD:-}" ]; then
    WARP_SUITE_BUILD="$(python3 "$here/../meta/version.py" build bump)"
fi
facts="$(python3 "$here/../meta/version.py" show --json)"
WARP_SUITE_VERSION="$version"
WARP_SUITE_COMMIT="$(json_get "$facts" commit)"
WARP_SUITE_CHANNEL="$(json_get "$facts" channel)"
WARP_SUITE_RELEASED="$(json_get "$facts" released)"
export WARP_SUITE_VERSION WARP_SUITE_BUILD WARP_SUITE_COMMIT WARP_SUITE_CHANNEL WARP_SUITE_RELEASED

if ! pkg-config --exists wayland-client xkbcommon 2>/dev/null; then
    echo "warning: wayland-client/xkbcommon dev packages missing - the build will likely fail" >&2
    echo "  Arch:   sudo pacman -S wayland libxkbcommon vulkan-headers fontconfig" >&2
    echo "  Ubuntu: sudo apt install libwayland-dev libxkbcommon-dev libvulkan-dev libfontconfig1-dev" >&2
fi

dist_root="$root/dist/linux"
bundle="$dist_root/warp-suite-$version-linux"
rm -rf "$bundle"
mkdir -p "$bundle/bin" "$bundle/applications" "$bundle/icons/hicolor"

# One shared target dir for the whole suite (see .cargo/config.toml): the
# GPUI dependency tree is compiled once for every app.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/.build/target}"

suite_sha=""
if git -C "$root" rev-parse HEAD >/dev/null 2>&1; then
    suite_sha="$(git -C "$root" rev-parse HEAD)"
    # the whole tree, not just apps/ and kits/: the deliverable also depends
    # on scripts/, apps.toml, Cargo.toml and Cargo.lock
    # version.toml carries the uncommitted build counter by design: ignore it
    dirty="$(git -C "$root" status --porcelain 2>/dev/null | grep -v ' version.toml$' || true)"
    [ -z "$dirty" ] || suite_sha="$suite_sha-dirty"
fi

printf 'suite %s build %s commit %s channel %s released %s\n' \
    "$WARP_SUITE_VERSION" "$WARP_SUITE_BUILD" "$WARP_SUITE_COMMIT" \
    "$WARP_SUITE_CHANNEL" "${WARP_SUITE_RELEASED:-not released}" \
    > "$bundle/REVISIONS.txt"
built=0
while read -r app; do
    if [ ${#selected[@]} -gt 0 ]; then
        printf '%s\n' "${selected[@]}" | grep -qx "$app" || continue
    fi

    linux_skip="$(python3 "$here/../meta/manifest.py" get "$app" linux_skip 2>/dev/null || true)"
    if [ "$linux_skip" = "true" ]; then
        echo "== $app: skipped on Linux (Windows-only)"
        continue
    fi

    app_dir="$root/apps/$app"
    # The cargo package name; defaults to the app name.
    package="$(python3 "$here/../meta/manifest.py" get "$app" package 2>/dev/null || echo "$app")"
    build_args="$(python3 "$here/../meta/manifest.py" get "$app" linux_build 2>/dev/null || true)"
    icon_prefix="$(python3 "$here/../meta/manifest.py" get "$app" icon_prefix 2>/dev/null || echo "$app")"
    svg_rel="$(python3 "$here/../meta/manifest.py" get "$app" scalable_svg 2>/dev/null || true)"

    echo "== $app"
    # The suite is one workspace (root Cargo.toml), so builds run from the root
    # with a package selector; -p also picks the right member when a directory
    # is not a package root (apps/warp-station/crates/...).
    # shellcheck disable=SC2086
    # --locked: a release ships the committed lockfile's graph; a forgotten
    # lockfile update fails here instead of drifting into the bundle
    (cd "$root" && cargo build --release --locked -p "$package" $build_args)

    meta="$(cd "$root" && cargo metadata --no-deps --format-version 1)"
    target_dir="$(jq -r '.target_directory' <<<"$meta")"
    # The bin target may differ from the package name.
    bin_name="$(jq -r --arg pkg "$package" \
        '[.packages[] | select(.name == $pkg) | .targets[]
          | select(.kind | index("bin")) | .name] | .[0]' <<<"$meta")"
    if [ -z "$bin_name" ] || [ "$bin_name" = "null" ]; then
        echo "$app (package $package) has no bin target" >&2; exit 1
    fi

    install -m755 "$target_dir/release/$bin_name" "$bundle/bin/$bin_name"
    echo "$app ${suite_sha:-unknown}" >> "$bundle/REVISIONS.txt"

    desktop_src="$app_dir/packaging/linux/$app.desktop"
    [ -f "$desktop_src" ] || { echo "missing $desktop_src" >&2; exit 1; }
    sed "s|^Exec=.*|Exec=__WARP_SUITE_BIN__/$bin_name|" "$desktop_src" \
        > "$bundle/applications/$app.desktop"

    # a missing or typo'd icon_prefix used to yield a silent iconless bundle;
    # a missing .desktop was already a hard error, icons are too now
    icon_glob="$app_dir/packaging/linux/icons/$icon_prefix"-*.png
    compgen -G "$icon_glob" >/dev/null || {
        echo "no icons match $icon_glob (check icon_prefix in apps.toml)" >&2
        exit 1
    }
    for f in $icon_glob; do
        size="$(basename "$f" .png)"; size="${size#"$icon_prefix"-}"
        mkdir -p "$bundle/icons/hicolor/${size}x${size}/apps"
        cp "$f" "$bundle/icons/hicolor/${size}x${size}/apps/$icon_prefix.png"
    done
    if [ -n "$svg_rel" ]; then
        install -Dm644 "$app_dir/$svg_rel" \
            "$bundle/icons/hicolor/scalable/apps/$icon_prefix.svg"
    fi
    built=$((built+1))
done < <(printf '%s\n' "$known")

if [ "$built" -eq 0 ]; then
    echo "nothing built - every selected app is skipped on Linux" >&2
    exit 1
fi

# The kit crates live in this repo (kits/), so the suite commit is their
# provenance: record it against each crate in the bundle.
if [ -n "$suite_sha" ]; then
    {
        echo
        echo "# warpui-kit crates from this suite checkout (kits/)"
        for kit_toml in "$root"/kits/*/Cargo.toml; do
            echo "lib $(basename "$(dirname "$kit_toml")") $suite_sha"
        done
    } >> "$bundle/REVISIONS.txt"
fi

install -m755 "$here/../deploy/bundle-install.sh" "$bundle/bundle-install.sh"

if [ "$make_tar" = 1 ]; then
    tar -czf "$dist_root/warp-suite-$version-linux.tar.gz" \
        -C "$dist_root" "warp-suite-$version-linux"
    echo "tarball: $dist_root/warp-suite-$version-linux.tar.gz"
fi
echo "bundle staged in $bundle"
