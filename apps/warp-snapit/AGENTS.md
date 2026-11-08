# warp-snapit — agent instructions

Read [`../../AGENTS.md`](../../AGENTS.md) first. This file adds only the
app-specific facts. The user-facing description is in
[`README.md`](README.md).

## Shape

warp-snapit is a standalone Rust port of the .NET `Tools.SnapIt` companion.
It is **Windows-only**: the package is `snapit` (the directory is
`apps/warp-snapit/`) and the binary is `warp-snapit`. It does not link GPUI or
the `warpui-kit-*` crates.

The platform-neutral modules compile and test on Linux. The engine is gated:

```text
cli, geometry, log, render, settings, zones     # every platform, unit-tested
app, cornering, engine, hooks, overlay,
parent_watch, win32                              # #[cfg(windows)]
```

`src/lib.rs` is the module list; `src/main.rs` parses the CLI and dispatches to
`snapit::run` on Windows.

## CLI

```text
warp-snapit [--parent <PID>] [--dump-zones] [-h|--help] [-V|--version]
```

- `--parent <pid>` — exit when the given process exits; Warp Station uses it
  to tie the companion to its own lifetime.
- `--dump-zones` — print monitors and snap zones, then exit; the
  platform-neutral layout check.
- The release binary is console-subsystem on purpose; it logs to `snapit.log`
  and can be diagnosed from a terminal.

## Shared configuration

Reads the versioned snake_case files; Warp Station reads the same layouts:

```text
%USERPROFILE%\.warp\snapit\settings.json
%USERPROFILE%\.warp\snapit\excluded_applications.json
%USERPROFILE%\.warp\snapit\layouts\*.json
```

The old .NET-shaped `Settings.json`, `ExcludedApplicationSettings.json` and
PascalCase layouts are importer input only: they are read once, rewritten
native, then retired. Keep the value quirks the importer tolerates: numbers may
be strings, points/sizes are `"x,y"`, colours are `"#AARRGGBB"`, and
`AreaPadding`/`BorderThickness` may be a number or a thickness string. Missing
user files are seeded from `<exe dir>\settings\snapit`; `scripts/build.ps1`
stages the default seed next to the exe.

Changing a field, a file name or the settings location is a cross-app contract
change: update Warp Station in the same commit. The schema contract is
[`../../docs/agents/settings-schema.md`](../../docs/agents/settings-schema.md).

## Engine notes

- The overlay is layered Win32 windows; `render` draws areas, borders, mini
  overlays and the merged-zone icon into a `tiny-skia` pixmap, and
  `win32::LayerSurface` uploads it with `UpdateLayeredWindow`.
- The engine thread owns all windows and processes coalesced hook messages
  (`WM_APP_DRAG_START` / `MOUSE_MOVE` / `DRAG_END` / `CANCEL`). Mouse moves
  never block the hook threads.
- Placement is delayed by 100 ms with a one-shot `WM_TIMER` and repeated when
  the window crosses monitors with different DPI, matching
  `SnapManager.MoveWindow`.
- The process holds the mutex `Local\DevTools.SnapIt.Rust`. The .NET companion
  used `Local\DevTools.Tools.SnapIt.Companion`; it cannot read the native
  files, so its old files are importer input, not a live format.
- DPI awareness is per-monitor v2; the startup path also handles
  `SetConsoleCtrlHandler` and the parent watcher.

## Tests and builds

```sh
cargo test -p snapit          # Linux: cli, geometry, render, settings, zones
cargo build -p snapit         # only on Windows
```

The unit tests live beside the neutral modules (`cli`, `geometry`, `render`,
`settings`, `zones`). There are no Linux tests for the Win32 engine.

Windows:

```powershell
scripts\build.ps1 -App warp-snapit
```

The suite ships a portable zip with `warp-snapit.exe` plus the default settings
seed. `linux_skip = true` keeps it out of the Linux bundle. There is no `.iss`
and no installer.

## Do not

- Make a neutral module depend on `windows` or `tiny-skia` in a way that
  breaks its Linux unit tests.
- Reintroduce the PascalCase field names; the old spelling belongs in the
  importer fixture only.
- Change the `"x,y"` encoding.
- Add a desktop entry or an installer; this app is a Windows portable zip.
- Hide the console in the release binary; the log and `--dump-zones` are
  diagnostic surfaces.
