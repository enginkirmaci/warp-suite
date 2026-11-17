//! warpui-kit-worktree: the family's one implementation of "a chat in a second
//! working directory".
//!
//! A worktree is a second working directory of one repository, checked out on
//! its own branch, so a chat can work without moving the checkout its owner is
//! looking at. The rules live here, once:
//!
//! - the folder is `<parent repo>/.worktrees/<slug>`, one flat level, where the
//!   slug is the branch with its separators flattened;
//! - the branch is the caller's choice (a chat derives it from its title with
//!   [`branch_name`]), and a name that is already taken gets the next free
//!   numeric suffix instead of checking out someone else's branch;
//! - `worktrees/` is written into the repository's local exclude, so the
//!   parent's own changes stay clean without a tracked file changing.
//!
//! The crate drives the `git` command line rather than `git2`: warp-agent has
//! no libgit2 and a C dependency for this job is not worth its build cost.
//! Nothing here touches GPUI, so the headless app crates can use it too.
//!
//! ```no_run
//! use std::path::Path;
//! # fn main() -> anyhow::Result<()> {
//! let repo = Path::new("/path/to/repo");
//! let branch = warpui_kit_worktree::branch_name("Fix the login retry");
//! let worktree = warpui_kit_worktree::create(repo, &branch)?;
//! println!("{} on {}", worktree.path.display(), worktree.label());
//! # Ok(())
//! # }
//! ```

mod git;
mod name;

use anyhow::{Context as _, Result, anyhow};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub use name::{BRANCH_PREFIX, branch_name, slug};

/// The folder inside a repository that holds its worktrees.
pub const WORKTREES_DIR: &str = ".worktrees";

/// The exclude entry the crate keeps in the repository's local exclude file.
pub const EXCLUDE_ENTRY: &str = ".worktrees/";

/// One worktree of a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    /// The worktree's own folder.
    pub path: PathBuf,
    /// The branch it is checked out on; `None` when it is detached.
    pub branch: Option<String>,
    /// The commit it is at, when git reported one.
    pub head: Option<String>,
    /// Whether this is the repository's main checkout rather than a worktree.
    pub main: bool,
    /// Whether the entry is locked against pruning.
    pub locked: bool,
    /// Whether git considers the folder gone and the entry prunable.
    pub prunable: bool,
}

impl Worktree {
    /// The label a row shows: the branch, or the folder name when detached.
    pub fn label(&self) -> String {
        self.branch.clone().unwrap_or_else(|| {
            self.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
        })
    }
}

/// Whether `folder` is inside a git repository at all. The cheap check every
/// caller runs before it offers a worktree.
pub fn is_repository(folder: &Path) -> bool {
    git::run(folder, &["rev-parse", "--show-toplevel"])
        .is_ok_and(|run| run.ok && !run.stdout.trim().is_empty())
}

/// The repository root that contains `folder`, which is where every other
/// function works.
pub fn toplevel(folder: &Path) -> Result<PathBuf> {
    let run = git::run(folder, &["rev-parse", "--show-toplevel"])
        .with_context(|| format!("could not run git in {}", folder.display()))?;
    if !run.ok || run.stdout.trim().is_empty() {
        return Err(anyhow!("{} is not a git repository", folder.display()));
    }
    Ok(PathBuf::from(run.stdout.trim()))
}

/// Every worktree of the repository, the main checkout included and marked.
/// The entry marked `main` is git's first one wherever this is called from: a
/// linked worktree lists the same repository, so its own folder is a child
/// there and the real checkout still leads.
pub fn list(repo: &Path) -> Result<Vec<Worktree>> {
    let root = toplevel(repo)?;
    let run = git::run(&root, &["worktree", "list", "--porcelain"])
        .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!("could not list worktrees: {}", run.message()));
    }
    Ok(git::parse_worktrees(&run.stdout)
        .into_iter()
        .enumerate()
        .map(|(index, entry)| Worktree {
            main: index == 0,
            path: entry.path,
            branch: entry.branch,
            head: entry.head,
            locked: entry.locked,
            prunable: entry.prunable,
        })
        .collect())
}

/// The folder a branch's worktree would occupy. Callers pass a repository
/// root; a subdirectory would put the worktrees folder in the wrong place.
pub fn path_for(repo: &Path, branch: &str) -> PathBuf {
    repo.join(WORKTREES_DIR).join(slug(branch))
}

/// `wanted`, or the first free `<wanted>-N`, checking both branch refs and
/// folders so a leftover folder cannot make creation fail.
pub fn unique_branch(repo: &Path, wanted: &str) -> Result<String> {
    let root = toplevel(repo)?;
    let mut number = 1;
    loop {
        let candidate = if number == 1 {
            wanted.to_string()
        } else {
            format!("{wanted}-{number}")
        };
        let taken = branch_exists(&root, &candidate)? || path_for(&root, &candidate).exists();
        if !taken {
            return Ok(candidate);
        }
        number += 1;
        if number > 99 {
            return Err(anyhow!("no free worktree name for {wanted}"));
        }
    }
}

/// Creates a worktree for a branch starting at the repository's current HEAD,
/// and returns what it created — with the final branch name, which differs from
/// `wanted` when the name was taken.
///
/// A repository with no commit yet is refused: git would silently create an
/// orphan branch, and a chat started there would have nothing to diff against.
pub fn create(repo: &Path, wanted: &str) -> Result<Worktree> {
    let root = toplevel(repo)?;
    if !git::run(&root, &["rev-parse", "--verify", "--quiet", "HEAD"])?.ok {
        return Err(anyhow!(
            "{} has no commit yet, so there is nothing to branch from",
            root.display()
        ));
    }
    let branch = unique_branch(&root, wanted)?;
    let path = path_for(&root, &branch);
    ensure_excluded(&root)?;
    let run = git::run_os(
        &root,
        &[
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new(&branch),
            path.as_os_str(),
        ],
    )
    .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!("could not create the worktree: {}", run.message()));
    }
    Ok(list(&root)
        .ok()
        .and_then(|entries| {
            entries
                .into_iter()
                .find(|entry| paths_equal(&entry.path, &path))
        })
        .unwrap_or(Worktree {
            path,
            branch: Some(branch),
            head: None,
            main: false,
            locked: false,
            prunable: false,
        }))
}

/// Re-attaches an existing branch at `path`: the repair path for a checkout
/// whose folder disappeared, and the one place an existing branch is attached.
///
/// A stale registration for the same path is pruned first — that is exactly
/// what a deleted folder leaves behind, and git would refuse the add otherwise.
pub fn attach(repo: &Path, branch: &str, path: &Path) -> Result<Worktree> {
    let root = toplevel(repo)?;
    if !branch_exists(&root, branch)? {
        return Err(anyhow!("branch {branch} does not exist"));
    }
    if path.exists() {
        return Err(anyhow!("{} already exists", path.display()));
    }
    let _ = prune(&root);
    ensure_excluded(&root)?;
    let run = git::run_os(
        &root,
        &[
            OsStr::new("worktree"),
            OsStr::new("add"),
            path.as_os_str(),
            OsStr::new(branch),
        ],
    )
    .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!("could not restore the worktree: {}", run.message()));
    }
    Ok(list(&root)
        .ok()
        .and_then(|entries| {
            entries
                .into_iter()
                .find(|entry| paths_equal(&entry.path, path))
        })
        .unwrap_or(Worktree {
            path: path.to_path_buf(),
            branch: Some(branch.to_string()),
            head: None,
            main: false,
            locked: false,
            prunable: false,
        }))
}

/// Removes a worktree's folder and its entry. A dirty worktree refuses until
/// `force` is set; its branch always survives.
pub fn remove(repo: &Path, path: &Path, force: bool) -> Result<()> {
    let root = toplevel(repo)?;
    let main_is_target = list(&root)?
        .into_iter()
        .find(|entry| entry.main)
        .is_some_and(|entry| paths_equal(&entry.path, path));
    if main_is_target {
        return Err(anyhow!(
            "{} is the repository's main checkout",
            path.display()
        ));
    }
    let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
    if force {
        args.push(OsStr::new("--force"));
    }
    args.push(path.as_os_str());
    let run = git::run_os(&root, &args)
        .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!("could not remove the worktree: {}", run.message()));
    }
    Ok(())
}

/// Drops entries whose folder is gone, and returns how many went.
pub fn prune(repo: &Path) -> Result<usize> {
    let root = toplevel(repo)?;
    let before = list(&root)?.len();
    let run = git::run_os(&root, &[OsStr::new("worktree"), OsStr::new("prune")])
        .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!("could not prune worktrees: {}", run.message()));
    }
    let after = list(&root)?.len();
    Ok(before.saturating_sub(after))
}

/// Appends [`EXCLUDE_ENTRY`] to the repository's local exclude file when it is
/// not there yet, and returns whether it wrote. The file is repository-local
/// and untracked, so the parent stays clean without a change to its own files.
pub fn ensure_excluded(repo: &Path) -> Result<bool> {
    let root = toplevel(repo)?;
    let run = git::run(&root, &["rev-parse", "--git-common-dir"])
        .with_context(|| format!("could not run git in {}", root.display()))?;
    if !run.ok {
        return Err(anyhow!(
            "could not find the git directory: {}",
            run.message()
        ));
    }
    let reported = run.stdout.trim();
    let common = if Path::new(reported).is_absolute() {
        PathBuf::from(reported)
    } else {
        root.join(reported)
    };
    let exclude = common.join("info").join("exclude");
    let existing = fs::read_to_string(&exclude).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == EXCLUDE_ENTRY) {
        return Ok(false);
    }
    if let Some(parent) = exclude.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(EXCLUDE_ENTRY);
    text.push('\n');
    fs::write(&exclude, text).with_context(|| format!("could not write {}", exclude.display()))?;
    Ok(true)
}

fn branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    let full = format!("refs/heads/{branch}");
    let run = git::run_os(
        repo,
        &[
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new(&full),
        ],
    )?;
    Ok(run.ok)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
