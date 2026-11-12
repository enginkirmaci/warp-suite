#!/usr/bin/env bash
# Assemble one suite release from the pinned app releases.
#
#   suite-assemble.sh --version <suite version> [--artifacts DIR] [--out DIR]
#
# Each app release contributes a bundle-ready Linux tarball and a portable
# Windows ZIP. This script never builds an app: it unpacks, merges and records
# what it used, then writes one Linux tarball and one Windows ZIP. The suite
# release workflow calls it after downloading the pinned releases.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
manifest="$root/scripts/meta/manifest.py"
py=python3

version=""
artifacts="$root/artifacts"
out="$root/dist"

while [ $# -gt 0 ]; do
    case "$1" in
        --version) version="${2:?--version needs a value}"; shift 2 ;;
        --artifacts) artifacts="${2:?--artifacts needs a value}"; shift 2 ;;
        --out) out="${2:?--out needs a value}"; shift 2 ;;
        -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

[ -n "$version" ] || { echo "--version is required" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }
command -v zip >/dev/null || { echo "zip required" >&2; exit 1; }
command -v unzip >/dev/null || { echo "unzip required" >&2; exit 1; }

suite_version="$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["suite"]["version"])' "$root/suite.toml")"
suite_build="$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb")).get("suite",{}).get("build",0))' "$root/suite.toml")"
[ "$version" = "$suite_version" ] || {
    echo "version mismatch: --version $version, suite.toml says $suite_version" >&2
    exit 1
}

sha="$(git -C "$root" rev-parse --short=7 HEAD 2>/dev/null || echo unknown)"
linux="$out/linux/warp-suite-$version-linux"
windows="$out/windows/warp-suite-$version-windows-x64"
rm -rf "$linux"
rm -rf "$out/windows"
mkdir -p "$linux/bin" "$linux/applications" "$linux/icons"
mkdir -p "$windows/apps"

apps="$("$py" "$manifest" list)"
linux_seen=0
windows_seen=0
for app in $apps; do
    tag="$("$py" "$manifest" pin "$app" tag)"
    dir="$artifacts/$app"
    [ -d "$dir" ] || { echo "missing downloaded release: $dir" >&2; exit 1; }

    tarball="$(find "$dir" -maxdepth 1 -name '*-linux.tar.gz' -print -quit)"
    if [ -n "$tarball" ]; then
        linux_seen=1
        unpack="$dir/unpacked"
        rm -rf "$unpack"
        mkdir -p "$unpack"
        tar -xzf "$tarball" -C "$unpack"
        top="$(find "$unpack" -mindepth 1 -maxdepth 1 -type d -print -quit)"
        [ -n "$top" ] || { echo "$tarball has no top directory" >&2; exit 1; }
        if [ -d "$top/bin" ]; then cp -a "$top/bin/." "$linux/bin/"; fi
        if [ -d "$top/applications" ]; then cp -a "$top/applications/." "$linux/applications/"; fi
        if [ -d "$top/icons" ]; then cp -a "$top/icons/." "$linux/icons/"; fi
    fi

    winzip="$(find "$dir" -maxdepth 1 -name '*-windows-x64.zip' -print -quit)"
    if [ -n "$winzip" ]; then
        windows_seen=1
        appdir="$windows/apps/$app"
        rm -rf "$appdir"
        mkdir -p "$appdir"
        unzip -q "$winzip" -d "$appdir"
    fi
    echo "assembled $app $tag"
done

[ "$linux_seen" = 1 ] || { echo "no Linux bundle in any pinned app release" >&2; exit 1; }

install -m755 "$root/scripts/deploy/bundle-install.sh" "$linux/bundle-install.sh"
{
    echo "suite $version build $suite_build commit $sha channel dev"
    echo "kits $("$py" "$manifest" pin kits tag)"
    for app in $apps; do
        echo "app $app $("$py" "$manifest" pin "$app" tag)"
    done
} > "$linux/REVISIONS.txt"

tar -czf "$out/linux/warp-suite-$version-linux.tar.gz" -C "$out/linux" "warp-suite-$version-linux"

if [ "$windows_seen" = 1 ]; then
    install -m644 "$root/scripts/deploy/bundle-install.ps1" "$windows/bundle-install.ps1"
    install -m644 "$root/scripts/deploy/bundle-uninstall.ps1" "$windows/bundle-uninstall.ps1"
    install -m644 "$root/scripts/deploy/install.cmd" "$windows/install.cmd"
    install -m644 "$root/scripts/deploy/uninstall.cmd" "$windows/uninstall.cmd"
    install -m644 "$root/scripts/deploy/README-windows.txt" "$windows/README.txt"
    cp "$linux/REVISIONS.txt" "$windows/REVISIONS.txt"
    (
        cd "$out/windows"
        zip -qr "warp-suite-$version-windows-x64.zip" "warp-suite-$version-windows-x64"
    )
    rm -rf "$windows"
fi

(
    cd "$out"
    {
        sha256sum "linux/warp-suite-$version-linux.tar.gz"
        if [ "$windows_seen" = 1 ]; then
            sha256sum "windows/warp-suite-$version-windows-x64.zip"
        fi
    } > SHA256SUMS
)

echo "suite $version assembled under $out"
