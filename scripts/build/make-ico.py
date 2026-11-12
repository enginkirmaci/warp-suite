#!/usr/bin/env python3
"""Regenerate packaging/windows/<app>.ico from the Linux icon art.

Source of truth is each app's packaging/linux/icons set, the same files
build.sh installs into the hicolor theme: every size that exists as
<icon_prefix>-<size>.png is embedded verbatim; sizes the app does not ship
as a PNG are rendered from its scalable SVG (apps.toml scalable_svg) with
resvg. The result is the icon build.rs embeds into the exe and the .iss
scripts use for the installers, so Linux art and Windows marks stay one
regeneration apart.

Requires:
  pip install pillow
  cargo install resvg      (only when an app misses PNG sizes)

Usage:
  python scripts/build/make-ico.py                 # every app in apps.toml
  python scripts/build/make-ico.py --app warp-agent
"""

import argparse
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent.parent
SIZES = [16, 24, 32, 48, 64, 128, 256]


def render_svg(svg, size, out_png):
    # resvg writes exactly w x h for -w/-h; fail loudly otherwise.
    subprocess.run(
        ["resvg", "-w", str(size), "-h", str(size), str(svg), str(out_png)],
        check=True,
    )
    with Image.open(out_png) as im:
        if im.size != (size, size):
            sys.exit(f"resvg produced {im.size} for {size}px, expected {(size, size)}")


def load_size(png_path, size):
    im = Image.open(png_path)
    if im.size != (size, size):
        sys.exit(f"{png_path} is {im.size}, expected {(size, size)}")
    return im.convert("RGBA")


def write_ico(out_path, images):
    # Base must be the largest frame: Pillow drops requested sizes larger
    # than the base image and falls back to storing the base under a smaller
    # header otherwise. Every requested size is provided exactly, so no
    # fallback path can trigger.
    largest = max(SIZES)
    base = images[largest]
    others = [images[s] for s in SIZES if s != largest]
    base.save(out_path, format="ICO", sizes=[(s, s) for s in SIZES], append_images=others)

    # Verify what landed on disk: ICONDIR (6 bytes) + one ICONDIRENTRY (16
    # bytes) per image, width/height byte 0 meaning 256.
    data = out_path.read_bytes()
    count = struct.unpack_from("<H", data, 4)[0]
    entries = []
    for i in range(count):
        w, h = data[6 + 16 * i], data[7 + 16 * i]
        entries.append((256 if w == 0 else w, 256 if h == 0 else h))
    if sorted(entries) != sorted((s, s) for s in SIZES):
        sys.exit(f"{out_path} ended up with entries {entries}, expected {SIZES}")
    return count


def build_app(name, cfg, svg_render_dir):
    app_dir = ROOT / "apps" / name
    icons_dir = app_dir / "packaging" / "linux" / "icons"
    prefix = cfg.get("icon_prefix", name)
    svg = cfg.get("scalable_svg")

    images = {}
    rendered = []
    for size in SIZES:
        png = icons_dir / f"{prefix}-{size}.png"
        if png.exists():
            images[size] = load_size(png, size)
            continue
        if not svg:
            sys.exit(f"{name}: no {png} and no scalable_svg in apps.toml")
        out_png = svg_render_dir / f"{name}-{size}.png"
        render_svg(app_dir / svg, size, out_png)
        images[size] = load_size(out_png, size)
        rendered.append(size)

    out = app_dir / "packaging" / "windows" / f"{name}.ico"
    out.parent.mkdir(parents=True, exist_ok=True)
    count = write_ico(out, images)
    from_png = [s for s in SIZES if s not in rendered]
    print(f"{name}: wrote {out.name} ({count} entries)")
    print(f"  from PNGs : {from_png or 'none'}")
    print(f"  from SVG  : {rendered or 'none'}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--app", action="append", help="limit to these app names")
    args = parser.parse_args()

    with (ROOT / "apps.toml").open("rb") as f:
        apps = tomllib.load(f).get("apps", {})
    names = args.app or list(apps)
    unknown = [n for n in names if n not in apps]
    if unknown:
        sys.exit(f"apps.toml has no [apps.{unknown[0]}] (known: {', '.join(apps)})")

    needs_resvg = any(
        not (ROOT / "apps" / n / "packaging" / "linux" / "icons" / f"{apps[n].get('icon_prefix', n)}-{s}.png").exists()
        for n in names
        for s in SIZES
    )
    if needs_resvg and not shutil.which("resvg"):
        sys.exit("resvg not found - needed for sizes missing as PNG (cargo install resvg)")

    with tempfile.TemporaryDirectory(prefix="make-ico-") as tmp:
        for name in names:
            build_app(name, apps[name], Path(tmp))


if __name__ == "__main__":
    main()
