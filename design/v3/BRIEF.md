# warp-suite-v3 — brief

One dark design language for every Warp Suite desktop app: warp-station (flagship),
warp-agent, warp-notes, warp-snapit. Apps are Rust/GPUI, so everything must be
drawable with flat fills, 1 px borders and small radii. No blur, glass, gradients.

## What the owner rejected in the shipped app

1. **Two competing detail surfaces.** A right inspector AND a bottom workspace
   sheet (Changes / PRs / Issues / README). The sheet covers most of the window
   and squeezes the repo list to a strip. Consistency is gone.
2. **Too blue, too dark.** Near-black blue ramp `#0B0D10`..`#151A21` and a blue
   accent `#2A68DF`. Must stay a dark theme, but lifted and with nicer colour.
3. **Low legibility.** Type ladder 8 / 9 / 9.5 / 10 / 10.5 / 11 / 11.5 px. It is
   hard to tell at a glance what is what. More than bigger text is needed.

## Decisions

- **One shell, no exceptions:** rail + list pane + detail pane. Every screen of
  every app is that shape. Detail never appears anywhere else: no right
  inspector, no bottom sheet, no drawer. Blocking dialogs are the only overlay.
- **Tabs replace panels:** the repo detail holds Overview, Changes, Pull
  requests, Issues, README in one tab strip.
- **Floor of 12 px.** Body 14, row names 15/600, page title 22/600. No all-caps
  micro labels.
- **Entities have a colour and an icon:** branch, changes, pull requests, issues,
  behind, ahead each keep one hue everywhere, always with an icon and a number.
- **Filled accent primary,** one per view.

## Owner feedback from the Ember round that still applies

- Repo row stats use icons: branch, changes, issues, PRs, pull count, push count.
- Summary line reads `7 of 11 need attention · 82 changes · 19 issues`.
- Filters read `All 10 | Needs attention 0 | Favorites 2 | Archive 8`.
- Row has no pull or push buttons; the only row action opens the default app.
- The link into the Changes tab is called **Review**, not Commit.

## Deliverables

- `DESIGN.md` — the contract, with a token map onto `warpui-kit-core`
- `design-system.html` — tokens and components, live
- `app.html` — working prototype of Station, Agent, Notes, SnapIt in the same shell
