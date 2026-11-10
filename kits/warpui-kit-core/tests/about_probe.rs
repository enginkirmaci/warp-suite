//! The About screen: the family table, the clipboard line, and the two panes
//! the shell expects. The apps host the same content, so this is where the
//! shape is measured.

use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*, px, size};
use warpui_kit_core::about::{self, AppId, Build, Link};

const BUILD: Build = Build {
    version: "0.1.0",
    build: "1b7d006",
    channel: "dev",
    released: "2026-11-10",
};

struct Page {
    selected: AppId,
    current: AppId,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .debug_selector(|| "about-root".to_string())
            .child(
                warpui_kit_core::shell::stage().child(
                    warpui_kit_core::shell::panes()
                        .child(about::list(
                            432.,
                            self.selected,
                            self.current,
                            &BUILD,
                            |_, _, _, _| {},
                        ))
                        .child(about::detail(
                            self.selected,
                            self.current,
                            &BUILD,
                            "Open controls",
                            |_, _, _, _| {},
                            |_, _, _, _| {},
                        )),
                ),
            )
    }
}

#[test]
fn the_clipboard_line_is_one_line() {
    let station = about::app(AppId::Station);
    assert_eq!(
        BUILD.version_line(station),
        "Warp Station 0.1.0 (1b7d006) · dev"
    );
    assert_eq!(BUILD.suite_line(), "Warp Suite 0.1.0 · dev · 4 apps");
}

#[test]
fn the_suite_is_the_four_members_in_order() {
    let ids: Vec<AppId> = about::SUITE.iter().map(|app| app.id).collect();
    assert_eq!(
        ids,
        vec![AppId::Station, AppId::Agent, AppId::Notes, AppId::Snapit]
    );
    let keys: Vec<&str> = about::SUITE.iter().map(|app| app.key()).collect();
    for key in ["station", "agent", "notes", "snapit"] {
        assert!(keys.contains(&key), "missing key {key}");
    }
    // SnapIt is the headless one: hosted, and its license lives in its own app.
    let snapit = about::app(AppId::Snapit);
    assert!(snapit.hosted);
    assert_eq!(snapit.license_path, "apps/warp-snapit/LICENSE");
    // One workspace, one repository: every link is built from it.
    assert!(about::repository().contains("github.com"));
    assert!(
        Link::License
            .url(about::app(AppId::Notes))
            .ends_with("/apps/warp-notes/LICENSE")
    );
    assert!(
        Link::Repository
            .url(about::app(AppId::Notes))
            .contains("github.com")
    );
}

#[gpui_kit::gpui::test]
fn the_two_panes_split_the_window(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page {
        selected: AppId::Station,
        current: AppId::Station,
    });
    cx.simulate_resize(size(px(1280.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let root = cx.debug_bounds("about-root").expect("root");
    let list = cx.debug_bounds("about-list").expect("list");
    let detail = cx.debug_bounds("about-detail").expect("detail");
    assert_eq!(list.size.width, px(432.));
    assert_eq!(detail.origin.x, list.origin.x + list.size.width);
    assert_eq!(
        detail.size.width,
        root.size.width - list.size.width,
        "the detail takes what the list leaves"
    );
}

#[gpui_kit::gpui::test]
fn every_member_has_a_row_and_the_current_app_is_the_primary(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page {
        selected: AppId::Station,
        current: AppId::Station,
    });
    cx.simulate_resize(size(px(1280.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    for key in [
        "about-row-station",
        "about-row-agent",
        "about-row-notes",
        "about-row-snapit",
    ] {
        cx.debug_bounds(key).expect(key);
    }
    // The current app owns one filled primary and the copy verb.
    cx.debug_bounds("about-primary").expect("primary");
    cx.debug_bounds("about-copy-version").expect("copy version");
}

#[gpui_kit::gpui::test]
fn snap_it_has_no_primary_on_linux(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page {
        selected: AppId::Snapit,
        current: AppId::Station,
    });
    cx.simulate_resize(size(px(1280.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // The row stays (the family is discoverable), the launch verb does not:
    // there is nothing on Linux for it to start.
    cx.debug_bounds("about-row-snapit").expect("snapit row");
    if cfg!(windows) {
        cx.debug_bounds("about-primary").expect("open controls");
    } else {
        assert!(
            cx.debug_bounds("about-primary").is_none(),
            "SnapIt must not offer a launch on Linux"
        );
    }
    // A sibling's About still offers Copy version.
    cx.debug_bounds("about-copy-version").expect("copy version");
}

#[gpui_kit::gpui::test]
fn the_colophon_follows_the_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page {
        selected: AppId::Station,
        current: AppId::Station,
    });
    cx.simulate_resize(size(px(1280.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let last_row = cx.debug_bounds("about-row-snapit").expect("the last row");
    let note = cx.debug_bounds("about-note").expect("the colophon");
    let list = cx.debug_bounds("about-list").expect("the list");
    let row_bottom = last_row.origin.y + last_row.size.height;
    assert!(
        note.origin.y >= row_bottom,
        "the note sits under the rows: row bottom {row_bottom:?}, note {:?}",
        note.origin.y
    );
    assert!(
        note.origin.y < row_bottom + px(80.),
        "the note follows the rows instead of the pane's foot: row bottom {row_bottom:?}, note {:?}",
        note.origin.y
    );
    assert!(
        note.origin.y < list.origin.y + list.size.height - px(100.),
        "the note is not pinned to the bottom of a short list"
    );
}
