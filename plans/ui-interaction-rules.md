# Motion, text surfaces and focus-scoped list navigation

Status: the rules landed 2026-11-11; implementation has not started. The
decisions below still need settling. Delete this file when the last step lands.

Goal: make three family rules real in Station, Agent and Notes — a running or
loading state shows at the object row, in the surface's title and in the window
title bar/OS title; content text is a markdown surface with a working context
menu while labels stay plain; keyboard navigation follows focus, with one
cursor per list.

The rules themselves are in
[`docs/agents/design-system.md`](../docs/agents/design-system.md) (`## Motion`
› *Where it shows*, `## Text surfaces`, `## Keyboard navigation`) and in the
three bullets under [`AGENTS.md`](../AGENTS.md) › *GPUI Kit app conventions*.
This file is the order of work and the checks; it does not restate the rules.

## Decisions to settle

1. **OS title format.** Recommended: activity first, `2 running — Warp Agent`,
   so a truncated taskbar label still shows it; the idle title is plain
   `Warp Agent`. The same string feeds the titlebar chip's aria label and
   tooltip. Say if you want the app name first instead.
2. **Station's cursor model.** Recommended: a click opens the repo immediately
   as it does today, arrows move a cursor, `Enter` opens. This is notes' model
   and it keeps a slow network repo from opening on every keypress. The
   alternative — arrows move the selection itself — changes the list's feel.
3. **Which station lists get a cursor.** Repo list, Changes file list, History
   commit list, PR list and Issues list. Branch and model pickers already
   navigate their own popover; the rail, settings rows and the About list stay
   out.
4. **Agent transcript arrows.** Recommended: scroll (line, page, top, bottom),
   not a row cursor. The transcript has no selectable row, and a cursor would
   fight markdown text selection.
5. **Notes activity.** Notes has no long-running background job today, so it
   gets no titlebar chip until one exists; its OS title keeps the note name and
   gains no fake count. When the store walk grows a real job, it enters the same
   `activity()` shape Station uses.
6. **Comment and tool payloads.** A volatile list of small comments and a
   streaming tool payload stay on `TextView` (one `LiveEditor` per item is the
   wrong cost) but must keep `text_menu`. Document-shaped content — a chat
   message, reasoning, a question, a note — moves to the kit viewer.

## Landed

- `docs/agents/design-system.md`: `### Where it shows` under `## Motion`,
  `## Text surfaces`, `## Keyboard navigation`, plus two table rows.
- `AGENTS.md`: three bullets under *GPUI Kit app conventions* pointing at the
  sections above.
- Check: the three headings and the three bullets are present, and the links
  use the section anchors (`#motion`, `#text-surfaces`,
  `#keyboard-navigation`).

## Steps

1. **Shared key map.** Add `kits/warpui-kit-core/src/list_nav.rs`: classify a
   `KeyDownEvent` into a `ListKey` and step a cursor (`step`, `jump`, `page`),
   with clamp semantics and no focus or selection state; export it from
   `lib.rs`. Check: unit tests cover empty, first/last, page clamps and the
   `Left`/`Right`/`Shift+F10` mapping; `cargo test -p warpui-kit-core list_nav`
   is green.
2. **Station repo list.** Add a `FocusHandle` and `track_focus` to the list, a
   `repo_cursor: Option<String>`, focus on row/pane click, arrows via `list_nav`,
   `Enter` → `pick_repo`, `Shift+F10` → the row menu, and scroll the cursor's
   row into view through the list's `ScrollHandle`. The cursor carries the
   accent only while the list has focus; the open repo keeps its selection
   edge. Check: a new
   `apps/warp-station/crates/warp-station/tests/list_keys_probe.rs` sends
   `down` with the list focused and again with the filter `InputState` focused;
   only the first moves the cursor; filtering the cursor's repo out resets it to
   a projected row (the `ensure_row_selected` pattern). Check with
   `cargo test -p warp-station list_keys_probe`: green.
3. **Station detail lists.** Repeat step 2 for the Changes file list, History
   commit list, PR list and Issues list; each pane owns its handle and handler,
   and only the focused one moves. The list head shows which pane owns the keys
   (accent rule on the head, or the cursor edge). Check: extend
   `list_keys_probe` to two panes; moving the focused one leaves the other
   cursor untouched; green.
4. **Agent sidebar and transcript.** The sidebar `uniform_list` gets a cursor
   over the flattened rows (workspaces, chats), focus and click, `Enter` opens,
   and `UniformListScrollHandle::scroll_to_item` keeps the cursor in view. The
   transcript takes focus on click and handles Up/Down/Page/Home/End as scroll.
   Check: a new `apps/warp-agent/tests/sidebar_keys_probe.rs` moves the sidebar
   cursor only while the sidebar is focused, and the transcript's scroll offset
   moves while the sidebar cursor stays put. Check with
   `cargo test -p warp-agent sidebar_keys_probe`: green.
5. **Notes reference alignment.** The existing tree is the reference key map,
   but its cursor can walk off-screen and it keeps the focus emphasis while the
   editor has focus. Scroll the cursor with
   `tree_scroll.scroll_to_item(index, ScrollStrategy::Nearest)` and dim the
   cursor row when `tree_focus` is not focused, mirroring the composer's
   mirrored-focus-flag pattern. Check: extend `apps/warp-notes/tests/notes_probe.rs`
   to move the cursor past the viewport and assert the deferred scroll target,
   and to assert the row loses its emphasis when focus moves to the editor;
   green.
6. **Motion gaps.** Add the missing indicators found by the sweep: a running
   tool's title line (spinner, status word, aria/tooltip), a list head's
   `Loading…` where only rows spin today, and the tile head's word for every
   async state. Every `motion::spinner` call gets a neighbouring word and an
   accessible name. Check: the source sweep finds no spinner without a word; a
   hidden capture shows the running tool row and the loading list head; with
   reduced motion on, the words remain and only the turns stop.
7. **Titles.** Add one `activity_title(app, activity)` formatter in the kit and
   feed it from Station's `Store::activity`, Agent's running count and Notes'
   `activity()` once it has work. Station and Agent update
   `window.set_window_title` only when the count crosses zero or changes, while
   the in-app chip keeps the spinner; Notes keeps its note name until it has
   activity. Check: unit tests cover idle, one job and many; a hidden launch
   reads the idle title through `hyprctl clients -j`, and the same reading shows
   the activity form once a run is live; each app that has activity exposes an
   `activity-chip` selector.
8. **Text surfaces.** Move the document-shaped agent surfaces (user message,
   reasoning, question, note) from `text_view` to the kit read-only viewer with
   per-item editors; make tool titles/statuses and section labels plain text;
   keep the bubble's Edit/Reprompt/Fork actions and add Copy/Select all for a
   selection. Station: verify README/summary and PR comments keep their menu,
   and move any remaining plain content surface. Check: a new
   `apps/warp-agent/tests/text_surfaces_probe.rs` asserts a Copy/Select-all menu
   on a message, a reasoning block and a tool detail, and no markdown node on a
   tool title; `cargo test -p warp-agent` and the station readme/diff probes are
   green.
9. **Docs, gates, cleanup.** Add one implementation line per app to its
   `AGENTS.md` pointing at the new sections, run the full gate, then delete this
   plan (or fold anything still useful into `docs/agents/design-system.md`).

## Out of scope

- Settings pages, the rail, About, and the branch/model pickers.
- A general tab order or focus ring on every control; only list cursors and
  click-to-focus are in scope.
- Pointer hover, drag and selection behavior outside the context-menu rule.
- Android.

## Verification

```sh
cargo fmt --all --check
cargo test -p warpui-kit-core -p warp-station -p warp-agent -p warp-notes
cargo clippy --locked --all-targets \
  -p warpui-kit-core -p warp-station -p warp-agent -p warp-notes -- -D warnings
```

Captures follow [`docs/agents/testing-and-captures.md`](../docs/agents/testing-and-captures.md);
do not launch these apps on the owner's screen for a capture.
