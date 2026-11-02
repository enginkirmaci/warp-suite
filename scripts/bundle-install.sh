#!/usr/bin/env bash
# Runs INSIDE an extracted warp-suite bundle (the same layout build.sh stages
# and the release publishes): installs the Linux deliverables into ~/.local
# and records every path it wrote, so `scripts/deploy.sh --uninstall` removes
# exactly those files and nothing else.
#
#   WARP_SUITE_BIN   install the binaries here (default: ~/.local/bin)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin_root="${WARP_SUITE_BIN:-${XDG_BIN_HOME:-$HOME/.local/bin}}"
data_root="${XDG_DATA_HOME:-$HOME/.local/share}"
state_dir="$data_root/warp-suite"
record="$state_dir/installed.txt"

mkdir -p "$bin_root" "$state_dir"
# Keep paths from an earlier install of a different subset (build.sh --app).
previous="$(cat "$record" 2>/dev/null || true)"
: > "$record"
[ -n "$previous" ] && printf '%s\n' "$previous" >> "$record"

for f in "$here"/bin/*; do
    install -m755 "$f" "$bin_root/"
    echo "$bin_root/$(basename "$f")" >> "$record"
done

app_dir="$data_root/applications"
mkdir -p "$app_dir"
for f in "$here"/applications/*.desktop; do
    name="$(basename "$f")"
    sed "s|__WARP_SUITE_BIN__|$bin_root|g" "$f" > "$app_dir/$name"
    echo "$app_dir/$name" >> "$record"
done

icon_root="$data_root/icons/hicolor"
mkdir -p "$icon_root"
cp -r "$here"/icons/hicolor/. "$icon_root/"
(cd "$here/icons/hicolor" && find . -type f) | while read -r rel; do
    echo "$icon_root/${rel#./}" >> "$record"
done

sort -u -o "$record" "$record"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$app_dir" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -tf "$icon_root" || true
echo "installed $(wc -l < "$record") files: $bin_root, $app_dir, $icon_root"
