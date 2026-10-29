#!/usr/bin/env bash
# set-pin.sh <app> <ref> - verify <ref> exists in the app's repo, then write
# it into apps.toml. A suite deploy = set pins -> tag/dispatch the
# suite-release workflow.
#
# <ref> may be a branch, tag or commit SHA. Branches and tags are verified
# with git ls-remote; a SHA is accepted as-is (with a warning when it does
# not head any branch or tag).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest_py="$here/manifest.py"

app="${1:?usage: set-pin.sh <app> <ref>}"
ref="${2:?usage: set-pin.sh <app> <ref>}"
repo="$(python3 "$manifest_py" get "$app" repo)"

out="$(git ls-remote "$repo" "$ref" || true)"
if [ -z "$out" ] && [[ "$ref" =~ ^[0-9a-fA-F]{40,64}$ ]]; then
    # Not a branch/tag name - a commit SHA. Check whether it heads any ref.
    if git ls-remote "$repo" | grep -qi "^$ref"; then
        echo "note: $ref is the tip of a branch or tag in $repo"
    else
        echo "warning: $ref heads no branch/tag; trusting it as an in-between commit" >&2
    fi
    out="ok"
fi
if [ -z "$out" ]; then
    echo "error: '$ref' not found in $repo" >&2
    exit 1
fi

python3 "$manifest_py" set-ref "$app" "$ref"
echo "$app -> $ref"
