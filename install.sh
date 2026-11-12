#!/usr/bin/env bash
# warp-suite installer - see scripts/deploy/deploy.sh.
#
#   install.sh                  install the latest published suite release
#   install.sh --release <tag>  install a specific release
#   install.sh --local          build this checkout, then install
#   install.sh --uninstall      remove what the installer wrote
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ $# -eq 0 ]; then set -- --release latest; fi
exec bash "$here/scripts/deploy/deploy.sh" "$@"
