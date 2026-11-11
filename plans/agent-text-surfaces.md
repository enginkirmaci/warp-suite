# Streaming text surfaces: move the TextView guard to the kit viewer

Status: proposal — not started. The rule and the named exceptions are recorded
in `docs/agents/design-system.md#text-surfaces`; `apps/warp-agent/src/ui/markdown.rs`
names each surface and the interaction it is waiting on. This plan is the
migration path for the three content surfaces that still violate the rule:
the user message, reasoning and the question. Tool details stay on `TextView`
under the sanctioned wrong-cost exception; the system note needs a design
choice before it can move at all.

## Decisions to settle first

1. **User-message menu ownership.** The bubble's right-click is the item menu
   (Copy, Edit in input, Re-prompt, Fork). A read-only `LiveEditor` always
   installs its own Copy/Select-all menu and stops propagation. Either the
   kit grows a host-menu hook that appends those rows, or the item menu stays
   outside the editor and the editor's menu is suppressed per surface. Pick
   the hook; suppressing one menu makes two right-click behaviours in one
   transcript.
2. **Reasoning stream semantics.** Reasoning text is replaced wholesale per
   part update. Define whether `apply_streamed` is append-only (the common
   case) or handles arbitrary replacement by mapping the selection through
   the unchanged prefix and suffix. Selection and scroll must survive a
   chunk; otherwise the current guard is the better renderer.
3. **Question focus.** A question card owns option rows and a custom-answer
   `InputState`. Decide whether the question text gets one editor per entry
   or a single editor for the whole card, and who owns the menu while the
   answer field has focus. This is a focus contract, not a renderer swap.
4. **System note role.** It is an app-authored status string, but error notes
   are worth copying. Choose a plain status line, or a status line with an
   explicit `Copy error` action; do not give notes a full editor by accident.

## Steps

1. **Kit: selection-preserving stream feed.** Add
   `LiveEditorState::apply_streamed(text)` that finds the common prefix and
   suffix, leaves an append untouched, maps a selection or caret through the
   unchanged text otherwise, preserves scroll, then reparses and notifies.
   Test in `kits/warpui-kit-markdown/tests/live_editor_probe.rs`: select a
   phrase, append a chunk, assert the range and selected text are unchanged;
   replace text after the selection, assert the selection survives; replace
   text before it and assert the mapped caret stays on the same word.
2. **Kit: host-menu hook.** Add a read-only `LiveEditor` builder method that
   appends host rows after Copy/Select all while keeping
   `stop_propagation()`. Test one menu with the selection Copy plus two host
   actions; clicking a host action must reach its handler once.
3. **Agent: reasoning.** Move reasoning to the existing `md_editor` /
   `render_markdown` path (the item id is already the cache key) and feed it
   through `apply_streamed`. Delete the `text-reasoning` guard call. Extend
   `tests/text_surfaces_probe.rs` so the reasoning fold is opened, a phrase is
   selected, a chunk is applied, and the selection plus the Copy menu still
   work.
4. **Agent: user message.** After step 2, render the user message through a
   read-only editor whose menu carries Copy / Select all plus the bubble's
   Edit in input / Re-prompt / Fork rows. Delete the bubble's
   `on_mouse_down` menu and the old tile-menu path for the message. Update
   `text_surfaces_probe` to assert exactly one menu with all four actions and
   that Copy writes the selection.
5. **Agent: question.** After steps 1–2, cache one editor per entry (keyed
   `{item}#{qi}`) and one for the resolution, render them inside the card with
   the card's top margin, and keep the custom-answer `InputState`. First add a
   probe that types into the field while a selection exists in the question
   text; the answer must reach the store and the field must keep focus.
6. **System note.** Apply decision 4. If it becomes a plain status line,
   remove its `text_view` call and add a real-view assertion that an error
   note exposes the chosen copy affordance. If it keeps copy, state that
   exception in the docs with its probe.
7. **Finish.** Remove every migrated surface from `text_view`; update the
   module docs, `docs/agents/design-system.md#text-surfaces` and
   `apps/warp-agent/AGENTS.md`; delete this plan once the work lands.

## Done when

- `cargo test -p warpui-kit-markdown -p warp-agent` and the touched-crate
  clippy gate pass.
- `tests/text_surfaces_probe.rs` proves for every migrated surface: one menu,
  Copy writes the selection, and the item actions still work.
- A document that trips the markdown parser can no longer reach `TextView`
  from a content surface; the only remaining guard callers are the sanctioned
  tool details and, until step 6 lands, the system note.
- The docs list any remaining exception with its constraint and a probe.

Out of scope: the assistant-answer path (already on `LiveEditor`), JSON tool
details (`warpui_kit_diff::render_code`), and any change to the parser guard's
`catch_unwind` behaviour.
