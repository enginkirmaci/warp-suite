#!/usr/bin/env bash
# Installs or removes the suite's Linux deliverables.
#
#   deploy.sh --local                 build this checkout, then install it
#   deploy.sh --bundle <dir|tar.gz>   install an already-built bundle
#   deploy.sh --release [tag]         install a published release (default: latest)
#   deploy.sh --uninstall             remove exactly what the last install wrote
#
#   --prefix DIR   install the binaries here (default: ~/.local/bin)
#   --app NAME     with --local, build only these apps (repeatable)
#   --dry-run      print what would happen
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
repo="enginkirmaci/warp-suite"

mode=""
tag="latest"
bundle=""
prefix=""
apps=()
dry=0

while [ $# -gt 0 ]; do
    case "$1" in
        --local) mode=local; shift ;;
        --bundle) mode=bundle; bundle="${2:?--bundle needs a value}"; shift 2 ;;
        --release)
            mode=release; shift
            if [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; then tag="$1"; shift; fi ;;
        --uninstall) mode=uninstall; shift ;;
        --prefix) prefix="${2:?--prefix needs a value}"; shift 2 ;;
        --app) apps+=("${2:?--app needs a value}"); shift 2 ;;
        --dry-run) dry=1; shift ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

if [ -z "$mode" ]; then
    sed -n '2,13p' "$0" >&2
    exit 1
fi

data_root="${XDG_DATA_HOME:-$HOME/.local/share}"
record="$data_root/warp-suite/installed.txt"

install_bundle() {
    local dir="$1"
    local installer="$dir/bundle-install.sh"
    [ -f "$installer" ] || { echo "not a suite bundle (no bundle-install.sh): $dir" >&2; exit 1; }
    if [ "$dry" = 1 ]; then
        echo "+ WARP_SUITE_BIN=${prefix:-<default>} bash $installer"
        return
    fi
    if [ -n "$prefix" ]; then
        WARP_SUITE_BIN="$prefix" bash "$installer"
    else
        bash "$installer"
    fi
}

if [ "$mode" = uninstall ]; then
    [ -f "$record" ] || { echo "nothing recorded in $record"; exit 0; }
    if [ "$dry" = 1 ]; then
        echo "would remove:"; cat "$record"; exit 0
    fi
    removed=0
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        rm -f "$path" && removed=$((removed+1))
    done < "$record"
    rm -f "$record"
    rmdir "$data_root/warp-suite" 2>/dev/null || true
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data_root/applications" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -tf "$data_root/icons/hicolor" || true
    echo "removed $removed files"
    exit 0
fi

if [ "$mode" = local ]; then
    build_args=()
    for a in ${apps[@]+"${apps[@]}"}; do build_args+=(--app "$a"); done
    bash "$here/build.sh" ${build_args[@]+"${build_args[@]}"}
    bundle="$(ls -dt "$root"/dist/linux/warp-suite-*-linux 2>/dev/null | head -1)"
    [ -n "$bundle" ] || { echo "no bundle under $root/dist/linux" >&2; exit 1; }
    install_bundle "$bundle"
    exit 0
fi

if [ "$mode" = bundle ]; then
    [ -e "$bundle" ] || { echo "no such bundle: $bundle" >&2; exit 1; }
    if [ -d "$bundle" ]; then
        install_bundle "$bundle"
    else
        tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
        tar -xzf "$bundle" -C "$tmp"
        install_bundle "$(ls -d "$tmp"/*/ | head -1)"
    fi
    exit 0
fi

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
if [ "$tag" = latest ]; then
    curl -fsSL "https://api.github.com/repos/$repo/releases/latest" -o "$tmp/release.json"
else
    curl -fsSL "https://api.github.com/repos/$repo/releases/tags/$tag" -o "$tmp/release.json"
fi
url="$(python3 - "$tmp/release.json" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    assets = json.load(f).get("assets", [])
linux = [a["browser_download_url"] for a in assets if a["name"].endswith("-linux.tar.gz")]
if not linux:
    sys.exit("no -linux.tar.gz asset on this release")
print(linux[0])
PY
)"
echo "downloading $url"
curl -fL "$url" -o "$tmp/suite.tar.gz"
tar -xzf "$tmp/suite.tar.gz" -C "$tmp"
install_bundle "$(ls -d "$tmp"/*/ | head -1)"
