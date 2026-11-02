#!/usr/bin/env bash
# warp-suite installer.
#
#   install.sh                     install the latest published suite release
#   install.sh --release <tag>     install a specific suite release (tag or latest)
#   install.sh --local             build the apps in this checkout
#                                  (scripts/bundle-linux.sh) and install that
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
api="https://api.github.com/repos/enginkirmaci/warp-suite/releases"

case "${1:---release}" in
    --local)
        bash "$here/scripts/bundle-linux.sh"
        bundle="$(ls -dt "$here"/dist/warp-suite-*-linux | head -1)"
        bash "$bundle/install-root.sh"
        ;;
    --release)
        tag="${2:-latest}"
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        if [ "$tag" = "latest" ]; then
            curl -fsSL "$api/latest" -o "$tmp/release.json"
        else
            curl -fsSL "$api/tags/$tag" -o "$tmp/release.json"
        fi
        url="$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    assets = json.load(f).get("assets", [])
linux = [a["browser_download_url"] for a in assets if a["name"].endswith("-linux.tar.gz")]
if not linux:
    sys.exit("no -linux.tar.gz asset on this release")
print(linux[0])
' "$tmp/release.json")"
        echo "downloading $url"
        curl -fL "$url" -o "$tmp/suite.tar.gz"
        tar -xzf "$tmp/suite.tar.gz" -C "$tmp"
        bash "$tmp"/warp-suite-*-linux/install-root.sh
        ;;
    *)
        echo "usage: install.sh [--release <tag|latest>] [--local]" >&2
        exit 1
        ;;
esac
