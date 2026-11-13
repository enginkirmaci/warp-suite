#!/usr/bin/env bash
# Pull the latest main of every app checkout, then build the suite.
#
#   refresh.sh                 sync every app, then build
#   refresh.sh --app NAME      only these apps (sync and build)
#   refresh.sh --check         fetch and report; change nothing, build nothing
#   refresh.sh --no-build      sync the checkouts only
#   refresh.sh --dry-run       print the git and build commands, run nothing
#
# The build leaves installers out: Windows runs build.ps1 -NoPackage, and the
# Linux build has no installer step.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

source_dir=""
apps=()
check=0
build=1
dry=0

while [ $# -gt 0 ]; do
    case "$1" in
        --source) source_dir="${2:?--source needs a value}"; shift 2 ;;
        --app) apps+=("${2:?--app needs a value}"); shift 2 ;;
        --check) check=1; shift ;;
        --no-build) build=0; shift ;;
        --dry-run) dry=1; shift ;;
        -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

if command -v python3 >/dev/null 2>&1; then
    py=python3
elif command -v python >/dev/null 2>&1; then
    py=python
else
    echo "refresh: python3 not found" >&2
    exit 1
fi

sync_args=()
if [ -n "$source_dir" ]; then sync_args+=(--source "$source_dir"); fi
for a in ${apps[@]+"${apps[@]}"}; do sync_args+=(--app "$a"); done
if [ "$check" = 1 ]; then sync_args+=(--check); fi
if [ "$dry" = 1 ]; then sync_args+=(--dry-run); fi

"$py" "$here/sync_apps.py" ${sync_args[@]+"${sync_args[@]}"}

if [ "$check" = 1 ] || [ "$build" = 0 ]; then
    exit 0
fi

build_args=()
for a in ${apps[@]+"${apps[@]}"}; do build_args+=(--app "$a"); done

if [ "$dry" = 1 ]; then
    printf '+ bash %s' "$root/scripts/build/build.sh"
    for a in ${apps[@]+"${apps[@]}"}; do printf ' --app %s' "$a"; done
    printf '\n'
    exit 0
fi

exec bash "$root/scripts/build/build.sh" ${build_args[@]+"${build_args[@]}"}
