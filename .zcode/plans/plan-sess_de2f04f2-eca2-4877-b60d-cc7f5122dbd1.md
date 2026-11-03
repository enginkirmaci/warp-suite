# Update Windows exe icons to match the Linux logos

## Context
The Linux icon art was redesigned on Nov 3 (`2871189` "feat: … logos", `2885eaa`) but no Windows icon was touched:
- `apps\warp-agent\packaging\windows\warp-agent.ico` — 7 PNG entries but **stale Nov-2 art**
- `apps\warp-notes\packaging\windows\warp-notes.ico` — stale art, all-BMP, **missing 24px**
- `apps\warp-station\packaging\windows\warp-station.ico` — **single 75×75 BMP**, no standard sizes at all
- `apps\warp-snapit` — **no icon whatsoever** (no build.rs, no winresource dep, no .ico; only `packaging\linux\icons\snapit.svg`)

Goal: every Windows exe (and installer, via the existing `SetupIconFile=` lines) carries the current Linux logo art. Exe icons come purely from build.rs/winresource embedding — 3 apps already have that wiring; only snapit needs it.

## Steps

1. **Tooling** (none installed on this machine): `pip install pillow` (assembles ICO from PNGs) and `cargo install resvg` (SVG rasterizer for the two SVG-only gaps; registry cache is warm from the suite build).

2. **New `scripts/make-ico.py`** — regenerates each app's `packaging\windows\<name>.ico` at sizes 16/24/32/48/64/128/256:
   - Source of truth = the Linux icons: use `packaging\linux\icons\<icon_prefix>-<size>.png` where it exists.
   - Missing sizes render from the app's scalable SVG via resvg (warp-station: 24px from `crates/warp-station/assets/icons/logo.svg`; warp-snapit: all sizes from `snapit.svg`).
   - Warp-station SVG path and snapit's keys come from `apps.toml`; add `icon_prefix = "snapit"` + `scalable_svg` to the `[apps.warp-snapit]` section (harmless — it's `linux_skip = true`, and `build.sh` never reads it for skipped apps).
   - Print which sizes came from PNG vs SVG render; refuse to write a partial set.

3. **Regenerate all 4 `.ico` files** by running the script.

4. **Wire warp-snapit like its siblings**:
   - `apps\warp-snapit\build.rs` (new): windows-gated `winresource` block embedding `packaging/windows/warp-snapit.ico`, with `cargo:rerun-if-changed` — same shape as `apps\warp-agent\build.rs`.
   - `apps\warp-snapit\Cargo.toml`: add `[target.'cfg(windows)'.build-dependencies] winresource = "0.1"` (already locked at 0.1.34 via the other apps).

5. **Drive-by fix**: add the missing `println!("cargo:rerun-if-changed=…ico")` line to `apps\warp-agent\build.rs` and `apps\warp-notes\build.rs` (currently an icon change doesn't trigger a rebuild there; warp-station already has it).

6. **Rebuild portable zips**: once the in-flight background release build finishes, rerun `scripts\build.ps1 -NoPackage` with the `%TEMP%\warp-shim` PATH shim from earlier — incremental now (deps cached). This refreshes `dist\windows\*.zip` with icon-bearing exes.

7. **Verify**:
   - Parse each regenerated `.ico` and list its entries (expect 7 sizes, PNG or BMP entries).
   - Extract the associated icon from each staged release exe (incl. `snapit.exe`) with PowerShell `System.Drawing` and compare visually against the Linux 32px PNGs.
   - Confirm `dist\windows\<app>-*-portable.zip` timestamps updated.

## Notes
- No installer changes needed: the three `.iss` files already reference their `.ico` filenames; snapit intentionally stays zip-only (no `.iss`).
- Won't commit — changes stay in the working tree next to the other session's `warp-agent/src/ui/window.rs` edit.
- Running debug exes are under `.build\target\debug\` and don't lock the release rebuild.