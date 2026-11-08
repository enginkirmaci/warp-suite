# Warp Station

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives under `~/.warp`: this app's own folder `~/.warp/station`
(versioned `settings.json`, `cache/repos.json`, `cache/models.json` and
`assistant/` with the prompt templates), the suite's shared `logs/`, the kit's
`ui.json` and the SnapIt settings. Repo tags live in `settings.json`, so the
cache is disposable. The notes store is user content in `~/notes`. An old
`opencode/` folder — or a stray `ai/` from a development build — is split on
startup, a legacy `~/.devtools` root is absorbed into `~/.warp` on first
launch, and the root files move into `~/.warp/station` once.

## warp-agent integration

The AI surface (commit-message / README wands, the daily summary, the
harness/model/variant catalog) runs in [warp-agent](../warp-agent) over its
loopback door (`~/.warp/agent/ipc.port` + `ipc.token`) — Warp Station no longer
spawns `opencode serve` itself. Rules: a missing warp-agent binary turns the
AI features off; a merely closed one is launched on demand. Wand runs happen
as visible warp-agent chats in the repo's workspace (added when missing), the
daily summary runs in warp-agent's General workspace, and tool-permission
asks are answered in warp-agent's own UI. Settings picks the harness (the
door reports the options), one model and that model's variant; commit-message
generation uses the same selection as every other wand.

The switches are independent: **Enable warp-agent** arms the whole AI
surface (wands, summary, the catalog), while **Enable OpenCode** and
**Enable DeepSeek** only show the repo rows' CLI buttons, which launch the
`opencode` / `dsh` CLIs themselves in a terminal (`opencode` with
`--model <model>` when the opencode harness is selected; `dsh` boots its own
profile and takes no model) — that launch never goes through warp-agent, and
the AI features never need the CLI binaries.

## Package

Packaging is driven by the suite, not from this directory:

- Linux: `scripts/build.sh --app warp-station` stages the binary, the desktop
  entry and hicolor icons; `scripts/deploy.sh --local` installs them.
- Windows (build on a Windows box — GPUI has no Linux→Windows cross build):
  `scripts/build.ps1 -App warp-station` writes the portable zip and the Inno
  Setup installer. The app icon is embedded into the exe by `build.rs` from
  `packaging/windows/warp-station.ico`.

This directory only carries the assets: `packaging/linux/{*.desktop,icons}`,
`packaging/windows/{*.ico,*.iss}`.

## SnapIt

The Windows zone snapper lives in its own repo,
[enginkirmaci/warp-snapit](https://github.com/enginkirmaci/warp-snapit), and
reads the same `~/.warp/snapit` settings/layouts. `warp-snapit.exe` is
distributed through [warp-suite](https://github.com/enginkirmaci/warp-suite)
releases (`snapit.executable` in the settings points Warp Station at the exe).
