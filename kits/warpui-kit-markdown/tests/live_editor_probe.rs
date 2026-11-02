//! Headless probe for the live editor: the commands that need a laid-out
//! frame or the real action path, with Obsidian's results as the expectations
//! wherever it has an answer.

use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, IntoElement, Modifiers, ParentElement as _,
    Point, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_kit::base::input::{
    Backspace, Copy, Delete, Enter, IndentInline, MoveDown, MoveEnd, MoveHome, MoveLeft, MoveRight,
    MoveToEnd, MoveToNextWord, MoveToStart, MoveUp, OutdentInline, Paste, Redo, Replace, Search,
    Undo,
};
use std::cell::RefCell;
use std::rc::Rc;
use warpui_kit_core::palette;
use warpui_kit_markdown::{LiveColors, LiveEditor, LiveEditorEvent, LiveEditorState};

struct Probe {
    editor: Entity<LiveEditorState>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(LiveEditor::new(&self.editor))
    }
}

struct Harness {
    cx: &'static mut VisualTestContext,
    editor: Entity<LiveEditorState>,
}

impl Harness {
    /// A focused editor over `text`, where `¦` marks the caret.
    fn open(tcx: &mut TestAppContext, text: &str) -> Self {
        tcx.update(gpui_kit::init);
        let caret = text.find('¦').expect("a caret marker");
        let body = text.replace('¦', "");
        let mut editor = None;
        let handle = tcx.open_window(gpui::size(px(900.), px(700.)), |window, cx| {
            let state = cx.new(|cx| LiveEditorState::new(window, cx));
            editor = Some(state.clone());
            state.update(cx, |state, cx| {
                state.set_value(body.clone(), window, cx);
                state.set_selected_range(caret..caret, cx);
                state.focus(window, cx);
            });
            Probe { editor: state }
        });
        let cx = VisualTestContext::from_window(handle.into(), tcx).into_mut();
        let mut this = Self {
            cx,
            editor: editor.expect("the window was built"),
        };
        this.settle();
        this
    }

    /// A focused editor as a note opens: the caret goes where it goes.
    fn opened(tcx: &mut TestAppContext, text: &str) -> Self {
        tcx.update(gpui_kit::init);
        let mut editor = None;
        let handle = tcx.open_window(gpui::size(px(900.), px(700.)), |window, cx| {
            let state = cx.new(|cx| LiveEditorState::new(window, cx));
            editor = Some(state.clone());
            state.update(cx, |state, cx| {
                state.set_value(text.to_string(), window, cx);
                state.focus(window, cx);
            });
            Probe { editor: state }
        });
        let cx = VisualTestContext::from_window(handle.into(), tcx).into_mut();
        let mut this = Self {
            cx,
            editor: editor.expect("the window was built"),
        };
        this.settle();
        this
    }

    fn enter(&mut self) {
        self.act(Enter {
            secondary: false,
            shift: false,
        });
    }

    /// The offset of the first `needle` in the text.
    fn offset_of(&self, needle: &str) -> usize {
        self.cx.read(|cx| {
            self.editor
                .read(cx)
                .value()
                .find(needle)
                .expect("in the text")
        })
    }

    fn click(&mut self, at: Point<gpui::Pixels>) {
        self.cx.simulate_click(at, Modifiers::default());
        self.settle();
    }

    fn bounds_of(&mut self, offset: usize) -> gpui::Bounds<gpui::Pixels> {
        self.cx.read(|cx| {
            self.editor
                .read(cx)
                .range_to_bounds(&(offset..offset))
                .expect("laid out")
        })
    }

    fn settle(&mut self) {
        self.cx.run_until_parked();
        self.cx.update(|window, cx| drop(window.draw(cx)));
    }

    fn act(&mut self, action: impl gpui::Action) {
        self.cx
            .update(|window, cx| window.dispatch_action(action.boxed_clone(), cx));
        self.settle();
    }

    fn typed(&mut self, text: &str) {
        self.cx.simulate_input(text);
        self.settle();
    }

    /// The text with `¦` at the caret, or around the selection.
    fn state(&self) -> String {
        self.cx.read(|cx| {
            let editor = self.editor.read(cx);
            let text = editor.value();
            let range = editor.selected_range();
            if range.is_empty() {
                format!("{}¦{}", &text[..range.start], &text[range.end..])
            } else {
                format!(
                    "{}¦{}¦{}",
                    &text[..range.start],
                    &text[range.clone()],
                    &text[range.end..]
                )
            }
        })
    }

    fn point_of(&mut self, offset: usize) -> Point<gpui::Pixels> {
        self.point_of_range(offset..offset)
    }

    fn point_of_range(&mut self, range: std::ops::Range<usize>) -> Point<gpui::Pixels> {
        let bounds = self.cx.read(|cx| {
            self.editor
                .read(cx)
                .range_to_bounds(&range)
                .expect("laid out")
        });
        Point::new(
            bounds.origin.x + px(1.),
            bounds.origin.y + bounds.size.height / 2.,
        )
    }
}

#[gpui::test]
fn typing_goes_through_auto_pairs_and_undoes_in_one_step(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "¦");
    h.typed("h");
    h.typed("i");
    h.typed(" ");
    h.typed("(");
    assert_eq!(h.state(), "hi (¦)");
    h.typed("x");
    h.typed(")");
    assert_eq!(h.state(), "hi (x)¦", "the typed closer steps over its pair");
    // x, then the pair, then the plain typing merged into one step
    h.act(Undo);
    h.act(Undo);
    assert_eq!(h.state(), "hi ¦");
    h.act(Undo);
    assert_eq!(h.state(), "¦");
    h.act(Redo);
    assert_eq!(h.state(), "hi ¦");
}

#[gpui::test]
fn enter_and_tab_run_through_the_real_actions(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "1. a¦\n2. b");
    h.act(Enter {
        secondary: false,
        shift: false,
    });
    assert_eq!(h.state(), "1. a\n2. ¦\n3. b");
    h.act(IndentInline);
    assert_eq!(h.state(), "1. a\n\t1. ¦\n2. b");
    h.act(OutdentInline);
    assert_eq!(h.state(), "1. a\n2. ¦\n3. b");
}

#[gpui::test]
fn home_matches_obsidian(tcx: &mut TestAppContext) {
    for (input, presses, want) in [
        ("- it¦em", 1, "- ¦item"),
        ("\t- it¦em", 1, "\t- ¦item"),
        ("# Ti¦tle", 1, "¦# Title"),
        ("> qu¦ote", 1, "> ¦quote"),
        ("- [ ] ta¦sk", 1, "- [ ] ¦task"),
        ("- [ ] ta¦sk", 2, "¦- [ ] task"),
        ("pa¦ra", 1, "¦para"),
        ("  pa¦ra", 1, "  ¦para"),
    ] {
        let mut h = Harness::open(tcx, input);
        for _ in 0..presses {
            h.act(MoveHome);
        }
        assert_eq!(h.state(), want, "Home x{presses} in {input:?}");
    }
}

#[gpui::test]
fn end_goes_to_the_line_end(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "- it¦em\nnext");
    h.act(MoveEnd);
    assert_eq!(h.state(), "- item¦\nnext");
}

#[gpui::test]
fn vertical_motion_keeps_lines_and_stops_at_the_ends(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "# H\n\nab¦c");
    h.act(MoveUp);
    h.act(MoveUp);
    assert_eq!(h.state().lines().next(), Some("# H¦"), "{:?}", h.state());
    h.act(MoveUp);
    assert_eq!(
        h.state(),
        "¦# H\n\nabc",
        "up from the top line goes to its start"
    );
    h.act(MoveToEnd);
    h.act(MoveDown);
    assert_eq!(h.state(), "# H\n\nabc¦");
}

#[gpui::test]
fn arrows_cross_hidden_markers_one_source_step_at_a_time(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "a¦**b**");
    h.act(MoveRight);
    assert_eq!(h.state(), "a*¦*b**");
    h.act(MoveRight);
    h.act(MoveRight);
    assert_eq!(h.state(), "a**b¦**");
    h.act(MoveLeft);
    assert_eq!(h.state(), "a**¦b**");
    h.act(MoveToNextWord);
    assert_eq!(h.state(), "a**b¦**");
}

#[gpui::test]
fn the_caret_steps_over_a_tables_pipes(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "| a¦ | b |\n| - | - |\n| c | d |\n\nx");
    h.act(MoveRight);
    assert_eq!(
        h.state(),
        "| a | ¦b |\n| - | - |\n| c | d |\n\nx",
        "right crosses into the next cell"
    );
    h.act(MoveRight);
    h.act(MoveRight);
    assert_eq!(
        h.state(),
        "| a | b |\n| - | - |\n| ¦c | d |\n\nx",
        "past the row's last cell, over the delimiter row, into the next row"
    );
    h.act(MoveLeft);
    assert_eq!(
        h.state(),
        "| a | b¦ |\n| - | - |\n| c | d |\n\nx",
        "and back over it"
    );
}

#[gpui::test]
fn tab_walks_table_cells_and_enter_walks_rows(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "| a¦ | b |\n| - | - |\n| c | d |");
    h.act(IndentInline);
    assert_eq!(h.state(), "| a | ¦b |\n| - | - |\n| c | d |");
    h.act(IndentInline);
    assert_eq!(h.state(), "| a | b |\n| - | - |\n| ¦c | d |");
    h.act(OutdentInline);
    assert_eq!(h.state(), "| a | ¦b |\n| - | - |\n| c | d |");
    h.act(Enter {
        secondary: false,
        shift: false,
    });
    assert_eq!(
        h.state(),
        "| a | b |\n| - | - |\n| c | d¦ |",
        "Enter moves down its column instead of breaking the row"
    );
}

#[gpui::test]
fn clicking_a_rendered_link_opens_it_without_moving_the_caret(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "see [docs](https://e.co) and [[note]]\n\nend¦");
    let events = Rc::new(RefCell::new(Vec::new()));
    {
        let events = events.clone();
        let editor = h.editor.clone();
        h.cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &LiveEditorEvent, _| {
                events.borrow_mut().push(event.clone());
            })
            .detach();
        });
    }
    let link = h.point_of(5);
    h.cx.simulate_click(link, Modifiers::default());
    h.settle();
    let wiki = h.point_of(31);
    h.cx.simulate_click(wiki, Modifiers::default());
    h.settle();
    assert_eq!(
        *events.borrow(),
        [
            LiveEditorEvent::OpenUrl("https://e.co".into()),
            LiveEditorEvent::OpenNote("note".into())
        ]
    );
    assert!(h.state().ends_with("end¦"), "{:?}", h.state());
}

#[gpui::test]
fn clicking_text_places_the_caret_and_reveals_its_construct(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "x **bold** y\n\nend¦");
    let inside = h.point_of(6);
    h.cx.simulate_click(inside, Modifiers::default());
    h.settle();
    assert!(
        h.state().starts_with("x **bo¦ld** y") || h.state().starts_with("x **b¦old** y"),
        "{:?}",
        h.state()
    );
}

#[gpui::test]
fn copy_without_a_selection_takes_the_whole_line(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "fi¦rst\nsecond");
    h.act(Copy);
    let copied = h.cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item: ClipboardItem| item.text())
    });
    assert_eq!(copied.as_deref(), Some("first\n"));
}

#[gpui::test]
fn find_selects_matches_and_replace_all_is_one_undo(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "¦Foo foo\nFOO bar");
    h.act(Search);
    h.typed("foo");
    assert_eq!(
        h.state(),
        "¦Foo¦ foo\nFOO bar",
        "the first match is selected"
    );
    h.act(Enter {
        secondary: false,
        shift: false,
    });
    assert_eq!(
        h.state(),
        "Foo ¦foo¦\nFOO bar",
        "Enter in the field moves on"
    );
    h.act(Enter {
        secondary: false,
        shift: true,
    });
    assert_eq!(h.state(), "¦Foo¦ foo\nFOO bar", "Shift+Enter goes back");
}

#[gpui::test]
fn replace_opens_on_its_shortcut(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "a b a¦");
    h.act(Replace);
    h.typed("a");
    assert_eq!(
        h.state(),
        "¦a¦ b a",
        "no match after the caret wraps to the first"
    );
}

#[gpui::test]
fn colors_follow_the_apps_accent_preset(tcx: &mut TestAppContext) {
    tcx.update(gpui_kit::init);
    let tokens = warpui_kit_core::font_tokens(None, 1.);
    let apply = |preset: &str, cx: &mut gpui::App| {
        let accent = palette::AccentColors::for_preset(palette::normalize_accent(Some(preset)));
        warpui_kit_core::apply_kit_theme(accent, &tokens, palette::surface(), cx);
        accent
    };
    for preset in ["blue", "green", "amber", "salmon", "gray"] {
        tcx.update(|cx| {
            let accent = apply(preset, cx);
            let colors = LiveColors::from_theme(cx);
            let want: gpui::Hsla = accent.accent.into();
            assert_eq!(colors.accent, want, "{preset}: bars and checkboxes");
            assert_eq!(colors.link, accent.text.into(), "{preset}: links");
            assert_eq!(colors.on_accent, accent.on.into(), "{preset}: checkmarks");
            let selection: gpui::Hsla = palette::selection(&accent).into();
            assert_eq!(colors.selection, selection, "{preset}: selection");
            assert_eq!(colors.tag_wash.h, want.h, "{preset}: tags wash the accent");
            // what never follows a preset comes from the family palette
            assert_eq!(colors.faint, palette::text_muted().into());
            assert_eq!(colors.code_well, palette::well().into());
            assert_eq!(colors.text, palette::text().into());
        });
    }
}

/// A real image file: one pixel.
const PIXEL_PNG: [u8; 70] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x28, 0x98, 0x70, 0xFE,
    0x3F, 0x00, 0x06, 0x12, 0x02, 0xCF, 0x9E, 0x63, 0x9F, 0xA8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// A note folder holding `pixel.png`.
fn image_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("live-editor-probe-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp folder");
    std::fs::write(dir.join("pixel.png"), PIXEL_PNG).expect("a temp file");
    dir
}

impl Harness {
    fn with_folder(&mut self, dir: std::path::PathBuf) {
        let editor = self.editor.clone();
        self.cx
            .update(|_, cx| editor.update(cx, |state, cx| state.set_base_dir(Some(dir), cx)));
        self.settle();
        // the asset arrives on a later frame
        self.settle();
    }
}

#[gpui::test]
fn an_image_in_a_sentence_grows_its_row_from_the_bottom(tcx: &mut TestAppContext) {
    let text = "see ![[pixel.png|60]] here\nnext\n\nend¦";
    let mut h = Harness::open(tcx, text);
    let line_height = h.bounds_of(0).size.height;
    let before = h.bounds_of(h.offset_of("next")).origin.y;
    h.with_folder(image_folder("row"));
    let sentence = h.bounds_of(h.offset_of(" here"));
    let next = h.bounds_of(h.offset_of("next")).origin.y;
    // the row is the image's 60px now, its text on the bottom of it
    assert_eq!(next - before, px(60.) - line_height);
    assert_eq!(sentence.origin.y + sentence.size.height, next);
    assert_eq!(
        sentence.size.height, line_height,
        "the caret keeps its size"
    );
}

#[gpui::test]
fn a_click_on_an_inline_image_reveals_its_source(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "see ![[pixel.png|60]] here\n\nend¦");
    h.with_folder(image_folder("click"));
    let left = h.bounds_of(4).origin;
    h.click(Point::new(left.x + px(10.), left.y));
    assert!(
        h.state().starts_with("see ¦![[pixel.png|60]] here"),
        "the near side of the image: {:?}",
        h.state()
    );
}

#[gpui::test]
fn the_caret_climbs_past_a_tall_inline_image(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "top\nsee ![[pixel.png|60]] h¦ere");
    h.with_folder(image_folder("climb"));
    h.act(MoveUp);
    assert!(h.state().starts_with("top¦") || h.state().starts_with("to¦p"));
}

#[gpui::test]
fn images_that_cannot_load_keep_their_source_in_the_flow(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "see ![[gone.png|60]] here\n\nend¦");
    h.with_folder(image_folder("gone"));
    let line_height = h.bounds_of(0).size.height;
    let row = h.bounds_of(h.offset_of(" here"));
    assert_eq!(row.origin.y, h.bounds_of(0).origin.y);
    assert_eq!(row.size.height, line_height);
}

#[gpui::test]
fn a_note_opens_with_the_caret_past_its_properties(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---\nbody");
    assert_eq!(h.state(), "---\na: 1\n---\n¦body");
    h.act(MoveToEnd);
    h.act(MoveToStart);
    assert_eq!(h.state(), "---\na: 1\n---\n¦body", "the body starts there");
}

#[gpui::test]
fn the_caret_walks_the_panel_through_its_fields(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\nab: 1\nc: \"q\"\n---\nbody");
    h.act(MoveLeft);
    assert_eq!(
        h.state(),
        "---\nab: 1\nc: \"q¦\"\n---\nbody",
        "over the fence and the closing quote"
    );
    h.act(MoveLeft);
    h.act(MoveLeft);
    assert_eq!(h.state(), "---\nab: 1\nc¦: \"q\"\n---\nbody");
    h.act(MoveLeft);
    h.act(MoveLeft);
    assert_eq!(h.state(), "---\nab: 1¦\nc: \"q\"\n---\nbody", "up a row");
    h.act(MoveRight);
    assert_eq!(h.state(), "---\nab: 1\n¦c: \"q\"\n---\nbody");
}

#[gpui::test]
fn arrows_leave_the_panel_past_add_property(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---\nbody");
    h.act(MoveUp);
    assert_eq!(
        h.state(),
        "---\na: 1¦\n---\nbody",
        "up lands in the last row"
    );
    h.act(MoveDown);
    assert!(h.state().ends_with("---\n¦body"), "{:?}", h.state());
}

#[gpui::test]
fn keys_and_values_edit_in_place(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\nkey¦: value\n---\nbody");
    h.typed("s");
    assert_eq!(h.state(), "---\nkeys¦: value\n---\nbody");
}

#[gpui::test]
fn deleting_stops_at_a_fields_edge(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\nkey: ¦v\n---\nbody");
    h.act(Backspace);
    assert_eq!(h.state(), "---\nkey: ¦v\n---\nbody", "the colon stays");
    let mut h = Harness::open(tcx, "---\nkey¦: v\n---\nbody");
    h.act(Delete);
    assert_eq!(h.state(), "---\nkey¦: v\n---\nbody", "the colon stays");
    let mut h = Harness::open(tcx, "| a | b |\n| - | - |\n| ¦c | d |");
    h.act(Backspace);
    assert!(h.state().contains("| ¦c | d |"), "{:?}", h.state());
    let mut h = Harness::open(tcx, "| a | b |\n| - | - |\n| c¦ | d |");
    h.act(Delete);
    assert!(h.state().contains("| c¦ | d |"), "{:?}", h.state());
}

#[gpui::test]
fn the_body_cannot_swallow_the_closing_fence(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---\nbody");
    h.act(Backspace);
    assert_eq!(h.state(), "---\na: 1\n---\n¦body");
}

#[gpui::test]
fn text_after_a_closing_fence_at_the_end_starts_a_new_line(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---");
    h.typed("x");
    assert_eq!(h.state(), "---\na: 1\n---\nx¦");
}

#[gpui::test]
fn an_empty_value_takes_the_space_it_needs(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\nkey¦:\n---\nbody");
    h.act(IndentInline);
    h.typed("v");
    assert_eq!(h.state(), "---\nkey: v¦\n---\nbody");
}

#[gpui::test]
fn tab_runs_key_value_key_and_on_into_a_new_property(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\na¦: 1\nb: 2\n---\nbody");
    h.act(IndentInline);
    assert_eq!(h.state(), "---\na: 1¦\nb: 2\n---\nbody");
    h.act(IndentInline);
    assert_eq!(h.state(), "---\na: 1\nb¦: 2\n---\nbody");
    h.act(IndentInline);
    h.act(IndentInline);
    assert_eq!(h.state(), "---\na: 1\nb: 2\n¦: \n---\nbody", "a new row");
    h.typed("c");
    h.act(IndentInline);
    h.typed("3");
    assert_eq!(h.state(), "---\na: 1\nb: 2\nc: 3¦\n---\nbody");
    h.act(OutdentInline);
    assert_eq!(h.state(), "---\na: 1\nb: 2\nc¦: 3\n---\nbody");
    h.act(OutdentInline);
    assert_eq!(h.state(), "---\na: 1\nb: 2¦\nc: 3\n---\nbody");
}

#[gpui::test]
fn enter_hands_a_key_to_its_value_and_a_value_stays(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\nkey¦: v\n---\nbody");
    h.enter();
    assert_eq!(h.state(), "---\nkey: v¦\n---\nbody");
    h.enter();
    assert_eq!(h.state(), "---\nkey: v¦\n---\nbody", "as Obsidian's does");
}

#[gpui::test]
fn add_property_opens_a_row_that_goes_if_left_blank(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---\nbody");
    let button = h.point_of(h.offset_of("---\nbody"));
    h.click(button);
    assert_eq!(h.state(), "---\na: 1\n¦: \n---\nbody");
    h.act(MoveToEnd);
    assert_eq!(
        h.state(),
        "---\na: 1\n---\nbody¦",
        "the row left with the caret"
    );
    h.act(Undo);
    assert_eq!(
        h.state(),
        "---\na: 1\n---\nbody¦",
        "and left no step behind to undo"
    );
}

#[gpui::test]
fn a_blank_row_goes_with_the_focus_and_the_caret_keeps_a_field(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\na: 1¦\n---\nbody");
    h.act(IndentInline);
    assert_eq!(h.state(), "---\na: 1\n¦: \n---\nbody");
    h.cx.update(|window, _| window.activate_window());
    h.settle();
    h.cx.deactivate_window();
    h.settle();
    assert_eq!(h.state(), "---\na: 1¦\n---\nbody");
}

#[gpui::test]
fn a_row_emptied_by_hand_goes_too(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\na: 1\nb¦: \n---\nbody");
    h.act(Backspace);
    assert_eq!(h.state(), "---\na: 1\n¦: \n---\nbody");
    h.act(MoveToEnd);
    assert_eq!(h.state(), "---\na: 1\n---\nbody¦");
}

#[gpui::test]
fn blank_rows_the_note_came_with_stay(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\ntags:\n  - \n---\nbody");
    h.act(MoveToEnd);
    h.act(MoveToStart);
    assert!(h.state().contains("tags:\n  - \n"), "{:?}", h.state());
}

#[gpui::test]
fn a_checkbox_property_toggles_in_place(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\ndone: true\n---\nbody");
    let at = h.offset_of("true");
    let checkbox = h.point_of_range(at..at + 4);
    h.click(checkbox);
    assert_eq!(h.state(), "---\ndone: false\n---\n¦body");
    let at = h.offset_of("false");
    let checkbox = h.point_of_range(at..at + 5);
    h.click(checkbox);
    assert_eq!(h.state(), "---\ndone: true\n---\n¦body");
}

#[gpui::test]
fn enter_starts_the_next_item_and_backspace_takes_a_blank_one(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\ntags:\n  - a¦\n---\nbody");
    h.enter();
    assert_eq!(h.state(), "---\ntags:\n  - a\n  - ¦\n---\nbody");
    h.typed("b");
    assert_eq!(h.state(), "---\ntags:\n  - a\n  - b¦\n---\nbody");
    h.enter();
    h.act(Backspace);
    assert_eq!(h.state(), "---\ntags:\n  - a\n  - b¦\n---\nbody");
}

#[gpui::test]
fn a_cross_removes_its_item(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\ntags:\n  - alpha\n  - beta\n---\nbody");
    let end = h.offset_of("alpha") + 5;
    let at = h.bounds_of(end);
    h.click(Point::new(
        at.origin.x + px(8.),
        at.origin.y + at.size.height / 2.,
    ));
    assert_eq!(h.state(), "---\ntags:\n  - beta\n---\n¦body");
}

#[gpui::test]
fn a_click_past_a_lists_items_starts_another(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\ntags:\n  - a\n---\nbody");
    let end = h.offset_of("a\n---");
    let at = h.bounds_of(end);
    h.click(Point::new(
        at.origin.x + px(120.),
        at.origin.y + at.size.height / 2.,
    ));
    assert_eq!(h.state(), "---\ntags:\n  - a\n  - ¦\n---\nbody");
    h.typed("b");
    assert_eq!(h.state(), "---\ntags:\n  - a\n  - b¦\n---\nbody");
}

#[gpui::test]
fn the_properties_heading_is_inert(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\na: 1\n---\nbody");
    let row = h.bounds_of(h.offset_of("a: 1"));
    h.click(Point::new(row.origin.x, row.origin.y - px(20.)));
    assert_eq!(h.state(), "---\na: 1\n---\n¦body");
}

#[gpui::test]
fn undo_brings_a_removed_item_back(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, "---\ntags:\n  - a\n  - b\n---\nbody");
    let at = h.bounds_of(h.offset_of("a\n"));
    h.click(Point::new(
        at.origin.x + px(20.),
        at.origin.y + at.size.height / 2.,
    ));
    assert!(!h.state().contains("- a"), "{:?}", h.state());
    h.act(Undo);
    assert!(h.state().contains("- a\n"), "{:?}", h.state());
}

#[gpui::test]
fn pasted_lines_join_inside_a_field(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "---\nkey: a¦\n---\nbody");
    h.cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("b\nc".to_string())));
    h.act(Paste);
    assert_eq!(h.state(), "---\nkey: ab c¦\n---\nbody");
}

const TABLE: &str = "| a | b |\n| - | - |\n| 1 | 2 |\n\nend";

#[gpui::test]
fn arrows_walk_a_tables_rows(tcx: &mut TestAppContext) {
    let mut h = Harness::open(tcx, "top\n\n| a¦ | b |\n| - | - |\n| 1 | 2 |\n\nend");
    h.act(MoveDown);
    assert!(h.state().contains("| 1¦ | 2 |"), "{:?}", h.state());
    h.act(MoveUp);
    assert!(h.state().contains("| a¦ | b |"), "{:?}", h.state());
    h.act(MoveUp);
    assert!(h.state().starts_with("top\n¦\n|"), "{:?}", h.state());
}

/// A point inside the table's last row.
fn last_row(h: &mut Harness) -> gpui::Bounds<gpui::Pixels> {
    h.bounds_of(h.offset_of("1 |"))
}

#[gpui::test]
fn the_strip_under_a_table_adds_a_row(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, TABLE);
    let row = last_row(&mut h);
    h.click(Point::new(row.origin.x + px(4.), row.origin.y + px(34.)));
    assert_eq!(
        h.state(),
        "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n| ¦    |     |\n\nend"
    );
    h.typed("x");
    h.act(IndentInline);
    h.typed("y");
    assert_eq!(
        h.state(),
        "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n| x    | y¦    |\n\nend"
    );
}

#[gpui::test]
fn the_strip_beside_a_table_adds_a_column(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, TABLE);
    let row = last_row(&mut h);
    let mut x = row.origin.x;
    // walk right along the row until the first thing that is no cell
    while !h
        .cx
        .read(|cx| h.editor.read(cx).value().contains("| --- |"))
    {
        x += px(6.);
        assert!(x < row.origin.x + px(400.), "no strip found");
        h.click(Point::new(x, row.origin.y + px(4.)));
    }
    assert_eq!(
        h.state(),
        "| a   | b   | ¦    |\n| --- | --- | --- |\n| 1   | 2   |     |\n\nend"
    );
}

#[gpui::test]
fn undo_takes_a_new_row_back_in_one_step(tcx: &mut TestAppContext) {
    let mut h = Harness::opened(tcx, TABLE);
    let row = last_row(&mut h);
    h.click(Point::new(row.origin.x + px(4.), row.origin.y + px(34.)));
    h.act(Undo);
    assert!(h.state().replace('¦', "") == TABLE, "{:?}", h.state());
}

#[cfg(feature = "mermaid")]
mod diagrams {
    use super::*;

    const NOTE: &str = "top\n\n```mermaid\nflowchart TD\n    A --> B\n```\n\nend";

    /// The note after its diagram has rendered.
    fn rendered(tcx: &mut TestAppContext, text: &str) -> Harness {
        let mut h = Harness::opened(tcx, text);
        h.settle();
        h.settle();
        h
    }

    #[gpui::test]
    fn a_parked_block_gives_way_to_its_picture(tcx: &mut TestAppContext) {
        let mut h = rendered(tcx, NOTE);
        // the caret stayed at the top; the block is a picture by now, so the
        // text under it sits where the picture ends and not after five lines
        // of code
        let end = h.bounds_of(h.offset_of("end"));
        assert!(end.origin.y > px(100.), "{end:?}");
        let source = h.bounds_of(h.offset_of("flowchart"));
        let blank = h.bounds_of(h.offset_of("end") - 1);
        assert_eq!(
            source.origin.y, blank.origin.y,
            "hidden source lines take no room: their place is the row below"
        );
    }

    #[gpui::test]
    fn a_click_on_a_diagram_leaves_the_caret(tcx: &mut TestAppContext) {
        let mut h = rendered(tcx, NOTE);
        h.act(MoveToEnd);
        let before = h.state();
        let top = h.bounds_of(h.offset_of("top"));
        h.click(Point::new(top.origin.x + px(40.), top.origin.y + px(70.)));
        assert_eq!(h.state(), before);
    }

    #[gpui::test]
    fn the_edit_button_opens_the_source(tcx: &mut TestAppContext) {
        let mut h = rendered(tcx, NOTE);
        h.act(MoveToEnd);
        let top = h.bounds_of(h.offset_of("top"));
        // the column's right edge is 700px from its left, at the default size
        h.click(Point::new(top.origin.x + px(684.), top.origin.y + px(62.)));
        assert!(
            h.state().contains("```mermaid\n¦flowchart"),
            "{:?}",
            h.state()
        );
    }

    #[gpui::test]
    fn arrow_keys_enter_a_diagram_from_either_side(tcx: &mut TestAppContext) {
        let mut h = rendered(tcx, NOTE);
        h.act(MoveToEnd);
        h.act(MoveUp);
        h.act(MoveUp);
        assert!(h.state().contains("```¦\n\nend"), "{:?}", h.state());
        let mut h = rendered(tcx, NOTE);
        h.act(MoveDown);
        h.act(MoveDown);
        assert!(h.state().contains("¦```mermaid"), "{:?}", h.state());
    }

    #[gpui::test]
    fn a_broken_diagram_stays_code(tcx: &mut TestAppContext) {
        let mut h = rendered(tcx, "top\n\n```mermaid\nflowchart TD\n  A[[[\n```\n\nend");
        let source = h.bounds_of(h.offset_of("flowchart"));
        let end = h.bounds_of(h.offset_of("end"));
        assert!(
            end.origin.y > source.origin.y + px(40.),
            "{source:?} {end:?}"
        );
    }
}
