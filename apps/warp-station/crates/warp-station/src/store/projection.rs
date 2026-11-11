//! The repos list projection: filter (search text, tag chip) + sort (mode,
//! favorites always floating to the top), optionally bucketed into folder
//! groups. Pure functions over the entry list, so ordering rules are
//! unit-testable without the store.

use std::ffi::OsString;
use std::path::PathBuf;

use warp_station_core::paths::same_folder_text;
use warp_station_core::repo_store::{ARCHIVED_TAG, FAVORITES_TAG, RepoEntry};
use warp_station_core::settings::RepoSortMode;

/// One row of the list: either a repo entry or a collapsible folder group
/// header. Grouping is a pure projection — the header rows interleave with
/// the repo rows, so no per-row heights are needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectedRow {
    Group {
        /// Stable identity for collapse/solo state: the group's canonical
        /// folder path (or "other" for repos with no folder above them).
        key: String,
        /// Display name: the folder's last path segment, as the filesystem
        /// spells it ("Other" for other).
        name: String,
        /// Repos currently projected in the group (post-filter).
        count: usize,
        collapsed: bool,
    },
    Repo(usize),
}

impl ProjectedRow {
    /// The repo index when this row is a repo, None for group headers.
    pub fn repo_index(&self) -> Option<usize> {
        match self {
            ProjectedRow::Repo(ix) => Some(*ix),
            ProjectedRow::Group { .. } => None,
        }
    }
}

/// Grouping inputs: the scan roots that admit repos into the list, the
/// folder level the headers sit at, the persisted collapsed keys, and the
/// session-only solo filter. A `None` grouping keeps the historical flat
/// list (repo rows only, no headers).
pub struct Grouping<'a> {
    pub roots: &'a [String],
    /// Folder levels above each repo's own folder: 1 = the folder that
    /// contains it, 2 = that folder's parent, and so on. Every repo under a
    /// root groups under its own parent at that level, so two siblings fold
    /// together and two repos with different parents stay apart. A volume (a
    /// drive letter, a UNC server, the filesystem root) is not a folder, so it
    /// never becomes a header; ascension stops at the topmost named folder.
    pub depth: usize,
    pub collapsed: &'a std::collections::HashSet<String>,
    /// When set, only this group's repos are projected (the header's
    /// solo-filter icon; the toolbar's Clear removes it).
    pub solo: Option<&'a str>,
}

/// The fallback group for repos under no scan root (or with no named folder
/// to group under).
pub const OTHER_GROUP_KEY: &str = "other";
/// Display name of the fallback group.
pub const OTHER_GROUP_NAME: &str = "Other";

/// The projection without an attention scope.
#[cfg(test)]
pub fn project(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
    show_archived: bool,
    grouping: Option<&Grouping>,
) -> Vec<ProjectedRow> {
    project_scoped(
        repos,
        checked_tags,
        sort,
        ascending,
        show_archived,
        false,
        grouping,
    )
}

/// The projection with the scope row's attention filter: `attention_only`
/// keeps just the repos something is waiting on.
pub fn project_scoped(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
    show_archived: bool,
    attention_only: bool,
    grouping: Option<&Grouping>,
) -> Vec<ProjectedRow> {
    // One shared sort for both shapes: the comparators (favorites first,
    // archived last, mode order, name tiebreak) are identical, so grouping
    // only re-buckets the sorted rows — a group's repos keep the flat order.
    let sorted = sort_indices(
        repos,
        checked_tags,
        sort,
        ascending,
        show_archived,
        attention_only,
    );
    let Some(grouping) = grouping else {
        return sorted.into_iter().map(ProjectedRow::Repo).collect();
    };

    // OR-tag filter first (as flat), then the solo filter: only the soloed
    // group's repos survive, the rest of the buckets come up empty.
    let solo = grouping.solo;
    // roots that resolve away from their lexical text (a symlink, a mount
    // alias) mean the repos need the same resolution before their parents
    // are compared: resolve once per projection, not per row on every list
    let resolve_aliases = grouping.roots.iter().any(|root| {
        let lexical = root.trim_end_matches(['/', '\\']);
        let resolved = canonical_text(root);
        !same_folder_text(lexical, resolved.trim_end_matches(['/', '\\']))
    });
    let mut groups: Vec<(String, String, Vec<usize>)> = Vec::new();
    for &ix in &sorted {
        let (key, name) = assign_group(
            &repos[ix].folder_path.to_string_lossy(),
            grouping.roots,
            grouping.depth,
            resolve_aliases,
        );
        if solo.is_some_and(|solo| !keys_match(solo, &key)) {
            continue;
        }
        let bucket = groups
            .iter_mut()
            .find(|(bucket_key, _, _)| keys_match(bucket_key, &key));
        match bucket {
            Some((_, _, rows)) => rows.push(ix),
            None => groups.push((key, name, vec![ix])),
        }
    }

    // buckets sort by key so the projection stays deterministic even when
    // the settings file lists the roots unsorted
    groups.sort_by_key(|(key, _, _)| key.to_lowercase());
    // the archive partition's "listed bottom": buckets holding only archived
    // repos sink below buckets with any live one. Stable, so the key order
    // holds inside each region and "other" still trails its own region. The
    // exclusive archive filter projects archived rows only, so every bucket
    // carries the same key there and the rule only keeps "other" last.
    groups.sort_by_key(|(key, _, bucket)| {
        (
            bucket.iter().all(|&ix| repos[ix].is_archived()),
            key == OTHER_GROUP_KEY,
        )
    });
    let mut rows = Vec::new();
    for (key, name, bucket) in &groups {
        push_group(&mut rows, key, name, bucket, grouping.collapsed);
    }
    rows
}

fn push_group(
    rows: &mut Vec<ProjectedRow>,
    key: &str,
    name: &str,
    bucket: &[usize],
    collapsed: &std::collections::HashSet<String>,
) {
    // empty buckets never happen post-filter, but a soloed-away group must
    // not leave a header behind either — drop them here, uniformly
    if bucket.is_empty() {
        return;
    }
    let is_collapsed = collapsed.iter().any(|k| keys_match(k, key));
    rows.push(ProjectedRow::Group {
        key: key.to_string(),
        name: name.to_string(),
        count: bucket.len(),
        collapsed: is_collapsed,
    });
    if is_collapsed {
        return;
    }
    rows.extend(bucket.iter().copied().map(ProjectedRow::Repo));
}

fn keys_match(a: &str, b: &str) -> bool {
    // Windows paths are case-insensitive: two spellings of one parent must
    // land in one bucket, not two headers that differ only by case.
    same_folder_text(a, b)
}

/// A repo groups only when some root contains its folder (roots may
/// themselves be repo folders); no match → the "other" fallback group.
/// Otherwise it groups under the folder `depth` levels above its own: depth 1
/// is the folder containing the repo, 2 that folder's parent, and so on, so
/// the header always names a real parent rather than a repo folder.
fn assign_group(
    folder: &str,
    roots: &[String],
    depth: usize,
    resolve_aliases: bool,
) -> (String, String) {
    if !roots.iter().any(|root| is_under(folder, root)) {
        return (OTHER_GROUP_KEY.to_string(), OTHER_GROUP_NAME.to_string());
    }
    let base = if resolve_aliases {
        canonical_text(folder)
    } else {
        folder.to_string()
    };
    let base = base.trim_end_matches(['/', '\\']).to_string();
    let key = ancestor_folder(&base, depth);
    // no named folder above the repo itself: there is nothing to name a
    // group after (a repo at a volume's root lands here)
    if key == base && depth > 0 {
        return (OTHER_GROUP_KEY.to_string(), OTHER_GROUP_NAME.to_string());
    }
    // a header is a folder, never a volume: a `C:\` scan root has no name of
    // its own to group under, so its repos fall back like unreachable ones
    if !has_named_folder(&key) {
        return (OTHER_GROUP_KEY.to_string(), OTHER_GROUP_NAME.to_string());
    }
    let name = folder_name(&key).to_string();
    (key, name)
}

/// One level higher, `levels` times. Ascension stops at the filesystem root,
/// whose segment would name nothing, so an over-deep level folds into the
/// topmost named folder instead.
fn ancestor_folder(base: &str, levels: usize) -> String {
    let mut current = base.trim_end_matches(['/', '\\']).to_string();
    for _ in 0..levels {
        match parent_folder(&current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    current
}

/// Path components in order, either separator, empty segments dropped.
fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect()
}

/// How many leading components name the volume rather than a folder: one for a
/// drive (`C:`) and one for a UNC server (`\\server` — the share name below it
/// is a folder the user sees). Nothing to skip on a Unix path.
fn volume_components(path: &str) -> usize {
    let parts = components(path);
    let Some(first) = parts.first() else {
        return 0;
    };
    if first.len() == 2 && first.ends_with(':') && first.as_bytes()[0].is_ascii_alphabetic() {
        return 1;
    }
    if path.starts_with("\\\\") || path.starts_with("//") {
        return 1;
    }
    0
}

/// Whether the folder has a name below its volume root: `C:`, `\\server` and
/// `/` do not, so a header never names itself after one.
fn has_named_folder(path: &str) -> bool {
    components(path).len() > volume_components(path)
}

/// The folder's text with symlinks resolved, even when the folder itself no
/// longer exists: the longest existing prefix is canonicalized and the
/// missing tail is re-appended (so a stale scan root still resolves through
/// its parent instead of splitting the shared tree). Falls back to the
/// lexical path when nothing above it exists.
fn canonical_text(folder: &str) -> String {
    // Only an absolute path may resolve upward: Windows answers
    // canonicalize("/") with the current drive's root, so a drive-relative
    // spelling (`/work`, `/home/u`) would come back as `C:\work` and split
    // the group keys away from the text the list was built with. Unix
    // absolute paths stay absolute there, and real Windows roots
    // (`C:\…`, `\\?\…`, UNC) are absolute in this sense too.
    if !PathBuf::from(folder).is_absolute() {
        return folder.to_string();
    }
    let mut path = PathBuf::from(folder);
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(resolved) = std::fs::canonicalize(&path) {
            let mut full = resolved;
            for part in tail.iter().rev() {
                full.push(part);
            }
            return strip_verbatim_prefix(&full.to_string_lossy());
        }
        let Some(name) = path.file_name().map(|name| name.to_os_string()) else {
            return folder.to_string();
        };
        tail.push(name);
        let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        else {
            return folder.to_string();
        };
        path = parent.to_path_buf();
    }
}

/// Windows canonicalization prefixes verbatim paths (`\\?\C:\…`); strip it
/// so the key is the path the user sees (and the selector stays readable).
fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// The folder one level up, with either separator (Windows paths stay
/// readable in tests on every platform). None at the topmost named folder:
/// `/work`, `C:\dev` and `\\server\share` have no parent left to group by, so
/// a larger depth keeps them rather than inventing `/`, `C:` or `\\server` as
/// a header.
fn parent_folder(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let sep = trimmed.rfind(['/', '\\'])?;
    let parent = trimmed[..sep].trim_end_matches(['/', '\\']);
    if parent.is_empty() || !has_named_folder(parent) {
        return None;
    }
    Some(parent.to_string())
}

/// The last named segment, with either separator. Empty for a root (`/`,
/// `C:\`); callers guard those with [`has_named_folder`].
fn folder_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(sep) => &trimmed[sep + 1..],
        None => trimmed,
    }
}

/// Folder identity with prefix semantics: equal, or below the root through a
/// path separator. Roots pointing at repo folders match themselves.
fn is_under(folder: &str, root: &str) -> bool {
    let root = root.trim_end_matches(['/', '\\']);
    if root.is_empty() {
        return false;
    }
    let folder = folder.trim_end_matches(['/', '\\']);
    if same_folder_text(folder, root) {
        return true;
    }
    // get() keeps a non-char-boundary slice from panicking on odd paths
    match folder.get(..root.len()) {
        Some(prefix) if folder.len() > root.len() => {
            same_folder_text(prefix, root)
                && (folder.as_bytes()[root.len()] == b'/' || folder.as_bytes()[root.len()] == b'\\')
        }
        _ => false,
    }
}

/// The flat pipeline: archive split → OR-tag filter → sort. Returns
/// indices into `repos`; grouping re-buckets these without re-sorting.
/// A repo the "Needs attention" scope keeps: something is waiting on the
/// developer — uncommitted work, an open issue, or commits still to push.
pub fn needs_attention(repo: &RepoEntry) -> bool {
    repo.git_modified_count > 0 || repo.issue_total() > 0 || repo.git_to_push_count > 0
}

fn sort_indices(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
    show_archived: bool,
    attention_only: bool,
) -> Vec<usize> {
    let mut rows: Vec<usize> = repos
        .iter()
        .enumerate()
        // `show_archived` is the exclusive Archive filter: on lists archived
        // repos alone, off lists the live ones. The split is independent of
        // the OR-tag filter so an archived repo never satisfies a checked
        // chip outside the Archive view.
        .filter(|(_, repo)| repo.is_archived() == show_archived)
        .filter(|(_, repo)| !attention_only || needs_attention(repo))
        .filter(|(_, repo)| {
            // OR semantics: a repo matches when it carries any checked tag
            checked_tags.is_empty()
                || checked_tags.iter().any(|t| {
                    t.eq_ignore_ascii_case(FAVORITES_TAG) && repo.is_favorite()
                        || repo.tags.iter().any(|rt| rt.eq_ignore_ascii_case(t))
                })
        })
        .map(|(ix, _)| ix)
        .collect();

    // case-insensitive name order needs the lowercase form twice (the Name
    // mode and the universal tiebreaker); precompute once instead of two
    // allocations per comparison
    let names_lower: Vec<String> = repos.iter().map(|repo| repo.name.to_lowercase()).collect();
    let name_order = |ascending: bool, a: usize, b: usize| {
        if ascending {
            names_lower[a].cmp(&names_lower[b])
        } else {
            names_lower[b].cmp(&names_lower[a])
        }
    };

    rows.sort_by(|&a, &b| {
        let (ra, rb) = (&repos[a], &repos[b]);
        // archived repos sink below every live one when a projection mixes
        // the two (the exclusive archive filter keeps one partition only, so
        // this is a no-op there); favorites float within their partition;
        // then the mode's order — `ascending` flips the comparator around the
        // mode's default; name breaks ties. Repos without a commit date sink
        // in BOTH LastActivity directions.
        ra.is_archived()
            .cmp(&rb.is_archived())
            .then_with(|| rb.is_favorite().cmp(&ra.is_favorite()))
            .then_with(|| match sort {
                RepoSortMode::Name => name_order(ascending, a, b),
                RepoSortMode::LastActivity => {
                    order_opt(ascending, ra.git_last_commit_at, rb.git_last_commit_at)
                }
                RepoSortMode::Branch => order_opt(
                    ascending,
                    ra.git_branch_name.as_deref(),
                    rb.git_branch_name.as_deref(),
                ),
                RepoSortMode::Changes => {
                    order_key(ascending, changes_weight(ra), changes_weight(rb))
                }
                RepoSortMode::GitHub => order_key(
                    ascending,
                    ra.pull_total() + ra.issue_total(),
                    rb.pull_total() + rb.issue_total(),
                ),
                RepoSortMode::Attention => {
                    order_key(ascending, needs_attention(ra), needs_attention(rb)).then_with(|| {
                        order_opt(false, ra.git_last_commit_at, rb.git_last_commit_at)
                    })
                }
            })
            .then_with(|| names_lower[a].cmp(&names_lower[b]))
    });
    rows
}

/// Options order by their value, but `None` sinks regardless of direction.
fn order_opt<T: Ord + Copy>(ascending: bool, a: Option<T>, b: Option<T>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => order_key(ascending, a, b),
    }
}

fn order_key<T: Ord + Copy>(ascending: bool, a: T, b: T) -> std::cmp::Ordering {
    if ascending { a.cmp(&b) } else { b.cmp(&a) }
}

fn changes_weight(repo: &RepoEntry) -> u32 {
    repo.git_modified_count * 10 + repo.git_to_push_count * 5 + repo.git_to_pull_count
}

/// Every distinct tag across the list; favorites and platform always lead.
/// The archived marker is a mechanism, not a filter chip — it never lists.
pub fn all_tags(repos: &[RepoEntry]) -> Vec<String> {
    let mut tags: Vec<String> = vec![
        FAVORITES_TAG.to_string(),
        warp_station_core::scan::PLATFORM_TAG.to_string(),
    ];
    for repo in repos {
        for tag in &repo.tags {
            if tag.eq_ignore_ascii_case(ARCHIVED_TAG) {
                continue;
            }
            if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.clone());
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
        RepoEntry {
            name: name.into(),
            folder_path: PathBuf::from(path),
            solution_path: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: false,
            github_pr_count: 0,
            github_issue_count: 0,
            azure_pull_count: 0,
            github_web_url: None,
            azure_work_item_count: 0,
        }
    }

    fn repos() -> Vec<RepoEntry> {
        vec![
            repo("alpha", "/x/alpha", &["favorites"]),
            repo("beta", "/x/beta", &["work"]),
            repo("gamma", "/x/gamma", &[]),
        ]
    }

    /// The flat projection (grouping off), as the tests below read it.
    fn flat(
        repos: &[RepoEntry],
        tags: &[String],
        mode: RepoSortMode,
        asc: bool,
        archived: bool,
    ) -> Vec<ProjectedRow> {
        project(repos, tags, mode, asc, archived, None)
    }

    /// Names of the projected repo rows; panics on an unexpected header so
    /// the flat-mode assertions fail loudly when grouping leaks in.
    fn repo_names<'a>(rows: &[ProjectedRow], all: &'a [RepoEntry]) -> Vec<&'a str> {
        rows.iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => all[*ix].name.as_str(),
                ProjectedRow::Group { .. } => {
                    panic!("unexpected group header in a flat projection")
                }
            })
            .collect()
    }

    /// A compact read of a grouped projection for assertions: header keys
    /// prefixed "#", repo names as-is.
    fn sketch(rows: &[ProjectedRow], all: &[RepoEntry]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => all[*ix].name.to_string(),
                ProjectedRow::Group { key, count, .. } => format!("#{key}({count})"),
            })
            .collect()
    }

    /// The default grouping for the /roots fixtures: two roots in disjoint
    /// trees, so depth 1 keeps their folders apart (a shared tree would fold
    /// every repo into one header).
    fn grouping<'a>(
        roots: &'a [String],
        collapsed: &'a std::collections::HashSet<String>,
    ) -> Grouping<'a> {
        grouping_at(roots, collapsed, 1)
    }

    fn grouping_at<'a>(
        roots: &'a [String],
        collapsed: &'a std::collections::HashSet<String>,
        depth: usize,
    ) -> Grouping<'a> {
        Grouping {
            roots,
            depth,
            collapsed,
            solo: None,
        }
    }

    #[test]
    fn favorites_float_and_name_sorts() {
        let all = repos();
        let rows = flat(&all, &[], RepoSortMode::Name, true, false);
        assert_eq!(repo_names(&rows, &all), vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn tag_filter_favorites_and_user_tags() {
        let all = repos();
        let rows = flat(
            &all,
            &["favorites".to_string()],
            RepoSortMode::Name,
            true,
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "alpha");

        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "beta");
    }

    #[test]
    fn checked_tags_union_their_repos() {
        let all = repos();
        let rows = flat(
            &all,
            &["favorites".to_string(), "work".to_string()],
            RepoSortMode::Name,
            true,
            false,
        );
        assert_eq!(rows.len(), 2);
        let names: Vec<&str> = rows
            .iter()
            .map(|row| all[row.repo_index().unwrap()].name.as_str())
            .collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn all_tags_leads_with_favorites_then_platform() {
        let mut all = repos();
        all[1].tags.push("platform".into());
        let tags = all_tags(&all);
        assert_eq!(tags[0], "favorites");
        assert_eq!(tags[1], "platform");
        assert!(tags.contains(&"work".to_string()));
    }

    #[test]
    fn name_direction_flips_and_favorites_still_float() {
        let all = repos();
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Name, true, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
        // alpha is a favorite, so it leads in both directions
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Name, false, false), &all),
            vec!["alpha", "gamma", "beta"]
        );
    }

    #[test]
    fn attention_sort_leads_with_waiting_repos_newest_first() {
        let mut all = repos();
        all[0].tags.clear();
        // alpha is quiet but the newest; beta and gamma wait on the developer
        all[0].git_last_commit_at = Some(chrono::Local::now());
        all[1].git_modified_count = 1;
        all[1].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
        all[2].git_to_push_count = 1;
        all[2].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(1));
        assert_eq!(
            repo_names(
                &flat(&all, &[], RepoSortMode::Attention, false, false),
                &all
            ),
            vec!["gamma", "beta", "alpha"],
            "waiting repos lead, newest first"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Attention, true, false), &all),
            vec!["alpha", "gamma", "beta"],
            "the flip moves the line, not the recency inside each side"
        );
    }

    #[test]
    fn last_activity_flips_but_unknown_dates_stay_last() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
        all[1].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(1));
        assert_eq!(
            repo_names(
                &flat(&all, &[], RepoSortMode::LastActivity, false, false),
                &all
            ),
            vec!["beta", "alpha", "gamma"],
            "descending leads with the most recent commit"
        );
        assert_eq!(
            repo_names(
                &flat(&all, &[], RepoSortMode::LastActivity, true, false),
                &all
            ),
            vec!["alpha", "beta", "gamma"],
            "ascending leads with the oldest commit; no date still sinks"
        );
    }

    #[test]
    fn changes_direction_flips_best_first() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_modified_count = 5;
        all[1].git_modified_count = 2;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Changes, false, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Changes, true, false), &all),
            vec!["gamma", "beta", "alpha"]
        );
    }

    #[test]
    fn branch_sorts_alphabetically_and_unknown_sinks() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_branch_name = Some("zeta".into());
        all[1].git_branch_name = Some("alpha".into());
        // gamma stays on the entry default ("main" in most setups); give it a
        // distinct middle value here
        all[2].git_branch_name = Some("main".into());
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, true, false), &all),
            vec!["beta", "gamma", "alpha"],
            "ascending is alphabetical by branch"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, false, false), &all),
            vec!["alpha", "gamma", "beta"],
            "descending flips the branch order"
        );

        all[2].git_branch_name = None;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, true, false), &all),
            vec!["beta", "alpha", "gamma"],
            "a repo without a branch sinks in both directions"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, false, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn github_sort_sums_prs_and_issues() {
        let mut all = repos();
        all[0].tags.clear();
        all[1].github_pr_count = 3;
        all[2].github_issue_count = 4;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::GitHub, false, false), &all),
            vec!["gamma", "beta", "alpha"],
            "descending leads with the most open PRs + issues"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::GitHub, true, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn the_archive_filter_lists_only_the_archived_repos() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        let names = |show: bool| -> Vec<&str> {
            repo_names(&flat(&all, &[], RepoSortMode::Name, true, show), &all)
        };
        assert_eq!(
            names(false),
            vec!["alpha", "gamma"],
            "the archived repo hides by default"
        );
        assert_eq!(
            names(true),
            vec!["beta"],
            "the Archive filter lists the archived repos alone"
        );
    }

    #[test]
    fn the_archive_view_keeps_favorites_first_within_itself() {
        let mut all = repos();
        // alpha: live favorite; beta: archived favorite; gamma: plain archived
        all[1].tags = vec!["archived".into(), "favorites".into()];
        all[2].tags = vec!["archived".into()];
        all[0].git_branch_name = Some("zeta".into());
        let rows = flat(&all, &[], RepoSortMode::Branch, true, true);
        assert_eq!(
            repo_names(&rows, &all),
            vec!["beta", "gamma"],
            "the archive view holds only archived rows; favorites lead inside it"
        );
    }

    #[test]
    fn archived_repo_matching_a_checked_tag_stays_hidden() {
        let mut all = repos();
        all[1].tags.push("work".into());
        all[1].tags.push("archived".into());
        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, false);
        assert!(
            rows.is_empty(),
            "the archived flag excludes before the OR-tag filter"
        );
        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "beta");
    }

    #[test]
    fn all_tags_never_lists_the_archived_marker() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        all[1].tags.push("work".into());
        let tags = all_tags(&all);
        assert!(
            !tags.iter().any(|t| t.eq_ignore_ascii_case("archived")),
            "archived is a mechanism, not a filter chip"
        );
        assert!(tags.contains(&"work".to_string()));
    }

    // ---- grouping ----------------------------------------------------------

    fn grouped_repos() -> Vec<RepoEntry> {
        vec![
            repo("alpha", "/roots/a/alpha", &[]),
            repo("beta", "/roots/a/beta", &[]),
            repo("gamma", "/second/b/gamma", &[]),
            repo("delta", "/elsewhere/delta", &[]),
        ]
    }

    #[test]
    fn depth_counts_up_from_each_repos_parent() {
        let all = vec![
            repo("alpha", "/home/u/work/alpha", &[]),
            repo("beta", "/home/u/work/rust/beta", &[]),
            repo("gamma", "/home/u/work/rust/deep/gamma", &[]),
        ];
        let roots = vec!["/home/u/work".to_string(), "/home/u/work/rust".to_string()];
        let collapsed = std::collections::HashSet::new();
        let sketch_at = |depth| {
            let rows = project(
                &all,
                &[],
                RepoSortMode::Name,
                true,
                false,
                Some(&grouping_at(&roots, &collapsed, depth)),
            );
            sketch(&rows, &all)
        };
        assert_eq!(
            sketch_at(1),
            vec![
                "#/home/u/work(1)",
                "alpha",
                "#/home/u/work/rust(1)",
                "beta",
                "#/home/u/work/rust/deep(1)",
                "gamma",
            ],
            "depth 1 groups each repo under the folder that contains it"
        );
        assert_eq!(
            sketch_at(2),
            vec![
                "#/home/u(1)",
                "alpha",
                "#/home/u/work(1)",
                "beta",
                "#/home/u/work/rust(1)",
                "gamma",
            ],
            "depth 2 raises each header to the parent's parent"
        );
        assert_eq!(
            sketch_at(3),
            vec![
                "#/home(1)",
                "alpha",
                "#/home/u(1)",
                "beta",
                "#/home/u/work(1)",
                "gamma",
            ],
            "depth 3 keeps climbing: the header always names a real folder"
        );

        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        let ProjectedRow::Group {
            name,
            count,
            collapsed: open,
            ..
        } = &rows[0]
        else {
            panic!("expected a group header");
        };
        assert_eq!(name, "work", "the header names itself after the parent");
        assert_eq!(*count, 1, "only the repo directly under it joins");
        assert!(!open, "groups start expanded");
    }

    #[test]
    fn repos_under_no_root_land_in_other() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/roots/a(2)",
                "alpha",
                "beta",
                "#/second/b(1)",
                "gamma",
                "#other(1)",
                "delta"
            ],
            "the fallback group sorts last and names itself Other"
        );
        let ProjectedRow::Group { key, name, .. } = &rows[5] else {
            panic!("expected the other-group header");
        };
        assert_eq!(key, "other");
        assert_eq!(name, "Other");
    }

    #[test]
    fn an_empty_other_group_leaves_no_header() {
        let mut all = grouped_repos();
        all[3].folder_path = PathBuf::from("/second/b/moved");
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/roots/a(2)",
                "alpha",
                "beta",
                "#/second/b(2)",
                "delta",
                "gamma"
            ],
            "every repo sits under a root, so no fallback header appears; repos sort by name within the group"
        );
    }

    #[test]
    fn grouping_disabled_matches_the_flat_projection() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        let rows = project(&all, &[], RepoSortMode::Name, true, true, None);
        assert_eq!(
            repo_names(&rows, &all),
            vec!["beta"],
            "grouping off reproduces the archive view's flat projection with no headers"
        );
    }

    #[test]
    fn collapsing_a_group_leaves_only_its_header() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed: std::collections::HashSet<String> = ["/roots/a".to_string()].into();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/roots/a(2)",
                "#/second/b(1)",
                "gamma",
                "#other(1)",
                "delta"
            ],
            "the collapsed group keeps its header and count but drops its repo rows"
        );
    }

    #[test]
    fn tag_filter_composes_with_groups_and_drops_empty_ones() {
        let mut all = grouped_repos();
        all[0].tags.push("work".into()); // alpha only
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &["work".to_string()],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(1)", "alpha"],
            "the OR-tag filter applies per repo; groups without matches vanish"
        );
    }

    #[test]
    fn archive_rows_stay_within_their_group() {
        let mut all = grouped_repos();
        all[1].tags.push("archived".into()); // beta archives under /roots/a
        all[3].tags.push("archived".into()); // delta archives under other
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            true,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(1)", "beta", "#other(1)", "delta"],
            "the Archive view projects the archived rows only, each under its own group header"
        );
    }

    #[test]
    fn the_archive_view_lists_every_group_with_an_archived_repo() {
        let mut all = grouped_repos();
        all[0].tags.push("archived".into()); // alpha archives → /roots/a all-archived
        all[1].tags.push("archived".into()); // so does beta
        all[3].tags.push("archived".into()); // and delta, making "other" all-archived too
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            true,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "alpha", "beta", "#other(1)", "delta"],
            "the Archive view drops gamma's live-only group and keeps the archived groups"
        );
    }

    #[test]
    fn the_archive_view_lists_only_the_archived_rows_of_a_group() {
        let mut all = grouped_repos();
        all[1].tags.push("archived".into()); // only beta archives; /roots/a keeps a live repo
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            true,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(1)", "beta"],
            "only beta is archived, so the Archive view projects just beta under its group"
        );
    }

    #[test]
    fn with_the_archive_filter_off_live_groups_remain_alone() {
        let mut all = grouped_repos();
        all[0].tags.push("archived".into());
        all[1].tags.push("archived".into());
        all[3].tags.push("archived".into());
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/second/b(1)", "gamma"],
            "archive filter off: archived repos leave the projection, so no archived region and no ghost headers"
        );
    }

    #[test]
    fn solo_filter_keeps_only_that_group() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/second/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let solo = Grouping {
            roots: &roots,
            depth: 1,
            collapsed: &collapsed,
            solo: Some("/roots/a"),
        };
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&solo));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "alpha", "beta"],
            "soloing one group hides the other groups entirely"
        );

        let solo_other = Grouping {
            roots: &roots,
            depth: 1,
            collapsed: &collapsed,
            solo: Some("other"),
        };
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&solo_other),
        );
        assert_eq!(sketch(&rows, &all), vec!["#other(1)", "delta"]);
    }

    #[test]
    fn roots_may_point_at_repo_folders_themselves() {
        // scan roots are repo folders too (the adopt/add flow persists them),
        // so a root must match its own folder, not only subfolders; the
        // header counts from the repo's parent, never from the repo itself
        let all = vec![repo("alpha", "/repos/alpha", &[])];
        let roots = vec!["/repos/alpha".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(sketch(&rows, &all), vec!["#/repos(1)", "alpha"]);
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/repos(1)", "alpha"],
            "depth 1 is the repo's parent; ascending past the topmost named folder keeps it"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 9)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/repos(1)", "alpha"],
            "depth past the topmost named folder keeps that folder, not /"
        );
    }

    #[test]
    fn depth_one_groups_by_the_repos_parent() {
        let all = vec![
            repo("alpha", "/roots/a/work/alpha", &[]),
            repo("beta", "/roots/a/work/beta", &[]),
            repo("gamma", "/roots/a/side/gamma", &[]),
        ];
        let roots = vec!["/roots/a".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/roots/a/side(1)",
                "gamma",
                "#/roots/a/work(2)",
                "alpha",
                "beta"
            ],
            "depth 1 groups by the repo's own parent, not the scan root"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(3)", "alpha", "beta", "gamma"],
            "depth 2 raises both parents to the shared folder"
        );
        let ProjectedRow::Group { name, .. } = &rows[0] else {
            panic!("expected a group header");
        };
        assert_eq!(name, "a", "the header names itself after the folder");
    }

    #[test]
    fn an_over_deep_level_folds_into_the_topmost_named_folder() {
        let all = vec![repo("alpha", "/roots/a/work/deep/alpha", &[])];
        let roots = vec!["/roots/a".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 9)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots(1)", "alpha"],
            "walking past the topmost folder keeps it rather than inventing a / header"
        );
    }

    #[test]
    fn depth_levels_count_up_from_each_repo() {
        let all = vec![
            repo("alpha", "/home/u/work/rust/alpha", &[]),
            repo("beta", "/home/u/work/go/beta", &[]),
        ];
        let roots = vec!["/home/u/work".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/home/u/work/go(1)",
                "beta",
                "#/home/u/work/rust(1)",
                "alpha"
            ],
            "depth 1 keeps each repo under its own parent"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/home/u/work(2)", "alpha", "beta"],
            "depth 2 raises both parents to the folder they share"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 3)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/home/u(2)", "alpha", "beta"],
            "depth 3 keeps climbing, still one group"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 4)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/home(2)", "alpha", "beta"],
            "depth 4 reaches the topmost named folder"
        );
    }

    #[test]
    fn leaf_roots_group_under_their_own_parents() {
        // the adopt/add flow seeds one root per repo folder: depth 1 names
        // each repo's parent, so a nested repo keeps its own parent's header
        // instead of folding into the folder all the roots share
        let all = vec![
            repo("alpha", "/home/e/Developments/Warp Station", &[]),
            repo("beta", "/home/e/Developments/lumux", &[]),
            repo("gamma", "/home/e/Developments/test/lumux", &[]),
        ];
        let roots = vec![
            "/home/e/Developments/Warp Station".to_string(),
            "/home/e/Developments/lumux".to_string(),
            "/home/e/Developments/test/lumux".to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/home/e/Developments(2)",
                "alpha",
                "beta",
                "#/home/e/Developments/test(1)",
                "gamma",
            ],
            "depth 1 hangs the sibling roots under Developments and the nested one under test"
        );
        let ProjectedRow::Group { name, .. } = &rows[0] else {
            panic!("expected a group header");
        };
        assert_eq!(
            name, "Developments",
            "the header keeps the parent folder's own spelling"
        );

        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/home/e(2)",
                "alpha",
                "beta",
                "#/home/e/Developments(1)",
                "gamma",
            ],
            "depth 2 raises each parent one folder: the roots' shared folder reappears"
        );
    }

    #[test]
    fn disjoint_roots_group_under_their_parents() {
        let all = vec![repo("alpha", "/work/x", &[]), repo("beta", "/mnt/y", &[])];
        let roots = vec!["/work/x".to_string(), "/mnt/y".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/mnt(1)", "beta", "#/work(1)", "alpha"],
            "depth 1 names each repo's parent folder"
        );
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/mnt(1)", "beta", "#/work(1)", "alpha"],
            "depth 2 stops at the topmost named folder, not at /"
        );
    }

    /// Three repo-folder roots with three different parents render three
    /// headers, each named after its parent — not two roots folded under a
    /// shared ancestor and the third under the repo itself.
    #[test]
    fn three_repo_roots_with_different_parents_keep_three_headers() {
        let all = vec![
            repo("alpha", r"C:\Projects\Alpha\repo-alpha", &[]),
            repo("beta", r"C:\Projects\Beta\repo-beta", &[]),
            repo("gamma", r"D:\Personal\Gamma\repo-gamma", &[]),
        ];
        let roots = vec![
            r"C:\Projects\Alpha\repo-alpha".to_string(),
            r"C:\Projects\Beta\repo-beta".to_string(),
            r"D:\Personal\Gamma\repo-gamma".to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                r"#C:\Projects\Alpha(1)",
                "alpha",
                r"#C:\Projects\Beta(1)",
                "beta",
                r"#D:\Personal\Gamma(1)",
                "gamma",
            ],
            "each repo groups under its own parent, never a shared ancestor or itself"
        );
    }

    #[test]
    fn depth_extends_over_backslash_paths() {
        let all = vec![repo("alpha", "C:\\dev\\work\\alpha", &[])];
        let roots = vec!["C:\\dev\\work".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(sketch(&rows, &all), vec!["#C:\\dev\\work(1)", "alpha"]);
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 2)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#C:\\dev(1)", "alpha"],
            "backslash paths climb too and stop at the drive folder"
        );
    }

    /// Windows: two trees on one drive share only the `C:` volume, which is
    /// not a folder. It must not become the single header every repo falls
    /// into, and the depth picker must keep meaning something (`C:` was the
    /// one header, and no level can climb above a bare drive).
    #[test]
    fn a_drive_letter_never_names_a_group() {
        let all = vec![
            repo("alpha", r"C:\Users\e\Developments\alpha", &[]),
            repo("beta", r"C:\src\beta", &[]),
        ];
        let roots = vec![
            r"C:\Users\e\Developments".to_string(),
            r"C:\src".to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let at = |depth| {
            project(
                &all,
                &[],
                RepoSortMode::Name,
                true,
                false,
                Some(&grouping_at(&roots, &collapsed, depth)),
            )
        };
        assert_eq!(
            sketch(&at(1), &all),
            vec![
                r"#C:\src(1)",
                "beta",
                r"#C:\Users\e\Developments(1)",
                "alpha"
            ],
            "each tree keeps a folder header of its own, not one C: band"
        );
        assert_eq!(
            sketch(&at(2), &all),
            vec![r"#C:\src(1)", "beta", r"#C:\Users\e(1)", "alpha"],
            "depth still raises the tree that has a folder above it"
        );
        assert_eq!(
            sketch(&at(9), &all),
            vec![r"#C:\src(1)", "beta", r"#C:\Users(1)", "alpha"],
            "an over-deep level stops at the topmost named folder, never C:"
        );
    }

    /// A root on another volume groups under its own parent, not the repo
    /// folder itself; the tree that shares a folder keeps sharing it.
    #[test]
    fn a_root_on_another_drive_groups_under_its_parent() {
        let all = vec![
            repo("alpha", r"C:\Users\e\Developments\alpha", &[]),
            repo("beta", r"C:\Users\e\Developments\beta", &[]),
            repo("gamma", r"D:\repos\gamma", &[]),
        ];
        let roots = vec![
            r"C:\Users\e\Developments\alpha".to_string(),
            r"C:\Users\e\Developments\beta".to_string(),
            r"D:\repos\gamma".to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![
                r"#C:\Users\e\Developments(2)",
                "alpha",
                "beta",
                r"#D:\repos(1)",
                "gamma"
            ],
            "the two C: roots share Developments; the lone D: repo takes its parent's header"
        );
    }

    /// A UNC server is the volume, the share name below it is a folder: the
    /// header stops at the share and never names itself after `\\server`.
    #[test]
    fn unc_roots_stop_at_the_share() {
        let all = vec![
            repo("alpha", r"\\nas\dev\alpha", &[]),
            repo("beta", r"\\nas\dev\beta", &[]),
        ];
        let roots = vec![
            r"\\nas\dev\alpha".to_string(),
            r"\\nas\dev\beta".to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 9)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec![r"#\\nas\dev(2)", "alpha", "beta"],
            "the share is the deepest header an over-deep level can reach"
        );
    }

    /// A scan root at the drive itself still groups its repos under their own
    /// parent; the drive never becomes a `C:` band.
    #[test]
    fn a_scan_root_at_the_drive_still_groups_by_the_repos_parent() {
        let all = vec![repo("alpha", r"C:\src\alpha", &[])];
        let roots = vec![r"C:\".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(sketch(&rows, &all), vec![r"#C:\src(1)", "alpha"]);
    }

    /// Two spellings of one folder (a scan root under a symlink and the same
    /// tree under its target) must land in one header, not two.
    #[cfg(unix)]
    #[test]
    fn symlinked_folders_merge_into_one_group() {
        let dir =
            std::env::temp_dir().join(format!("warp-station-group-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let real = dir.join("Developments");
        std::fs::create_dir_all(real.join("warp-agent")).unwrap();
        std::fs::create_dir_all(real.join("warp-suite")).unwrap();
        let alias = dir.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        let all = vec![
            repo(
                "warp-agent",
                alias.join("warp-agent").to_str().unwrap(),
                &[],
            ),
            repo("warp-suite", real.join("warp-suite").to_str().unwrap(), &[]),
        ];
        let roots = vec![
            alias.join("warp-agent").to_string_lossy().to_string(),
            real.join("warp-suite").to_string_lossy().to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(rows.len(), 3, "one header + both repos");
        let ProjectedRow::Group {
            key, name, count, ..
        } = &rows[0]
        else {
            panic!("expected a group header");
        };
        assert_eq!(*count, 2, "the symlink and its target share the group");
        assert_eq!(name, "Developments", "the header is the shared folder");
        assert!(
            key.ends_with("Developments"),
            "the key is the canonical folder (got {key})"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A scan root whose folder no longer exists must still resolve through
    /// its existing parent — otherwise one stale root splits the whole tree
    /// apart and every repo falls back to its own header.
    #[cfg(unix)]
    #[test]
    fn a_stale_scan_root_does_not_split_the_shared_folder() {
        let dir =
            std::env::temp_dir().join(format!("warp-station-group-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let real = dir.join("Developments");
        std::fs::create_dir_all(real.join("warp-agent")).unwrap();
        let alias = dir.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        let all = vec![repo(
            "warp-agent",
            alias.join("warp-agent").to_str().unwrap(),
            &[],
        )];
        // the second root does not exist, but resolves through `alias`
        let roots = vec![
            alias.join("warp-agent").to_string_lossy().to_string(),
            alias.join("deleted-repo").to_string_lossy().to_string(),
        ];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &[],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping_at(&roots, &collapsed, 1)),
        );
        assert_eq!(rows.len(), 2, "one header + the repo");
        let ProjectedRow::Group { key, count, .. } = &rows[0] else {
            panic!("expected a group header");
        };
        assert_eq!(*count, 1, "the stale root did not steal the repo");
        assert!(
            key.ends_with("Developments"),
            "the shared folder survives the stale root (got {key})"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
