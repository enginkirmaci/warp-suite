# Warp Suite design contract (v3)

This folder is the committed copy of the design the apps implement. The live
source is the OpenDesign project **warp-suite-v3**
(<http://127.0.0.1:7456/projects/warp-suite-v3>) — `DESIGN.md` is the contract
and wins when it disagrees with the prototype.

| Path | What it is |
|---|---|
| `DESIGN.md` | the contract: shell, colour, type, components, per-app pages, motion |
| `BRIEF.md` | why the shipped app was rejected and what the design decided |
| `source/app.src.html` | the working prototype, all three apps in one shell |
| `source/warp.css` | the prototype's tokens and components |
| `source/icons.js` | the prototype's inline icon set |
| `source/design-system.src.html` | the browsable design-system page |
| `app.html`, `design-system.html` | single-file bundles for opening without the server |

The same rules are implemented in Rust in
[`warpui-kit-core`](https://github.com/enginkirmaci/warpui-kit-core):
`palette` (Copper and the entity hues), `fonts` (Inter + JetBrains Mono and the
type scale), `shell` (title bar, rail, tabs, list and detail panes, rows) and
`motion`. An app keeps no colour, font family or font size of its own; a screen
that does not fit the shell is a design bug, not an app exception.

## Porting rules

- One shell everywhere: rail + list + detail. Settings, Tools and Summary
  included. Detail never appears anywhere else — no inspector, no bottom
  sheet, no drawer. A blocking dialog is the only overlay.
- Tabs replace panels.
- Twelve px is the floor; the ladder is `caption` 12 to `title` 22.
- Colour means something: one hue per entity, always with an icon and a
  number/word, never colour alone.
- One filled primary per view. Elevation comes from surface value; only
  dialogs, menus and toasts have a shadow.
- Waiting states say what is happening where it started, and pair motion with a
  word; reduce motion keeps the words and drops the movement.
- Copper is the shipped palette. Plum and Pine stay in the prototype for
  comparison.

## Status

| App | Design work | Notes |
|---|---|---|
| warp-station | shipped | shell, the seven detail tabs, Summary, Tools and Settings on the v3 design |
| warp-agent | shipped | shell rail, workspace tree, tile grid and the four-section Settings page |
| warp-notes | shipped | shell rail, note tree, editor and the General/Appearance Settings page |
| warp-snapit | overlay colours | headless Windows engine; Station hosts its page under Tools |
