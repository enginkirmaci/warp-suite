//! The Changes and History tabs: the selected repo's commit workspace. Changes
//! is the git toolbar (branch, Fetch, Pull, Push, Sync) over the file list and
//! commit box on the left and the selected file's diff on the right. History is
//! the day-grouped commits on the left and the selected commit's head band over
//! a file rail and that file's diff on the right. The heavy data lives on the
//! store; this view owns only ephemeral state (the selected file, fetched
//! patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::Store;
use crate::theme;
use crate::ui::diff_controls;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px, uniform_list};
use gpui_kit::component::input::{InputState, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station_core::model::GitChangedFile;
use warp_station_core::settings::DiffViewMode;
use warpui_kit_diff::{DiffScrollHandles, render_diff_with_scroll};
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData, PickerIcons, PickerItem,
    PickerRow, PickerSpec, PickerTarget, Selection, TrailingDelete, TriggerScale,
};

/// One history row's display strings, precomputed when the history (or the
/// day) changes so a render never re-derives them per row.
#[derive(Clone)]
struct HistoryRow {
    /// Some(..) on the first commit of a day: the group header label.
    day_label: Option<String>,
    initials: String,
    author: String,
    rel: String,
    short_hash: String,
    subject: String,
    hash: String,
}

/// The staged/unstaged split produced by `GitChangeSnapshot::change_groups`.
type GroupedFiles = (Vec<GitChangedFile>, Vec<GitChangedFile>);

/// One snapshot's grouped split plus the flat unstaged path list the header's
/// discard-all hands to git. Both are built once per generation: a 10k-file
/// tree must not recollect its paths on every repaint.
struct FileGroups {
    groups: GroupedFiles,
    unstaged_paths: Rc<Vec<String>>,
}

/// Which of the file list's two sections a header folds.
#[derive(Clone, Copy)]
enum FileSection {
    Staged,
    Unstaged,
}

/// One fixed-height item of the virtualized file list: a section header or a
/// row on one of the two sides.
#[derive(Clone, Copy)]
enum ChangeListItem {
    StagedHeader,
    StagedFile(usize),
    UnstagedHeader,
    UnstagedFile(usize),
}

impl FileSection {
    fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged Changes",
            Self::Unstaged => "Unstaged Changes",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Staged => "changes-section-staged",
            Self::Unstaged => "changes-section-unstaged",
        }
    }
}

/// One history row: a commit, with its day header on the row above the first
/// of each day. Fixed so `uniform_list` can size the whole list.
const HISTORY_ROW_HEIGHT: f32 = 58.;
/// The file list and commit box's width.
const FILE_COLUMN_W: f32 = 320.;
/// The file list's row height. Section headers match it, so `uniform_list`
/// can size the whole tree from one measured item.
const FILE_ROW_H: f32 = 34.;
/// The commit list's width.
const HISTORY_COLUMN_W: f32 = 320.;
/// The drilled commit's file rail, and the height of its group header and rows.
const COMMIT_RAIL_W: f32 = 240.;
const COMMIT_RAIL_ROW_H: f32 = 34.;
/// The git toolbar's height.
const TOOLBAR_H: f32 = 52.;
/// The branch selector's width.
const BRANCH_W: f32 = 260.;
/// The commit box's height: the message over its button.
const COMMIT_BOX_H: f32 = 190.;

pub struct ChangesView {
    pub store: Entity<Store>,
    /// The (path, staged) row the diff shows; its patch loads on selection.
    selected_file: Option<(String, bool)>,
    /// The persisted diff layout; global, not per repository.
    diff_view: DiffViewMode,
    /// Whether long diff rows wrap; persisted with the layout.
    diff_wrap: bool,
    /// The diff's horizontal scroll handles, kept so the pinned bars work.
    diff_scroll: DiffScrollHandles,
    file_by_repo: std::collections::HashMap<String, (String, bool)>,
    /// The row whose discard took its first click; the second one discards.
    discard_armed: Option<(String, bool)>,
    /// The unstaged header's discard-all has been clicked once: the second
    /// click throws the whole working tree away.
    discard_all_armed: bool,
    /// The file list's two sections fold independently; their headers and
    /// whole-tree verbs stay visible while the rows hide.
    staged_collapsed: bool,
    unstaged_collapsed: bool,
    patches: std::collections::HashMap<(String, bool), String>,
    /// The branch selector: the shared FilteredPicker in branch mode.
    branch_picker: Entity<FilteredPicker>,
    /// The branch picker's ephemeral state (armed delete, open form); shared
    /// with the picker's closures.
    branch_ui: Rc<RefCell<BranchUi>>,
    commit_message: Entity<TextareaState>,
    history_open: bool,
    /// Hash of the drilled-in commit (None = the list view).
    drilled_commit: Option<String>,
    /// Armed destructive drill-down action ("checkout"/"revert") — the second
    /// click confirms the arm.
    detail_armed: Option<&'static str>,
    detail_patches: std::collections::HashMap<String, String>,
    /// Keys (`hash|path`) whose patch is already on the wire or landed; stops
    /// a rail selection from re-asking on every render.
    detail_requests: std::collections::HashSet<String>,
    /// The file the drilled commit's rail shows; reset when the drill moves.
    commit_file: usize,
    /// The repo the view's local state belongs to; a mismatch resets it all.
    loaded_for: Option<String>,
    /// Staged/unstaged groups, rebuilt only when the store's change snapshot
    /// generation moves (any store notify otherwise re-cloned everything).
    groups_cache: Option<(u64, Rc<FileGroups>)>,
    /// Precomputed history row labels, keyed by history generation + today.
    history_rows: Vec<HistoryRow>,
    history_cache_key: (u64, chrono::NaiveDate),
}

impl ChangesView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("branch name"));
        let branch_ui: Rc<RefCell<BranchUi>> = Rc::new(RefCell::new(BranchUi::default()));
        let branch_picker = build_branch_picker(
            &store,
            branch_ui.clone(),
            new_branch_name.clone(),
            window,
            cx,
        );
        // the picker's open popup is appended by the toolbar, so picker-internal changes (filter typing, arming,
        // dismissal) must repaint this view too
        cx.observe(&branch_picker, |_, _, cx| cx.notify()).detach();
        let commit_message =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe your changes…"));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let diff_view = store.read(cx).diff_view();
        let diff_wrap = store.read(cx).diff_wrap();
        Self {
            store,
            selected_file: None,
            diff_view,
            diff_wrap,
            diff_scroll: DiffScrollHandles::default(),
            file_by_repo: std::collections::HashMap::new(),
            discard_armed: None,
            discard_all_armed: false,
            staged_collapsed: false,
            unstaged_collapsed: false,
            patches: std::collections::HashMap::new(),
            branch_picker,
            branch_ui,
            commit_message,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            detail_requests: std::collections::HashSet::new(),
            commit_file: 0,
            loaded_for: None,
            groups_cache: None,
            history_rows: Vec::new(),
            history_cache_key: (0, chrono::NaiveDate::MIN),
        }
    }

    /// One-time reset when the selected repo changed: the tab must never show
    /// the previous repo's draft, selection, menu or drill-down state.
    fn sync_repo_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.store.read(cx).selected_repo.clone();
        if self.loaded_for == repo {
            return;
        }
        self.loaded_for = repo;
        self.selected_file = None;
        self.discard_armed = None;
        self.discard_all_armed = false;
        self.staged_collapsed = false;
        self.unstaged_collapsed = false;
        self.patches.clear();
        self.branch_picker.update(cx, FilteredPicker::dismiss);
        {
            // the branch popup must never show the previous repo's list,
            // filter or form
            let mut ui = self.branch_ui.borrow_mut();
            ui.delete_armed = None;
            ui.new_branch_open = false;
        }
        self.branch_picker
            .update(cx, |picker, cx| picker.clear_filter(window, cx));
        self.commit_message
            .update(cx, |f, cx| f.set_value("", window, cx));
        self.history_open = false;
        self.drilled_commit = None;
        self.detail_armed = None;
        self.detail_patches.clear();
        self.detail_requests.clear();
        self.commit_file = 0;
    }

    /// Closes the branch picker from an outside click or Escape. Returns
    /// whether it was actually open so the window can stop propagating.
    pub fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.branch_picker.update(cx, FilteredPicker::dismiss)
    }

    /// Folds or unfolds one of the file list's sections.
    fn toggle_section(&mut self, section: FileSection, cx: &mut Context<Self>) {
        match section {
            FileSection::Staged => self.staged_collapsed = !self.staged_collapsed,
            FileSection::Unstaged => self.unstaged_collapsed = !self.unstaged_collapsed,
        }
        cx.notify();
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history().is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        self.commit_file = 0;
        if needs_load && let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.store.update(cx, |store, cx| {
                store.start_history_pass();
                cx.notify();
            });
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::LoadHistory {
                    folder: std::path::PathBuf::from(folder),
                });
        }
        cx.notify();
    }

    /// Drills into one commit's detail.
    pub fn drill_commit(&mut self, hash: &str, cx: &mut Context<Self>) {
        self.drilled_commit = Some(hash.to_string());
        self.commit_file = 0;
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::GetCommitDetails {
                    folder: std::path::PathBuf::from(folder),
                    hash: hash.to_string(),
                });
        }
        cx.notify();
    }

    /// Asks for one commit file's patch, once per key.
    fn request_commit_patch(&mut self, hash: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{hash}|{path}");
        if self.detail_patches.contains_key(&key) {
            return;
        }
        let Some(folder) = self.store.read(cx).selected_repo.clone() else {
            return;
        };
        if !self.detail_requests.insert(key) {
            return;
        }
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(NetCommand::GetCommitFilePatch {
                folder: std::path::PathBuf::from(folder),
                hash: hash.to_string(),
                path: path.to_string(),
            });
    }

    /// Selects one file on the drilled commit's rail and loads its diff.
    fn select_commit_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.commit_file = index;
        self.ensure_commit_file_selected(cx);
        cx.notify();
    }

    /// Keeps the drilled commit's rail selection on a live file: the first
    /// one stands in until the user picks another, and the patch it needs is
    /// put on the wire here (renders stay idempotent).
    fn ensure_commit_file_selected(&mut self, cx: &mut Context<Self>) {
        let Some(hash) = self.drilled_commit.clone() else {
            return;
        };
        let (detail_hash, path) = {
            let store = self.store.read(cx);
            let Some(details) = store.commit_details.as_ref() else {
                return;
            };
            if !details.hash.starts_with(&hash[..hash.len().min(7)]) {
                return;
            }
            if details.files.is_empty() {
                self.commit_file = 0;
                return;
            }
            if self.commit_file >= details.files.len() {
                self.commit_file = 0;
            }
            (
                details.hash.clone(),
                details.files[self.commit_file].path.clone(),
            )
        };
        self.request_commit_patch(&detail_hash, &path, cx);
    }

    /// Shows a file's diff, loading its patch the first time.
    fn select_file(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        let key = (path.to_string(), staged);
        self.selected_file = Some(key.clone());
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.file_by_repo.insert(folder, key.clone());
        }
        if !self.patches.contains_key(&key)
            && let Some(folder) = self.store.read(cx).selected_repo.clone()
        {
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::GetChangePatch {
                    folder: std::path::PathBuf::from(folder),
                    path: path.to_string(),
                    staged,
                });
        }
        cx.notify();
    }

    /// Keeps a live row selected: the first file stands in until the user picks
    /// another, and again when the selected one leaves the lists.
    fn ensure_file_selected(&mut self, groups: &GroupedFiles, cx: &mut Context<Self>) {
        let (staged, unstaged) = groups;
        let live = self
            .selected_file
            .as_ref()
            .is_some_and(|(path, is_staged)| {
                let side = if *is_staged { staged } else { unstaged };
                side.iter().any(|file| &file.path == path)
            });
        if live {
            return;
        }
        self.selected_file = None;
        // each repository keeps the file it was left on
        let kept = self
            .store
            .read(cx)
            .selected_repo
            .as_ref()
            .and_then(|folder| self.file_by_repo.get(folder))
            .filter(|(path, is_staged)| {
                let side = if *is_staged { staged } else { unstaged };
                side.iter().any(|file| &file.path == path)
            })
            .cloned();
        let first = kept
            .or_else(|| unstaged.first().map(|file| (file.path.clone(), false)))
            .or_else(|| staged.first().map(|file| (file.path.clone(), true)));
        if let Some((path, is_staged)) = first {
            self.select_file(&path, is_staged, cx);
        }
    }
}

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.sync_repo_reset(window, cx);
        // the History tab and the Changes tab are this one view
        let want_history =
            self.store.read(cx).active_detail_tab() == crate::store::WorkspaceTab::History;
        if want_history != self.history_open {
            if want_history {
                self.open_history(cx);
            } else {
                self.history_open = false;
                self.drilled_commit = None;
            }
        }
        // drain freshly loaded patches + the commit wand's draft; patches are
        // \r-stripped once here so the diff renderer never rewrites the text
        // per render
        if let Some((path, staged, patch)) = self
            .store
            .update(cx, |store, _| store.change_patch_result.take())
        {
            self.patches.insert((path, staged), patch.replace('\r', ""));
        }
        // a settled commit empties the message box (before the draft drain so
        // a wand draft landing in the same pass survives)
        if self.store.update(cx, |store, _| {
            std::mem::take(&mut store.commit_message_clear)
        }) {
            self.commit_message
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if let Some(draft) = self
            .store
            .update(cx, |store, _| store.commit_message_draft.take())
        {
            let draft = draft.trim().to_string();
            self.commit_message
                .update(cx, |state, cx| state.set_value(draft.as_str(), window, cx));
        }
        for (key, patch) in self.store.update(cx, |store, _| {
            std::mem::take(&mut store.commit_file_patches)
        }) {
            self.detail_requests.remove(&key);
            self.detail_patches.insert(key, patch.replace('\r', ""));
        }

        let (generation, busy) = {
            let store = self.store.read(cx);
            (store.changes_generation(), store.git_busy())
        };
        if self
            .groups_cache
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            // the snapshot is cloned into the groups anyway; cloning it once
            // more first would copy a ten-thousand-file tree for nothing
            let groups = {
                let store = self.store.read(cx);
                store
                    .changes()
                    .map(|snapshot| snapshot.change_groups())
                    .unwrap_or_default()
            };
            let unstaged_paths = Rc::new(
                groups
                    .1
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>(),
            );
            self.groups_cache = Some((
                generation,
                Rc::new(FileGroups {
                    groups,
                    unstaged_paths,
                }),
            ));
            // a new snapshot can add or drop files under an armed discard-all,
            // so the arming never outlives the tree it was given for
            self.discard_all_armed = false;
            // a new snapshot means the fetched patches may be stale
            self.patches.clear();
            if let Some((path, staged)) = self.selected_file.clone() {
                self.select_file(&path, staged, cx);
            }
        }
        let file_groups = self.groups_cache.as_ref().expect("groups cached").1.clone();
        let groups = &file_groups.groups;
        self.ensure_file_selected(groups, cx);
        self.ensure_commit_file_selected(cx);

        if self.history_open {
            return div()
                .size_full()
                .flex()
                .flex_row()
                .min_h_0()
                .child(self.render_history(window, cx));
        }

        let toolbar = self.render_toolbar(busy, cx);
        let popup = self
            .branch_picker
            .update(cx, |picker, cx| picker.overlay_popup(window, cx));
        // A clean tree is not a file table with nothing in it: it is one
        // statement, so it takes the body and the commit box steps aside
        // until there is something to commit (the design's empty state).
        let clean = groups.0.is_empty()
            && groups.1.is_empty()
            && self.store.read(cx).changes_loaded_for.is_some();
        // only a staged file can be committed: an unstaged-only tree keeps the
        // file list but grows no commit box (a commit here could only fail)
        let committing = self.store.read(cx).running_job() == Some("commit");
        let body = if clean {
            clean_state(&self.store, cx)
        } else {
            let mut files = div()
                .w(px(warpui_kit_core::shell::scaled(FILE_COLUMN_W)))
                .flex_shrink_0()
                .h_full()
                .min_h_0()
                .flex()
                .flex_col()
                .bg(theme::surface())
                .border_r_1()
                .border_color(theme::hairline())
                .child(self.render_file_list(&file_groups, cx));
            files = if groups.0.is_empty() && !committing {
                files.child(stage_hint(self.store.clone(), groups.1.len()))
            } else {
                files.child(self.render_commit_card(busy, window, cx))
            };
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(files)
                .child(self.render_diff(window, cx))
                .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(toolbar)
            .child(body)
            // painted last, so it hangs over the columns under the toolbar
            .children(popup.map(|popup| {
                gpui::deferred(
                    div()
                        .absolute()
                        .top(px(TOOLBAR_H))
                        .left(px(8.))
                        .w(px(BRANCH_W))
                        .child(popup),
                )
            }))
    }
}

impl ChangesView {
    /// The selected file's diff.
    fn render_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(key) = self.selected_file.clone() else {
            return div()
                .id("changes-diff")
                .debug_selector(|| "changes-diff".into())
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(theme::well())
                .into_any_element();
        };
        let patch = self.patches.get(&key).map(String::as_str);
        self.render_diff_pane(
            "changes-diff",
            "changes-diff-menu",
            &key.0,
            patch,
            window,
            cx,
        )
    }

    /// The Changes and History tabs' shared diff pane: the 44-high path bar
    /// with the persisted wrap/layout toggles over the selectable diff body.
    /// `None` is a patch still on the wire, `Some("")` a settled answer with
    /// nothing textual to draw (binary or mode-only).
    fn render_diff_pane(
        &self,
        pane_selector: &'static str,
        menu_name: &'static str,
        path: &str,
        patch: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let split = self.diff_view == DiffViewMode::Split;
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        // an empty patch is a state, not a one-row diff: a binary file or a
        // mode-only change has nothing textual to draw
        let has_patch = matches!(patch, Some(patch) if !patch.is_empty());
        let body: gpui::AnyElement = match patch {
            Some(patch) if !patch.is_empty() => div()
                .id(SharedString::from(format!("{pane_selector}-scroll")))
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(diff_controls::attach_filling(
                    gpui::ElementId::Name(menu_name.into()),
                    render_diff_with_scroll(
                        patch,
                        None,
                        warpui_kit_diff::DiffOptions {
                            mode: diff_controls::kit_mode(self.diff_view),
                            wrap: self.diff_wrap,
                        },
                        &self.diff_scroll,
                        theme::font_mono_meta(),
                        window,
                        cx,
                    ),
                    warpui_kit_core::text_menu::focus(cx),
                ))
                .into_any_element(),
            Some(_) => diff_controls::state("No textual changes"),
            None => diff_controls::state("Loading patch…"),
        };
        let wrap_view = cx.entity();
        let layout_view = wrap_view.clone();
        div()
            .id(pane_selector)
            .debug_selector(move || pane_selector.to_string())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::well())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(44.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .bg(theme::panel())
                    .border_b_1()
                    .border_color(theme::hairline())
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(path.to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(diff_controls::wrap_toggle(
                                "diff",
                                self.diff_wrap,
                                self.store.clone(),
                                move |next, cx| {
                                    wrap_view.update(cx, |this, cx| {
                                        this.diff_wrap = next;
                                        cx.notify();
                                    });
                                },
                                show_tooltips,
                            ))
                            .child(diff_controls::view_toggle(
                                "diff",
                                split,
                                self.store.clone(),
                                move |next, cx| {
                                    layout_view.update(cx, |this, cx| {
                                        this.diff_view = next;
                                        cx.notify();
                                    });
                                },
                                show_tooltips,
                            )),
                    ),
            )
            .child(body)
            .children((has_patch && !self.diff_wrap).then(|| {
                diff_controls::scrollbars(
                    "diff",
                    &self.diff_scroll,
                    self.diff_view == DiffViewMode::Split,
                )
            }))
            .into_any_element()
    }

    fn render_file_list(
        &mut self,
        files: &Rc<FileGroups>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (staged, unstaged) = (&files.groups.0, &files.groups.1);

        if staged.is_empty() && unstaged.is_empty() {
            let loaded = self.store.read(cx).changes_loaded_for.is_some();
            return div()
                .id("changes-list")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .py_1()
                .child(if loaded {
                    clean_state(&self.store, cx)
                } else {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_dim())
                        .child("Loading…")
                        .into_any_element()
                })
                .into_any_element();
        }

        // Headers and rows flatten into one fixed-height item list, and
        // `uniform_list` builds only the items in the viewport: a 10k-file
        // tree costs the same per repaint as a 30-file one. The old loop laid
        // out every row — its status letter, its dir/name split, its two
        // hover actions and their listeners — on every frame.
        let mut items: Vec<ChangeListItem> = Vec::with_capacity(staged.len() + unstaged.len() + 2);
        if !staged.is_empty() {
            items.push(ChangeListItem::StagedHeader);
            if !self.staged_collapsed {
                items.extend((0..staged.len()).map(ChangeListItem::StagedFile));
            }
        }
        if !unstaged.is_empty() {
            items.push(ChangeListItem::UnstagedHeader);
            if !self.unstaged_collapsed {
                items.extend((0..unstaged.len()).map(ChangeListItem::UnstagedFile));
            }
        }
        let items = Rc::new(items);
        let files = files.clone();
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        uniform_list(
            SharedString::from("changes-list"),
            items.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let entity = cx.entity();
                let (staged, unstaged) = (&files.groups.0, &files.groups.1);
                range
                    .map(|ix| match items[ix] {
                        ChangeListItem::StagedHeader => section_header(
                            FileSection::Staged,
                            staged.len(),
                            this.staged_collapsed,
                            entity.clone(),
                            vec![staging_icon(
                                "stage-all-clear",
                                "icons/icon-minus.svg",
                                "Unstage all",
                                GitAction::UnstageAll,
                                this.store.clone(),
                                show_tooltips,
                            )],
                        )
                        .into_any_element(),
                        ChangeListItem::StagedFile(index) => {
                            this.render_row(&staged[index], true, cx)
                        }
                        ChangeListItem::UnstagedHeader => section_header(
                            FileSection::Unstaged,
                            unstaged.len(),
                            this.unstaged_collapsed,
                            entity.clone(),
                            vec![
                                discard_all_button(
                                    this.discard_all_armed,
                                    files.unstaged_paths.clone(),
                                    this.store.clone(),
                                    entity.clone(),
                                    show_tooltips,
                                ),
                                staging_icon(
                                    "stage-all",
                                    "icons/icon-plus.svg",
                                    "Stage all",
                                    GitAction::StageAll,
                                    this.store.clone(),
                                    show_tooltips,
                                ),
                            ],
                        )
                        .into_any_element(),
                        ChangeListItem::UnstagedFile(index) => {
                            this.render_row(&unstaged[index], false, cx)
                        }
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .py_1()
        .debug_selector(|| "changes-list".into())
        .into_any_element()
    }

    fn render_row(
        &mut self,
        file: &GitChangedFile,
        staged: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = (file.path.clone(), staged);
        let selected = self.selected_file.as_ref() == Some(&key);
        let armed = self.discard_armed.as_ref() == Some(&key);
        let status_color = diff_controls::status_color(file.status_code.as_str());
        let row_id: SharedString =
            format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();
        let row_selector = row_id.clone();
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let discard = {
            let store = self.store.clone();
            let this = cx.entity();
            let path = file.path.clone();
            let key = key.clone();
            let selector = format!("discard-{staged}-{}", file.path);
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("Discard {}", file.path))
                .cursor_pointer()
                .rounded(px(4.))
                .h(px(24.))
                .min_w(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|s| s.bg(theme::tint(theme::DANGER)))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if armed {
                        this.update(cx, |this, _| this.discard_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(
                                GitAction::DiscardFile {
                                    path: path.clone(),
                                    is_staged: staged,
                                },
                                cx,
                            )
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.discard_armed = Some(key.clone());
                            cx.notify();
                        });
                    }
                })
                .when(armed, |d| {
                    d.px_1p5()
                        .text_size(px(theme::font_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::DANGER)
                        .child("Confirm")
                })
                .when(!armed, |d| {
                    d.child(icon(
                        "icons/icon-undo.svg",
                        px(theme::icon_tiny()),
                        theme::text_dim(),
                    ))
                })
        };
        div()
            .id(row_id)
            .debug_selector(move || row_selector.to_string())
            .a11y_button(file.path.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .group("file-row")
            .relative()
            .flex_shrink_0()
            .h(px(FILE_ROW_H))
            .pl(px(13.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_l_3()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::surface()
            })
            .when(selected, |d| d.bg(theme::raised()))
            .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
            .on_click({
                let path = file.path.clone();
                cx.listener(move |this, _, _, cx| this.select_file(&path, staged, cx))
            })
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color)
                    .child(file.display_status_code().to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap_1p5()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(theme::font_row_meta()))
                    .child(div().flex_shrink_0().text_color(theme::text()).child(name))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(theme::text_muted())
                            .child(dir),
                    ),
            )
            .when(file.additions.is_some() && !armed, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .flex_row()
                        .gap_1p5()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_micro()))
                        .group_hover("file-row", |s| s.invisible())
                        .when_some(file.additions, |row, added| {
                            row.child(
                                div()
                                    .text_color(theme::SUCCESS_BRIGHT)
                                    .child(format!("+{added}")),
                            )
                        })
                        .when_some(file.deletions, |row, deleted| {
                            row.child(div().text_color(theme::DANGER).child(format!("−{deleted}")))
                        }),
                )
            })
            // hover-revealed stage toggle + discard, over the counts
            .child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_0p5()
                    .when(!armed, |d| {
                        d.invisible().group_hover("file-row", |s| s.visible())
                    })
                    .child(discard)
                    .child(row_action_button(file, staged, self.store.clone())),
            )
            .into_any_element()
    }

    /// The History tab: the commits on the left and the selected one on the
    /// right. The newest stands in until the user picks another.
    fn render_history(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let list = self.render_history_list(cx);
        if self.drilled_commit.is_none()
            && let Some(first) = self.history_rows.first().map(|row| row.hash.clone())
        {
            self.drill_commit(&first, cx);
        }
        let detail = match self.drilled_commit.clone() {
            Some(hash) => {
                let details = self.store.read(cx).commit_details.clone();
                self.render_commit_detail(hash, details, window, cx)
            }
            None => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .child(
                div()
                    .w(px(warpui_kit_core::shell::scaled(HISTORY_COLUMN_W)))
                    .flex_shrink_0()
                    .h_full()
                    .min_h_0()
                    .bg(theme::surface())
                    .border_r_1()
                    .border_color(theme::hairline())
                    .child(list),
            )
            .child(
                div()
                    .debug_selector(|| "history-detail".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .min_h_0()
                    .child(detail),
            )
            .into_any_element()
    }

    /// Day-grouped commits with unpushed markers. Row labels are precomputed
    /// (see [`HistoryRow`]); only the element tree is rebuilt per render.
    fn render_history_list(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (unpushed, generation) = {
            let store = self.store.read(cx);
            (store.unpushed.clone(), store.history_generation())
        };
        let today = chrono::Local::now().date_naive();
        if self.history_cache_key != (generation, today) {
            let yesterday = today - chrono::Duration::days(1);
            let mut current_day: Option<chrono::NaiveDate> = None;
            self.history_rows = self
                .store
                .read(cx)
                .history()
                .iter()
                .map(|commit| {
                    let day = commit.date.date_naive();
                    let day_label = if current_day != Some(day) {
                        current_day = Some(day);
                        Some(if day == today {
                            "Today".to_string()
                        } else if day == yesterday {
                            "Yesterday".to_string()
                        } else {
                            day.format("%b %e").to_string()
                        })
                    } else {
                        None
                    };
                    HistoryRow {
                        day_label,
                        initials: commit.initials(),
                        author: commit.author.clone().unwrap_or_else(|| "Unknown".into()),
                        rel: warp_station_core::rel_time::format(commit.date),
                        short_hash: commit.short_hash().to_string(),
                        subject: commit.subject.clone(),
                        hash: commit.hash.clone(),
                    }
                })
                .collect();
            self.history_cache_key = (generation, today);
        }
        if self.history_rows.is_empty() {
            // An empty answer and a pending pass are different states: the
            // store settles the pass, so only a live one says "Loading".
            if !self.store.read(cx).history_loaded() {
                return div()
                    .id("history-list")
                    .debug_selector(|| "history-loading".into())
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_dim())
                    .child("Loading history…")
                    .into_any_element();
            }
            return div()
                .id("history-list")
                .debug_selector(|| "history-empty".into())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1p5()
                .p_6()
                .child(
                    div()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .text_align(gpui::TextAlign::Center)
                        .child("No commits yet"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .text_align(gpui::TextAlign::Center)
                        .child("Commits on this branch will show here."),
                )
                .into_any_element();
        }

        // 200 commits is a normal history, so the list is virtualized: a day
        // header and a commit row are both one fixed line, and `uniform_list`
        // builds only the ones in the viewport. The old loop laid out every
        // commit — its avatar, its hash chip and its listeners — on every
        // repaint, and the store keeps 200 of them.
        // shared, not moved out: `render_history` reads the newest row off the
        // same field to pick the commit it drills into, before this list is
        // even built
        let rows = Rc::new(self.history_rows.clone());
        let unpushed = Rc::new(unpushed);
        uniform_list(
            SharedString::from("history-list"),
            rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|ix| this.render_history_row(&rows[ix], &unpushed, cx))
                    .collect()
            }),
        )
        .debug_selector(|| "history-list".into())
        .size_full()
        .into_any_element()
    }

    /// One row of the virtualized history: a day header above its first commit,
    /// then the commit itself. Both are [`HISTORY_ROW_HEIGHT`] so the list can
    /// size itself without measuring.
    fn render_history_row(
        &mut self,
        row: &HistoryRow,
        unpushed: &Option<std::collections::HashSet<String>>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let unpushed_mark = unpushed.as_ref().is_some_and(|set| set.contains(&row.hash));
        let selected = self.drilled_commit.as_ref() == Some(&row.hash);
        let day_label = row.day_label.clone();
        div()
            .id(SharedString::from(format!("commit-{}", row.short_hash)))
            .debug_selector({
                let short = row.short_hash.clone();
                move || format!("commit-{short}")
            })
            .a11y_button(format!("{} — {}, {}", row.subject, row.author, row.rel))
            .cursor_pointer()
            .flex_shrink_0()
            .h(px(HISTORY_ROW_HEIGHT))
            .flex()
            .flex_col()
            .justify_center()
            .when_some(day_label, |d, label| {
                d.child(
                    div()
                        .px_4()
                        .pt_1()
                        .text_size(px(theme::font_sub()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text_dim())
                        .child(label),
                )
            })
            .child(
                div()
                    .id(SharedString::from(format!(
                        "commit-body-{}",
                        row.short_hash
                    )))
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_0p5()
                    .pl(px(13.))
                    .pr_3()
                    .border_l_3()
                    .border_color(if selected {
                        theme::accent()
                    } else {
                        theme::surface()
                    })
                    .when(selected, |d| d.bg(theme::raised()))
                    .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
                    .on_click({
                        let hash = row.hash.clone();
                        cx.listener(move |this, _, _, cx| this.drill_commit(&hash, cx))
                    })
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(row.subject.clone()),
                    )
                    .child(
                        // the design's commit meta line: the avatar sits with
                        // author, age, the unpushed mark and the SHA, so the
                        // subject gets the column's full width
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .rounded_full()
                                    .bg(theme::chip())
                                    .w(px(20.))
                                    .h(px(20.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(theme::font_micro()))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text_muted())
                                    .child(row.initials.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_dim())
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(format!("{} · {}", row.author, row.rel)),
                            )
                            .when(unpushed_mark, |meta| {
                                // the design asks for an unpushed mark,
                                // not a word: the arrow keeps the SHA on
                                // a 320 column
                                meta.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "unpushed-{}",
                                            row.short_hash
                                        )))
                                        .role(gpui::Role::Status)
                                        .aria_label("unpushed")
                                        .flex_shrink_0()
                                        .text_size(px(theme::font_meta()))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme::AHEAD)
                                        .tooltip(crate::ui::tooltip_text("Not pushed yet"))
                                        .child("\u{2191}"),
                                )
                            })
                            .child(
                                // hash chip: clicking copies the full SHA without
                                // drilling in
                                div()
                                    .id(SharedString::from(format!("copy-sha-{}", row.short_hash)))
                                    .a11y_button(format!("Copy SHA {}", row.short_hash))
                                    .cursor_pointer()
                                    .flex_shrink_0()
                                    .rounded_md()
                                    .px_1()
                                    .text_size(px(theme::font_meta()))
                                    .font_family(gpui::SharedString::from(theme::code_font()))
                                    .text_color(theme::text_faint())
                                    .hover(|s| s.text_color(theme::blue_bright()))
                                    .on_click({
                                        let store = self.store.clone();
                                        let full = row.hash.clone();
                                        let short = row.short_hash.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                full.clone(),
                                            ));
                                            store.update(cx, |s, cx| {
                                                s.notify_success(format!(
                                                    "Copied {short} to clipboard"
                                                ));
                                                cx.notify();
                                            });
                                        }
                                    })
                                    .child(row.short_hash.clone()),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The drilled commit: the design's head band (subject, meta, actions)
    /// over its two-level split — the file rail and the selected file's diff.
    fn render_commit_detail(
        &mut self,
        hash: String,
        details: Option<std::sync::Arc<warp_station_core::model::GitCommitDetails>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(details) = details.filter(|d| d.hash.starts_with(&hash[..hash.len().min(7)]))
        else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_dim())
                .child("Loading commit…")
                .into_any_element();
        };

        // author + time from the loaded history rows (the detail model
        // doesn't carry them)
        let history_meta = self
            .store
            .read(cx)
            .history()
            .iter()
            .find(|c| {
                c.hash == details.hash
                    || c.hash
                        .starts_with(&details.hash[..details.hash.len().min(7)])
            })
            .map(|c| {
                (
                    c.subject.clone(),
                    c.author.clone().unwrap_or_else(|| "Unknown".into()),
                    warp_station_core::rel_time::format(c.date),
                )
            });

        let additions = details
            .files
            .iter()
            .filter_map(|f| f.additions)
            .sum::<u32>();
        let deletions = details
            .files
            .iter()
            .filter_map(|f| f.deletions)
            .sum::<u32>();
        let file_count = details.files.len();
        let short_hash = details.hash[..7.min(details.hash.len())].to_string();

        // two-click armed destructive actions
        let armed = self.detail_armed;
        let action_button = |id: &'static str, label: &'static str, armed_now: bool| {
            let text = if armed_now { "Confirm" } else { label };
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .a11y_button(text)
                .cursor_pointer()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::line_strong()
                })
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::text()
                })
                .hover(|s| s.bg(theme::raised()))
                .child(text.to_string())
        };

        // the design's commit-acts row: 8 gap, 10 under the meta line
        let mut actions_row = div()
            .mt(px(10.))
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .flex_shrink_0();
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            div()
                .id("detail-copy-sha")
                .debug_selector(|| "detail-copy-sha".into())
                .a11y_button("Copy SHA")
                .cursor_pointer()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::line_strong())
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text())
                .hover(|s| s.bg(theme::raised()))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(full_hash.clone()));
                    store.update(cx, |s, cx| {
                        s.notify_success(format!(
                            "Copied {} to clipboard",
                            &full_hash[..7.min(full_hash.len())]
                        ));
                        cx.notify();
                    });
                })
                .child("Copy SHA")
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-revert", "Revert", armed == Some("revert")).on_click(
                move |_, _, cx| {
                    if armed == Some("revert") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Revert(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("revert");
                            cx.notify();
                        });
                    }
                },
            )
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-checkout", "Checkout", armed == Some("checkout")).on_click(
                move |_, _, cx| {
                    if armed == Some("checkout") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Checkout(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("checkout");
                            cx.notify();
                        });
                    }
                },
            )
        });

        // the head band: subject, then author · age · SHA · N files · +a −d,
        // then the verbs, on the panel surface
        let (author, rel) = history_meta
            .as_ref()
            .map(|(_, author, rel)| (author.clone(), rel.clone()))
            .unwrap_or_else(|| ("Unknown".into(), String::new()));
        let separator = || div().text_color(theme::text_faint()).child("·");
        let meta = div()
            .mt_0p5()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_dim())
            .child(format!("{author} · {rel}"))
            .child(separator())
            .child(
                div()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_color(theme::text_faint())
                    .child(short_hash.clone()),
            )
            .child(separator())
            .child(format!(
                "{file_count} {}",
                if file_count == 1 { "file" } else { "files" }
            ))
            .child(separator())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .child(
                        div()
                            .text_color(theme::SUCCESS_BRIGHT)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_color(theme::DANGER)
                            .child(format!("−{deletions}")),
                    ),
            );
        let head = div()
            .flex_shrink_0()
            .px_4()
            .py(px(14.))
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "commit-subject".into())
                    .text_size(px(theme::font_heading()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(
                        history_meta
                            .as_ref()
                            .map(|(subject, ..)| subject.clone())
                            .unwrap_or_else(|| short_hash.clone()),
                    ),
            )
            .child(meta)
            .child(actions_row);

        // the files rail: a 240 column's group header over one 34-high row
        // per changed file, selected file on the raised surface
        let selected_file = details
            .files
            .len()
            .checked_sub(1)
            .map(|last| self.commit_file.min(last))
            .unwrap_or(0);
        // the rail is virtualized too: a ten-thousand-file commit builds only
        // the rows on screen, and the details are an Arc so this pane does not
        // clone the whole file list on every repaint
        let rail_files = details.clone();
        let rows = uniform_list(
            SharedString::from("commit-files"),
            details.files.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|index| this.render_commit_file_row(&rail_files, index, selected_file, cx))
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .py_1()
        .debug_selector(|| "commit-files".into())
        .into_any_element();
        let rail = div()
            .w(px(warpui_kit_core::shell::scaled(COMMIT_RAIL_W)))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::surface())
            .border_r_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(COMMIT_RAIL_ROW_H))
                    .pl_4()
                    .pr_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_meta()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text_dim())
                    .child("Files")
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text_muted())
                            .child(file_count.to_string()),
                    ),
            )
            .child(rows);

        // the selected file's diff, through the Changes tab's own pane
        let diff = match details.files.get(selected_file) {
            Some(file) => {
                let key = format!("{}|{}", details.hash, file.path);
                let patch = self.detail_patches.get(&key).map(String::as_str);
                self.render_diff_pane(
                    "commit-diff",
                    "commit-diff-menu",
                    &file.path,
                    patch,
                    window,
                    cx,
                )
            }
            None => div()
                .id("commit-diff")
                .debug_selector(|| "commit-diff".into())
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(theme::well())
                .child(diff_controls::state("No files in this commit"))
                .into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(rail)
                    .child(diff),
            )
            .into_any_element()
    }

    /// One fixed-height row of the virtualized commit rail: the status
    /// letter, the dir/name split and the line counts, on the raised surface
    /// when it is the selected file.
    fn render_commit_file_row(
        &mut self,
        details: &std::sync::Arc<warp_station_core::model::GitCommitDetails>,
        index: usize,
        selected_file: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let file = &details.files[index];
        let selected = index == selected_file;
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let status = file.display_status_code().to_string();
        let mut counts = div()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .font_family(gpui::SharedString::from(theme::code_font()))
            .text_size(px(theme::font_micro()));
        if let Some(added) = file.additions.filter(|added| *added > 0) {
            counts = counts.child(
                div()
                    .text_color(theme::SUCCESS_BRIGHT)
                    .child(format!("+{added}")),
            );
        }
        if let Some(deleted) = file.deletions.filter(|deleted| *deleted > 0) {
            counts = counts.child(div().text_color(theme::DANGER).child(format!("−{deleted}")));
        }
        div()
            .id(SharedString::from(format!("commit-file-{index}")))
            .debug_selector(move || format!("commit-file-{index}"))
            .a11y_button(file.path.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .flex_shrink_0()
            .h(px(COMMIT_RAIL_ROW_H))
            .pl(px(13.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_l_3()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::surface()
            })
            .when(selected, |d| d.bg(theme::raised()))
            .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
            .on_click(cx.listener(move |this, _, _, cx| this.select_commit_file(index, cx)))
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_size(px(theme::font_mono_meta()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(diff_controls::status_color(&status))
                    .child(status),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap_1p5()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(theme::font_meta()))
                    .child(div().flex_shrink_0().text_color(theme::text()).child(name))
                    .when(!dir.is_empty(), |line| {
                        line.child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(theme::text_muted())
                                .child(dir),
                        )
                    }),
            )
            .child(counts)
            .into_any_element()
    }

    /// The middle card: "Commit message" title, the message textarea with
    /// the wand in its corner, and the outlined commit button. An empty box
    /// turns the button into "Generate & Commit": the wand drafts the
    /// message first, then the commit fires itself.
    fn render_commit_card(
        &mut self,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (wand_active, wand_queued_here, wand_live) = {
            let state = self.store.read(cx);
            let here = state.selected_repo.as_deref();
            let active = state.wand_busy == Some(crate::net::WandKind::Commit)
                && state.wand_repo.as_deref() == here;
            let queued =
                here.is_some_and(|folder| state.wand_queued(crate::net::WandKind::Commit, folder));
            (active, queued, state.wand_busy.is_some())
        };
        let wand_on = self.store.read(cx).wand_available();
        let message_empty = self.commit_message.read(cx).value().trim().is_empty();
        // one run owns the slot at a time; a request made while it is busy
        // queues for this repo and starts when the live run ends
        let can_generate = wand_on && !wand_active && !wand_queued_here;
        let generate_now = message_empty && can_generate && !wand_live;
        let queue_generation = message_empty && can_generate && wand_live;
        let generate_and_commit = generate_now;

        let mut card = div()
            .w_full()
            .flex_shrink_0()
            .h(px(COMMIT_BOX_H))
            .p_3()
            .border_t_1()
            .border_color(theme::hairline())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .on_key_down({
                        let store = self.store.clone();
                        let message = self.commit_message.clone();
                        move |event, _, cx| {
                            if event.keystroke.key == "enter"
                                && event.keystroke.modifiers.control
                                && !message.read(cx).value().trim().is_empty()
                            {
                                let text = message.read(cx).value().to_string();
                                store.update(cx, |s, cx| {
                                    s.run_git(GitAction::Commit(text.clone()), cx)
                                });
                            }
                        }
                    })
                    .child(
                        // the design's `textarea.area`: the recessed well with
                        // the default line, the body step on 9/10 padding
                        warpui_kit_core::field::area(
                            "commit-message",
                            &self.commit_message,
                            window,
                            cx,
                        )
                        .h_full()
                        // the well is one flex slot, not a content-sized box:
                        // without the zero floor a long draft grows the box
                        // past the card's row and paints under the button;
                        // the editor scrolls inside the fixed well instead
                        .min_h(px(0.))
                        // the card carries one accent — the commit button
                        // under the box — so the box keeps its resting edge
                        // while the draft has focus instead of turning accent
                        // and reading as a second, competing action
                        .focus(|s| s.border_color(theme::hairline())),
                    )
                    // the corner wand: overlay inside the message box
                    .when(can_generate, |box_area| {
                        let store = self.store.clone();
                        box_area.child(
                            div()
                                .id("wand-corner")
                                .a11y_button("Generate commit message")
                                .absolute()
                                .bottom_1()
                                .right_2()
                                .cursor_pointer()
                                .rounded_md()
                                .p_1()
                                .flex()
                                .hover(|s| s.bg(theme::hover_bg()))
                                .on_click(move |_, _, cx| {
                                    store.update(cx, |s, cx| {
                                        s.run_wand(crate::net::WandKind::Commit, false, cx)
                                    });
                                })
                                .child(icon(
                                    "icons/icon-wand.svg",
                                    px(theme::icon_small()),
                                    theme::text_dim(),
                                )),
                        )
                    }),
            );

        let mut button_row = div().flex().flex_row().items_center().gap_2();
        button_row = button_row.child({
            let store = self.store.clone();
            let message = self.commit_message.clone();
            let (generating, committing) = (
                wand_active,
                self.store.read(cx).running_job() == Some("commit"),
            );
            let working = generating || committing;
            let label = if generating {
                "Generating…"
            } else if committing {
                "Committing…"
            } else if message_empty && wand_queued_here {
                "Queued…"
            } else if generate_and_commit {
                "Generate & Commit"
            } else if queue_generation {
                "Queue & Commit"
            } else {
                "Commit"
            };
            let can_commit = !busy && (!message_empty || can_generate);
            let color = if can_commit {
                theme::on_accent()
            } else {
                theme::text_muted()
            };
            div()
                .id("commit-button")
                .debug_selector(|| "commit-button".into())
                .a11y_button(label)
                .cursor_pointer()
                .flex_1()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .text_color(color)
                .when(can_commit, |b| {
                    b.bg(theme::accent()).hover(|s| s.bg(theme::accent_hover()))
                })
                .when(!can_commit, |b| b.bg(theme::raised()))
                .on_click(move |_, _, cx| {
                    if busy {
                        return;
                    }
                    // the box only exists with staged files, but the state may
                    // have changed since this frame was built
                    let staged = store
                        .read(cx)
                        .changes()
                        .is_some_and(|snapshot| !snapshot.staged_files.is_empty());
                    if !staged {
                        return;
                    }
                    let text = message.read(cx).value().to_string();
                    if text.trim().is_empty() {
                        // no wand, or this repo's request is already live or
                        // queued: there is nothing this click could start
                        if !can_generate {
                            return;
                        }
                        // empty box: generate, then commit in this repo once
                        // the run lands — even if another repo is on screen
                        // by then; the store owns that follow-up
                        store.update(cx, |s, cx| {
                            s.run_wand(crate::net::WandKind::Commit, true, cx)
                        });
                        return;
                    }
                    store.update(cx, |s, cx| s.run_git(GitAction::Commit(text.clone()), cx));
                })
                .when(working, |b| {
                    b.child(warpui_kit_core::motion::spinner(
                        "commit-spinner",
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        color,
                    ))
                })
                .when(!working, |b| {
                    b.child(icon(
                        if generate_and_commit
                            || queue_generation
                            || (message_empty && wand_queued_here)
                        {
                            "icons/icon-wand.svg"
                        } else {
                            "icons/icon-check.svg"
                        },
                        px(theme::icon_small()),
                        color,
                    ))
                })
                .child(
                    div()
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(label),
                )
        });
        if wand_active {
            let store = self.store.clone();
            button_row = button_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::blue_bright())
                    .child(icon(
                        "icons/icon-wand.svg",
                        px(theme::icon_small() - 1.),
                        theme::blue_bright(),
                    ))
                    .child("working…")
                    // the one run can hang on a dead harness; never leave the
                    // box without a way out
                    .child(
                        div()
                            .id("wand-cancel")
                            .debug_selector(|| "wand-cancel".into())
                            .a11y_button("Cancel generation")
                            .cursor_pointer()
                            .ml_2()
                            .px_1p5()
                            .rounded(px(4.))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                            .on_click(move |_, _, cx| store.update(cx, |s, cx| s.cancel_wand(cx)))
                            .child("Cancel"),
                    ),
            );
        }
        if wand_queued_here {
            let store = self.store.clone();
            let folder = self
                .store
                .read(cx)
                .selected_repo
                .clone()
                .unwrap_or_default();
            button_row = button_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_dim())
                    .child(icon(
                        "icons/icon-wand.svg",
                        px(theme::icon_small() - 1.),
                        theme::text_dim(),
                    ))
                    .child("queued…")
                    // the request waits for the slot: pulling it back must
                    // not stop the run that is live now
                    .child(
                        div()
                            .id("wand-dequeue")
                            .debug_selector(|| "wand-dequeue".into())
                            .a11y_button("Cancel queued generation")
                            .cursor_pointer()
                            .ml_2()
                            .px_1p5()
                            .rounded(px(4.))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.dequeue_wand(crate::net::WandKind::Commit, &folder, cx)
                                })
                            })
                            .child("Cancel"),
                    ),
            );
        }
        card = card.child(button_row);
        card.into_any_element()
    }

    /// The git toolbar over the columns: the branch selector, then the sync
    /// verbs. Pull and Push carry what they would move; a branch with no
    /// upstream publishes instead of pushing.
    fn render_toolbar(&mut self, busy: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let entry = store.selected_entry();
        let branch = entry.and_then(|e| e.git_branch_name.clone());
        let has_remote = entry.is_some_and(|e| e.git_has_remote);
        let published = entry.is_some_and(|e| e.git_branch_published);
        let to_push = entry.map_or(0, |e| e.git_to_push_count);
        let to_pull = entry.map_or(0, |e| e.git_to_pull_count);
        let job = store.running_job();
        let finished = store.finished_job();

        let action = |icon_path: &'static str,
                      label: &'static str,
                      enabled: bool,
                      badge: Option<(u32, gpui::Rgba)>,
                      git: GitAction| {
            let store = self.store.clone();
            let running = job == Some(git.label());
            let landed = finished == Some(git.label());
            let word = if running {
                crate::net::git_working_word(git.label())
            } else if landed {
                crate::net::git_done_word(git.label())
            } else {
                label
            };
            div()
                .id(SharedString::from(format!("rail-{label}")))
                .debug_selector(move || format!("rail-{label}"))
                .a11y_button(word)
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
                .when(enabled, |d| {
                    d.cursor_pointer()
                        .text_color(theme::text())
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            store.update(cx, |s, cx| s.run_git(git.clone(), cx));
                        })
                })
                // the running button stays at full strength
                .when(!enabled && !running, |d| {
                    d.opacity(0.45).text_color(theme::text_dim())
                })
                .when(running, |d| d.text_color(theme::text()))
                .when(landed, |d| d.text_color(theme::SUCCESS_BRIGHT))
                .child(if running {
                    warpui_kit_core::motion::spinner(
                        SharedString::from(format!("rail-spinner-{label}")),
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )
                } else if landed {
                    icon(
                        "icons/icon-check.svg",
                        px(theme::icon_small()),
                        theme::SUCCESS_BRIGHT,
                    )
                    .into_any_element()
                } else {
                    icon(icon_path, px(theme::icon_small()), theme::text_dim()).into_any_element()
                })
                .child(word)
                .when_some(badge.filter(|_| enabled), |row, (count, color)| {
                    let selector = format!("rail-badge-{label}");
                    row.child(
                        div()
                            .debug_selector(move || selector.clone())
                            .text_color(color)
                            .child(format!("{count}")),
                    )
                })
        };

        // Only the verbs that can do something right now: a pull with nothing
        // behind, a push with nothing ahead, or a fetch with no remote is not
        // offered at all. The running/just-landed button stays visible so its
        // progress and past-tense confirmation are not yanked away.
        let on_branch = branch.as_deref().is_some_and(|name| name != "(detached)");
        let visible =
            |label: &str, relevant: bool| relevant || job == Some(label) || finished == Some(label);
        let mut bar = div()
            .flex_shrink_0()
            .h(px(TOOLBAR_H))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(theme::hairline())
            .child(div().w(px(BRANCH_W)).child(self.branch_picker.clone()));
        if visible("fetch", has_remote) {
            bar = bar.child(action(
                "icons/icon-cloud-download.svg",
                "Fetch",
                !busy,
                None,
                GitAction::Fetch,
            ));
        }
        if visible("pull", has_remote && on_branch && to_pull > 0) {
            bar = bar.child(action(
                "icons/icon-arrow-down.svg",
                "Pull",
                !busy,
                (to_pull > 0).then_some((to_pull, theme::BEHIND)),
                GitAction::Pull,
            ));
        }
        match branch.clone() {
            // a branch the remote does not have yet: publishing is the move
            Some(name)
                if has_remote
                    && on_branch
                    && (!published || job == Some("publish") || finished == Some("publish")) =>
            {
                bar = bar.child(action(
                    "icons/icon-arrow-up.svg",
                    "Publish",
                    !busy,
                    None,
                    GitAction::Publish { branch: name },
                ));
            }
            Some(_)
                if has_remote
                    && on_branch
                    && (to_push > 0 || job == Some("push") || finished == Some("push")) =>
            {
                bar = bar.child(action(
                    "icons/icon-arrow-up.svg",
                    "Push",
                    !busy,
                    (to_push > 0).then_some((to_push, theme::AHEAD)),
                    GitAction::Push,
                ));
            }
            _ => {}
        }
        if visible(
            "sync",
            has_remote && on_branch && (to_pull > 0 || to_push > 0),
        ) {
            bar = bar.child(action(
                "icons/icon-refresh.svg",
                "Sync",
                !busy,
                None,
                GitAction::Sync,
            ));
        }
        bar.into_any_element()
    }
}

fn section_header(
    section: FileSection,
    count: usize,
    collapsed: bool,
    this: Entity<ChangesView>,
    trailing: Vec<gpui::AnyElement>,
) -> impl IntoElement + use<> {
    let label = section.label();
    let selector = section.selector();
    let title = div()
        .id(SharedString::from(selector))
        .debug_selector(move || selector.to_string())
        .a11y_button(format!("{label} ({count})"))
        .aria_expanded(!collapsed)
        .cursor_pointer()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .on_click(move |_, _, cx| {
            this.update(cx, |view, cx| view.toggle_section(section, cx));
        })
        .child(icon(
            if collapsed {
                "icons/icon-chevron-right.svg"
            } else {
                "icons/icon-chevron-down.svg"
            },
            px(theme::icon_tiny()),
            theme::text_muted(),
        ))
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_faint())
                .child(format!("({count})")),
        );

    // the header's verbs are hover-revealed like the rows' actions: the
    // container keeps their width, so the title does not jump when they show
    let mut actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .flex_shrink_0()
        .invisible()
        .group_hover(selector, |s| s.visible());
    for element in trailing {
        actions = actions.child(element);
    }

    div()
        .group(selector)
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .h(px(FILE_ROW_H))
        .px_1()
        .rounded(px(4.))
        .hover(|s| s.bg(theme::panel()))
        .child(title.flex_1())
        .child(actions)
}

/// The unstaged header's discard-all: the row discard's arrow glyph, so the
/// whole-tree verb reads as the same action its rows offer. Its first click
/// arms it (the glyph becomes "Confirm") and its second throws every unstaged
/// change away — the row discards take their confirmation the same way.
fn discard_all_button(
    armed: bool,
    files: Rc<Vec<String>>,
    store: Entity<Store>,
    this: Entity<ChangesView>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let label = if armed {
        "Confirm discard"
    } else {
        "Discard all unstaged changes"
    };
    let button = div()
        .id("discard-all-u")
        .debug_selector(|| "discard-all-u".into())
        .a11y_button(label)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .hover(|s| s.bg(theme::tint(theme::DANGER)))
        .on_click(move |_, _, cx| {
            if armed {
                this.update(cx, |this, _| this.discard_all_armed = false);
                store.update(cx, |s, cx| {
                    s.run_git(GitAction::DiscardUnstaged((*files).clone()), cx)
                });
            } else {
                this.update(cx, |this, cx| {
                    this.discard_all_armed = true;
                    cx.notify();
                });
            }
        })
        .when(armed, |d| {
            d.px_1p5()
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::DANGER)
                .child("Confirm")
        })
        .when(!armed, |d| {
            d.child(icon(
                "icons/icon-undo.svg",
                px(theme::icon_small()),
                theme::text_muted(),
            ))
        });
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// A whole-list staging verb as an icon button — the compact form both
/// section headers use, with the verb in the tooltip and the a11y name. The
/// stage footer keeps the primary button.
fn staging_icon(
    id: &'static str,
    icon_path: &'static str,
    label: &'static str,
    action: GitAction,
    store: Entity<Store>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let selector = id.to_string();
    let button = div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .a11y_button(label)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .hover(|s| s.bg(theme::raised()))
        .on_click(move |_, _, cx| {
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(icon(
            icon_path,
            px(theme::icon_small()),
            theme::text_muted(),
        ));
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// The no-staged-files footer: the commit box only exists when a commit can
/// succeed, so this says why it is missing instead of leaving the column
/// blank. The stage-all verb stands where the commit button would be and wears
/// the same primary fill, so the column's one action reads the same either way.
fn stage_hint(store: Entity<Store>, unstaged: usize) -> gpui::AnyElement {
    let mut footer = div()
        .debug_selector(|| "changes-stage-hint".into())
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(theme::hairline())
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(
            // wraps rather than truncating: at the column's narrowest the
            // sentence the footer exists for must stay readable
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("Stage changes to commit"),
        );
    if unstaged > 0 {
        let action_store = store.clone();
        footer = footer.child(crate::ui::primary_button(
            "stage-all-footer",
            "Stage all",
            true,
            move |_, _, cx| {
                action_store.update(cx, |s, cx| s.run_git(GitAction::StageAll, cx));
            },
        ));
    }
    footer.into_any_element()
}

/// The clean working tree: what is true, plus the last commit so the tab still
/// says where the branch stands.
fn clean_state(store: &Entity<Store>, cx: &mut gpui::App) -> gpui::AnyElement {
    let last = store
        .read(cx)
        .recent_commits
        .first()
        .map(|commit| {
            format!(
                "Last commit {}: {}",
                warp_station_core::rel_time::format(commit.date),
                commit.subject
            )
        })
        .or_else(|| {
            store
                .read(cx)
                .selected_entry()
                .and_then(|repo| repo.git_last_commit_at)
                .map(|at| format!("Last commit {}", warp_station_core::rel_time::format(at)))
        });
    div()
        .debug_selector(|| "changes-clean".into())
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p(px(warpui_kit_core::shell::DETAIL_PADDING))
        .child(crate::ui::icon(
            "icons/icon-check.svg",
            px(theme::icon_large()),
            theme::SUCCESS_BRIGHT,
        ))
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child("Working tree clean"),
        )
        .children(last.map(|last| {
            div()
                .max_w(px(warpui_kit_core::shell::READING_WIDTH))
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(last)
        }))
        .into_any_element()
}

/// The +/− stage-toggle button per row: unstaged rows stage, staged rows
/// unstage.
fn row_action_button(
    file: &GitChangedFile,
    staged: bool,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    let action = if staged {
        GitAction::Unstage(file.path.clone())
    } else {
        GitAction::Stage(file.path.clone())
    };
    let glyph = if staged {
        "icons/icon-minus.svg"
    } else {
        "icons/icon-plus.svg"
    };
    let selector = format!("toggle-{staged}-{}", file.path);
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(if staged {
            format!("Unstage {}", file.path)
        } else {
            format!("Stage {}", file.path)
        })
        .cursor_pointer()
        .rounded_md()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::blue_bright())
        .hover(|s| s.bg(theme::tint(theme::accent())))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(icon(
            glyph,
            px(theme::icon_small() - 2.),
            theme::blue_bright(),
        ))
}

// ---- the branch picker ------------------------------------------------------

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// The branch picker's ephemeral state: the armed delete, whether the
/// new-branch form replaced the list, and the repo the popup was loaded for
/// (a mismatch resets it).
#[derive(Default)]
struct BranchUi {
    delete_armed: Option<String>,
    new_branch_open: bool,
}

/// The branch selector: a pick checks the branch out; locals carry the delete
/// trash and the popup carries the new-branch form.
fn build_branch_picker(
    store: &Entity<Store>,
    branch_ui: Rc<RefCell<BranchUi>>,
    new_branch_name: Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<ChangesView>,
) -> Entity<FilteredPicker> {
    let spec = PickerSpec {
        key: "Branch",
        trigger_label: "Branch",
        filter_placeholder: "Filter branches",
        icons: PICKER_ICONS,
        trigger_icon: Some("icons/icon-source-branch.svg"),
        provider_badge: false,
        refresh: false,
        refresh_label: "",
        stop_trigger_mousedown: true,
        visible_rows: 8,
        selected_accent: true,
        overlay: true,
        anchor: OverlayAnchor::Rail,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: Some("branch-button"),
        panel_id: Some("branch-menu"),
        filter_selector: Some("branch-filter"),
        list_id: Some("branch-list"),
    };
    let pick_store = store.clone();
    let branch_picker = cx.new(|cx| {
        FilteredPicker::new(
            spec,
            PickerTarget::Callback(Box::new(move |value, _window, cx| {
                pick_store.update(cx, |s, cx| {
                    s.run_git(GitAction::Checkout(value.to_string()), cx)
                });
            })),
            Box::new(|_, _, _| unreachable!("the real source lands right after construction")),
            window,
            cx,
        )
    });
    {
        let data_store = store.clone();
        let data_ui = branch_ui.clone();
        let data_name = new_branch_name.clone();
        let data_picker = branch_picker.downgrade();
        branch_picker.update(cx, |picker, _cx| {
            picker.set_data_source(Box::new(
                move |filter: &str, window: &Window, cx: &gpui::App| {
                    // the picker hands itself in for the form's repaints;
                    // a dying entity renders an empty list
                    match data_picker.upgrade() {
                        Some(picker) => branch_data(
                            &data_store,
                            &data_ui,
                            &data_name,
                            &picker,
                            filter,
                            window,
                            cx,
                        ),
                        None => PickerData {
                            body: PickerBody::Rows(Vec::new()),
                            selected: Selection::default(),
                            footer: None,
                            fonts: theme::active_tokens(),
                            accent: theme::blue_bright(),
                            show_tooltips: false,
                            refreshing: false,
                        },
                    }
                },
            ));
        });
    }
    // dismissal (outside click, Escape, a pick) clears the ephemeral state
    branch_picker.update(cx, |picker, cx| {
        picker.set_on_dismiss({
            let ui = branch_ui.clone();
            Box::new(move |_cx| {
                let mut ui = ui.borrow_mut();
                ui.delete_armed = None;
                ui.new_branch_open = false;
            })
        });
        picker.set_on_trailing({
            let ui = branch_ui.clone();
            let store = store.clone();
            Box::new(move |name, cx| {
                let confirmed = {
                    let mut ui = ui.borrow_mut();
                    if ui.delete_armed.as_deref() == Some(name) {
                        ui.delete_armed = None;
                        true
                    } else {
                        ui.delete_armed = Some(name.to_string());
                        false
                    }
                };
                if confirmed {
                    store.update(cx, |s, cx| {
                        s.run_git(GitAction::DeleteBranch(name.to_string()), cx)
                    });
                }
                // true = done: the shell dismisses (no re-entrant update —
                // this closure runs inside the picker's own row listener)
                confirmed
            })
        });
        picker.observe_entity(store, cx);
    });
    branch_picker
}

/// One render's worth of the branch list: locals with their delete trash, the
/// inert "Remote" group label, and — when the form is open — the form as the
/// whole panel body.
fn branch_data(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    filter: &str,
    window: &Window,
    cx: &gpui::App,
) -> PickerData {
    let store_read = store.read(cx);
    let current = store_read
        .selected_entry()
        .and_then(|e| e.git_branch_name.clone())
        .unwrap_or_default();
    let show_tooltips = store_read.settings().general.show_tooltips;
    let armed = branch_ui.borrow().delete_armed.clone();
    let form_open = branch_ui.borrow().new_branch_open;

    if form_open {
        return PickerData {
            body: PickerBody::Replace(new_branch_form(
                store,
                branch_ui,
                new_branch_name,
                picker,
                window,
                cx,
            )),
            selected: Selection {
                value: current.clone(),
                label: "—".to_string(),
                badge: None,
            },
            footer: None,
            fonts: theme::active_tokens(),
            accent: theme::blue_bright(),
            show_tooltips,
            refreshing: false,
        };
    }

    let filter = filter.to_lowercase();
    let mut rows = Vec::new();
    let mut remotes_labeled = false;
    for branch in &store_read.branches {
        if !filter.is_empty() && !branch.name.to_lowercase().contains(&filter) {
            continue;
        }
        let is_remote = branch.kind == warp_station_core::model::GitBranchKind::Remote;
        // locals first, then the inert "Remote" group label (re-emerges
        // before the first matching remote while filtering)
        if is_remote && !remotes_labeled {
            remotes_labeled = true;
            rows.push(PickerRow::Group {
                id: "branch-group-remote".into(),
                label: "Remote".into(),
                selector: Some("branch-group-remote".into()),
            });
        }
        let deletable = !is_remote && branch.name != current;
        rows.push(PickerRow::Item(PickerItem {
            id: format!("branch-row-{}", branch.name),
            selector: format!("branch-row-{}", branch.name),
            value: branch.name.clone(),
            // the full name stays the pick payload; the display label sheds
            // its head only when it would overflow
            label: crate::ui::branch_label(&branch.name, window),
            selected: branch.name == current,
            muted: is_remote,
            trailing_delete: deletable.then(|| TrailingDelete {
                id: format!("branch-delete-{}", branch.name),
                selector: format!("branch-delete-{}", branch.name),
                armed: armed.as_deref() == Some(branch.name.as_str()),
                tooltip: "Delete branch",
            }),
        }));
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            label: if current.is_empty() {
                "—".to_string()
            } else {
                crate::ui::branch_label(&current, window)
            },
            badge: None,
            value: current,
        },
        footer: Some(new_branch_footer(
            store,
            branch_ui,
            new_branch_name,
            picker,
            cx,
        )),
        fonts: theme::active_tokens(),
        accent: theme::blue_bright(),
        show_tooltips,
        refreshing: false,
    }
}

/// The panel footer: "+ New branch…" opens the form seeded with the repo's
/// branch-name prefix and focused, ready to type.
fn new_branch_footer(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    cx: &gpui::App,
) -> gpui::AnyElement {
    let prefix = store.read(cx).settings().repos.branch_name_prefix.clone();
    let seed = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let ui = branch_ui.clone();
    let name = new_branch_name.clone();
    let picker = picker.downgrade();
    div()
        .id("new-branch-row")
        .debug_selector(|| "new-branch-row".into())
        .a11y_button("New branch…")
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .text_size(px(theme::font_body()))
        .text_color(theme::blue_bright())
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(move |_, window, cx| {
            // the form always opens seeded with the repo's prefix and
            // focused, ready to type the branch name
            name.update(cx, |state, cx| {
                state.set_value(seed.as_str(), window, cx);
                state.focus(window, cx);
            });
            ui.borrow_mut().new_branch_open = true;
            let _ = picker.update(cx, |_, cx| cx.notify());
        })
        .child("+ New branch…")
        .into_any_element()
}

/// The open new-branch form: the name input over Cancel/Create, replacing the
/// whole panel body.
fn new_branch_form(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    window: &Window,
    cx: &gpui::App,
) -> gpui::AnyElement {
    let cancel_ui = branch_ui.clone();
    let create_ui = branch_ui.clone();
    let create_store = store.clone();
    let create_name = new_branch_name.clone();
    let cancel_picker = picker.downgrade();
    let create_picker = picker.downgrade();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().debug_selector(|| "new-branch-name".into()).child(
            warpui_kit_core::field::mono("new-branch-name-field", new_branch_name, window, cx),
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("new-branch-cancel")
                        .debug_selector(|| "new-branch-cancel".into())
                        .a11y_button("Cancel new branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_dim())
                        .border_1()
                        .border_color(theme::hairline())
                        .hover(|s| s.bg(theme::hover_bg()))
                        .on_click(move |_, _, cx| {
                            cancel_ui.borrow_mut().new_branch_open = false;
                            let _ = cancel_picker.update(cx, |_, cx| cx.notify());
                        })
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id("new-branch-create")
                        .debug_selector(|| "new-branch-create".into())
                        .a11y_button("Create branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme::accent())
                        .text_color(theme::on_accent())
                        .hover(|s| s.opacity(0.9))
                        .on_click(move |_, window, cx| {
                            let name = create_name.read(cx).value().trim().to_string();
                            if name.is_empty() {
                                return;
                            }
                            create_store.update(cx, |s, cx| {
                                s.run_git(
                                    GitAction::CreateBranch {
                                        name,
                                        start: None,
                                        checkout: true,
                                    },
                                    cx,
                                )
                            });
                            // a created branch is a selection: the popup
                            // closes and the form resets
                            create_ui.borrow_mut().new_branch_open = false;
                            create_name.update(cx, |f, cx| f.set_value("", window, cx));
                            let _ = create_picker.update(cx, |p, cx| {
                                p.dismiss(cx);
                            });
                        })
                        .child("Create"),
                ),
        )
        .into_any_element()
}
