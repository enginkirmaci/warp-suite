//! The Settings page: every settings section as a card of labeled inputs and
//! switches, seeded from the live settings when the page opens. Save writes
//! the whole graph atomically and live-applies (a scan-related change also
//! kicks a rescan). The AI features card's harness radio scopes the Model and
//! Variant pickers to warp-agent's catalog, and it manages the wand prompt
//! templates.

use crate::net::TemplateOp;
use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px, relative};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Editor, EditorState, Input, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use std::cell::RefCell;
use std::rc::Rc;
use warpui_kit_markdown::{LIVE_LANGUAGE, live_highlighter_factory};
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData, PickerIcons, PickerItem,
    PickerRow, PickerSpec, PickerTarget, Selection, TriggerScale, model_name, model_provider,
};

/// The harness the AI card is showing. A tiny entity rather than a page field
/// because the pickers' data closures outlive a render and cannot read the
/// page entity that owns them.
struct HarnessScope {
    harness: String,
}

/// One list row of the section.
#[derive(Clone, Copy, PartialEq)]
enum Section {
    General,
    Appearance,
    Repositories,
    Git,
    ExternalTools,
    Ai,
    Integrations,
}

/// The sections in list order: value, row id, title, what it holds.
const SECTIONS: [(Section, &str, &str, &str); 7] = [
    (
        Section::General,
        "settings-general",
        "General",
        "Startup and tooltips",
    ),
    (
        Section::Appearance,
        "settings-appearance",
        "Appearance",
        "Text size and motion",
    ),
    (
        Section::Repositories,
        "settings-repositories",
        "Repository scanning",
        "Folders, patterns and grouping",
    ),
    (
        Section::Git,
        "settings-git",
        "Git",
        "Branches and auto fetch",
    ),
    (
        Section::ExternalTools,
        "settings-tools",
        "External tools",
        "Editors, terminals and CLIs",
    ),
    (
        Section::Ai,
        "settings-ai",
        "AI",
        "Harness, model and prompt templates",
    ),
    (
        Section::Integrations,
        "settings-integrations",
        "Integrations",
        "Notes, GitHub and Azure DevOps",
    ),
];

pub struct SettingsPage {
    pub store: Entity<Store>,
    // General
    notes_store_path: Entity<InputState>,
    show_tooltips: bool,
    start_minimized: bool,
    start_at_boot: bool,
    // Scanning
    scan_folders: Entity<TextareaState>,
    excluded_folders: Entity<TextareaState>,
    git_folder_pattern: Entity<InputState>,
    solution_file_pattern: Entity<InputState>,
    platform_folder_name: Entity<InputState>,
    max_scan_depth: Entity<InputState>,
    group_by_root: bool,
    group_depth: usize,
    // Git
    branch_name_prefix: Entity<InputState>,
    auto_fetch_minutes: Option<u16>,
    auto_fetch_only_focused: bool,
    // External tools
    vscode_executable: Entity<InputState>,
    vscode_profile: Entity<InputState>,
    terminal_executable: Entity<InputState>,
    ide_executable: Entity<InputState>,
    enable_code_search: bool,
    code_search_executable: Entity<InputState>,
    enable_terminal: bool,
    enable_vscode: bool,
    enable_visual_studio: bool,
    snap_it_auto_start: bool,
    snap_it_executable: Entity<InputState>,
    // OpenCode CLI (the repo row's launch button)
    enable_opencode: bool,
    opencode_executable: Entity<InputState>,
    // DeepSeek CLI (the repo row's launch button)
    enable_deepseek: bool,
    deepseek_executable: Entity<InputState>,
    // AI features (warp-agent). The harness is an entity so both pickers'
    // data closures can read the current radio pick without borrowing the
    // page entity (which owns them).
    warp_enabled: bool,
    warp_executable: Entity<InputState>,
    harness_scope: Entity<HarnessScope>,
    model: Entity<InputState>,
    variant: Entity<InputState>,
    model_picker: Entity<FilteredPicker>,
    variant_picker: Entity<FilteredPicker>,
    // Notes (warp-notes)
    warp_notes_enabled: bool,
    warp_notes_executable: Entity<InputState>,
    // The template being edited inline (registry id); None = the rows list.
    editing_template: Option<&'static str>,
    /// True once the open editor's TemplateLoaded landed and seeded the buffer.
    template_editor_seeded: bool,
    /// The embedded markdown editor for the open template.
    template_editor: Entity<EditorState>,
    /// The row whose Reset is armed (a second click confirms; arming another
    /// row disarms this one).
    reset_armed: Option<&'static str>,
    // Integrations
    enable_github: bool,
    github_executable: Entity<InputState>,
    enable_azure: bool,
    azure_pat: Entity<InputState>,
    azure_url: Entity<InputState>,
    saved_note: Option<std::time::Instant>,
    /// set by the header back button; MainWindow drains it and swaps pages
    section: Section,
}

impl SettingsPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = store.read(cx).settings();
        let repos = &settings.repos;
        let general = &settings.general;
        let line_list = |items: &Option<Vec<String>>| items.clone().unwrap_or_default().join("\n");

        fn field(
            window: &mut Window,
            cx: &mut Context<SettingsPage>,
            value: &str,
        ) -> Entity<InputState> {
            let initial = if value.is_empty() {
                String::new()
            } else {
                value.to_string()
            };
            cx.new(|cx| InputState::new(window, cx).default_value(initial))
        }
        fn area(
            window: &mut Window,
            cx: &mut Context<SettingsPage>,
            value: &str,
        ) -> Entity<TextareaState> {
            cx.new(|cx| TextareaState::new(window, cx).default_value(value.to_string()))
        }

        // the prompt-template editor mirrors the Notes live editor: markdown
        // live highlighting with caret-driven marker fading
        let template_caret = Rc::new(RefCell::new(Some(0..0)));
        let highlighter_caret = template_caret.clone();
        let template_editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(LIVE_LANGUAGE)
                .line_number(false)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
                .placeholder("loading…");
            state.set_highlighter_factory(live_highlighter_factory(highlighter_caret), cx);
            state
        });
        let observe_caret = template_caret.clone();
        cx.observe(&template_editor, move |_, editor, cx| {
            *observe_caret.borrow_mut() = Some(editor.read(cx).selected_range());
        })
        .detach();
        // settled TemplateIo ops land on the store; the template rows and
        // the open editor redraw from them
        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        let harness_scope = cx.new(|_| HarnessScope {
            harness: settings.warp_agent.selected_harness().to_string(),
        });
        let model = field(window, cx, &settings.opencode.model);
        let variant = field(
            window,
            cx,
            &settings.warp_agent.variant.clone().unwrap_or_default(),
        );
        let model_picker = model_picker(&harness_scope, &model, &variant, &store, window, cx);
        let variant_picker = variant_picker(&harness_scope, &model, &variant, &store, window, cx);
        let selected_harness = settings.warp_agent.selected_harness().to_string();

        let page = Self {
            store: store.clone(),
            notes_store_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(general.notes_store_path.clone())
                    .placeholder(
                        warp_station_core::notes::resolve_store_root("")
                            .to_string_lossy()
                            .to_string(),
                    )
            }),
            show_tooltips: general.show_tooltips,
            start_minimized: general.start_minimized,
            start_at_boot: general.start_at_boot,
            scan_folders: area(window, cx, &line_list(&repos.repo_scan_folders)),
            excluded_folders: area(window, cx, &line_list(&repos.excluded_folders)),
            git_folder_pattern: field(window, cx, &repos.git_folder_pattern),
            solution_file_pattern: field(window, cx, &repos.solution_file_pattern),
            platform_folder_name: field(window, cx, &repos.platform_folder_name),
            max_scan_depth: field(window, cx, &repos.max_scan_depth.to_string()),
            group_by_root: repos.group_repos_by_root.unwrap_or(false),
            group_depth: repos.group_root_depth.unwrap_or(1).clamp(1, 4) as usize,
            branch_name_prefix: field(window, cx, &repos.branch_name_prefix),
            auto_fetch_minutes: repos.auto_fetch_minutes,
            auto_fetch_only_focused: repos.auto_fetch_only_focused.unwrap_or(false),
            vscode_executable: field(window, cx, &repos.vscode_executable),
            vscode_profile: field(window, cx, &repos.vscode_profile),
            terminal_executable: field(window, cx, &repos.terminal_executable),
            ide_executable: field(window, cx, &repos.ide_executable),
            opencode_executable: field(window, cx, &repos.opencode_executable),
            deepseek_executable: field(window, cx, &repos.deepseek_executable),
            enable_code_search: repos.enable_code_search.unwrap_or(true),
            code_search_executable: field(
                window,
                cx,
                &repos
                    .code_search_executable
                    .clone()
                    .unwrap_or_else(|| "rg".into()),
            ),
            enable_terminal: repos.enable_terminal,
            enable_vscode: repos.enable_vscode,
            enable_visual_studio: repos.enable_visual_studio,
            snap_it_auto_start: settings.snap_it.auto_start,
            snap_it_executable: field(window, cx, &settings.snap_it.executable),
            enable_opencode: settings.opencode.enable_opencode,
            enable_deepseek: settings.deepseek.enable_deepseek,
            warp_enabled: settings.warp_agent.enable_warp_agent,
            warp_executable: field(window, cx, &settings.warp_agent.executable),
            harness_scope,
            model,
            variant,
            model_picker,
            variant_picker,
            warp_notes_enabled: settings.warp_notes.enable_warp_notes,
            warp_notes_executable: field(window, cx, &settings.warp_notes.executable),
            editing_template: None,
            template_editor_seeded: false,
            template_editor,
            reset_armed: None,
            enable_github: repos.enable_git_hub,
            github_executable: field(window, cx, &repos.git_hub_executable),
            enable_azure: repos.enable_azure_dev_ops,
            azure_pat: field(window, cx, &repos.azure_dev_ops_pat),
            azure_url: field(window, cx, &repos.azure_dev_ops_url),
            saved_note: None,
            section: Section::General,
        };
        store.update(cx, |s, cx| s.load_models_for(selected_harness, cx));
        page
    }

    fn value(field: &Entity<InputState>, cx: &Context<Self>) -> String {
        field.read(cx).value().to_string()
    }

    fn lines(field: &Entity<TextareaState>, cx: &Context<Self>) -> Vec<String> {
        field
            .read(cx)
            .value()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let scan_folders = Self::lines(&self.scan_folders, cx);
        let excluded = Self::lines(&self.excluded_folders, cx);
        let max_scan_depth = Self::value(&self.max_scan_depth, cx)
            .trim()
            .parse::<i32>()
            .unwrap_or(3);
        let harness = self.harness_scope.read(cx).harness.clone();
        let (notes_path, git_pattern, solution_pattern, platform) = (
            Self::value(&self.notes_store_path, cx),
            Self::value(&self.git_folder_pattern, cx),
            Self::value(&self.solution_file_pattern, cx),
            Self::value(&self.platform_folder_name, cx),
        );
        let (vscode_exe, vscode_profile, terminal_exe) = (
            Self::value(&self.vscode_executable, cx),
            Self::value(&self.vscode_profile, cx),
            Self::value(&self.terminal_executable, cx),
        );
        let (ide_exe, opencode_exe, deepseek_exe) = (
            Self::value(&self.ide_executable, cx),
            Self::value(&self.opencode_executable, cx),
            Self::value(&self.deepseek_executable, cx),
        );
        let (model, github_exe) = (
            Self::value(&self.model, cx),
            Self::value(&self.github_executable, cx),
        );
        let (azure_pat, azure_url) = (
            Self::value(&self.azure_pat, cx),
            Self::value(&self.azure_url, cx),
        );
        let flags = (
            self.show_tooltips,
            self.start_minimized,
            self.start_at_boot,
            self.enable_terminal,
            self.enable_vscode,
            self.enable_visual_studio,
            self.enable_opencode,
            self.enable_github,
            self.enable_azure,
            self.enable_deepseek,
        );
        let snap_it_auto_start = self.snap_it_auto_start;
        let snap_it_exe = Self::value(&self.snap_it_executable, cx);
        let warp_enabled = self.warp_enabled;
        let warp_exe = Self::value(&self.warp_executable, cx);
        let variant = Self::value(&self.variant, cx);
        let harness_for_load = harness.clone();
        let warp_notes_enabled = self.warp_notes_enabled;
        let warp_notes_exe = Self::value(&self.warp_notes_executable, cx);
        let branch_prefix = Self::value(&self.branch_name_prefix, cx);
        let auto_fetch_minutes = self.auto_fetch_minutes;
        // absence = fetch always; the only meaningful write is the opt-in
        let auto_fetch_only_focused = self.auto_fetch_only_focused;
        let group_by_root = self.group_by_root;
        let group_depth = self.group_depth;
        let code_search_enabled = self.enable_code_search;
        let code_search_exe = Self::value(&self.code_search_executable, cx);

        let old_scan_folders = self
            .store
            .read(cx)
            .settings()
            .repos
            .repo_scan_folders
            .clone();

        self.store.update(cx, |store, cx| {
            store.update_settings(|s| {
                s.general.notes_store_path = notes_path;
                s.general.show_tooltips = flags.0;
                s.general.start_minimized = flags.1;
                s.general.start_at_boot = flags.2;
                s.repos.repo_scan_folders = if scan_folders.is_empty() {
                    None
                } else {
                    Some(scan_folders.clone())
                };
                s.repos.excluded_folders = if excluded.is_empty() {
                    None
                } else {
                    Some(excluded.clone())
                };
                s.repos.git_folder_pattern = git_pattern;
                s.repos.solution_file_pattern = solution_pattern;
                s.repos.platform_folder_name = platform;
                s.repos.max_scan_depth = max_scan_depth;
                s.repos.branch_name_prefix = branch_prefix;
                s.repos.auto_fetch_minutes = auto_fetch_minutes;
                s.repos.auto_fetch_only_focused = auto_fetch_only_focused.then_some(true);
                s.repos.group_repos_by_root = Some(group_by_root);
                s.repos.group_root_depth = Some(group_depth as u8);
                // enabled stays absent: absence IS the enabled state, so the
                // only meaningful write is the opt-out
                s.repos.enable_code_search = (!code_search_enabled).then_some(false);
                s.repos.code_search_executable =
                    (!code_search_exe.trim().is_empty()).then_some(code_search_exe);
                s.repos.vscode_executable = vscode_exe;
                s.repos.vscode_profile = vscode_profile;
                s.repos.terminal_executable = terminal_exe;
                s.repos.ide_executable = ide_exe;
                s.repos.opencode_executable = opencode_exe;
                s.repos.deepseek_executable = deepseek_exe;
                s.repos.enable_terminal = flags.3;
                s.repos.enable_vscode = flags.4;
                s.repos.enable_visual_studio = flags.5;
                s.opencode.enable_opencode = flags.6;
                s.deepseek.enable_deepseek = flags.9;
                s.warp_agent.enable_warp_agent = warp_enabled;
                s.warp_agent.executable = warp_exe;
                s.warp_agent.harness = harness;
                s.warp_agent.variant = (!variant.trim().is_empty()).then_some(variant);
                s.warp_notes.enable_warp_notes = warp_notes_enabled;
                s.warp_notes.executable = warp_notes_exe;
                s.opencode.model = model;
                s.repos.enable_git_hub = flags.7;
                s.repos.git_hub_executable = github_exe;
                s.repos.enable_azure_dev_ops = flags.8;
                s.repos.azure_dev_ops_pat = azure_pat;
                s.repos.azure_dev_ops_url = azure_url;
                s.snap_it.auto_start = snap_it_auto_start;
                s.snap_it.executable = snap_it_exe;
            });
            // the interval may have changed: re-arm the periodic fetch
            store.refresh_auto_fetch_schedule();
            // grouping only reshapes the list: re-project, never rescan —
            // the repos and their folders did not change
            store.set_group_by_root(group_by_root);
            store.set_group_depth(group_depth);
            // a scan-input change live-applies with a rescan; a text-size
            // change re-runs the kit theme (which refreshes every window, so
            // tokens and the kit rem base resize together); every save
            // re-probes activity so the integration switches take effect immediately
            if store.settings().repos.repo_scan_folders != old_scan_folders {
                let settings = store.settings();
                store.kick_scan(&settings);
            }
            let folders: Vec<std::path::PathBuf> = store
                .repos
                .iter()
                .filter(|repo| !repo.is_archived())
                .map(|r| r.folder_path.clone())
                .collect();
            store.kick_github_pass(&folders);
            store.kick_azure_pass(&folders);
            store.load_models_for(harness_for_load, cx);
            cx.notify();
        });
        // the boot registration follows the saved flag
        warp_station_core::autostart::reconcile(self.start_at_boot);
        // so does the execution state; a failure leaves the chip as the truth
        self.saved_note = Some(std::time::Instant::now());
        cx.notify();
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // the settled TemplateLoaded seeds the open editor here (set_value
        // needs a Window, which the store observer does not carry)
        if self.editing_template.is_some()
            && !self.template_editor_seeded
            && let Some((id, content, _)) = self.store.read(cx).template_loaded.clone()
            && self.editing_template == Some(id.as_str())
        {
            self.template_editor_seeded = true;
            self.template_editor.update(cx, |state, cx| {
                state.set_value(content.as_str(), window, cx)
            });
        }
        let just_saved = self
            .saved_note
            .map(|at| at.elapsed() < std::time::Duration::from_secs(2))
            .unwrap_or(false);
        let section = self.section;
        let mut rows = div().flex().flex_col().pt_1();
        for (item, id, title, hint) in SECTIONS {
            rows = rows.child(
                warpui_kit_core::shell::list_row(id, item == section)
                    .debug_selector(move || id.into())
                    .a11y_tab(title, item == section)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = item;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(theme::font_name()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_0p5()
                            .text_size(px(theme::font_row_meta()))
                            .text_color(theme::text_dim())
                            .child(hint),
                    ),
            );
        }
        let list = warpui_kit_core::shell::list_pane(self.store.read(cx).list_width())
            .child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .pt_3p5()
                    .pb_3()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .text_size(px(theme::font_list_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("Settings"),
            )
            .child(rows);

        let (title, hint) = SECTIONS
            .iter()
            .find(|entry| entry.0 == section)
            .map_or(("Settings", ""), |entry| (entry.2, entry.3));
        let head = div()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px(px(warpui_kit_core::shell::DETAIL_PADDING))
            .py_4()
            .border_b_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(theme::font_screen_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_0p5()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_dim())
                            .child(hint),
                    ),
            )
            .when(just_saved, |row| {
                row.child(
                    div()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::SUCCESS_BRIGHT)
                        .child("saved ✓"),
                )
            })
            .child(
                div()
                    .id("settings-open-folder")
                    .a11y_button("Open data folder")
                    .cursor_pointer()
                    .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                    .px_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::line_strong())
                    .bg(theme::panel())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text())
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(|_, _, _| {
                        let folder = warp_station_core::paths::AppPaths::user_data_root();
                        let _ = std::fs::create_dir_all(&folder);
                        let _ = warp_station_core::terminal::open_folder(&folder.to_string_lossy());
                    })
                    .child(crate::ui::icon(
                        "icons/icon-folder.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    ))
                    .child("Data folder"),
            )
            .child(save_button(cx));

        let mut cards = div()
            .w_full()
            .max_w(px(warpui_kit_core::shell::READING_WIDTH))
            .flex()
            .flex_col()
            .gap_3();
        for card in self.section_cards(section, cx) {
            cards = cards.child(card);
        }

        warpui_kit_core::shell::panes().child(list).child(
            warpui_kit_core::shell::detail_pane()
                .debug_selector(|| "settings-detail".into())
                .child(head)
                .child(
                    div()
                        .id("settings-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(warpui_kit_core::shell::DETAIL_PADDING))
                        .child(cards),
                ),
        )
    }
}

impl SettingsPage {
    /// The cards of one section.
    fn section_cards(&mut self, section: Section, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        match section {
            Section::General => vec![self.card("Startup", cx, |this, cx, card| {
                card.child(switch_row("Start at boot", this.start_at_boot, cx.listener(|this, _: &bool, _, cx| {
                    this.start_at_boot = !this.start_at_boot;
                    cx.notify();
                })))
                .child(switch_row("Start minimized", this.start_minimized, cx.listener(|this, _, _, cx| {
                    this.start_minimized = !this.start_minimized;
                    cx.notify();
                })))
                .child(switch_row("Show tooltips", this.show_tooltips, cx.listener(|this, _: &bool, _, cx| {
                    this.show_tooltips = !this.show_tooltips;
                    cx.notify();
                })))
                .child(field_row("Notes store path", &this.notes_store_path))
            })],
            Section::Appearance => vec![
                self.card("Palette", cx, |_, _, card| {
                    card.child(warpui_kit_core::palette_control())
                        .child(label("Applied right away. The other Warp apps follow when they next open."))
                }),
                self.card("Text", cx, |_, _, card| {
                    card.child(text_size_row())
                        .child(label("Applied right away. The other Warp apps follow when they next open."))
                }),
                self.card("Motion", cx, |this, cx, card| {
                    let reduced = this.store.read(cx).settings().general.reduce_motion;
                    let store = this.store.clone();
                    card.child(switch_row("Reduce motion", reduced, move |_, _, cx| {
                        store.update(cx, |s, cx| {
                            s.set_reduce_motion(!reduced, cx);
                        });
                        cx.refresh_windows();
                    }))
                    .child(label("Turns animation off, spinners included. The busy words stay. Applied right away."))
                }),
            ],
            Section::Repositories => vec![self.card("Repository scanning", cx, |this, cx, card| {
                card.child(label("Scan folders (one per line)"))
                    .child(Textarea::new(&this.scan_folders).bordered(true).with_size(theme::field_size()).h(px(240.)).aria_label("Scan folders"))
                    .child(label("Excluded folders (one per line)"))
                    .child(Textarea::new(&this.excluded_folders).bordered(true).with_size(theme::field_size()).h(px(100.)).aria_label("Excluded folders"))
                    .child(field_row("Git folder pattern", &this.git_folder_pattern))
                    .child(field_row("Solution file pattern", &this.solution_file_pattern))
                    .child(field_row("Platform folder name", &this.platform_folder_name))
                    .child(field_row("Max scan depth", &this.max_scan_depth))
                    .child(switch_row("Group repos by folder", this.group_by_root, cx.listener(|this, _, _, cx| {
                        this.group_by_root = !this.group_by_root;
                        cx.notify();
                    })))
                    .child(label("Group the list by the folder each repository lives in. Collapsing and filtering never rescan."))
                    .when(this.group_by_root, |card| {
                        card.child(group_depth_row(this, cx))
                            .child(label("1 groups repos under the scan folders' shared folder, 2 under that folder's parent, and so on. Folders that resolve to the same path (a symlink and its target) share one group."))
                    })
            })],
            Section::Git => vec![self.card("Git", cx, |this, cx, card| {
                card.child(field_row("Branch name prefix", &this.branch_name_prefix))
                    .child(auto_fetch_row(this, cx))
                    .child(switch_row("Only fetch while active", this.auto_fetch_only_focused, cx.listener(|this, _, _, cx| {
                        this.auto_fetch_only_focused = !this.auto_fetch_only_focused;
                        cx.notify();
                    })))
                    .child(label("With an interval set, pauses the fetch while no window has focus; with Auto fetch off, fetches once on launch."))
            })],
            Section::ExternalTools => vec![
                self.card("Editors and terminal", cx, |this, cx, card| {
                    card.child(switch_row("Enable Visual Studio (Windows)", this.enable_visual_studio, cx.listener(|this, _, _, cx| {
                            this.enable_visual_studio = !this.enable_visual_studio;
                            cx.notify();
                        })))
                    .child(switch_row("Enable VS Code", this.enable_vscode, cx.listener(|this, _, _, cx| {
                            this.enable_vscode = !this.enable_vscode;
                            cx.notify();
                        })))
                    .child(switch_row("Enable Terminal", this.enable_terminal, cx.listener(|this, _, _, cx| {
                        this.enable_terminal = !this.enable_terminal;
                        cx.notify();
                    })))
                    .child(field_row("VS Code executable", &this.vscode_executable))
                    .child(field_row("VS Code profile", &this.vscode_profile))
                    .child(field_row("Terminal executable", &this.terminal_executable))
                    .child(field_row("IDE executable", &this.ide_executable))
                    .child(switch_row("Cross-repo code search", this.enable_code_search, cx.listener(|this, _, _, cx| {
                        this.enable_code_search = !this.enable_code_search;
                        cx.notify();
                    })))
                    .child(field_row("ripgrep executable", &this.code_search_executable))
                    .when(cfg!(windows), |card| {
                        card.child(switch_row("Start SnapIt automatically (Windows)", this.snap_it_auto_start, cx.listener(|this, _, _, cx| {
                            this.snap_it_auto_start = !this.snap_it_auto_start;
                            cx.notify();
                        })))
                        .child(field_row("SnapIt executable", &this.snap_it_executable))
                    })
                }),
                self.card("OpenCode", cx, |this, cx, card| {
                    card.child(switch_row("Enable OpenCode", this.enable_opencode, cx.listener(|this, _, _, cx| {
                        this.enable_opencode = !this.enable_opencode;
                        cx.notify();
                    })))
                    .child(field_row("OpenCode executable", &this.opencode_executable))
                    .child(label("Adds the OpenCode launcher: it runs this CLI in a terminal with the selected model when the harness is opencode. Independent of warp-agent runs."))
                }),
                self.card("DeepSeek", cx, |this, cx, card| {
                    card.child(switch_row("Enable DeepSeek", this.enable_deepseek, cx.listener(|this, _, _, cx| {
                        this.enable_deepseek = !this.enable_deepseek;
                        cx.notify();
                    })))
                    .child(field_row("DeepSeek executable", &this.deepseek_executable))
                    .child(label("Adds the DeepSeek launcher: it runs this CLI in a terminal. Independent of warp-agent runs."))
                }),
            ],
            Section::Ai => vec![self.card("AI features (warp-agent)", cx, |this, cx, card| {
                card.child(switch_row("Enable warp-agent", this.warp_enabled, cx.listener(|this, _, _, cx| {
                    this.warp_enabled = !this.warp_enabled;
                    cx.notify();
                })))
                .child(field_row("warp-agent executable", &this.warp_executable))
                .child(this.harness_row(cx))
                .child(label("Model"))
                .child(this.model_picker.clone())
                .child(label("Variant"))
                .child(this.variant_picker.clone())
                .child(label("Prompt templates"))
                .child(this.templates_section(cx))
                .child(label("All wands, commit messages and the daily summary run over warp-agent's door with this harness, model and variant. Switching the harness resets the model pick."))
            })],
            Section::Integrations => vec![
                self.card("Notes (warp-notes)", cx, |this, cx, card| {
                    card.child(switch_row("Enable warp-notes", this.warp_notes_enabled, cx.listener(|this, _, _, cx| {
                        this.warp_notes_enabled = !this.warp_notes_enabled;
                        cx.notify();
                    })))
                    .child(field_row("warp-notes executable", &this.warp_notes_executable))
                    .child(label("The title bar and the Notes launcher open this app over the shared notes store."))
                }),
                self.card("Integrations", cx, |this, cx, card| {
                    card.child(switch_row("GitHub", this.enable_github, cx.listener(|this, _, _, cx| {
                        this.enable_github = !this.enable_github;
                        cx.notify();
                    })))
                    .child(field_row("GitHub CLI executable", &this.github_executable))
                    .child(switch_row("Azure DevOps", this.enable_azure, cx.listener(|this, _, _, cx| {
                        this.enable_azure = !this.enable_azure;
                        cx.notify();
                    })))
                    .child(field_row("Azure DevOps PAT", &this.azure_pat))
                    .child(field_row("Azure DevOps server URL", &this.azure_url))
                }),
            ],
        }
    }
}

impl SettingsPage {
    // ---- prompt templates -------------------------------------------------

    /// The card's template manager: one row per registry entry, or the
    /// embedded editor while a template is open for editing.
    fn templates_section(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.editing_template {
            Some(id) => self.template_editor_view(id, cx),
            None => self.template_rows(cx),
        }
    }

    /// One row per registry entry: title, the customized badge (stored body
    /// differs from the built-in default), the placeholder chips, and the
    /// Edit/Open/Reset actions.
    fn template_rows(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut list = div().flex().flex_col().gap_2();
        for template in warp_station_core::opencode::PROMPT_TEMPLATES {
            let id = template.id;
            // read from disk each draw so externally-edited bodies badge on
            // the next render; every TemplateIo settle re-notifies the store
            let customized = warp_station_core::opencode::is_customized(id);
            let armed = self.reset_armed == Some(id);
            let buttons = self.template_buttons(id, armed, cx);
            list = list.child(
                div()
                    .id(SharedString::from(format!("template-row-{id}")))
                    .debug_selector(move || format!("template-row-{id}"))
                    .aria_label(template.title)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::text())
                                    .child(template.title),
                            )
                            .when(customized, |row| {
                                row.child(
                                    div()
                                        .id(SharedString::from(format!("template-customized-{id}")))
                                        .debug_selector(move || format!("template-customized-{id}"))
                                        .rounded_full()
                                        .bg(theme::tint(theme::accent()))
                                        .text_color(theme::blue_bright())
                                        .text_size(px(theme::font_control()))
                                        .px_1p5()
                                        .child("customized"),
                                )
                            })
                            .child(div().flex_1())
                            .child(buttons),
                    )
                    .child(placeholder_chips(template.placeholders)),
            );
        }
        list.into_any_element()
    }

    /// Edit swaps the card into the embedded editor, Open hands the file to
    /// the system editor, and Reset is two-click armed (arming another row
    /// replaces the arm).
    fn template_buttons(
        &mut self,
        id: &'static str,
        armed: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let this = cx.entity();
        let reset_store = self.store.clone();
        div()
            .flex()
            .flex_row()
            .gap_1()
            .child(template_action_button(
                format!("template-edit-{id}"),
                "Edit",
                false,
                cx.listener(move |this, _, window, cx| {
                    this.editing_template = Some(id);
                    this.template_editor_seeded = false;
                    this.reset_armed = None;
                    // blank the buffer so the previously-edited template
                    // never flashes while the Read round-trip is in flight
                    this.template_editor
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    this.store
                        .update(cx, |s, _| s.template_io(id, TemplateOp::Read));
                    cx.notify();
                }),
            ))
            .child(template_action_button(
                format!("template-open-{id}"),
                "Open",
                false,
                move |_, _, _| {
                    // never hand the system editor a file that does not exist
                    if let Some(template) = warp_station_core::opencode::PROMPT_TEMPLATES
                        .iter()
                        .find(|t| t.id == id)
                        && warp_station_core::opencode::read_template(id).is_none()
                    {
                        let _ = warp_station_core::opencode::write_template(id, template.default);
                    }
                    if let Some(path) = warp_station_core::opencode::template_path(id) {
                        let _ = warp_station_core::terminal::open_folder(&path.to_string_lossy());
                    }
                },
            ))
            .child(template_action_button(
                format!("template-reset-{id}"),
                "Reset",
                armed,
                move |_, _, cx| {
                    if armed {
                        reset_store.update(cx, |s, _| s.template_io(id, TemplateOp::Reset));
                        this.update(cx, |this, cx| {
                            this.reset_armed = None;
                            cx.notify();
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.reset_armed = Some(id);
                            cx.notify();
                        });
                    }
                },
            ))
            .into_any_element()
    }

    /// The embedded editor for one template. The body arrives via the store
    /// (TemplateLoaded) and render seeds the buffer, so the editor shows a
    /// loading label until the event lands.
    fn template_editor_view(
        &mut self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let template = warp_station_core::opencode::PROMPT_TEMPLATES
            .iter()
            .find(|t| t.id == id)
            .expect("editing_template ids come from the registry");
        let title = template.title;
        let editor = Editor::new(&self.template_editor)
            .bordered(false)
            .appearance(false)
            .h_full()
            .aria_label(format!("{title} template body"))
            .font_family(SharedString::from(theme::ui_font()))
            .text_size(px(theme::font_heading()))
            .line_height(relative(1.7));
        div()
            .id(SharedString::from(format!("template-editor-{id}")))
            .debug_selector(move || format!("template-editor-{id}"))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .text_color(theme::text())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_faint())
                            .child(if self.template_editor_seeded {
                                "re-read by every wand run"
                            } else {
                                "loading…"
                            }),
                    )
                    .child(div().flex_1())
                    .child(template_action_button(
                        format!("template-save-{id}"),
                        "Save",
                        false,
                        cx.listener(move |this, _, _, cx| {
                            let content = this.template_editor.read(cx).value().to_string();
                            this.store
                                .update(cx, |s, _| s.template_io(id, TemplateOp::Write(content)));
                            this.store.update(cx, |s, cx| {
                                s.notify_success(format!("{title} template saved"));
                                cx.notify();
                            });
                            this.editing_template = None;
                            this.template_editor_seeded = false;
                            cx.notify();
                        }),
                    ))
                    .child(template_action_button(
                        format!("template-cancel-{id}"),
                        "Cancel",
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.editing_template = None;
                            this.template_editor_seeded = false;
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .h(px(320.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme::hairline())
                    .p_2()
                    .child(editor),
            )
            .child(placeholder_chips(template.placeholders))
            .into_any_element()
    }

    fn card(
        &mut self,
        title: &str,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Self, &mut Context<Self>, gpui::Div) -> gpui::Div,
    ) -> gpui::AnyElement {
        let shell = div()
            .w_full()
            .rounded_xl()
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::card())
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(theme::font_heading()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            );
        build(self, cx, shell).into_any_element()
    }
}

fn save_button(cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    div()
        .id("settings-save")
        .debug_selector(|| "settings-save".into())
        .a11y_button("Save settings")
        .cursor_pointer()
        .rounded_md()
        .bg(theme::accent())
        .px_4()
        .py_1p5()
        .text_size(px(theme::font_body()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .hover(|s| s.opacity(0.9))
        .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
        .child("Save")
        .into_any_element()
}

fn label(text: &str) -> impl IntoElement + use<> {
    div()
        .text_size(px(theme::font_body()))
        .text_color(theme::text_muted())
        .child(text.to_string())
}

/// A small bordered row action; the armed state turns red with "Confirm".
fn template_action_button(
    id: String,
    label: &str,
    armed: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let text = if armed { "Confirm" } else { label };
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .a11y_button(text)
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(if armed {
            theme::DANGER
        } else {
            theme::hairline()
        })
        .px_2()
        .py_0p5()
        .text_size(px(theme::font_meta()))
        .text_color(if armed {
            theme::DANGER
        } else {
            theme::text_dim()
        })
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(on_click)
        .child(text.to_string())
        .into_any_element()
}

/// The template's fill-in placeholders as tiny mono chips.
fn placeholder_chips(placeholders: &[&'static str]) -> impl IntoElement + use<> {
    let mut row = div().flex().flex_row().flex_wrap().gap_1();
    for placeholder in placeholders {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::chip())
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::font_control()))
                .text_color(theme::text_muted())
                .font_family(SharedString::from(theme::code_font()))
                .child(placeholder.to_string()),
        );
    }
    row
}

fn field_row(label_text: &str, field: &Entity<InputState>) -> impl IntoElement + use<> {
    // a stable per-row selector so probes can reach the field through its row.
    // The design's setting is one row: label left, control right, 60 high
    // (multi-line fields are the ones that stack, and they have their own row).
    let selector = format!("settings-field-{label_text}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_h(px(60.))
        .child(div().flex_1().min_w_0().child(label(label_text)))
        .child(
            div()
                .w(px(240.))
                .flex_shrink_0()
                .child(warpui_kit_core::shell::field(
                    "settings-field",
                    Input::new(field)
                        .with_size(theme::field_size())
                        .aria_label(label_text),
                )),
        )
}

fn switch_row(
    label_text: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    // a stable per-row selector so probes can reach the switch through its row
    let selector = format!("settings-row-{label_text}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_h(px(60.))
        // label left, control right, like every other setting row
        .child(div().flex_1().min_w_0().child(label(label_text)))
        .child(
            Switch::new(SharedString::from(format!("switch-{label_text}")))
                .checked(checked)
                .accessibility_label(label_text)
                .on_click(on_click),
        )
        .into_any_element()
}

fn text_size_row() -> gpui::AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_h(px(60.))
        .child(div().flex_1().min_w_0().child(label("Text size")))
        .child(warpui_kit_core::text_size_control())
        .into_any_element()
}

/// The group-depth picker: 1/2/3/4 segmented buttons; 1 = the scan roots'
/// shared folder, each further level one folder higher. Only rendered while
/// grouping is on.
fn group_depth_row(this: &SettingsPage, cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let mut row = div()
        .id("group-depth-row")
        .debug_selector(|| "group-depth-row".into())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(label("Group depth"));
    for level in 1..=4usize {
        let id = SharedString::from(format!("group-depth-{level}"));
        let selected = this.group_depth == level;
        row = row.child(
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .a11y_toggle(format!("Group depth {level}"), selected)
                .cursor_pointer()
                .rounded_md()
                .px_3()
                .py_1()
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(selected, |d| {
                    d.bg(theme::tint(theme::accent()))
                        .text_color(theme::blue_bright())
                })
                .when(!selected, |d| {
                    d.text_color(theme::text_muted())
                        .hover(|s| s.bg(theme::hover_bg()))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.group_depth = level;
                    cx.notify();
                }))
                .child(level.to_string()),
        );
    }
    row.into_any_element()
}

/// The periodic-fetch picker: Off/15m/30m/60m segmented buttons; None = off.
fn auto_fetch_row(this: &SettingsPage, cx: &mut Context<SettingsPage>) -> gpui::AnyElement {
    let options: [(Option<u16>, &str, &'static str); 4] = [
        (None, "Off", "auto-fetch-off"),
        (Some(15), "15m", "auto-fetch-15"),
        (Some(30), "30m", "auto-fetch-30"),
        (Some(60), "60m", "auto-fetch-60"),
    ];
    let mut row = div()
        .id("auto-fetch-row")
        .debug_selector(|| "auto-fetch-row".into())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(label("Auto fetch"));
    for (value, name, id) in options {
        let selected = this.auto_fetch_minutes == value;
        row = row.child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .a11y_toggle(format!("Auto fetch {name}"), selected)
                .cursor_pointer()
                .rounded_md()
                .px_3()
                .py_1()
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(selected, |d| {
                    d.bg(theme::tint(theme::accent()))
                        .text_color(theme::blue_bright())
                })
                .when(!selected, |d| {
                    d.text_color(theme::text_muted())
                        .hover(|s| s.bg(theme::hover_bg()))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.auto_fetch_minutes = value;
                    cx.notify();
                }))
                .child(name),
        );
    }
    row.into_any_element()
}

impl SettingsPage {
    /// Switches the AI card's harness. The model belongs to the harness, so a
    /// switch clears the model and variant picks; the door then loads the new
    /// harness's catalog.
    fn pick_harness(&mut self, harness: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.harness_scope.read(cx).harness == harness {
            return;
        }
        self.harness_scope.update(cx, |scope, cx| {
            scope.harness = harness.clone();
            cx.notify();
        });
        self.model
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.variant
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.store
            .update(cx, |store, cx| store.load_models_for(harness, cx));
        cx.notify();
    }

    /// The harness radio row: one round radio per option the door reports.
    fn harness_row(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let harnesses = self.store.read(cx).harnesses.clone();
        let selected = self.harness_scope.read(cx).harness.clone();
        let mut row = div()
            .id("warp-harness-row")
            .debug_selector(|| "warp-harness-row".into())
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(label("Harness"));
        for harness in harnesses {
            let id = harness.id;
            let display = harness.label;
            let is_on = id == selected;
            let selector = format!("warp-harness-{id}");
            let marker = if is_on {
                div()
                    .size(px(11.))
                    .rounded_full()
                    .bg(theme::accent())
                    .into_any_element()
            } else {
                div()
                    .size(px(11.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::text_muted())
                    .into_any_element()
            };
            let click_id = id.clone();
            row = row.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .role(gpui::Role::RadioButton)
                    .aria_label(SharedString::from(format!("Harness {display}")))
                    .aria_selected(is_on)
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_harness(click_id.clone(), window, cx);
                    }))
                    .child(marker)
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .text_color(theme::text())
                            .child(display),
                    ),
            );
        }
        row.into_any_element()
    }
}

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// The compact settings-slot picker look shared by Model and Variant.
fn picker_spec(
    key: &'static str,
    trigger_label: &'static str,
    filter_placeholder: &'static str,
    provider_badge: bool,
    refresh: bool,
    refresh_label: &'static str,
) -> PickerSpec {
    PickerSpec {
        key,
        trigger_label,
        filter_placeholder,
        icons: PICKER_ICONS,
        trigger_icon: None,
        provider_badge,
        refresh,
        refresh_label,
        stop_trigger_mousedown: false,
        visible_rows: 10,
        selected_accent: false,
        // a settings slot floats: the list is taller than the card and must
        // cover the rows under it, not stretch the form around itself
        overlay: true,
        anchor: OverlayAnchor::Field,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: None,
        panel_id: None,
        filter_selector: None,
        list_id: None,
    }
}

/// The Model picker: writes the pick into `field`, clears the variant (it
/// belongs to the old model), and lists only the selected harness's models.
fn model_picker(
    scope: &Entity<HarnessScope>,
    field: &Entity<InputState>,
    variant_field: &Entity<InputState>,
    store: &Entity<Store>,
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
) -> Entity<FilteredPicker> {
    let spec = picker_spec(
        "Model",
        "Model",
        "Filter models",
        true,
        true,
        "Refresh models",
    );
    let refresh_store = store.clone();
    let refresh_scope = scope.clone();
    cx.new(|cx| {
        let data = {
            let scope = scope.clone();
            let field = field.clone();
            let store = store.clone();
            Box::new(move |filter: &str, window: &gpui::Window, cx: &gpui::App| {
                model_data(&scope, &field, &store, filter, window, cx)
            }) as Box<dyn Fn(&str, &gpui::Window, &gpui::App) -> PickerData>
        };
        let target = PickerTarget::Callback(Box::new({
            let field = field.clone();
            let variant_field = variant_field.clone();
            move |value, window, cx| {
                field.update(cx, |state, cx| state.set_value(value, window, cx));
                variant_field.update(cx, |state, cx| state.set_value("", window, cx));
            }
        }));
        let mut picker = FilteredPicker::new(spec, target, data, window, cx);
        picker.set_on_refresh(Box::new(move |cx| {
            let harness = refresh_scope.read(cx).harness.clone();
            refresh_store.update(cx, |store, cx| store.load_models_for(harness, cx));
        }));
        picker.observe_entity(store, cx);
        picker.observe_entity(field, cx);
        picker.observe_entity(scope, cx);
        picker
    })
}

/// The Variant picker: the selected model's variants (as reported by the
/// harness), written straight into the hidden variant field.
fn variant_picker(
    scope: &Entity<HarnessScope>,
    field: &Entity<InputState>,
    variant_field: &Entity<InputState>,
    store: &Entity<Store>,
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
) -> Entity<FilteredPicker> {
    let spec = picker_spec("Variant", "Variant", "Filter variants", false, false, "");
    cx.new(|cx| {
        let data = {
            let scope = scope.clone();
            let field = field.clone();
            let variant_field = variant_field.clone();
            let store = store.clone();
            Box::new(move |filter: &str, window: &gpui::Window, cx: &gpui::App| {
                variant_data(&scope, &field, &variant_field, &store, filter, window, cx)
            }) as Box<dyn Fn(&str, &gpui::Window, &gpui::App) -> PickerData>
        };
        let mut picker = FilteredPicker::new(
            spec,
            PickerTarget::Field(variant_field.clone()),
            data,
            window,
            cx,
        );
        picker.observe_entity(store, cx);
        picker.observe_entity(field, cx);
        picker.observe_entity(variant_field, cx);
        picker.observe_entity(scope, cx);
        picker
    })
}

/// One render's worth of the harness's model catalog: the server-default
/// row, provider groups and the filtered, deduped models.
fn model_data(
    scope: &Entity<HarnessScope>,
    field: &Entity<InputState>,
    store: &Entity<Store>,
    filter: &str,
    _window: &gpui::Window,
    cx: &gpui::App,
) -> PickerData {
    let harness = scope.read(cx).harness.clone();
    let selected = field.read(cx).value().trim().to_string();
    let filter = filter.to_lowercase();
    let mut models: Vec<String> = store
        .read(cx)
        .catalog
        .models
        .iter()
        .filter(|entry| entry.harness == harness)
        .map(|entry| entry.key.clone())
        .filter(|model| {
            !model.is_empty()
                && !model.starts_with('(')
                && (filter.is_empty() || model.to_lowercase().contains(&filter))
        })
        .collect();
    models.sort_by(|a, b| {
        model_provider(a)
            .to_lowercase()
            .cmp(&model_provider(b).to_lowercase())
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });
    models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut rows = Vec::with_capacity(models.len() + 4);
    rows.push(PickerRow::Item(PickerItem {
        id: "model-row-default".into(),
        selector: "model-row-default".into(),
        value: String::new(),
        label: "Default".to_string(),
        selected: selected.is_empty(),
        muted: false,
        trailing_delete: None,
    }));
    if models.is_empty() {
        rows.push(PickerRow::Message {
            id: "model-message".into(),
            label: "No models for this harness. Start warp-agent, then refresh.".to_string(),
        });
    } else {
        let mut last_provider = String::new();
        for model in models {
            let provider = model_provider(&model).to_string();
            if !provider.is_empty() && provider != last_provider {
                rows.push(PickerRow::Group {
                    id: format!("model-provider-{provider}"),
                    label: provider.to_uppercase(),
                    selector: None,
                });
                last_provider = provider;
            }
            rows.push(PickerRow::Item(PickerItem {
                id: format!("model-row-{model}"),
                selector: format!("model-row-{model}"),
                value: model.clone(),
                label: model_name(&model).to_string(),
                selected: model.eq_ignore_ascii_case(&selected),
                muted: false,
                trailing_delete: None,
            }));
        }
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            badge: (!model_provider(&selected).is_empty())
                .then(|| model_provider(&selected).to_string()),
            label: if selected.is_empty() {
                "Default".to_string()
            } else {
                model_name(&selected).to_string()
            },
            value: selected,
        },
        footer: None,
        fonts: theme::active_tokens(),
        accent: theme::blue_bright(),
        show_tooltips: store.read(cx).settings().general.show_tooltips,
    }
}

/// The selected model's variants for the current harness. An empty model
/// explains itself; a model without variants says so instead of showing an
/// empty list.
fn variant_data(
    scope: &Entity<HarnessScope>,
    field: &Entity<InputState>,
    variant_field: &Entity<InputState>,
    store: &Entity<Store>,
    filter: &str,
    _window: &gpui::Window,
    cx: &gpui::App,
) -> PickerData {
    let harness = scope.read(cx).harness.clone();
    let model = field.read(cx).value().trim().to_string();
    let selected = variant_field.read(cx).value().trim().to_string();
    let filter = filter.to_lowercase();
    let variants: Vec<String> = store
        .read(cx)
        .catalog
        .variants_for(&harness, &model)
        .into_iter()
        .filter(|variant| filter.is_empty() || variant.to_lowercase().contains(&filter))
        .collect();

    let mut rows = Vec::with_capacity(variants.len() + 2);
    rows.push(PickerRow::Item(PickerItem {
        id: "variant-row-default".into(),
        selector: "variant-row-default".into(),
        value: String::new(),
        label: "Default".to_string(),
        selected: selected.is_empty(),
        muted: false,
        trailing_delete: None,
    }));
    if model.is_empty() {
        rows.push(PickerRow::Message {
            id: "variant-message-pick-model".into(),
            label: "Pick a model to see its variants.".to_string(),
        });
    } else if variants.is_empty() {
        rows.push(PickerRow::Message {
            id: "variant-message-empty".into(),
            label: "This model reports no variants.".to_string(),
        });
    } else {
        for variant in variants {
            rows.push(PickerRow::Item(PickerItem {
                id: format!("variant-row-{variant}"),
                selector: format!("variant-row-{variant}"),
                value: variant.clone(),
                label: variant.clone(),
                selected: variant.eq_ignore_ascii_case(&selected),
                muted: false,
                trailing_delete: None,
            }));
        }
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            badge: None,
            label: if selected.is_empty() {
                "Default".to_string()
            } else {
                selected.clone()
            },
            value: selected,
        },
        footer: None,
        fonts: theme::active_tokens(),
        accent: theme::blue_bright(),
        show_tooltips: store.read(cx).settings().general.show_tooltips,
    }
}
