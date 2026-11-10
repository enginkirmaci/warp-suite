//! The About screen: the family in one list, the selected app in the detail.
//!
//! Every GPUI app carries About above Settings and draws the same screen. The
//! list holds the suite's members — the row for the app you are in carries a
//! `This app` tag — and the detail shows the selected one's identity, a Build
//! card and a Legal card. The table lives here so a new app, a new blurb or a
//! copyright change is one edit for the whole family.
//!
//! SnapIt has no window of its own: it is the fourth row on every screen, and
//! its primary returns to Station's Tools on Windows and is absent elsewhere.
//! The chassis is [`shell`]'s, the ink is [`palette`]'s, so a palette or text
//! size switch moves this screen with the rest of the family.
//!
//! ```ignore
//! let build = warpui_kit_core::build_info!();
//! shell::panes()
//!     .child(about::list(width, selected, current, &build, on_pick))
//!     .child(about::detail(selected, current, &build, "Open controls", on_action, on_link))
//! ```

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, IntoElement, MouseButton, Role,
    SharedString, Window, div, px, relative,
};

use crate::controls::{BtnKind, button};
use crate::fonts;
use crate::icon::icon_data;
use crate::palette;
use crate::shell;

/// The suite's members, in the About list's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppId {
    Station,
    Agent,
    Notes,
    Snapit,
}

impl AppId {
    /// The element-id stem for this app's row and buttons.
    pub fn key(self) -> &'static str {
        match self {
            AppId::Station => "station",
            AppId::Agent => "agent",
            AppId::Notes => "notes",
            AppId::Snapit => "snapit",
        }
    }
}

/// One member's fixed facts. Versions are not here: one workspace means one
/// version for the whole suite, so they ride the [`Build`] the app passes.
#[derive(Clone, Copy, Debug)]
pub struct SuiteApp {
    pub id: AppId,
    pub name: &'static str,
    /// The short name in a button: `Open Notes`.
    pub short: &'static str,
    pub role: &'static str,
    pub blurb: &'static str,
    pub runtime: &'static str,
    /// SnapIt has no window of its own; Station hosts its controls.
    pub hosted: bool,
    /// The executable stem a sibling launch spawns.
    pub binary: &'static str,
    /// The app's LICENSE in the suite repository.
    pub license_path: &'static str,
}

impl SuiteApp {
    /// The element-id stem: `station`, `agent`, `notes`, `snapit`.
    pub fn key(&self) -> &'static str {
        self.id.key()
    }
}

/// The family, in the list's order.
pub const SUITE: [SuiteApp; 4] = [
    SuiteApp {
        id: AppId::Station,
        name: "Warp Station",
        short: "Station",
        role: "Repository workbench",
        blurb: "Every local repository, its changes, pull requests, issues and pipelines \
                in one window. One list, one detail pane and no second surface.",
        runtime: "Rust · GPUI",
        hosted: false,
        binary: "warp-station",
        license_path: "apps/warp-station/LICENSE",
    },
    SuiteApp {
        id: AppId::Agent,
        name: "Warp Agent",
        short: "Agent",
        role: "Agent workspace",
        blurb: "A workspace tree on the left and one tile per chat on the right. Every \
                tile keeps its own harness, model, transcript and permission queue.",
        runtime: "Rust · GPUI",
        hosted: false,
        binary: "warp-agent",
        license_path: "apps/warp-agent/LICENSE",
    },
    SuiteApp {
        id: AppId::Notes,
        name: "Warp Notes",
        short: "Notes",
        role: "Markdown notes",
        blurb: "Notes live in a tree, one store per repository. Preview tabs, quick open \
                and the outline stay out of the way of the page.",
        runtime: "Rust · GPUI",
        hosted: false,
        binary: "warp-notes",
        license_path: "apps/warp-notes/LICENSE",
    },
    SuiteApp {
        id: AppId::Snapit,
        name: "SnapIt",
        short: "SnapIt",
        role: "Headless snapping engine",
        blurb: "A Windows engine and zone overlay with no window of its own. Station \
                hosts its controls and layouts under Tools.",
        runtime: "Rust · Windows",
        hosted: true,
        binary: "warp-snapit",
        license_path: "apps/warp-snapit/LICENSE",
    },
];

/// The suite's legal line: the apps are GPL-3.0, the shared kits MIT.
pub const COPYRIGHT: &str = "© 2026 Engin Kirmaci";
pub const LICENSE: &str = "GPL-3.0";

/// The build an app was compiled from. Each app fills it from its own
/// `build.rs` through [`build_info!`]; the kit cannot hold the values because
/// `cargo:rustc-env` applies to the crate whose build script emitted it.
#[derive(Clone, Copy, Debug)]
pub struct Build {
    pub version: &'static str,
    pub build: &'static str,
    pub channel: &'static str,
    pub released: &'static str,
}

impl Build {
    /// The list head's line: `Warp Suite 0.1.0 · dev · 4 apps`.
    pub fn suite_line(&self) -> String {
        format!(
            "Warp Suite {} · {} · {} apps",
            self.version,
            self.channel,
            SUITE.len()
        )
    }

    /// The clipboard contract, one line: `Warp Station 0.1.0 (1b7d006) · dev`.
    pub fn version_line(&self, app: &SuiteApp) -> String {
        format!(
            "{} {} ({}) · {}",
            app.name, self.version, self.build, self.channel
        )
    }
}

/// The build facts, read where they were stamped: **in the app crate**. Every
/// suite `build.rs` sets these four, with `dev`/`unknown` fallbacks, so a plain
/// `cargo run` still compiles and says it is not a release.
#[macro_export]
macro_rules! build_info {
    () => {
        $crate::about::Build {
            version: env!("CARGO_PKG_VERSION"),
            build: env!("WARP_SUITE_BUILD"),
            channel: env!("WARP_SUITE_CHANNEL"),
            released: env!("WARP_SUITE_RELEASED"),
        }
    };
}

/// What the detail's primary does; the app owns the effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the suite's releases page.
    ReleaseNotes,
    /// Bring a sibling app up.
    OpenApp(AppId),
    /// Station's Tools › SnapIt page. On a sibling's About screen the app maps
    /// this to "open Station", which is the closest it can get to that page.
    SnapIt,
    /// Put [`Build::version_line`] on the clipboard.
    CopyVersion,
}

/// A link under the hero, or the Legal card's license row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Repository,
    Changelog,
    Issues,
    License,
}

impl Link {
    pub fn label(self) -> &'static str {
        match self {
            Link::Repository => "Repository",
            Link::Changelog => "Changelog",
            Link::Issues => "Report an issue",
            Link::License => "View license",
        }
    }

    /// The row's element-id stem.
    pub fn key(self) -> &'static str {
        match self {
            Link::Repository => "repository",
            Link::Changelog => "changelog",
            Link::Issues => "issues",
            Link::License => "license",
        }
    }
}

/// The suite repository, from the kit's own manifest: one source, no literal
/// URL in the UI code.
pub fn repository() -> &'static str {
    env!("CARGO_PKG_REPOSITORY")
}

impl Link {
    pub fn url(self, app: &SuiteApp) -> String {
        match self {
            Link::Repository => repository().to_string(),
            Link::Changelog => format!("{}/releases", repository()),
            Link::Issues => format!("{}/issues/new", repository()),
            Link::License => format!("{}/blob/main/{}", repository(), app.license_path),
        }
    }
}

/// The app behind an id. Total by construction, but a missing row falls back to
/// Station instead of panicking in a render path.
pub fn app(id: AppId) -> &'static SuiteApp {
    SUITE.iter().find(|app| app.id == id).unwrap_or(&SUITE[0])
}

/// The app's monochrome mark, tinted by the caller's color.
pub fn mark(id: AppId, size: f32, color: gpui::Rgba) -> gpui::Svg {
    icon_data(mark_bytes(id), px(shell::scaled(size)), color)
}

fn mark_bytes(id: AppId) -> &'static [u8] {
    match id {
        AppId::Station => include_bytes!("../assets/marks/station.svg"),
        AppId::Agent => include_bytes!("../assets/marks/agent.svg"),
        AppId::Notes => include_bytes!("../assets/marks/notes.svg"),
        AppId::Snapit => include_bytes!("../assets/marks/snapit.svg"),
    }
}

/// A link row's monochrome glyph, inline so the kit keeps shipping no assets.
fn link_glyph(link: Link) -> &'static [u8] {
    match link {
        Link::Repository => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"##,
        Link::Changelog => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l3 2"/></svg>"##,
        Link::Issues => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="1.6" fill="currentColor"/></svg>"##,
        Link::License => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6"/></svg>"##,
    }
}

type PickFn = Rc<dyn Fn(AppId, &ClickEvent, &mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(Action, &ClickEvent, &mut Window, &mut App)>;
type LinkFn = Rc<dyn Fn(Link, &ClickEvent, &mut Window, &mut App)>;

/// The list pane: one row per member, then the suite's colophon.
pub fn list(
    width: f32,
    selected: AppId,
    current: AppId,
    build: &Build,
    on_pick: impl Fn(AppId, &ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let on_pick: PickFn = Rc::new(on_pick);
    let mut rows = div()
        .id("about-list-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col();
    for member in SUITE.iter() {
        rows = rows.child(row(member, selected, current, build, on_pick.clone()));
    }
    // the colophon follows the rows, not the pane: four rows is a short list
    // and the design keeps the note right under them
    rows = rows.child(suite_note());

    shell::list_pane(width)
        .debug_selector(|| "about-list".to_string())
        .child(list_head(build))
        .child(rows)
}

/// The detail pane: the head's actions, the hero, Build and Legal.
///
/// `snap_it_label` is the SnapIt row's primary on Windows: Station passes
/// `Open controls` (it can reach its own Tools page), a sibling passes
/// `Open Station`.
pub fn detail(
    selected: AppId,
    current: AppId,
    build: &Build,
    snap_it_label: &'static str,
    on_action: impl Fn(Action, &ClickEvent, &mut Window, &mut App) + 'static,
    on_link: impl Fn(Link, &ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let member = app(selected);
    let is_current = selected == current;
    let on_action: ActionFn = Rc::new(on_action);
    let on_link: LinkFn = Rc::new(on_link);

    let mut actions: Vec<AnyElement> = Vec::new();
    if is_current {
        actions.push(primary(
            "about-primary",
            "Release notes",
            on_action.clone(),
            Action::ReleaseNotes,
        ));
    } else if member.id == AppId::Snapit {
        if cfg!(windows) {
            actions.push(primary(
                "about-primary",
                snap_it_label,
                on_action.clone(),
                Action::SnapIt,
            ));
        }
    } else {
        actions.push(primary(
            "about-primary",
            format!("Open {}", member.short),
            on_action.clone(),
            Action::OpenApp(member.id),
        ));
    }
    let copy = on_action.clone();
    actions.push(
        div()
            .debug_selector(|| "about-copy-version".to_string())
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(button(
                ElementId::Name("about-copy".into()),
                "Copy version",
                BtnKind::Chip,
                palette::accent(),
                fonts::font_control(),
                true,
                move |event, window, cx| copy(Action::CopyVersion, event, window, cx),
            ))
            .into_any_element(),
    );

    let body = div()
        .id("about-detail-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p(px(shell::DETAIL_PADDING))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .w_full()
                .max_w(px(shell::GRID_WIDTH))
                .child(hero(member, build, on_link.clone()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_4()
                        .child(
                            build_card(member, build)
                                .flex_1()
                                .min_w(px(shell::scaled(320.))),
                        )
                        .child(
                            legal_card(build, on_link)
                                .flex_1()
                                .min_w(px(shell::scaled(320.))),
                        ),
                ),
        );

    shell::detail_pane()
        .debug_selector(|| "about-detail".to_string())
        .child(head(member, build, actions))
        .child(body)
}

/// The app's LICENSE in a modal card over the caller's root. The body is the
/// caller's own rendered document — an app passes its `include_str!` through
/// `warpui_kit_markdown::read_only_document`, so the text is selectable,
/// scrollable prose in the family's type instead of one preformatted block.
/// The GPL text belongs to the app crate it ships, not to this MIT kit.
pub fn license_dialog(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    body: impl IntoElement,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = id.into();
    let title = title.into();
    let close = Rc::new(on_close);
    let close_on_scrim = close.clone();
    let close_on_button = close;

    div()
        .id(id)
        .debug_selector(|| "about-license-dialog".to_string())
        .role(Role::Dialog)
        .aria_label(title.clone())
        .absolute()
        .inset_0()
        .occlude()
        .bg(palette::SCRIM)
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_click(move |event, window, cx| close_on_scrim(event, window, cx))
        .child(
            div()
                .id("about-license-card")
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .w(px(shell::scaled(760.)))
                .max_w(relative(1.))
                .h(px(shell::scaled(620.)))
                .max_h(relative(1.))
                .flex()
                .flex_col()
                .rounded(px(10.))
                .border_1()
                .border_color(palette::line_strong())
                .bg(palette::popup())
                .shadow_lg()
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h(px(shell::scaled(52.)))
                        .px_4()
                        .border_b_1()
                        .border_color(palette::hairline())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(fonts::font_name()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette::text())
                                .child(title.clone()),
                        )
                        .child(
                            div()
                                .debug_selector(|| "about-license-close".to_string())
                                .flex_shrink_0()
                                .child(button(
                                    ElementId::Name("about-license-close-btn".into()),
                                    "Close",
                                    BtnKind::Chip,
                                    palette::accent(),
                                    fonts::font_control(),
                                    true,
                                    move |event, window, cx| close_on_button(event, window, cx),
                                )),
                        ),
                )
                .child(
                    div()
                        .id("about-license-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        // the document brings its own top margin and gutter;
                        // the card only owes it a side margin and a small
                        // lead-in, not a second frame of padding
                        .px_4()
                        .pt_2()
                        .pb_4()
                        .text_size(px(fonts::font_meta()))
                        .text_color(palette::text_dim())
                        .child(body),
                )
                .into_any_element(),
        )
        .into_any_element()
}

// ---- pieces ----------------------------------------------------------------

fn row(
    member: &'static SuiteApp,
    selected: AppId,
    current: AppId,
    build: &Build,
    on_pick: PickFn,
) -> gpui::Stateful<Div> {
    let is_selected = member.id == selected;
    let id = ElementId::Name(SharedString::from(format!("about-row-{}", member.key())));
    let selector = format!("about-row-{}", member.key());
    let pick = on_pick;

    shell::list_row(id, is_selected)
        .debug_selector(move || selector)
        .role(Role::Button)
        .aria_label(member.name)
        .on_click(move |event, window, cx| pick(member.id, event, window, cx))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(mark_box(member.id, 30., 18.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(fonts::font_name()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(member.name),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(fonts::font_row_meta()))
                        .text_color(palette::text_muted())
                        .child(format!("Version {}", build.version)),
                ),
        )
        .child(
            div()
                .mt_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(fonts::font_row_meta()))
                        .text_color(palette::text_dim())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(member.role),
                )
                .when(member.id == current, |row| row.child(tag("This app")))
                .when(member.id == AppId::Snapit && !cfg!(windows), |row| {
                    row.child(tag("Windows only"))
                }),
        )
}

fn list_head(build: &Build) -> Div {
    div()
        .flex_shrink_0()
        .px_4()
        .pt_3p5()
        .pb_3()
        .border_b_1()
        .border_color(palette::hairline())
        .child(
            div()
                .text_size(px(fonts::font_list_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette::text())
                .child("About"),
        )
        .child(
            div()
                .mt_0p5()
                .text_size(px(fonts::font_meta()))
                .text_color(palette::text_dim())
                .child(build.suite_line()),
        )
}

fn suite_note() -> Div {
    div()
        .debug_selector(|| "about-note".to_string())
        .flex_shrink_0()
        .mt(px(shell::scaled(18.)))
        .mx_4()
        .pt_3p5()
        .pb_2p5()
        .border_t_1()
        .border_color(palette::hairline())
        .flex()
        .flex_col()
        .gap_0p5()
        .text_size(px(fonts::font_micro()))
        .text_color(palette::text_muted())
        .child("One shell, one design language.")
        .child(format!("{COPYRIGHT} · {LICENSE} · kits MIT"))
}

fn head(member: &SuiteApp, build: &Build, actions: Vec<AnyElement>) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .flex_shrink_0();
    for action in actions {
        row = row.child(action);
    }
    div()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px(px(shell::DETAIL_PADDING))
        .py_4()
        .border_b_1()
        .border_color(palette::hairline())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(fonts::font_screen_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(format!("About {}", member.name)),
                )
                .child(
                    div()
                        .mt_0p5()
                        .text_size(px(fonts::font_meta()))
                        .text_color(palette::text_dim())
                        .child(format!("{} · {}", member.role, family(member, build))),
                ),
        )
        .child(row)
}

fn family(member: &SuiteApp, build: &Build) -> String {
    if member.hosted {
        "Hosted by Warp Station".to_string()
    } else {
        format!("Part of Warp Suite {}", build.version)
    }
}

fn hero(member: &SuiteApp, build: &Build, on_link: LinkFn) -> Div {
    let mut links = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_4()
        .pt_0p5();
    for link in [Link::Repository, Link::Changelog, Link::Issues] {
        links = links.child(link_row(link, on_link.clone()));
    }

    card().child(
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap_4()
            .pt(px(shell::scaled(18.)))
            .px_4()
            .pb_4()
            .child(mark_box(member.id, 56., 32.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(tag(build.channel)),
                    )
                    .child(
                        div()
                            .text_size(px(fonts::font_name()))
                            .line_height(relative(1.5))
                            .text_color(palette::text_dim())
                            .child(member.blurb),
                    )
                    .child(links),
            ),
    )
}

fn build_card(member: &SuiteApp, build: &Build) -> Div {
    card()
        .child(card_head(
            "Build",
            Some(format!("{} channel", build.channel)),
        ))
        .child(
            div()
                .px_4()
                .pt_2()
                .pb_3()
                .child(kv("Version", build.version.to_string()))
                .child(kv("Build", build.build.to_string()))
                .child(kv("Channel", build.channel.to_string()))
                .child(kv("Released", build.released.to_string()))
                .child(kv("Runtime", member.runtime.to_string())),
        )
}

fn legal_card(build: &Build, on_link: LinkFn) -> Div {
    card().child(card_head("Legal", None)).child(
        div()
            .px_4()
            .pt_2()
            .pb_3()
            .child(kv("License", LICENSE.to_string()))
            .child(kv("Copyright", COPYRIGHT.to_string()))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_4()
                    .pt_2()
                    .text_size(px(fonts::font_meta()))
                    .text_color(palette::text_muted())
                    .child(format!("Shared design language {}", build.version))
                    .child(format!("Build {}", build.build))
                    .child(link_row(Link::License, on_link)),
            ),
    )
}

fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(8.))
        .border_1()
        .border_color(palette::hairline())
        .bg(palette::surface())
}

fn card_head(title: &'static str, aside: Option<String>) -> Div {
    let mut head = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .min_h(px(shell::scaled(44.)))
        .px_4()
        .border_b_1()
        .border_color(palette::border_light())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(fonts::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette::text())
                .child(title),
        );
    if let Some(aside) = aside {
        head = head.child(
            div()
                .flex_shrink_0()
                .text_size(px(fonts::font_meta()))
                .text_color(palette::text_muted())
                .child(aside),
        );
    }
    head
}

fn kv(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_h(px(shell::scaled(34.)))
        .child(
            div()
                .flex_shrink_0()
                .w(px(shell::scaled(112.)))
                .text_size(px(fonts::font_meta()))
                .text_color(palette::text_dim())
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(fonts::font_mono_meta()))
                .text_color(palette::text())
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(value),
        )
}

fn link_row(link: Link, on_link: LinkFn) -> gpui::Stateful<Div> {
    let selector = format!("about-link-{}", link.key());
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "about-link-btn-{}",
            link.key()
        ))))
        .debug_selector(move || selector)
        .role(Role::Button)
        .aria_label(link.label())
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .h(px(shell::scaled(28.)))
        .cursor_pointer()
        .text_size(px(fonts::font_row_meta()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(palette::accent_ink())
        .hover(move |style| style.text_color(palette::text()))
        .on_click(move |event, window, cx| on_link(link, event, window, cx))
        .child(icon_data(link_glyph(link), px(14.), palette::accent_ink()))
        .child(link.label())
}

fn mark_box(id: AppId, box_size: f32, glyph: f32) -> Div {
    div()
        .flex_shrink_0()
        .size(px(shell::scaled(box_size)))
        .rounded(px(if box_size > 40. { 8. } else { 6. }))
        .bg(palette::accent_soft())
        .flex()
        .items_center()
        .justify_center()
        .child(mark(id, glyph, palette::accent_ink()))
}

fn tag(label: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .h(px(shell::scaled(18.)))
        .px_1p5()
        .rounded_full()
        .bg(palette::raised())
        .text_size(px(fonts::font_micro()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(palette::text_muted())
        .child(label)
}

fn primary(
    id: &'static str,
    label: impl Into<SharedString>,
    on_action: ActionFn,
    action: Action,
) -> AnyElement {
    div()
        .debug_selector(move || id.to_string())
        .flex_shrink_0()
        .flex()
        .items_center()
        .child(button(
            ElementId::Name(id.into()),
            label,
            BtnKind::Primary(palette::on_accent()),
            palette::accent(),
            fonts::font_control(),
            true,
            move |event, window, cx| on_action(action, event, window, cx),
        ))
        .into_any_element()
}
