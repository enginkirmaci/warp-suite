# warpui-kit-markdown

Obsidian-style live markdown editing for [gpui-kit](https://crates.io/crates/gpui-kit) apps — the stack behind the notes editor.

Two ways in, sharing one parser:

- **`LiveEditor`**, a full editor that works like Obsidian's live preview: headings at their real size, source syntax hidden until the caret touches it, drawn bullets, checkboxes, quote bars, callouts, tables, code blocks, images and Mermaid diagrams, a Properties panel for frontmatter, with Obsidian's editing keys. Its layout metrics, reveal rules and key behavior were measured against Obsidian itself; its colors are the app's.
- **`live_highlighter_factory`**, a highlighter for a kit code editor when a fixed-grid editor is what you want: source markers fade off the caret's line.

## What's inside

| Module | Contents |
| --- | --- |
| `editor` | The live editor: `LiveEditorState` (document, selection, undo, commands, find/replace) and the `LiveEditor` element |
| `live` | Pure parser: `parse_document` for the line and block model, `parse` for a flat span list, `toggle_task_at` — a deliberately line-based CommonMark subset plus the Obsidian flavor (wikilinks, tags, highlights, callouts, footnotes, frontmatter); splits only at ASCII syntax bytes, so byte ranges never land inside a multi-byte character |
| `highlighter` | The `InputHighlighter` that maps spans onto editor text styles with caret-line marker fading; install via `live_highlighter_factory(caret)` |
| `apply` | The toolbar's pure string ops: `markdown_apply` (bold/italic/strike/code/codeblock/link/h1–h3/bullet/task/numbered/quote/hr) and `continue_list` |

## The live editor

```rust
use warpui_kit_markdown::{LiveEditor, LiveEditorEvent, LiveEditorState};

// in the view's constructor
let editor = cx.new(|cx| LiveEditorState::new(window, cx).placeholder("# Title"));
cx.subscribe_in(&editor, window, |_, _, event: &LiveEditorEvent, _, cx| match event {
    LiveEditorEvent::OpenUrl(url) => cx.open_url(url),
    LiveEditorEvent::OpenNote(target) => { /* resolve a [[wikilink]] */ }
    LiveEditorEvent::Change => {}
})
.detach();

// opening a document
editor.update(cx, |state, cx| {
    state.set_base_dir(note_folder, cx); // relative images resolve here
    state.set_value(text, window, cx);
    state.set_note_names(note_names, cx); // optional: marks unresolved wikilinks
});

// in render: fills its parent
LiveEditor::new(&editor)
    .aria_label("Note body")
    .text_size(px(16.)) // every metric scales from this
    .surface(palette::CARD) // what you paint behind it, if not the theme's background
    .on_paste(|item, window, cx| handled_as_image(item, window, cx))
```

**Colors.** Nothing is hardcoded to Obsidian. Text, accent, link, selection and caret come from the kit theme, which `warpui_kit_core::apply_kit_theme` fills from the app's accent preset, so a preset swap recolors the editor on the next frame. The fixed roles are palette tokens: `TEXT_FAINT` syntax, `DIFF_BG` code wells, `CHIP` inline code, the semantic hues for callouts.

**Keys.** The editor's key context is `Input LiveEditor`, so movement, selection and clipboard keys are the kit's own bindings for each platform. The markdown ones (`secondary-b/i/k/l/d`, `secondary-shift-z`) bind themselves once per app in the `LiveEditor` context. Tab indents, Enter continues lists, typing `*`, `(`, `[` and friends auto-pairs or wraps the selection, Ctrl+F finds, Ctrl+H replaces.

**Bold.** gpui's Linux text system only chooses among a family's static faces, so a family that is one variable font file (Adwaita Sans, for one) renders every weight as regular. `Fonts::new` detects that by resolving a bold face and comparing ids, and the painter then emboldens the text itself; a family with real faces is used as is.

**Code blocks** highlight through `warpui-kit-diff`; enable the `tree-sitter-<lang>` features for the languages you want, anything else stays plain.

**Properties.** Frontmatter is a panel, never YAML, as in Obsidian: a row per property with its type's icon, keys and values edited in place, pills for tags, a box for `true` and `false`, a cross on every list item, and **Add property** at the bottom. The caret opens past the panel; arrows walk its fields, Tab goes key, value, next key, and past the last row starts a new one. A row added or emptied here goes away when the caret leaves it blank, while blank rows a note came with stay. Right-click a row to remove it. YAML the panel cannot read stays visible as a plain row.

**Images** render wherever markdown puts them. Alone on a line one takes the full width it needs; inside a sentence it flows with the text and grows its row, the text staying on the row's bottom like Obsidian's. A file that cannot be loaded leaves its source.

**Tables** edit in place, and hovering one shows strips under and beside it that add a row or a column, writing the table back aligned the way Obsidian does.

**Diagrams.** With the `mermaid` feature, ` ```mermaid ` blocks render in the app's colors, through Merman and resvg off the UI thread. A click on a diagram does nothing; arrow keys or its `</>` button open the source, and a block that does not parse stays code with the error under it. The feature leaves out Merman's ELK layout (EPL-2.0) and math labels. Without it they are ordinary code blocks and none of that is compiled.

**Known gaps.** No math or HTML rendering. While the caret is on an image its source shows beside the image, where Obsidian keeps the image only and edits blind. Dates show as their ISO text rather than Obsidian's date field, and a property's type follows from its key and value, with no menu to set one.

## The highlighter

```rust
use warpui_kit_markdown::{live_highlighter_factory, LIVE_LANGUAGE};
use std::{cell::RefCell, rc::Rc};

let caret: Rc<RefCell<Option<std::ops::Range<usize>>>> = Rc::new(RefCell::new(None));
state.set_highlighter_factory(live_highlighter_factory(caret), cx);
// the editor's language must be set to LIVE_LANGUAGE for the factory to engage
```

Task boxes render live; clicks toggle through `toggle_task_at(&text, byte)`.

## License

MIT
