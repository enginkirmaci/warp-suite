//! The Changes and History tabs: the selected repo's commit workspace. Changes
//! is the git toolbar (branch, Fetch, Pull, Push, Sync) over the file list and
//! commit box on the left and the selected file's diff on the right. History is
//! the day-grouped commits on the left and the selected commit on the right.
//! The heavy data lives on the store; this view owns only ephemeral state (the
//! selected file, fetched patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px, uniform_list};
use gpui_kit::component::input::{InputState, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station_core::model::GitChangedFile;
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

/// One history row: a commit, with its day header on the row above the first
/// of each day. Fixed so `uniform_list` can size the whole list.
const HISTORY_ROW_HEIGHT: f32 = 58.;
/// The file list and commit box's width.
const FILE_COLUMN_W: f32 = 320.;
/// The commit list's width.
const HISTORY_COLUMN_W: f32 = 420.;
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
    file_by_repo: std::collections::HashMap<String, (String, bool)>,
    /// The row whose discard took its first click; the second one discards.
    discard_armed: Option<(String, bool)>,
    /// The unstaged header's discard-all has been clicked once: the second
    /// click throws the whole working tree away.
    discard_all_armed: bool,
    patches: std::collections::HashMap<(String, bool), String>,
    /// The branch selector: the shared FilteredPicker in branch mode.
    branch_picker: Entity<FilteredPicker>,
    /// The branch picker's ephemeral state (armed delete, open form); shared
    /// with the picker's closures.
    branch_ui: Rc<RefCell<BranchUi>>,
    commit_message: Entity<TextareaState>,
    /// Set when the user pressed "Generate & Commit" with an empty box: the
    /// wand's draft lands here first, then commits automatically.
    auto_commit_after_draft: bool,
    history_open: bool,
    /// Hash of the drilled-in commit (None = the list view).
    drilled_commit: Option<String>,
    /// Armed destructive drill-down action ("checkout"/"revert") — the second
    /// click confirms the arm.
    detail_armed: Option<&'static str>,
    detail_patches: std::collections::HashMap<String, String>,
    expanded_details: std::collections::HashSet<String>,
    /// One scroll handle per expanded commit-detail patch. The patches scroll
    /// inside the commit detail's own scroller, so each consumes the wheel
    /// events it can act on (see the on_scroll_wheel at the patch) instead of
    /// letting one event move both it and the pane behind it.
    patch_scrolls: std::collections::HashMap<String, gpui::ScrollHandle>,
    /// The repo the view's local state belongs to; a mismatch resets it all.
    loaded_for: Option<String>,
    /// Staged/unstaged groups, rebuilt only when the store's change snapshot
    /// generation moves (any store notify otherwise re-cloned everything).
    groups_cache: Option<(u64, Rc<GroupedFiles>)>,
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
        Self {
            store,
            selected_file: None,
            file_by_repo: std::collections::HashMap::new(),
            discard_armed: None,
            discard_all_armed: false,
            patches: std::collections::HashMap::new(),
            branch_picker,
            branch_ui,
            commit_message,
            auto_commit_after_draft: false,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            expanded_details: std::collections::HashSet::new(),
            patch_scrolls: std::collections::HashMap::new(),
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
        self.auto_commit_after_draft = false;
        self.history_open = false;
        self.drilled_commit = None;
        self.detail_armed = None;
        self.detail_patches.clear();
        self.expanded_details.clear();
        self.patch_scrolls.clear();
    }

    /// Closes the branch picker from an outside click or Escape. Returns
    /// whether it was actually open so the window can stop propagating.
    pub fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.branch_picker.update(cx, FilteredPicker::dismiss)
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history().is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        if needs_load && let Some(folder) = self.store.read(cx).selected_repo.clone() {
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
        self.expanded_details.clear();
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

    fn toggle_detail_patch(&mut self, hash: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{hash}|{path}");
        if self.expanded_details.contains(&key) {
            self.expanded_details.remove(&key);
        } else {
            self.expanded_details.insert(key.clone());
            if !self.detail_patches.contains_key(&key)
                && let Some(folder) = self.store.read(cx).selected_repo.clone()
            {
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
        }
        cx.notify();
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
        let mut auto_commit_text: Option<String> = None;
        if let Some(draft) = self
            .store
            .update(cx, |store, _| store.commit_message_draft.take())
        {
            let draft = draft.trim().to_string();
            self.commit_message
                .update(cx, |state, cx| state.set_value(draft.as_str(), window, cx));
            if self.auto_commit_after_draft && !draft.is_empty() {
                self.auto_commit_after_draft = false;
                // the user may have unstaged everything while the run was in
                // flight: keep the message, but do not fire a commit that can
                // only fail
                let staged = self
                    .store
                    .read(cx)
                    .changes()
                    .is_some_and(|snapshot| !snapshot.staged_files.is_empty());
                if staged {
                    auto_commit_text = Some(draft);
                }
            }
        }
        if let Some(message) = auto_commit_text {
            self.store.update(cx, |store, cx| {
                store.run_git(GitAction::Commit(message), cx)
            });
        }
        // a generate-and-commit click stays armed only while this repo's run
        // is actually in flight: a failed or cancelled wand, a run kicked for
        // another repo, or a run that never started must not leave a commit
        // armed for some later draft to fire
        if self.auto_commit_after_draft {
            let generating_here = {
                let store = self.store.read(cx);
                store.wand_busy == Some(crate::net::WandKind::Commit)
                    && store.wand_repo == store.selected_repo
            };
            if !generating_here {
                self.auto_commit_after_draft = false;
            }
        }
        if let Some((key, patch)) = self
            .store
            .update(cx, |store, _| store.commit_file_patch.take())
        {
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
            let groups = self
                .store
                .read(cx)
                .changes()
                .cloned()
                .unwrap_or_default()
                .change_groups();
            self.groups_cache = Some((generation, Rc::new(groups)));
            // a new snapshot can add or drop files under an armed discard-all,
            // so the arming never outlives the tree it was given for
            self.discard_all_armed = false;
            // a new snapshot means the fetched patches may be stale
            self.patches.clear();
            if let Some((path, staged)) = self.selected_file.clone() {
                self.select_file(&path, staged, cx);
            }
        }
        let groups = self.groups_cache.as_ref().expect("groups cached").1.clone();
        self.ensure_file_selected(&groups, cx);

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
                .child(self.render_file_list(&groups, cx));
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
        let pane = div()
            .id("changes-diff")
            .debug_selector(|| "changes-diff".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::well());
        let Some(key) = self.selected_file.clone() else {
            return pane.into_any_element();
        };
        let patch = self
            .patches
            .get(&key)
            .map_or("loading patch…", |patch| patch.as_str());
        let diff = warpui_kit_diff::render_diff(patch, None, theme::font_mono_meta(), window, cx);
        pane.child(
            div()
                .flex_shrink_0()
                .h(px(44.))
                .px_4()
                .flex()
                .items_center()
                .bg(theme::panel())
                .border_b_1()
                .border_color(theme::hairline())
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::text())
                .child(key.0),
        )
        .child(
            div()
                .id("changes-diff-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(diff),
        )
        .into_any_element()
    }

    fn render_file_list(
        &mut self,
        groups: &Rc<GroupedFiles>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (staged, unstaged) = (&groups.0, &groups.1);
        let mut list = div()
            .id("changes-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();

        if staged.is_empty() && unstaged.is_empty() {
            let loaded = self.store.read(cx).changes_loaded_for.is_some();
            list = list.child(if loaded {
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
            });
            return list.into_any_element();
        }

        // order: unstaged first, then staged; headers render only when their
        // side has rows. Unstaged carries both whole-tree verbs — discard-all
        // (the arrow glyph the rows discard with, armed before it fires) and
        // stage-all — while staged only ever unstages: discarding the index
        // wholesale has no place there.
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        if !unstaged.is_empty() {
            let trailing = vec![
                discard_all_button(
                    self.discard_all_armed,
                    unstaged.iter().map(|f| f.path.clone()).collect(),
                    self.store.clone(),
                    cx.entity(),
                    show_tooltips,
                ),
                staging_link(
                    "stage-all",
                    "Stage all",
                    GitAction::StageAll,
                    self.store.clone(),
                ),
            ];
            list = list.child(section_header(
                "Unstaged Changes",
                unstaged.len(),
                false,
                trailing,
            ));
            for file in unstaged {
                list = list.child(self.render_row(file, false, cx));
            }
        }
        if !staged.is_empty() {
            list = list.child(section_header(
                "Staged Changes",
                staged.len(),
                true,
                vec![staging_link(
                    "stage-all-clear",
                    "Unstage all",
                    GitAction::UnstageAll,
                    self.store.clone(),
                )],
            ));
            for file in staged {
                list = list.child(self.render_row(file, true, cx));
            }
        }
        list.into_any_element()
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
        let status_color = status_color(file.status_code.as_str());
        let row_id: SharedString =
            format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let discard = {
            let store = self.store.clone();
            let this = cx.entity();
            let path = file.path.clone();
            let key = key.clone();
            div()
                .id(SharedString::from(format!(
                    "discard-{staged}-{}",
                    file.path
                )))
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
            .a11y_button(file.path.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .group("file-row")
            .relative()
            .flex_shrink_0()
            .h(px(34.))
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
                    .child(row_action_button(file, staged, self.store.clone()))
                    .child(discard),
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
            return div()
                .id("history-list")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_dim())
                .child("Loading history…")
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
                    .flex_row()
                    .items_center()
                    .gap_2()
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
                            .rounded_full()
                            .bg(theme::chip())
                            .w(px(22.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(theme::text_muted())
                            .child(row.initials.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
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
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_dim())
                                    .child(format!("{} · {}", row.author, row.rel)),
                            ),
                    )
                    .when(unpushed_mark, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(theme::font_meta()))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme::AHEAD)
                                .child("↑ unpushed"),
                        )
                    })
                    .child(
                        // hash chip: clicking copies the full SHA without
                        // drilling in
                        div()
                            .id(SharedString::from(format!("copy-sha-{}", row.short_hash)))
                            .a11y_button(format!("Copy SHA {}", row.short_hash))
                            .cursor_pointer()
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
                                        s.notify_success(format!("Copied {short} to clipboard"));
                                        cx.notify();
                                    });
                                }
                            })
                            .child(row.short_hash.clone()),
                    ),
            )
            .into_any_element()
    }

    fn render_commit_detail(
        &mut self,
        hash: String,
        details: Option<warp_station_core::model::GitCommitDetails>,
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

        let mut list = div()
            .id("commit-detail")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p_4()
            .gap_3();

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

        // two-click armed destructive actions
        let armed = self.detail_armed;
        let action_button = |id: &'static str, label: &'static str, armed_now: bool| {
            let text = if armed_now { "Confirm" } else { label };
            div()
                .id(id)
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

        // flex_shrink_0: the SHA beside it never wraps, so tight panes must
        // squeeze the files-changed text, not slide these buttons over it
        let mut actions_row = div().flex().flex_row().gap_2().flex_shrink_0();
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            div()
                .id("detail-copy-sha")
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
        // the list cuts a long subject off, so the detail shows it whole
        list = list.children(history_meta.as_ref().map(|(subject, ..)| {
            div()
                .debug_selector(|| "commit-subject".into())
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(subject.clone())
        }));

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
        list = list.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .text_size(px(theme::font_meta()))
                                .font_family(gpui::SharedString::from(theme::code_font()))
                                .text_color(theme::text_faint())
                                .child(details.hash[..7.min(details.hash.len())].to_string()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(format!(
                                    "{} files changed · +{additions} −{deletions}",
                                    details.files.len()
                                )),
                        ),
                )
                .child(actions_row),
        );

        list = list.child(div().id("detail-files").flex().flex_col().gap_0p5());

        let mut files = div().flex().flex_col().gap_0p5();
        for file in &details.files {
            let key = format!("{}|{}", details.hash, file.path);
            let expanded = self.expanded_details.contains(&key);
            let mut row = div()
                .id(SharedString::from(format!("detail-file-{}", file.path)))
                .a11y_tree_item(file.path.clone(), Some(expanded))
                .cursor_pointer()
                .flex()
                .flex_col()
                .rounded_md()
                .px_1()
                .when(expanded, |d| d.bg(theme::tint(theme::accent())))
                .hover(|s| s.bg(theme::hover_bg()))
                .on_click({
                    let hash = details.hash.clone();
                    let path = file.path.clone();
                    cx.listener(move |this, _, _, cx| this.toggle_detail_patch(&hash, &path, cx))
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .py_0p5()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_body()))
                                .font_family(gpui::SharedString::from(theme::code_font()))
                                .text_color(theme::text_dim())
                                .child(file.path.clone()),
                        )
                        .when_some(file.additions, |row, a| {
                            row.child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::SUCCESS_BRIGHT)
                                    .child(format!("+{a}")),
                            )
                        })
                        .when_some(file.deletions, |row, d| {
                            row.child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::DANGER)
                                    .child(format!("−{d}")),
                            )
                        }),
                );
            if expanded {
                let patch = self
                    .detail_patches
                    .get(&key)
                    .map(|p| p.as_str())
                    .unwrap_or("loading patch…");
                let handle = self
                    .patch_scrolls
                    .entry(file.path.clone())
                    .or_default()
                    .clone();
                row = row.child(
                    div()
                        .id(SharedString::from(format!("detail-patch-{}", file.path)))
                        .ml_4()
                        .mr_1()
                        .mb_1()
                        .rounded_md()
                        .bg(theme::card())
                        .border_1()
                        .border_color(theme::border_light())
                        .py_2()
                        .max_h(px(220.))
                        .overflow_hidden()
                        .overflow_y_scroll()
                        .track_scroll(&handle)
                        // the patch scrolls inside the commit detail's own
                        // scroller: an event it can act on is consumed here,
                        // so it never also moves the pane behind it, while an
                        // edge event still chains outward, browser-style
                        .on_scroll(move |event: &gpui::ScrollEvent, window, cx| {
                            let delta = event.delta.pixel_delta(window.line_height()).y;
                            let handle = &handle;
                            if wheel_reaches_content(handle.offset(), handle.max_offset(), delta) {
                                cx.stop_propagation();
                            }
                        })
                        .child(warpui_kit_diff::render_diff(
                            patch,
                            None,
                            theme::font_control(),
                            window,
                            cx,
                        )),
                );
            }
            files = files.child(row);
        }
        list = list.child(files);

        if let Some(body) = &details.body {
            let mut card = div()
                .id("detail-body")
                .rounded_md()
                .bg(theme::card())
                .border_1()
                .border_color(theme::border_light())
                .p_2()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_dim())
                .child(body.clone());
            if let Some((_, author, when)) = history_meta {
                card = card.child(
                    div()
                        .mt_1()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .text_size(px(theme::font_control()))
                        .text_color(theme::text_faint())
                        .child(div().child(author))
                        .child(div().child(when)),
                );
            }
            list = list.child(card);
        } else if let Some((_, author, when)) = history_meta {
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(theme::font_control()))
                    .text_color(theme::text_faint())
                    .child(div().child(author))
                    .child(div().child(when)),
            );
        }
        list.into_any_element()
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
        let (wand_active, wand_busy_elsewhere) = {
            let state = self.store.read(cx);
            let active = state.wand_busy == Some(crate::net::WandKind::Commit)
                && state.wand_repo.as_deref() == state.selected_repo.as_deref();
            (active, state.wand_busy.is_some() && !active)
        };
        let wand_on = self.store.read(cx).wand_available();
        let message_empty = self.commit_message.read(cx).value().trim().is_empty();
        // a run already going for another repo owns the one wand slot: this
        // box must not promise a generation that would be refused
        let can_generate = wand_on && !wand_busy_elsewhere;
        let generate_and_commit = message_empty && can_generate;

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
                        // the card carries one accent — the commit button
                        // under the box — so the box keeps its resting edge
                        // while the draft has focus instead of turning accent
                        // and reading as a second, competing action
                        .focus(|s| s.border_color(theme::hairline())),
                    )
                    // the corner wand: overlay inside the message box
                    .when(wand_on && !wand_busy_elsewhere, |box_area| {
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
                                        s.run_wand(crate::net::WandKind::Commit, cx)
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
            let this = cx.entity();
            let (generating, committing) = (
                wand_active,
                self.store.read(cx).running_job() == Some("commit"),
            );
            let working = generating || committing;
            let label = if generating {
                "Generating…"
            } else if committing {
                "Committing…"
            } else if generate_and_commit {
                "Generate & Commit"
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
                        // no wand, or another repo's run owns the slot: there
                        // is nothing this click could start
                        if !can_generate {
                            return;
                        }
                        // empty box: generate first; the drained draft
                        // lands in the textarea and then commits itself
                        let wand_busy = store.read(cx).wand_busy.is_some();
                        this.update(cx, |this, cx| {
                            this.auto_commit_after_draft = true;
                            cx.notify();
                        });
                        if !wand_busy {
                            store.update(cx, |s, cx| s.run_wand(crate::net::WandKind::Commit, cx));
                        }
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
                        if generate_and_commit {
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
    label: &str,
    count: usize,
    with_check: bool,
    trailing: Vec<gpui::AnyElement>,
) -> impl IntoElement + use<> {
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .py_1p5()
        .px_1()
        .when(with_check, |h| {
            h.child(icon(
                "icons/icon-check.svg",
                px(theme::icon_small() - 1.),
                theme::SUCCESS_DEEP,
            ))
        })
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
        )
        .child(div().flex_1());
    for element in trailing {
        header = header.child(element);
    }
    header
}

/// The unstaged header's discard-all: the row discard's arrow glyph, so the
/// whole-tree verb reads as the same action its rows offer. Its first click
/// arms it and its second throws every unstaged change away — the row
/// discards take their confirmation the same way.
fn discard_all_button(
    armed: bool,
    files: Vec<String>,
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
                    s.run_git(GitAction::DiscardUnstaged(files.clone()), cx)
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

/// A whole-list staging verb as a text action — no icon, no outline, just the
/// accent's ink with an underline on hover (the design's `.link`). The stage
/// footer and both section headers offer it, so the verb is learned once.
fn staging_link(
    id: &'static str,
    label: &'static str,
    action: GitAction,
    store: Entity<Store>,
) -> gpui::AnyElement {
    let selector = id.to_string();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .a11y_button(label)
        .cursor_pointer()
        .flex_shrink_0()
        // padding only grows the hit area: at rest the link is bare text
        .px(px(2.))
        .py(px(5.))
        .text_size(px(theme::font_meta()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::blue_bright())
        .hover(|s| s.underline())
        .on_click(move |_, _, cx| {
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(label)
        .into_any_element()
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

fn status_color(code: &str) -> gpui::Rgba {
    match code {
        "U" | "?" | "A" => theme::SUCCESS_BRIGHT,
        "D" => theme::DANGER,
        "R" => theme::blue_bright(),
        "C" => theme::AMBER_BRIGHT,
        _ => theme::AMBER_BRIGHT,
    }
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
    div()
        .id(SharedString::from(format!("toggle-{staged}-{}", file.path)))
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

/// True when a vertical scroller at `offset` (≤ 0, clamped to `max_offset`)
/// has room to move for `delta` — the "the inner one handled it" test. A
/// wheel event routed to an inner scrollable must not also scroll an outer
/// one, but a scroller already hard against the edge in the wheel's
/// direction lets the event pass, as browsers chain it. The notes app's
/// proven pattern, copied rather than shared.
fn wheel_reaches_content(
    offset: gpui::Point<gpui::Pixels>,
    max_offset: gpui::Point<gpui::Pixels>,
    delta: gpui::Pixels,
) -> bool {
    if delta < px(0.) {
        // scrolling toward the bottom: room is whatever is left below
        -offset.y < max_offset.y
    } else {
        // scrolling back toward the top: any offset off the top moves
        offset.y < px(0.)
    }
}
