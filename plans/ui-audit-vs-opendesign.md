# UI audit against the OpenDesign warp-suite-v3 design

Status: first fix pass landed 2026-11-11 (see Resolution); the remaining
items are listed at the end.
Scope: warp-station, warp-agent, warp-notes (desktop GPUI apps), checked against
`warp-suite-v3/DESIGN.md`, `source/warp.css`, `source/icons.js` and `app.html`
in OpenDesign.

## How it was checked

- **Test data**: a scratch `HOME` (`/tmp/warp-audit/home`) seeded with 12 git
  repositories in mixed states (dirty, staged, ahead/behind, archived,
  favourites, long-line and binary diffs, one no-README), a 9-note store with
  frontmatter, wiki links, tasks and pinned/recent entries, station settings
  with two scan roots and tags, and agent/notes settings.
- **Capture**: each app launched on a **headless Hyprland output**
  (`hyprctl output create headless`, workspace 81), never on the owner's
  monitor, screenshotted with `grim` and cropped to the window. 34 screens:
  Repositories, Overview, Changes, History, Pull requests, Issues, Pipelines
  hook, New pull request hook, Summary, Tools (Formatters, NuGet, Clipboard),
  Settings, About, About › license, grouped list, Notes panel, Notes (editor,
  pinned, settings, about, row menu, quick open, shortcut sheet), Agent (grid,
  Settings ×4, About), plus the four station dropdowns.
- **Source sweep**: every `text_size(px(…))`, `rounded(…)`, icon size and
  literal colour in `apps/*/src` and `kits/*/src`, plus an SVG geometry dump
  (viewBox, stroke width, path hash) of all 178 SVG assets.
- **Token parity**: palette CSS variables from OD parsed and diffed against
  `palette::RAMPS`; shell constants and type ladder parsed from the kit and
  compared with DESIGN.md.

Caveat: no `gh` and no Azure PAT in the scratch HOME, so GitHub/Azure-backed
surfaces rendered their empty states; a few controls are gated on that data
(see findings). Captures are of an unfocused window, so Hyprland dims them ~20%;
colour checks below were done on pixels, not by eye.

## What matches

| Area | Design | Implementation | Verdict |
|---|---|---|---|
| Palettes | 5 ramps, 17 tokens each | `palette::RAMPS` | **0 mismatches**, all five ramps, incl. entity hues and diff washes |
| Type ladder | 12/13/13/14/15/16/20/22 | `fonts::BASE_TOKENS` | matches |
| Shell | titlebar 46, rail 76, strip 40, list 432, padding 24, row 64, control 34, field 36, tab 42, underline 2 | `shell.rs` constants; measured 46/76/40/432/64 on captures | matches |
| Radii | control 6, card 8, dialog 10, keycap 4 | `.rounded(px(6.))` ×99, `px(8.)` ×40, `px(10.)` ×1, `px(4.)` ×45 | matches, except the dialog shells below |
| Icons (geometry) | one Lucide set, stroke 1.9 | shared concepts (`close`, `star`, `trash`, `refresh`, `search`, `plus`, `cog`, `check`, `folder`, `chevron`, `panel-left`) are **byte-identical across the three apps**, stroke 1.9 | matches |
| Scrim | every dialog and palette | `.bg(SCRIM)` in about, notes dialogs, station dialogs; pixels confirm a 50 % dim | matches |
| Type sizes in apps | tokens only | 200 `font_meta`, 98 `font_body`, 66 `font_control`, 44 `font_mono_meta`, 21 `font_name`, 19 `font_screen_title` … | only one literal below the floor (T1) |

## Findings

Severity: **P1** breaks a stated rule or a whole surface, **P2** visible
inconsistency, **P3** polish.

### Type

| # | Sev | Finding | Evidence | Design says |
|---|---|---|---|---|
| T1 | P1 | Markdown editor's find-bar match count is **11 px** — the one text below the 12 px floor | `kits/warpui-kit-markdown/src/editor/element.rs:574` | "Twelve is the floor… no text below 12 px, at any text size setting" |
| T2 | P2 | Diff body draws at **14 px with a 22 px row pitch**; the design's diff is mono **13/21** | `apps/warp-station/crates/warp-station/src/ui/changes.rs:525` passes `font_control`; measured 22 px pitch in the Changes capture | Type table: `mono 13/18`, "Diffs use 13 / 21" |
| T3 | P2 | README / summary prose is **14 px**, the design's running text is **15/25** | `apps/warp-station/crates/warp-station/src/ui/readme.rs:305` (`font_body`), `ui/markdown.rs:60` | "`note` 16/26 … README and summary at 15 / 25" |
| T4 | P2 | Dialog titles come in three sizes: station dialogs and the license dialog **15/600**, Notes' shortcut sheet **22/600** | `dialogs.rs:123` (`font_brand`), `about.rs:471` (`font_name`), `notes.rs:4509` (`font_screen_title`) | dialogs are one family shape |
| T5 | P3 | Two literal sizes in the markdown editor (12, 11) where a token belongs | `element.rs:511`, `:574` | tokens only |

The repo's own `docs/agents/design-system.md` now says "diff body `font_control`
(14)", which **contradicts OD's 13/21** — one of the two docs has to move.

### Surfaces and radii

| # | Sev | Finding | Evidence | Design says |
|---|---|---|---|---|
| R1 | P2 | App dialog shells are **radius 8 on `panel`**; OD dialogs are **radius 10 on `overlay`** | `dialogs.rs:100-104` (`.rounded(px(8.))`, `theme::card()`), `notes.rs:4158-4160` (same) | "Small radius 6, cards 8, dialogs 10"; `overlay` = dialogs and menus |
| R2 | P3 | The Notes shortcut sheet and station dialogs sit on `panel` while the kit's license dialog uses `popup()` | `notes.rs:4507`, `about.rs:454` | dialogs take `overlay` |
| R3 | P3 | Pills are hand-computed half-heights (`px(12.)` for h24, `px(14.)` for h28) instead of `rounded_full()`, which is used 28 times elsewhere | `overview.rs:608`, `:1499`, `window.rs:1232` | one pill shape |
| R4 | P3 | An 18 px hover square uses `rounded(px(3.))`, off the 4/6/8 grid | `repos_list.rs:217` | 4/6/8/10 only |

### Icons

| # | Sev | Finding | Evidence |
|---|---|---|---|
| I1 | P2 | Three icon languages in one set: Lucide stroke 1.9, **MDI solid** icons (`icon-account`, `icon-chart-bar`, `icon-clipboard`, `icon-home`, `icon-info`, `icon-lock`, `icon-menu`, `icon-sln-box`, `icon-text-format`, `icon-vscode`, and notes' `icon-info`) and Material-ish outlines — the solid ones read much heavier at the same size | asset dump of `apps/*/assets/icons` |
| I2 | P2 | `icon-window-restore.svg` has a **13.22 viewBox** (station and notes); at 16 px its stroke renders ~2.6, visibly heavier than its siblings | `apps/warp-notes/assets/icons/icon-window-restore.svg`, same file in station |
| I3 | P3 | Stroke weights drift: 1.8 (`icon-star-filled`, `icon-alert`, `icon-app-window`), 1.9 (norm), **2.2** (`icon-logo-mark`), **8** (`warp-agent/logo-suite`) | asset dump |
| I4 | P2 | Off-ladder icon sizes: **7 px** arrows in dialogs, **13 px** folder and chevron in the repo list, 12 px dialog close | `dialogs.rs:546`, `:567`; `repos_list.rs:225`, `:274`; `dialogs.rs:141` |
| I5 | P3 | Three separate sets (station 87, agent 47, notes 36) for ~90 concepts, with the same glyph stored under `icon-x.svg` and `x.svg`; OD ships one 82-icon set | `find apps kits -name '*.svg'` → 178 (incl. 4 kit marks and 7 logos) |

### Menus and dropdowns

| # | Sev | Finding | Evidence |
|---|---|---|---|
| M1 | P2 | Sort dropdown offers **six** modes (Last activity, Name, Attention first, Changes, GitHub, Branch); OD's `REPO_SORTS` has **three** | `repos.rs` sort menu; OD `app.src.html:297` |
| M2 | P2 | The filter trough can show **two active segments** at once: All and Archive. Archive is bound to `show_archived`, a visibility toggle, inside a single-select control | `repos.rs:875-890` (`show_archived` as the segment's `selected`) |
| M3 | P2 | Menu row heights differ by surface: **32** (Notes row menu), **33** (Sort), **42** (repo ⋯, launch menu) | measured on captures |
| M4 | P2 | Repo ⋯ menu drops **Open in GitHub** and **Copy GitHub URL** whenever no remote URL is known; the design lists them unconditionally | `overview.rs` repo menu builder; OD has all four |
| M5 | P2 | Notes' shortcut sheet is a **flat list**; OD groups the key map as **tabs** (tree, editor) | `notes.rs:4467` |
| M6 | P2 | Quick open sorts **by path** and never applies the pinned → recent → rest order | `notes.rs:1331-1337` |
| M7 | P3 | Tag dropdown opens flush over the first row with a single "Any" item when the store has no custom tags; card chrome itself is correct | `repos.rs:1001` |
| M8 | P3 | Launch menu repeats the "Open in" card verbatim (same five actions, same order) in a second shape | capture `619608-menu-launch.png` |

### Missing or gated surfaces

| # | Sev | Finding | Evidence | Design says |
|---|---|---|---|---|
| S1 | P1 | **Pipelines tab disappears** when neither a GitHub nor an Azure snapshot is available; the tab cannot be selected and no empty state is shown | `store/mod.rs:3516` (`WorkspaceTab::Pipelines => azure || actions`) | "A repository with no runs gets the tab and an empty state" |
| S2 | P1 | Issues toolbar loses **Filter issues** and **Sort** when the list is empty, and **New issue** disappears without a remote URL | `pr_issues.rs:478` (`total > 0`), `:269` (`when_some(new_target)`) | "Toolbar with Open and Closed, a Filter issues field, Sort, and New issue" |
| S3 | P1 | History commits column is **420 px**, not 320; and the commit detail has **no file rail + diff** — files expand inline folds instead | `changes.rs:72` (`HISTORY_COLUMN_W: 420.`), `render_commit_detail` | "a 320 commit column … its files in a secondary rail and the selected file's diff in the Changes viewer" |
| S4 | P2 | Summary page shows the title "Summary" twice, in the list head and again in the detail head | capture `28-station-summary.png` | list head carries the summary line, detail carries the content |
| S5 | P2 | About shows **"This app" twice** — on the list row and again as a hero tag | capture `33-station-about.png`, `kits/warpui-kit-core/src/about.rs` | "the row for the app you are in carries a `This app` tag" |
| S6 | P2 | Agent's tree scopes add a fourth chip, **Running**, next to All / Needs you / Favorites | `apps/warp-agent/src/ui/window.rs:63` | "the All / Needs you / Favorites scope" |
| S7 | P3 | Agent summary is "0 open" on an empty store instead of the designed `2 need you · 1 running · 4 open` shape | capture `50-agent-default.png` | tree summary line |

## What could not be exercised

- GitHub/Azure surfaces with real rows (PR detail, PR tabs, issues list, runs):
  no `gh`/PAT in the scratch HOME. Their empty states were audited instead.
- Right-click **Copy** context menu (needs a pointer drag to select text) and the
  input widget's Cut/Copy/Paste menu. Source: both are kit `PopupMenu`s, the
  read-only one carries a single Copy item, as the design says.
- Hover, focus rings, armed toast actions, motion and the busy verbs: a hidden,
  unfocused window gets no pointer and no keyboard.

## Suggested order of fixes

1. S1/S2 — the two surfaces a user cannot reach at all (Pipelines tab, Issues
   toolbar) and S3's 420 px column, which is a one-line constant.
2. T1 — the 11 px find count, the only floor breach.
3. M2 — the double-active filter; it misreports the list's state.
4. T2/T3/T4 + R1/R2 — settle the doc-vs-app type and dialog-surface questions
   in one pass, updating `docs/agents/design-system.md` and OD's `DESIGN.md`
   together.
5. I2/I4, M3, M5–M8 — icon and menu polish.

## Reproduce

```sh
python3 /tmp/warp-audit/seed_ui_data.py                 # writes /tmp/warp-audit
hyprctl output create headless                          # once per session
hyprctl eval 'hl.monitor({ output = "HEADLESS-1", mode = "1920x1080", position = "4800x0", scale = 1 })'
python3 /tmp/warp-audit/capture.py .build/target/debug/warp-station /tmp/out.png \
  --env WARP_STATION_PAGE=about --wait 7 --x 50 --y 60
grim -o HEADLESS-1 /tmp/full.png
```

Menus were captured with a throwaway env hook in `ReposPage::new`
(`WARP_STATION_MENU=launch|sort|tag|more|more-armed`) and in Notes' page
constructor (`WARP_NOTES_MENU=row|quick|shortcuts`); both hooks were reverted
and the binaries rebuilt clean.

## Resolution (2026-11-11)

Landed in this pass, with the full gate green (`cargo fmt --all --check`, clippy
`-D warnings` and tests on station, station-core, notes, kit-diff, kit-markdown):

| Finding | What changed | Where |
|---|---|---|
| S1 | Pipelines is a detail tab for every repository; its body owns the truth: "no GitHub or Azure DevOps remote", "both turned off", or the loading line while an enabled source settles | `store/mod.rs` `detail_tabs`, `ui/azure_tab.rs` |
| S2 | Issues keeps Filter issues and Sort at any row count; the empty state names the next step or the missing integration; New stays target-gated | `ui/pr_issues.rs` |
| S3 | History commit column 420 → 320, and the row now puts author, age, unpushed mark and SHA on the meta line so nothing clips | `ui/changes.rs` |
| T1 | the find bar's count and button use the 12 px floor token | `kits/warpui-kit-markdown/src/editor/element.rs` |
| M2 | Archive is an exclusive filter (on = archived alone, off = live alone); All/Needs attention/Favorites/Archive never double-light; counts follow | `store/projection.rs`, `ui/repos.rs` |
| T2 | diff body 13/21 (`font_mono_meta` + `DIFF_LINE_HEIGHT`), every Station diff surface | `ui/changes.rs`, `ui/pull_request.rs`, `ui/azure_pull.rs`, `kits/warpui-kit-diff/src/lib.rs` |
| T3 | README and summary prose at the 15 step | `ui/markdown.rs`, `ui/readme.rs` |
| T4 | every dialog title is 15/600 (the shortcut sheet left the 22 step) | `ui/notes.rs`, `ui/dialogs.rs` |
| R1/R2 | dialog cards: `overlay` surface, 10 radius, in Station and in every Notes scrim card | `ui/dialogs.rs`, `ui/notes.rs` |
| I2 | `icon-window-restore` replaced with a 24-grid stroke-1.9 glyph in both apps | assets |
| I4 | 7 px dialog arrows → the 12 px icon step; close icon uses the token | `ui/dialogs.rs` |
| M3 | menu rows normalised to 32 (the armed remove row keeps its second line) | `ui/overview.rs`, `ui/repos.rs` |
| M5 | shortcut sheet grouped as Tree / Editor tabs | `ui/notes.rs` |
| M6 | quick open orders pinned → recent → rest, deduped, with a unit test | `ui/notes.rs` |
| docs | diff 13/21, dialogs `overlay`/10 + 15/600 title, menu rows 32, running text 15/25 | `docs/agents/design-system.md`, OD `DESIGN.md` |

Two audit entries were withdrawn after checking the prototype: the launcher
dropdown repeating the "Open in" card (the prototype has both) and the tag
menu's anchor (the prototype opens its menus at the same offset).

Found and fixed during verification: with an exclusive filter the rail kept a
selected repository the list no longer showed. `ensure_row_selected` now checks
the projection, the four filter setters reconcile the selection, and an empty
projection shows the detail's empty state.

Still open (not in this pass): M4 (repo ⋯ menu hides the GitHub entries without
a remote), I1/I3/I5 (three icon languages, stray stroke weights, three sets),
R3/R4 (hand-computed pill radii, the 3 px hover square), S4 (Summary title
twice), S5 ("This app" twice), S6 (Agent's fourth scope chip), S7 (Agent's
short summary line).
