#!/usr/bin/env bash
# Runs INSIDE an extracted warp-suite bundle (same layout the CI publishes):
# installs the staged Linux deliverables into ~/.local - binaries, desktop
# entries (Exec=__WARP_SUITE_BIN__ resolved to the real bin dir) and hicolor
# icons.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # bundle root
bin_root="${WARP_SUITE_BIN:-${XDG_BIN_HOME:-$HOME/.local/bin}}"
data_root="${XDG_DATA_HOME:-$HOME/.local/share}"

mkdir -p "$bin_root"
install -m755 "$here"/bin/* "$bin_root/"

app_dir="$data_root/applications"
mkdir -p "$app_dir"
for f in "$here"/applications/*.desktop; do
    sed "s|__WARP_SUITE_BIN__|$bin_root|g" "$f" > "$app_dir/$(basename "$f")"
done

icon_root="$data_root/icons/hicolor"
mkdir -p "$icon_root"
cp -r "$here"/icons/hicolor/. "$icon_root/"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$app_dir" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -tf "$icon_root" || true
echo "installed: $bin_root + $app_dir + $icon_root"
