# Warp Design System v3

The shared dark language for Warp Station, Warp Agent and Warp Notes. SnapIt has no
window of its own, so Station hosts it under Tools. This language replaces the
near-black blue build and the Ember proposal.

- `app.html`: working prototype, three apps in one shell
- `design-system.html`: every token and component, live
- `source/`: the unbundled pages plus `warp.css` and `icons.js`. The two pages above
  are single files because the OpenDesign preview cannot load relative links
- `BRIEF.md`: what was rejected and why

When the prototype and this file disagree, this file wins.

## 1. Rules

1. **One shell.** Every screen of every app is rail + list + detail. Settings,
   Tools and Summary included. There is no screen with another shape.
2. **One detail surface.** The selected item opens in the detail pane and nowhere
   else. No right inspector, no bottom sheet, no drawer, no second detail column.
   A repository tab is that same pane at full width.
3. **Tabs replace panels.** What used to be a panel is a tab. A tab owns its
   toolbar and its one primary action.
4. **Twelve is the floor.** No text below 12 px, at any text size setting. The
   clamp lives in `fonts::scaled_by`, so even Small (×0.875) cannot breach it.
5. **Colour means something.** Each entity has one hue everywhere. A healthy row
   stays quiet, so a row that needs you stands out without being read.
6. **One overlay.** A blocking dialog is the only thing that floats over content,
   plus menus and toasts.

## 2. Shell

| Part | Measure | Notes |
|---|---|---|
| Title bar | 46 | App name, search (Station: everything; Notes: the note store), activity chip, list toggle, launches for the sibling apps, window controls |
| Rail | 76 | App sections, Settings pinned to the bottom. Icon 22, label 12/500 |
| Tab strip | 40 | Right of the rail, above list and detail. Tabs 120 to 240 wide |
| List | 432 | Resizable 320 to 560, remembered per app. Agent starts at 320: its tree rows are short and the grid wants the width |
| Detail | rest | Padding 24. Reading content capped at 760, card grids at 1180. Agent's tile grid takes the pane with a 12 gutter instead |
| Row | 64 | Selected: `raised` fill plus a 3 px `accent` left edge. Tree rows (Agent, Notes) are 40 and 34 |
| Control | 34 | Buttons. Fields 36. Small radius 6, cards 8, dialogs 10 |
| Tab | 42 | Selected: 600 weight plus a 2 px `accent` underline |

`Ctrl+\` hides the list. Below a 1180 px window, opening a wide tab (Changes) hides
it on its own, and so does a tile grid of two or more. The selected tab is kept while
moving between items.

### Repository tabs

A repository opens two ways. A click on a row previews it in the list and detail
view, as before. **Open in tab** gives it a tab of its own, with the list hidden so
the detail takes the full width.

- **Repositories** is the first tab, it holds the list and cannot be closed.
- Open a tab with the **Open in tab** button in the detail header, the same button
  on a row (hover), a middle click on a row, or `Ctrl+Enter` for the selected row.
- When a repository already has a tab, the button reads **Go to tab** and the row
  shows the tab icon in `accent-ink`. A repository has at most one tab.
- A tab shows the name and, when there are any, its uncommitted changes count.
- Close with the × (shown on hover and on the active tab) or a middle click. Closing
  the active tab moves to its neighbour, or to Repositories when none is left.
- Each tab keeps its own detail tab and selected file. Open tabs come back on restart.
- Tabs that do not fit scroll with the wheel, and the active tab is always in view.
- `Ctrl+\` shows the list inside a repository tab. There, a click on a row goes to
  that repository's tab if it has one, otherwise to its preview in Repositories.
- The strip belongs to the Repos section, so it is hidden on Summary, Tools and Settings.
- In the app: `Ctrl+T` opens, `Ctrl+W` closes, `Ctrl+Tab` and `Ctrl+1..9` move between
  tabs. The prototype leaves these out because the browser keeps them.

The strip is a shell part. Notes and Agent can adopt it for long-lived items; that
choice is made when each app is ported.

Elevation comes from surface value. Only dialogs, menus and toasts have a shadow.
Nothing here needs blur, gradients or transparency beyond the diff tints.

## 3. Colour

The darkest step is a recessed well, not the window. Every palette keeps the
same structure, so a palette is a swap of these values only. Copper is the default.

| Token | Role | Copper | Plum | Pine |
|---|---|---|---|---|
| `well` | Inputs, diff, code | `#1C1A18` | `#1C1A21` | `#171C1B` |
| `chrome` | Title bar, rail | `#201E1C` | `#211E27` | `#1B2120` |
| `base` | List pane, cards | `#252321` | `#26232D` | `#202625` |
| `panel` | Detail pane | `#2B2926` | `#2C2934` | `#262D2B` |
| `raised` | Selected, hover, active | `#34312E` | `#36323F` | `#2F3735` |
| `overlay` | Dialogs, menus | `#3A3733` | `#3C3846` | `#353E3B` |
| `line-soft` | Row separators | `#2F2C29` | `#302D38` | `#2B3331` |
| `line` | Pane edges | `#3B3834` | `#3C3846` | `#36403D` |
| `line-strong` | Control borders | `#4F4B46` | `#524D5C` | `#4A5551` |
| `text` | Names, values | `#F6F2EB` | `#F5F2F7` | `#EFF4F1` |
| `text-2` | Secondary copy | `#D2CBC0` | `#D1CBD8` | `#C8D2CD` |
| `text-3` | Timestamps, hints | `#ABA397` | `#AAA3B3` | `#9FABA6` |
| `accent` | Primary fill, selection | `#EE9A5F` | `#F2907F` | `#E5C066` |
| `accent-hover` | Primary hover | `#F5AC78` | `#F7A698` | `#EFCF80` |
| `on-accent` | Text on primary | `#24140A` | `#2A0F0B` | `#231B05` |
| `accent-ink` | Links, focus | `#F2A877` | `#F5A294` | `#E9C976` |
| `accent-soft` | Brand mark, active zone | `#43301F` | `#452B33` | `#3A3620` |

**Blue** is the fourth palette, shipped with the others. Its ramp is the same
structure on a cool slate field, so every step keeps the same role and the same
contrast relationships:

| Token | Role | Blue |
|---|---|---|
| `well` | Inputs, diff, code | `#181A20` |
| `chrome` | Title bar, rail | `#1D2027` |
| `base` | List pane, cards | `#22262E` |
| `panel` | Detail pane | `#282D36` |
| `raised` | Selected, hover, active | `#2F3540` |
| `overlay` | Dialogs, menus | `#363C48` |
| `line-soft` | Row separators | `#2A2F38` |
| `line` | Pane edges | `#363C47` |
| `line-strong` | Control borders | `#49505C` |
| `text` | Names, values | `#F0F4FB` |
| `text-2` | Secondary copy | `#C9D2E0` |
| `text-3` | Timestamps, hints | `#9FA9B8` |
| `accent` | Primary fill, selection | `#5B9DF5` |
| `accent-hover` | Primary hover | `#77B0F8` |
| `on-accent` | Text on primary | `#0A1626` |
| `accent-ink` | Links, focus | `#8FBBFA` |
| `accent-soft` | Brand mark, active zone | `#1E2F4A` |

Measured: `text` 10.0:1, `text-2` 7.3:1, `text-3` 4.7:1 and `accent-ink` 5.6:1
on the darkest surface; the primary label is 6.6:1 on its fill. Every palette
is checked by `palette::tests::every_palette_clears_aa_and_orders_its_ramp`, so
a new one cannot ship unmeasured.

Measured contrast (WCAG): `text` is at least 9.9:1 and `text-2` at least 7.1:1 on
every surface, `text-3` at least 4.6:1, primary label at least 7.7:1.

### Entities

Fixed across palettes and apps. Always an icon plus a number or word, never colour
alone. All clear 4.5:1 on every surface.

| Token | Hex | Meaning | Icon |
|---|---|---|---|
| `branch` | `#6ED0BE` | Branch (icon only, the name stays `text-2`) | git-branch |
| `changes` | `#F2BC5B` | Uncommitted work, anything that needs a person | file-plus |
| `pr` | `#BCA6F7` | Pull requests | git-pull-request |
| `issue` | `#F795AE` | Issues | circle-dot |
| `behind` | `#86C8F2` | Commits to pull, running work | arrow-down |
| `ahead` | `#A3D977` | Commits to push | arrow-up |
| `ok` | `#7FD69B` | Clean, added lines (icon only on rows) | check |
| `danger` | `#F88C86` | Failed, removed lines | x |

Diff tints: added `rgba(127,214,155,.13)`, removed `rgba(248,140,134,.13)`.

## 4. Type

Inter for the interface, JetBrains Mono for code, paths and branches, both SIL OFL.
Their taller x-height helps because GPUI draws grayscale text on Linux. The shipped
ladder was 8 to 11.5 px, so most of the interface sat under 12 px.

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `title` | 22 / 28 | 600 | Detail title |
| `list-title` | 20 / 26 | 600 | List title |
| `section` | 16 / 22 | 600 | Card and group headings |
| `name` | 15 / 20 | 600 | Row names |
| `body` | 14 / 20 | 400 | Copy. Controls and tabs at 500 |
| `meta` | 13 / 18 | 400 | Second lines, counts |
| `mono` | 13 / 18 | 400 | Paths, branches. Diffs use 13 / 21 |
| `caption` | 12 / 16 | 500 | Rail labels. The floor |
| `note` | 16 / 26 | 400 | Note editor. README and summary at 15 / 25 |

No all-caps, no tracked micro labels. Text size offers Small, Normal, Large and
Larger at 0.875, 1.0, 1.12 and 1.25. Small is the one step under 1, and it is
safe because the floor is enforced in the scaling math, not in the table: every
step is clamped at 12 px, so Small tightens chrome, padding and icon sizes while
the smallest text stays at the floor. The old `compact` spelling reads as Normal.

## 5. Components

- **Primary button:** `accent` fill with `on-accent` text. One per view. The open
  tab owns it, so Open in VS Code is primary on Overview and a plain button on
  Changes, where Generate & Commit is.
- **Repo row:** star, name 15/600, time. Second line: branch, changes, issues, PRs,
  pull count, push count, with zero values hidden. On hover the time gives way to
  two buttons: open in tab and open in the default app. There are no pull or push
  buttons on a row.
- **List head:** title, summary line, filters. On Repos the tab strip already names
  the list, so there is no title: the summary shares a line with Refresh, Add and
  Clone. Summary reads `7 of 11 need attention · 82 changes · 19 issues`. Filters
  read `All 10 | Needs attention 0 | Favorites 2 | Archive 8`.
- **List tools (Repos):** under the filters, Tag on the left and Sort on the right
  (Last activity, Name, Attention first). A group header can show only that group.
  Any narrowing brings **Clear filters**.
- **Global search:** results open under the title bar field, grouped as Repositories,
  Notes and Code with the match marked. It shows `Searching…`, then results or
  `No matches.`. It searches everything and never filters the list. Notes' own field is
  the same shape over its own store.
- **Repo header:** Open in VS Code has a split menu with every launcher and its key.
  The ⋯ menu holds refresh, copy path and URL, Open in GitHub, favorite, archive, and
  Remove from repositories, which needs a second click.
- **Overview tab:** Needs you, Open in, Status, Details. The link into Changes says
  **Review**.
- **Git toolbar:** branch menu, Fetch, Pull, Push, Sync. Push reads **Publish** when
  the branch has no upstream.
- **Branch menu:** filter, branches with the current one ticked, delete with a second
  click, New branch… with the prefix from Settings.
- **Changes tab:** Staged and Unstaged groups with Stage All and Unstage All, then the
  commit box on the left, diff on the right. On a row hover: stage or unstage, and
  discard, which needs a second click.
- **History tab:** commits grouped by day with an `unpushed` mark on the left. The
  selected commit on the right with Copy SHA, Revert, Checkout, its files and diff.
- **Pipelines tab:** only for Azure repositories. Latest run state, recent runs, and
  the project's pull requests and work items.
- **README tab:** renders the file. Without one it offers Draft with OpenCode, and a
  draft stays a draft until Save README.
- **Issues tab:** toolbar with Open and Closed, a **Filter issues** field, **Sort**, and
  New issue. The filter runs as you type over number, title and labels, and `Esc`
  clears it. Sort offers Updated (default), Newest, Oldest and Most comments, and the
  menu marks the current choice. The sort is remembered; the filter resets when the
  view changes. No match shows an empty state with Clear filter.
- **Pull request:** a click on a row in the Pull requests tab turns the whole detail
  pane into the pull request. Back or `Esc` returns to the list. It is the only
  view that replaces the repo head, and it has the same regions: title row, subtitle,
  then a 44 high status strip in `base` where tabs would be.
  - Title row: Back, title, Open pill in `pr`, Open on GitHub, Merge pull request.
    Merge is the primary, and it is disabled until the checks pass.
  - Subtitle: author, `repo #number`, branch → base, check summary, `+adds −dels`.
  - Status strip: merge state, approvals, conflicts, base, updated, reviewers on the right.
  - Body, capped at 920: comments with a 32 avatar, then a Checks card. A failed check
    row takes the removed-line tint, a running one a spinner, a skipped one a clock.
- **Settings:** one row per setting, label left and control right, 60 high. Multi-line
  fields stack. Edits wait for **Save**, except Appearance, which applies at once.
- **Workspace tree (Agent):** the list is one tree, General first, then a row per
  workspace folder with its chat count and a `behind` dot while its server runs. A
  workspace opens to its chats: title, state glyph, harness and age. Opening a workspace
  starts its server, and it stops itself after the idle time in Settings, so there is no
  start or stop control. On hover: New chat and a ⋯ menu with Open folder and Remove
  workspace. A chat's ⋯ holds Pin,
  Rename, Remove and Delete chat, which needs a second click. An empty workspace says
  `No chats yet`. The summary reads `2 need you · 1 running · 4 open`.
- **Tile grid (Agent):** the detail pane is the grid. It has no head and no tabs, because
  a head can't say which chat it means, so everything about a chat lives in its tile. The
  columns are the rounded-up square root of the open chats, so three tiles leave an empty
  cell, and one tile fills the pane. A row opens its chat as a tile or focuses it.
  The grid is the same `panel` detail pane every app has, with a 12 gutter and 12 padding
  around it. A tile is a `base` card with a 1 px `line` edge and an 8 radius; its edge goes
  `line-strong` on hover. The focused tile has the `accent` edge, a tile waiting on a
  person the `changes` edge.
- **Tile head:** close, title 15/600, the state word, then the context ring with
  `3% · 29.6k / 1.00M`, on a 44 row over a `line-soft` hairline. The state word is an
  entity stat, the same one the tree uses: `behind` with a spinner for `Running`,
  `changes` with the shield or the chat icon for `Needs approval` and `Has a question`.
  When the chat has tasks the ring opens the flyout, and a live todo batch opens it on
  its own.
- **Tile body:** the transcript is a 760 column centred in the tile, so one tile filling
  the pane still reads at the same measure. It takes 12 padding and a 10 gap, and the
  composer is the card foot, not a box in the tile: a `line-soft` hairline, the `well`
  field, then the pill row on the tile itself. A card inside a tile (approval, question)
  takes `panel` so it lifts off the `base` tile, a plain button inside a tile takes
  `raised`, and the queue rows above the composer are `panel` too.
- **Flyout:** Progress with `1/3` and its tasks, Agents with its running subagents. It is
  the one thing that floats inside a tile.
- **Transcript:** your messages sit right on `raised`, answers left on `panel`, both with
  the tokens and the time. `thinking` is a fold. Each tool is a mono chip, its title and
  a status, with a `Changes +12 −3` fold for its diff, and a failure shows its first
  line in `danger`. Finished runs collapse to `4 tool calls · done`. A ⋯ on a message
  holds Copy, Edit in input, Re-prompt, and Fork from here. Stream notes such as
  `Connected to the opencode event stream.` sit centred in italics.
- **Approval and question cards (Agent):** `changes` border, because they need a person.
  Approval: the tool's kind, its target in mono, a View diff fold, then Allow, Always and
  Deny. Question: `pick an answer for each question`, the options, Custom answer and
  Answer, which stays off until one is picked. The × dismisses it.
- **Queue:** sits above the composer. Each row has Steer (abort the running turn and send
  this next), Edit in the composer, and Remove. Text sent while a chat runs lands here.
- **Composer:** the field reads `Ask opencode...`. Below it: Attach, the auto-allow
  shield, the MCP plug, then the harness, agent, model and variant pills, then Stop while
  running, then Send. The harness pill locks once a chat exists, because a chat stays on
  its server. Under 560 wide the locked pill keeps only its icon and the ring drops the
  percent; under 440 the variant keeps its gauge. The model pill is the one that truncates.
- **Restore:** at launch, `3 chats were open last time` with Restore and Ignore, above
  the grid. With nothing open the pane says `Add a workspace folder to begin.`, `Start the
  server to load your chats.`, or with a server running `Open a chat from the sidebar, or
  start a new one with the + next to a workspace.`
- **Note tree (Notes):** New note and Collapse all folders over the tree. Folders nest,
  with `Summaries/` at the root and under each repository, and the tree is a real file
  tree: file names, `.md` included. New note opens two fields in place, the name and an
  optional folder, with Create and Cancel. A note with unsaved text has a `changes` dot.
- **Notes search:** the title bar's field (`Ctrl K`), searching the whole note store. Its
  results are a capped popup under the field — eight rows, then `N more` — and the tree
  never changes for a search, so the list keeps its scroll and selection while you type.
  The field and its popup are the app's one search; the list pane carries none.
- **Repo scope (Notes):** Station's Notes for this repo opens the tree scoped to that
  repository. The title bar shows the repository as a chip, and ↑ goes back to all notes.
- **Note editor (Notes):** the head is the file name in mono, `● unsaved`, Save, and Delete,
  which needs a second click. The format bar has fourteen buttons in four groups: B I S,
  code, link; H1 to H3; bullet, numbered, task, quote; code block, rule. Markdown renders
  in place and shows its source where the caret is. The status line reads
  `164 words · 951 chars · 49 lines`, `Unsaved · Ctrl+S to save` and `Ln 1, Col 1`. With
  nothing open: `No note open`, `Pick one from the tree, or start a new note.`, New note.
- **Tools:** each tool is a list row and a detail page, with its state in the row.
  Formatters converts line by line (Base64, Pascal Case, Snake Case) and keeps Recent.
  Clipboard Password stores one password and never shows it. NuGet watches a build
  folder and clears the global cache, with a live Activity Log. SnapIt starts and stops
  the engine and previews the layouts.
- **Summary:** lists the summaries already written to `Summaries/` in the notes store.
  **Summarize yesterday** covers every tracked repository and opens in Notes.
- **Dialogs:** Add Repositories scans a folder to a depth and lists what it found, with
  known repositories left out. Clone from URL checks URL, destination and folder name
  inline. Nothing else in the suite floats over content.
- **Toasts:** bottom right, stacked. Success and info leave on their own. An error stays
  8 seconds, has a close button and a `danger` border.
- **Second-click confirm:** anything destructive arms on the first click and shows
  Confirm in place. Any other click disarms it. There is no confirmation dialog.
- **Empty states:** say what is true and offer the next step, never placeholder text.

## 6. Motion and waiting

Quiet and short. Motion says something changed or work is running, it never decorates.
Only opacity and transform move. In GPUI that is `with_animation` plus `Transformation`.

| Token | Value | Use |
|---|---|---|
| `fast` | 120 ms | Hover, press |
| `base` | 180 ms | Entrances, switches, toasts, dialogs |
| `ease` | `cubic-bezier(.2, 0, 0, 1)` | Everything that is not a loop |
| `spinner` | 800 ms, linear | Running work |
| `progress` | 1200 ms, loop | Line under the list head or the detail head |

### Work that takes time

Anything that takes time shows it where it was started and wherever else it can be seen:

1. **Button:** the icon becomes a spinner, the label a verb (`Pull` to `Pulling…`), and
   the button stays at full strength but cannot be pressed. An icon-only button keeps
   its icon where it can spin, Refresh does.
2. **List row:** a spinner and the verb replace the time.
3. **Repo tab:** a spinner in front of the name.
4. **Detail head:** a 2 px `accent` progress line. Refresh all uses the same line
   under the list head.
5. **Title bar:** a chip names the job, or says `2 running`, so it is visible from any
   screen. It disappears when nothing is running.
6. **Finish:** the button shows a green check and the past tense (`Pulled`) for about a
   second, the numbers update, and a toast confirms. Failure uses `danger` and stays
   until dismissed.

A long job names its steps (`Generating…`, then `Committing…`). Git work on one
repository runs one at a time, so its other git buttons are disabled meanwhile. Other
repositories and the rest of the window stay free. A job never blocks input, and a
redraw keeps typed text, focus and scroll positions.

### Small moments

- Detail content fades and rises 6 px, only when the view actually changes.
- The active tab's underline grows in. A new repo tab rises in.
- A favorite star pops. Switches slide once the kit's switch does; 0.7 has them jump.
- Toasts and dialogs rise and fade. A chat that is thinking shows three dots.
- GPUI transforms only icons and images, so a box can't scale: buttons get no press
  shrink, and dialogs rise instead of growing from 97%.
- A row being cloned appears at the top of the list until the clone is done.

**Reduce motion** (Settings, Appearance, `general.reduce_motion`) and the system
setting (`enable-animations` on Linux, `MinAnimate` on Windows) turn all of it off,
spinners included. The busy words stay, so nothing depends on movement.

## 7. The apps

| App | Rail | List | Detail tabs |
|---|---|---|---|
| Station | Repos, Summary, Tools, Settings | Repositories, written summaries, tools, sections | Overview, Changes, History, Pull requests, Issues, Pipelines, README |
| Agent | Chats, Settings | The workspace tree | None: the tile grid fills the pane |
| Notes | Notes, Settings | The note tree, or search results | None: one note in the editor |

Agent has no page of its own for workspaces: they are rows in the tree, and Changes and
tasks live in each tile. Notes has no frontmatter or tags, so there is no Properties tab,
and its text size moves from the tree to Settings with every other app's.

SnapIt is a headless Windows engine with a zone overlay, so it has no rail or settings
of its own.

Station drops `ui/inspector.rs` and `ui/workspace_sheet.rs`; their contents move to
the Overview tab and to the tabs of a new detail pane. The tool drawers become Tools
pages, and the History pane inside Changes becomes its own tab.

## 8. Porting to warpui-kit-core

**Copper is the shipped palette**, with Inter and JetBrains Mono. All four
palettes now ship: `palette::RAMPS` holds Copper, Plum, Pine and Blue as one
`Ramp` struct each, the live palette is a process-wide switch (`set_ramp`), and
the settings control writes the shared `~/.warp/ui.json` under `palette` so the
apps agree on their next start. The published `SURFACE`/`TEXT`/`ACCENT`
constants stay as Copper's reference values and are pinned to it by
`the_default_palette_is_the_copper_constants`. The shell lives in
`warpui_kit_core::shell` (`title_bar`, `rail`, `tab_strip`, `list_pane`, `detail_pane`,
their tabs and rows) so an app cannot drift from it, and `motion` holds the durations
and the spinner.

| `palette.rs` today | v3 token |
|---|---|
| `WINDOW_CHROME`, `TOPBAR_BG`, `SETTINGS_BG`, `FOOT_BG` | `chrome` |
| `SURFACE`, `BACKGROUND`, `GROUP_BG` | `base` |
| `CARD`, `SETTINGS_CARD`, `GHOST_BG` | `panel` |
| `POPUP` | `overlay` |
| `INPUT`, `SEARCH_BG`, `SEG_BG`, `DIFF_BG`, `SYNC_BG`, `KEY_BG` | `well` |
| `CHIP`, `SELECTED`, `SEG_ACTIVE`, `HOVER_BG` | `raised` |
| `BORDER_LIGHT`, `ROW_LINE`, `ROWS_RULE` | `line-soft` |
| `HAIRLINE`, `SEARCH_BORDER`, `SEG_BORDER` | `line` |
| `LINE_STRONG`, `GHOST_BORDER`, `KEY_BORDER` | `line-strong` |
| `TEXT` | `text` |
| `TEXT_DIM`, `KEY_FG` | `text-2` |
| `TEXT_MUTED`, `TEXT_FAINT`, `MUTED_ICON` | `text-3` |
| `ACCENT`, `ACCENT_HOVER`, `ON_ACCENT` | `accent`, `accent-hover`, `on-accent` |
| `BRAND_BG` | `accent-soft` |

`ACCENT_PRESETS` stays until warp-agent is ported, since its settings still offer them.
Copper is the first preset, so a saved `blue` lands on it. `BLUE_BRIGHT` stays as an
alias of `ACCENT_INK` until the apps have moved. The design's names also exist as
`WELL`, `PANEL`, `RAISED`, `OVERLAY`, and the entity hues as `BRANCH`, `CHANGES`, `PR`,
`ISSUE`, `BEHIND`, `AHEAD`.

| `FontTokens` | Now | v3 |
|---|---|---|
| `font_micro` | 8 | 12 |
| `font_mono_meta`, `font_row_meta`, `font_label`, `font_sub`, `font_meta` | 9 to 11 | 13 |
| `font_control`, `font_body` | 10, 11.5 | 14 |
| `font_brand` | 13 | 15 |
| `font_heading` | 12.5 | 16 |
| `font_note` | 16 | 16 |
| `font_screen_title` | 20 | 22 |
| new `font_name` | none | 15 |
| `icon_nano`, `tiny`, `small`, `medium`, `large`, `xl` | 10 to 20 | 12, 14, 16, 18, 20, 22 |

gpui loads font files, not webfonts, so the kit needs Inter and JetBrains Mono as
TTF, italics included. `TEXT_SIZES` is Normal, Large, Larger, and a saved `compact`
reads as `normal`.

## 9. Rollout

1. warpui-kit-core: palette, type ladder, fonts, shell and motion. Done.
2. warpui-kit-diff, -markdown, -picker: read the new tokens.
3. warp-station: list and detail panes, remove the inspector and the sheet. Done.
4. warp-notes, then warp-agent, smallest change first. warp-snapit only takes the
   overlay colours.
