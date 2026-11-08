//! The Settings page: every settings section as a card of labeled inputs and
//! switches, seeded from the live settings when the page opens. Every
//! mutation autosaves: a keystroke or toggle schedules a debounced commit
//! that writes the whole graph atomically and live-applies (a scan-related
//! change also kicks a rescan), so no Save step exists. The AI card's
//! harness radio scopes the Model and Variant pickers to warp-agent's
//! catalog, and it manages the wand prompt templates.

use crate::net::TemplateOp;
use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{App, Context, Entity, FontWeight, SharedString, Window, div, px, relative};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, InputState, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station_core::settings::AppSettings;
use warpui_kit_core::settings as kit_settings;
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
    Notifications,
    Appearance,
    Repositories,
    Git,
    ExternalTools,
    Ai,
    Integrations,
}

/// The sections in list order: value, row id, title, what it holds.
const SECTIONS: [(Section, &str, &str, &str); 8] = [
    (
        Section::General,
        "settings-general",
        "General",
        "Startup and tooltips",
    ),
    (
        Section::Notifications,
        "settings-notifications",
        "Notifications",
        "Desktop toasts when unfocused",
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
    (Section::Git, "settings-git", "Git", "Branch naming"),
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
    // Notifications (the desktop mirror)
    notify_enabled: bool,
    notify_only_unfocused: bool,
    notify_errors: bool,
    notify_actions: bool,
    notify_fetch: bool,
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
    snapit_auto_start: bool,
    snapit_executable: Entity<InputState>,
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
    /// The pending debounced commit: a keystroke bumps it, and only the
    /// newest scheduled timer lands (see schedule_commit).
    commit_seq: u64,
    /// set by the header back button; MainWindow drains it and swaps pages
    section: Section,
}

/// Which background work a settings save must kick. Extracted so the policy is
/// testable without a window (audit B9): an unrelated edit (tooltips, a
/// palette, the branch prefix) must not start a full `gh`/Azure sweep, a model
/// reload or an autostart rewrite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SaveEffects {
    scan: bool,
    github: bool,
    azure: bool,
    models: bool,
    autostart: bool,
}

fn save_effects(before: &AppSettings, after: &AppSettings) -> SaveEffects {
    let scan = before.repos.repo_scan_folders != after.repos.repo_scan_folders
        || before.repos.excluded_folders != after.repos.excluded_folders
        || before.repos.git_folder_pattern != after.repos.git_folder_pattern
        || before.repos.solution_file_pattern != after.repos.solution_file_pattern
        || before.repos.platform_folder_name != after.repos.platform_folder_name
        || before.repos.max_scan_depth != after.repos.max_scan_depth;
    let github = before.repos.enable_github != after.repos.enable_github
        || before.repos.github_executable != after.repos.github_executable;
    let azure = before.repos.enable_azure_devops != after.repos.enable_azure_devops
        || before.repos.azure_devops_pat != after.repos.azure_devops_pat
        || before.repos.azure_devops_url != after.repos.azure_devops_url;
    let models = before.warp_agent.enable_warp_agent != after.warp_agent.enable_warp_agent
        || before.warp_agent.executable != after.warp_agent.executable
        || before.warp_agent.harness != after.warp_agent.harness
        || before.opencode.enable_opencode != after.opencode.enable_opencode
        || before.deepseek.enable_deepseek != after.deepseek.enable_deepseek;
    SaveEffects {
        scan,
        github,
        azure,
        models,
        autostart: before.general.start_at_boot != after.general.start_at_boot,
    }
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
            notify_enabled: settings.notifications.enabled,
            notify_only_unfocused: settings.notifications.only_when_unfocused,
            notify_errors: settings.notifications.errors,
            notify_actions: settings.notifications.action_confirmations,
            notify_fetch: settings.notifications.fetch_summaries,
            scan_folders: area(window, cx, &line_list(&repos.repo_scan_folders)),
            excluded_folders: area(window, cx, &line_list(&repos.excluded_folders)),
            git_folder_pattern: field(window, cx, &repos.git_folder_pattern),
            solution_file_pattern: field(window, cx, &repos.solution_file_pattern),
            platform_folder_name: field(window, cx, &repos.platform_folder_name),
            max_scan_depth: field(window, cx, &repos.max_scan_depth.to_string()),
            group_by_root: repos.group_repos_by_root.unwrap_or(false),
            group_depth: repos.group_root_depth.unwrap_or(1).clamp(1, 4) as usize,
            branch_name_prefix: field(window, cx, &repos.branch_name_prefix),
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
            snapit_auto_start: settings.snapit.auto_start,
            snapit_executable: field(window, cx, &settings.snapit.executable),
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
            enable_github: repos.enable_github,
            github_executable: field(window, cx, &repos.github_executable),
            enable_azure: repos.enable_azure_devops,
            azure_pat: field(window, cx, &repos.azure_devops_pat),
            azure_url: field(window, cx, &repos.azure_devops_url),
            saved_note: None,
            commit_seq: 0,
            section: Section::General,
        };
        // The page has no Save button: every text field schedules the
        // debounced commit on a keystroke (a picker pick writes through
        // set_value, which stays silent, so those hooks live in the pick
        // callbacks), and the toggles schedule one as they flip.
        for field in [
            &page.notes_store_path,
            &page.git_folder_pattern,
            &page.solution_file_pattern,
            &page.platform_folder_name,
            &page.max_scan_depth,
            &page.branch_name_prefix,
            &page.vscode_executable,
            &page.vscode_profile,
            &page.terminal_executable,
            &page.ide_executable,
            &page.opencode_executable,
            &page.deepseek_executable,
            &page.code_search_executable,
            &page.snapit_executable,
            &page.warp_executable,
            &page.model,
            &page.variant,
            &page.warp_notes_executable,
            &page.github_executable,
            &page.azure_pat,
            &page.azure_url,
        ] {
            cx.subscribe(field, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_commit(cx);
                }
            })
            .detach();
        }
        for area in [&page.scan_folders, &page.excluded_folders] {
            cx.subscribe(area, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_commit(cx);
                }
            })
            .detach();
        }
        store.update(cx, |s, cx| s.load_models_for(selected_harness, false, cx));
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
        let notifications = (
            self.notify_enabled,
            self.notify_only_unfocused,
            self.notify_errors,
            self.notify_actions,
            self.notify_fetch,
        );
        let snapit_auto_start = self.snapit_auto_start;
        let snapit_exe = Self::value(&self.snapit_executable, cx);
        let warp_enabled = self.warp_enabled;
        let warp_exe = Self::value(&self.warp_executable, cx);
        let variant = Self::value(&self.variant, cx);
        let warp_notes_enabled = self.warp_notes_enabled;
        let warp_notes_exe = Self::value(&self.warp_notes_executable, cx);
        let branch_prefix = Self::value(&self.branch_name_prefix, cx);
        let group_by_root = self.group_by_root;
        let group_depth = self.group_depth;
        let code_search_enabled = self.enable_code_search;
        let code_search_exe = Self::value(&self.code_search_executable, cx);

        // one snapshot for the whole save: the effects below compare it with
        // the settled settings after the write
        let before = self.store.read(cx).settings();
        let autostart_changed = before.general.start_at_boot != self.start_at_boot;

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
                s.repos.enable_github = flags.7;
                s.repos.github_executable = github_exe;
                s.repos.enable_azure_devops = flags.8;
                s.repos.azure_devops_pat = azure_pat;
                s.repos.azure_devops_url = azure_url;
                s.snapit.auto_start = snapit_auto_start;
                s.snapit.executable = snapit_exe;
                s.notifications.enabled = notifications.0;
                s.notifications.only_when_unfocused = notifications.1;
                s.notifications.errors = notifications.2;
                s.notifications.action_confirmations = notifications.3;
                s.notifications.fetch_summaries = notifications.4;
            });
            // grouping only reshapes the list: re-project, never rescan —
            // the repos and their folders did not change
            store.set_group_by_root(group_by_root);
            store.set_group_depth(group_depth);
            // A save only kicks the work its fields actually affect: a
            // tooltip or branch-prefix edit must not sweep every repo with gh
            // or reload the model catalog (audit B9).
            let after = store.settings();
            let effects = save_effects(&before, &after);
            if effects.scan {
                store.kick_scan(&after);
            }
            if effects.github || effects.azure {
                let folders: Vec<std::path::PathBuf> = store
                    .repos
                    .iter()
                    .filter(|repo| !repo.is_archived())
                    .map(|r| r.folder_path.clone())
                    .collect();
                if effects.github {
                    store.kick_github_pass(&folders);
                }
                if effects.azure {
                    store.kick_azure_pass(&folders);
                }
            }
            if effects.models {
                store.load_models_for(after.warp_agent.harness.clone(), false, cx);
            }
            cx.notify();
        });
        // a tool installed or moved while the app runs resolves right away
        warp_station_core::exe::invalidate();
        // the boot registration follows the saved flag, but only a change to
        // it rewrites the OS entry
        if autostart_changed {
            warp_station_core::autostart::reconcile(self.start_at_boot);
        }
        // so does the execution state; a failure leaves the chip as the truth
        self.saved_note = Some(std::time::Instant::now());
        cx.notify();
    }

    /// One commit a beat after the last keystroke: save() re-reads the whole
    /// page on every schedule but only the newest timer lands, so typing
    /// costs one settings write and the page never asks for a Save press.
    /// The toggles and pickers commit at once — they are discrete actions.
    fn schedule_commit(&mut self, cx: &mut Context<Self>) {
        self.commit_seq += 1;
        let seq = self.commit_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(600))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.commit_seq == seq {
                    this.save(cx);
                }
            });
        })
        .detach();
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
            // eight sections do not fit a short window; the selected section
            // must stay reachable instead of being clipped by the pane
            .child(
                div()
                    .id("settings-sections-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(rows),
            );

        let (title, hint) = SECTIONS
            .iter()
            .find(|entry| entry.0 == section)
            .map_or(("Settings", ""), |entry| (entry.2, entry.3));
        let head = div()
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px(px(warpui_kit_core::shell::DETAIL_PADDING))
            .py_4()
            .border_b_1()
            .border_color(theme::hairline())
            .child(
                // the title column takes what is left and never more: without
                // `overflow_hidden` a long hint sets its intrinsic width, the
                // row grows past the pane, and Save runs off the window edge
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(theme::font_screen_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_0p5()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_dim())
                            .whitespace_normal()
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
            // Appearance applies at once and has nothing to save, so its head
            // carries no actions (the prototype: `plain = k === 'appearance'`
            // drops the Data folder button; the Save button is gone — every
            // mutation commits itself)
            .when(section != Section::Appearance, |row| {
                row.child(
                    div()
                        .id("settings-open-folder")
                        .debug_selector(|| "settings-open-folder".into())
                        .a11y_button("Open data folder")
                        .cursor_pointer()
                        .flex_shrink_0()
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
                            // this app's own data, not the whole suite root
                            let folder = warp_station_core::paths::AppPaths::app_dir();
                            let _ = std::fs::create_dir_all(&folder);
                            let _ =
                                warp_station_core::terminal::open_folder(&folder.to_string_lossy());
                        })
                        .child(crate::ui::icon(
                            "icons/icon-folder.svg",
                            px(theme::icon_small()),
                            theme::text_dim(),
                        ))
                        .child("Data folder"),
                )
            });

        let rows = self.section_rows(section, window, cx);

        warpui_kit_core::shell::panes().child(list).child(
            warpui_kit_core::shell::detail_pane()
                .debug_selector(|| "settings-detail".into())
                .child(head)
                .child(
                    div()
                        .id("settings-scroll")
                        .flex_1()
                        // the card column measures against the pane, not
                        // against its own widest child: without these the
                        // Git card ran to the window edge
                        .w_full()
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(warpui_kit_core::shell::DETAIL_PADDING))
                        .child(
                            // the design's `.measure`: running content reads
                            // at 760, and the page is rows, not cards
                            div()
                                .w_full()
                                .max_w(px(warpui_kit_core::shell::READING_WIDTH))
                                .child(kit_settings::rows(rows)),
                        ),
                ),
        )
    }
}

impl SettingsPage {
    /// One section's rows: the design's flat `.setting` list, not cards.
    ///
    /// `DESIGN.md`: "one row per setting, label left and control right, 60
    /// high". The prototype draws the whole page as one `.measure` column of
    /// rows with a `line-soft` rule between them, so the sections here return
    /// rows and never a card.
    fn section_rows(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        match section {
            Section::General => {
                rows.push(kit_settings::switch_row(
                    "set-start-at-boot",
                    "Start at boot",
                    "settings-row-Start at boot".to_string(),
                    self.start_at_boot,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.start_at_boot = !this.start_at_boot;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row(
                    "set-start-minimized",
                    "Start minimized",
                    "settings-row-Start minimized".to_string(),
                    self.start_minimized,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.start_minimized = !this.start_minimized;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row(
                    "set-tooltips",
                    "Show tooltips",
                    "settings-row-Show tooltips".to_string(),
                    self.show_tooltips,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.show_tooltips = !this.show_tooltips;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "Notes store path",
                    &self.notes_store_path,
                    rows.is_empty(),
                    window,
                    cx,
                ));
            }
            Section::Notifications => {
                // the desktop mirror's master switch and its two dimensions:
                // when (the unfocused gate) and what (the three categories).
                // Every flip autosaves like the other sections'; the only side
                // effect is the copy, so save() carries no kicks for it.
                rows.push(kit_settings::switch_row_with_desc(
                    "set-notify-enabled",
                    "Desktop notifications",
                    "settings-row-Desktop notifications".to_string(),
                    "Mirrors the app's toasts to the desktop",
                    self.notify_enabled,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.notify_enabled = !this.notify_enabled;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row_with_desc(
                    "set-notify-unfocused",
                    "Only while unfocused",
                    "settings-row-Only while unfocused".to_string(),
                    "Nothing reaches the desktop while a Warp Station window has focus",
                    self.notify_only_unfocused,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.notify_only_unfocused = !this.notify_only_unfocused;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::subhead("What to hear about"));
                rows.push(kit_settings::switch_row(
                    "set-notify-errors",
                    "Errors",
                    "settings-row-Errors".to_string(),
                    self.notify_errors,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.notify_errors = !this.notify_errors;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row_with_desc(
                    "set-notify-actions",
                    "Action confirmations",
                    "settings-row-Action confirmations".to_string(),
                    "Merges, commits, agent handoffs, generation drafts, password copies",
                    self.notify_actions,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.notify_actions = !this.notify_actions;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row_with_desc(
                    "set-notify-fetch",
                    "Fetch summaries",
                    "settings-row-Fetch summaries".to_string(),
                    "One aggregate after a background fetch brings repos further behind — desktop only",
                    self.notify_fetch,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.notify_fetch = !this.notify_fetch;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
            }
            Section::Appearance => {
                // the three appearance rows apply at once and carry the
                // design's one-line explanation under the label
                rows.push(kit_settings::row_with_desc(
                    "settings-row-Palette".to_string(),
                    "Palette",
                    "Shared by every app in the suite",
                    warpui_kit_core::palette_control(),
                    rows.is_empty(),
                ));
                rows.push(kit_settings::row_with_desc(
                    "settings-row-Text size".to_string(),
                    "Text size",
                    "Also shared by every app",
                    warpui_kit_core::text_size_control(),
                    rows.is_empty(),
                ));
                let reduced = self.store.read(cx).settings().general.reduce_motion;
                let store = self.store.clone();
                rows.push(kit_settings::switch_row_with_desc(
                    "set-reduce-motion",
                    "Reduce motion",
                    "settings-row-Reduce motion".to_string(),
                    "Turns off transitions and keeps spinners still. Busy labels stay",
                    reduced,
                    rows.is_empty(),
                    move |_, _, cx| {
                        store.update(cx, |s, cx| {
                            s.set_reduce_motion(!reduced, cx);
                        });
                        cx.refresh_windows();
                    },
                ));
            }
            Section::Repositories => {
                // the prototype's `.setting.stack`: the label over a full-width
                // `.area` (a folder list is a block, not a 340-wide value)
                rows.push(kit_settings::stack_row(
                    "settings-row-Scan folders".to_string(),
                    "Scan folders",
                    warpui_kit_core::field::mono_area(
                        "field-scan-folders",
                        &self.scan_folders,
                        window,
                        cx,
                    )
                    .h(px(96.)),
                    rows.is_empty(),
                ));
                rows.push(kit_settings::stack_row(
                    "settings-row-Excluded folders".to_string(),
                    "Excluded folders",
                    warpui_kit_core::field::mono_area(
                        "field-excluded-folders",
                        &self.excluded_folders,
                        window,
                        cx,
                    )
                    .h(px(64.)),
                    rows.is_empty(),
                ));
                rows.push(field_row(
                    "Git folder pattern",
                    &self.git_folder_pattern,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "Solution file pattern",
                    &self.solution_file_pattern,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "Platform folder name",
                    &self.platform_folder_name,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "Max scan depth",
                    &self.max_scan_depth,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::switch_row(
                    "set-group-by-root",
                    "Group repos by folder",
                    "settings-row-Group repos by folder".to_string(),
                    self.group_by_root,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.group_by_root = !this.group_by_root;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                if self.group_by_root {
                    rows.push(group_depth_row(self, cx, rows.is_empty()));
                }
            }
            Section::Git => {
                rows.push(field_row(
                    "Branch name prefix",
                    &self.branch_name_prefix,
                    rows.is_empty(),
                    window,
                    cx,
                ));
            }
            Section::ExternalTools => {
                rows.push(kit_settings::switch_row(
                    "set-vs",
                    "Enable Visual Studio (Windows)",
                    "settings-row-Enable Visual Studio (Windows)".to_string(),
                    self.enable_visual_studio,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_visual_studio = !this.enable_visual_studio;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(kit_settings::switch_row(
                    "set-vscode",
                    "Enable VS Code",
                    "settings-row-Enable VS Code".to_string(),
                    self.enable_vscode,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_vscode = !this.enable_vscode;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "VS Code executable",
                    &self.vscode_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "VS Code profile",
                    &self.vscode_profile,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::switch_row(
                    "set-terminal",
                    "Enable Terminal",
                    "settings-row-Enable Terminal".to_string(),
                    self.enable_terminal,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_terminal = !this.enable_terminal;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "Terminal executable",
                    &self.terminal_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "IDE executable",
                    &self.ide_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::switch_row(
                    "set-code-search",
                    "Cross-repo code search",
                    "settings-row-Cross-repo code search".to_string(),
                    self.enable_code_search,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_code_search = !this.enable_code_search;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "ripgrep executable",
                    &self.code_search_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                if cfg!(windows) {
                    rows.push(kit_settings::switch_row(
                        "set-snapit",
                        "Start SnapIt automatically (Windows)",
                        "settings-row-Start SnapIt automatically (Windows)".to_string(),
                        self.snapit_auto_start,
                        rows.is_empty(),
                        cx.listener(|this, _, _, cx| {
                            this.snapit_auto_start = !this.snapit_auto_start;
                            this.save(cx);
                            cx.notify();
                        }),
                    ));
                    rows.push(field_row(
                        "SnapIt executable",
                        &self.snapit_executable,
                        rows.is_empty(),
                        window,
                        cx,
                    ));
                }
                rows.push(kit_settings::subhead("OpenCode"));
                rows.push(kit_settings::switch_row(
                    "set-opencode",
                    "Enable OpenCode",
                    "settings-row-Enable OpenCode".to_string(),
                    self.enable_opencode,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_opencode = !this.enable_opencode;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "OpenCode executable",
                    &self.opencode_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::subhead("DeepSeek"));
                rows.push(kit_settings::switch_row(
                    "set-deepseek",
                    "Enable DeepSeek",
                    "settings-row-Enable DeepSeek".to_string(),
                    self.enable_deepseek,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_deepseek = !this.enable_deepseek;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "DeepSeek executable",
                    &self.deepseek_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
            }
            Section::Ai => {
                rows.push(kit_settings::switch_row(
                    "set-warp-agent",
                    "Enable warp-agent",
                    "settings-row-Enable warp-agent".to_string(),
                    self.warp_enabled,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.warp_enabled = !this.warp_enabled;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "warp-agent executable",
                    &self.warp_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(self.harness_row(cx, rows.is_empty()));
                rows.push(kit_settings::row(
                    "settings-row-Model".to_string(),
                    "Model",
                    div()
                        .w(px(320.))
                        .flex_shrink_0()
                        .child(self.model_picker.clone()),
                    rows.is_empty(),
                ));
                rows.push(kit_settings::row(
                    "settings-row-Variant".to_string(),
                    "Variant",
                    div()
                        .w(px(240.))
                        .flex_shrink_0()
                        .child(self.variant_picker.clone()),
                    rows.is_empty(),
                ));
                rows.push(kit_settings::subhead("Prompt templates"));
                rows.push(self.templates_section(cx));
            }
            Section::Integrations => {
                rows.push(kit_settings::switch_row(
                    "set-notes",
                    "Enable warp-notes",
                    "settings-row-Enable warp-notes".to_string(),
                    self.warp_notes_enabled,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.warp_notes_enabled = !this.warp_notes_enabled;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "warp-notes executable",
                    &self.warp_notes_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::switch_row(
                    "set-github",
                    "GitHub",
                    "settings-row-GitHub".to_string(),
                    self.enable_github,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_github = !this.enable_github;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "GitHub CLI executable",
                    &self.github_executable,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(kit_settings::switch_row(
                    "set-azure",
                    "Azure DevOps",
                    "settings-row-Azure DevOps".to_string(),
                    self.enable_azure,
                    rows.is_empty(),
                    cx.listener(|this, _, _, cx| {
                        this.enable_azure = !this.enable_azure;
                        this.save(cx);
                        cx.notify();
                    }),
                ));
                rows.push(field_row(
                    "Azure DevOps PAT",
                    &self.azure_pat,
                    rows.is_empty(),
                    window,
                    cx,
                ));
                rows.push(field_row(
                    "Azure DevOps server URL",
                    &self.azure_url,
                    rows.is_empty(),
                    window,
                    cx,
                ));
            }
        }
        rows
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

fn field_row(
    label_text: &str,
    field: &Entity<InputState>,
    first: bool,
    window: &Window,
    cx: &App,
) -> gpui::AnyElement {
    // a stable per-row selector so probes can reach the field through its row
    let selector = format!("settings-field-{label_text}");
    let control = div()
        .debug_selector(move || selector.clone())
        // the design's `.setting .field`: 340 wide, never past 46% of the
        // measure, so the control shrinks with the pane
        .w(px(340.))
        .max_w(px(0.46 * warpui_kit_core::shell::READING_WIDTH))
        .min_w_0()
        .child(warpui_kit_core::field::custom(
            "settings-field",
            field,
            window,
            cx,
            move |input| {
                input
                    .aria_label(label_text)
                    .font_family(SharedString::from(theme::code_font()))
                    .text_size(px(theme::font_mono_meta()))
            },
        ));
    kit_settings::row(
        format!("settings-field-row-{label_text}"),
        label_text,
        control,
        first,
    )
}

/// The group-depth picker: 1/2/3/4 as the design's segmented control. Only
/// rendered while grouping is on.
fn group_depth_row(
    this: &SettingsPage,
    cx: &mut Context<SettingsPage>,
    first: bool,
) -> gpui::AnyElement {
    let options = (1..=4usize)
        .map(|level| {
            kit_settings::Choice::new(
                level.to_string(),
                level.to_string(),
                this.group_depth == level,
            )
        })
        .collect();
    let entity = cx.entity();
    let control =
        kit_settings::segmented("group-depth", options, move |key, _, cx: &mut gpui::App| {
            let level = key.parse::<usize>().unwrap_or(1);
            entity.update(cx, |this, cx| {
                this.group_depth = level;
                this.save(cx);
                cx.notify();
            });
        });
    kit_settings::row("group-depth-row", "Group depth", control, first)
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
            .update(cx, |store, cx| store.load_models_for(harness, false, cx));
        self.schedule_commit(cx);
        cx.notify();
    }

    /// The harness picker: the design's `.seg` over whatever harnesses the
    /// door reports (the prototype draws Harness as a segmented control, not a
    /// radio list).
    fn harness_row(&self, cx: &mut Context<Self>, first: bool) -> gpui::AnyElement {
        let harnesses = self.store.read(cx).harnesses.clone();
        let selected = self.harness_scope.read(cx).harness.clone();
        let choices = harnesses
            .iter()
            .map(|harness| {
                kit_settings::Choice::new(
                    harness.id.clone(),
                    harness.label.clone(),
                    harness.id == selected,
                )
            })
            .collect();
        let entity = cx.entity();
        let control = kit_settings::segmented(
            "warp-harness",
            choices,
            move |key, window, cx: &mut gpui::App| {
                entity.update(cx, |this, cx| {
                    this.pick_harness(key.to_string(), window, cx);
                });
            },
        );
        kit_settings::row("settings-row-Harness", "Harness", control, first)
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
    // a pick lands through set_value, which emits no Change event: the
    // callback schedules the page's autosave commit itself
    let page = cx.entity();
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
            let page = page.clone();
            move |value, window, cx| {
                field.update(cx, |state, cx| state.set_value(value, window, cx));
                variant_field.update(cx, |state, cx| state.set_value("", window, cx));
                page.update(cx, |this, cx| this.save(cx));
            }
        }));
        let mut picker = FilteredPicker::new(spec, target, data, window, cx);
        picker.set_on_refresh(Box::new(move |cx| {
            let harness = refresh_scope.read(cx).harness.clone();
            refresh_store.update(cx, |store, cx| store.load_models_for(harness, true, cx));
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
    // the pick lands through set_value (no Change event), so this picker
    // writes and schedules the page's autosave commit in one callback
    let page = cx.entity();
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
            PickerTarget::Callback(Box::new({
                let variant_field = variant_field.clone();
                let page = page.clone();
                move |value, window, cx| {
                    variant_field.update(cx, |state, cx| state.set_value(value, window, cx));
                    page.update(cx, |this, cx| this.save(cx));
                }
            })),
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
    let refreshing = store.read(cx).models_refresh_requested.as_deref() == Some(harness.as_str());
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
    // one lowercase allocation per model, not per comparison
    models.sort_by_cached_key(|model| (model_provider(model).to_lowercase(), model.to_lowercase()));
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
        refreshing,
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
        refreshing: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unrelated save (the branch prefix, tooltips, text size) must not
    /// kick a scan, an activity sweep, a model reload or an autostart rewrite.
    #[test]
    fn unrelated_saves_kick_nothing() {
        let before = AppSettings::default();
        let mut after = before.clone();
        after.repos.branch_name_prefix = "feat/".into();
        after.general.show_tooltips = false;
        assert_eq!(save_effects(&before, &after), SaveEffects::default());
    }

    #[test]
    fn each_setting_kicks_only_its_own_work() {
        let before = AppSettings::default();

        let mut scan = before.clone();
        scan.repos.repo_scan_folders = Some(vec!["/work".into()]);
        assert_eq!(
            save_effects(&before, &scan),
            SaveEffects {
                scan: true,
                ..SaveEffects::default()
            }
        );

        let mut github = before.clone();
        github.repos.github_executable = "gh-custom".into();
        assert_eq!(
            save_effects(&before, &github),
            SaveEffects {
                github: true,
                ..SaveEffects::default()
            }
        );

        let mut azure = before.clone();
        azure.repos.azure_devops_pat = "token".into();
        assert_eq!(
            save_effects(&before, &azure),
            SaveEffects {
                azure: true,
                ..SaveEffects::default()
            }
        );

        let mut models = before.clone();
        models.warp_agent.harness = "dsh".into();
        assert_eq!(
            save_effects(&before, &models),
            SaveEffects {
                models: true,
                ..SaveEffects::default()
            }
        );

        let mut boot = before.clone();
        boot.general.start_at_boot = true;
        assert_eq!(
            save_effects(&before, &boot),
            SaveEffects {
                autostart: true,
                ..SaveEffects::default()
            }
        );
    }

    /// The scan pattern and exclusion inputs are scan effects too; the old
    /// save path only watched the folder list.
    #[test]
    fn scan_inputs_rescan() {
        let before = AppSettings::default();
        let mut after = before.clone();
        after.repos.git_folder_pattern = "*.repo".into();
        after.repos.excluded_folders = Some(vec!["vendor".into()]);
        let effects = save_effects(&before, &after);
        assert!(effects.scan);
        assert!(!effects.github && !effects.azure && !effects.models && !effects.autostart);
    }
}
