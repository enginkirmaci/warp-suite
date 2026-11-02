# warp-snapit

Standalone Rust port of the Windows zone-snapper companion. The .NET original
(`Tools.SnapIt`) lives in the [SnapIt](https://github.com/enginkirmaci/SnapIt)
repo and was vendored into
[Warp Station](https://github.com/enginkirmaci/warp-station) (`src/Tools.SnapIt`)
before the C# companion was removed; `snapit.exe` is the distributed
binary.

Distributed through [warp-suite](https://github.com/enginkirmaci/warp-suite)
releases (Windows-only; the suite skips it on Linux).

## What it does

- Hooks the mouse and keyboard globally (low-level Win32 hooks).
- Detects a drag of the foreground window's title bar.
- Shows one click-through overlay per active monitor with the same layout
  zones as the .NET app.
- Highlights the zone under the pointer and places the window there on
  release. `Esc` cancels.
- Optionally applies `DWMWCP_DONOTROUND` to newly shown windows
  (`EnableAutomaticWindowCornering`).
- Exits with Warp Station when launched with `--parent <pid>`.

## Shared configuration

The port reads the same files as Warp Station:

```
%USERPROFILE%\.warp\snapit\Settings.json
%USERPROFILE%\.warp\snapit\ExcludedApplicationSettings.json
%USERPROFILE%\.warp\snapit\layouts\*.json
```

That includes the .NET quirks: numbers may be strings, points/sizes are
`"x,y"`, colours are `"#AARRGGBB"`, `AreaPadding`/`BorderThickness` may be a
number or a thickness string, and the misspelled `DeactivedScreens` field is
kept as-is. Missing user files are seeded from `<exe dir>\settings\snapit`,
just like the .NET companion (`packaging/windows/build.ps1` stages the default
seed next to the exe).

## Build and run

```sh
# Linux: shared math/settings tests only (the engine is Windows-only)
cargo test

# Windows
cargo build --release
target\release\snapit.exe --parent <warp-station pid>

# Print the monitor/layout/zone math without starting the engine
target\release\snapit.exe --dump-zones
```

`snapit.exe` uses the mutex `Local\DevTools.SnapIt.Rust`, while the .NET
companion uses `Local\DevTools.Tools.SnapIt.Companion`. Both can therefore be
started for comparison, but running both at once installs two global hooks and
is not recommended.

## Implementation notes

- `geometry`, `settings`, `zones`, `render` and `cli` are platform-neutral and
  unit tested on Linux. `zones` is a direct port of the .NET `FindRectangle`
  algorithm.
- The overlay UI is not Avalonia: `overlay` creates layered Win32 windows and
  `render` draws the areas, borders, mini overlays and merged-zone icon into a
  `tiny-skia` pixmap, which `win32::LayerSurface` uploads with
  `UpdateLayeredWindow`.
- The engine thread owns all windows and processes coalesced hook messages
  (`WM_APP_DRAG_START` / `MOUSE_MOVE` / `DRAG_END` / `CANCEL`). Mouse moves
  never block the hook threads.
- The left-button placement is delayed by 100 ms with a one-shot `WM_TIMER`,
  and repeated when the window crosses monitors with different DPI, matching
  `SnapManager.MoveWindow`.
- `snapit.log` in the settings folder records startup, hook and overlay
  failures; the release binary is a console-subsystem executable.
