#!/bin/sh
# Link the sibling app checkouts into apps/ (POSIX wrapper for link_apps.py).
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

if command -v python3 >/dev/null 2>&1; then
    py=python3
elif command -v python >/dev/null 2>&1; then
    py=python
else
    echo "link-apps: python3 not found" >&2
    exit 1
fi

exec "$py" "$here/link_apps.py" "$@"
