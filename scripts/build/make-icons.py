#!/usr/bin/env python3
"""Regenerate every app's packaging icons from its one scalable mark.

Source of truth is each app's `app.toml` (`icon_prefix`, `scalable_svg`), the
same fields `build.sh` reads: `scalable_svg` is rendered into the Linux
hicolor PNG set and into the Windows `.ico` that `build.rs` embeds into the
exe and the Inno Setup scripts use for the installer. An app with no
`scalable_svg` falls back to `packaging/linux/icons/<icon_prefix>.svg`, which
is how warp-snapit (Windows-only, no hicolor set) still gets an icon.

The art itself lives in OpenDesign's warp-suite-logos project (`logos/*.svg`);
this script only turns one transparent SVG into the sizes the desktop and
Windows need.

Requires:
  rsvg-convert   (librsvg; every Linux packaging machine has it)
  Pillow         (pip install pillow)

Usage:
  python3 scripts/build/make-icons.py              # every pinned app
  python3 scripts/build/make-icons.py --app warp-agent
  python3 scripts/build/make-icons.py --ico-only   # skip the hicolor PNGs
"""

import argparse
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
MANIFEST = ROOT / "scripts" / "meta" / "manifest.py"

# The hicolor steps LOGOS.md promises, plus 512 for the largest launcher tile.
HICOLOR = [16, 24, 32, 48, 64, 128, 256, 512]
# The frames Windows stores in one .ico.
ICO = [16, 24, 32, 48, 64, 128, 256]


def manifest(app: str, key: str) -> str:
    out = subprocess.run(
        [sys.executable, str(MANIFEST), "get", app, key],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        return ""
    return out.stdout.strip()


def apps() -> list[str]:
    out = subprocess.run(
        [sys.executable, str(MANIFEST), "list"], capture_output=True, text=True, check=True
    )
    return [line for line in out.stdout.split() if line]


def render(svg: Path, size: int, out_png: Path) -> None:
    # rsvg-convert writes exactly w x h; a mismatch means a broken render.
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(svg), "-o", str(out_png)],
        check=True,
    )
    from PIL import Image

    with Image.open(out_png) as im:
        if im.size != (size, size):
            sys.exit(f"{svg}: rendered {im.size} for {size}px, expected {(size, size)}")


def write_ico(out_path: Path, images: dict) -> int:
    # The base frame must be the largest one: Pillow drops sizes larger than
    # the base image and otherwise stores the base under a smaller header.
    largest = max(ICO)
    base = images[largest]
    others = [images[s] for s in ICO if s != largest]
    out_path.parent.mkdir(parents=True, exist_ok=True)
    base.save(out_path, format="ICO", sizes=[(s, s) for s in ICO], append_images=others)

    data = out_path.read_bytes()
    count = struct.unpack_from("<H", data, 4)[0]
    entries = []
    for i in range(count):
        w, h = data[6 + 16 * i], data[7 + 16 * i]
        entries.append((256 if w == 0 else w, 256 if h == 0 else h))
    if sorted(entries) != sorted((s, s) for s in ICO):
        sys.exit(f"{out_path} ended up with entries {entries}, expected {ICO}")
    return count


def build(name: str, ico_only: bool) -> None:
    app_dir = ROOT / "apps" / name
    prefix = manifest(name, "icon_prefix") or name
    linux_skip = manifest(name, "linux_skip") == "true"
    icons_dir = app_dir / "packaging" / "linux" / "icons"
    svg_rel = manifest(name, "scalable_svg") or f"packaging/linux/icons/{prefix}.svg"
    svg = app_dir / svg_rel
    if not svg.is_file():
        print(f"== {name}: no mark at {svg_rel}, skipped")
        return

    from PIL import Image

    images = {}
    rendered = []
    for size in HICOLOR:
        png = icons_dir / f"{prefix}-{size}.png"
        if not ico_only and not linux_skip:
            render(svg, size, png)
            rendered.append(size)
        if size in ICO:
            if png.is_file():
                with Image.open(png) as im:
                    if im.size != (size, size):
                        sys.exit(f"{png} is {im.size}, expected {(size, size)}")
                    images[size] = im.convert("RGBA").copy()
            else:
                tmp = icons_dir / f".{prefix}-{size}.tmp.png"
                render(svg, size, tmp)
                with Image.open(tmp) as im:
                    images[size] = im.convert("RGBA").copy()
                tmp.unlink()

    ico = app_dir / "packaging" / "windows" / f"{name}.ico"
    count = write_ico(ico, images)
    sizes = ", ".join(str(s) for s in rendered) or "none"
    print(f"== {name}: {ico.relative_to(ROOT)} ({count} frames), pngs {sizes}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--app", action="append", help="limit to these app names")
    parser.add_argument("--ico-only", action="store_true", help="regenerate the .ico files only")
    args = parser.parse_args()

    known = apps()
    names = args.app or known
    for name in names:
        if name not in known:
            sys.exit(f"suite.toml has no [apps.{name}] (known: {', '.join(known)})")
    for name in names:
        build(name, args.ico_only)


if __name__ == "__main__":
    main()
