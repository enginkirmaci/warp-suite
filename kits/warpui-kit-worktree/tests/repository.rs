//! The crate's contract is the git command line, so every test drives real
//! repositories in throwaway folders: no fake git and no injected runner.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use warpui_kit_worktree as worktree;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A repository with one commit, and a local identity so the commit does not
/// depend on the machine's git configuration.
fn repository() -> TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    git(dir.path(), &["init", "--initial-branch=main", "--quiet"]);
    git(dir.path(), &["config", "user.email", "probe@example.com"]);
    git(dir.path(), &["config", "user.name", "Probe"]);
    fs::write(dir.path().join("readme.md"), "probe\n").expect("write readme");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "--quiet", "-m", "first"]);
    dir
}

#[test]
fn a_worktree_lands_under_its_repository() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/first").expect("create");

    assert_eq!(created.path, repo.join(".worktrees").join("agent-first"));
    assert!(created.path.is_dir());
    assert_eq!(created.branch.as_deref(), Some("agent/first"));
    assert_eq!(created.label(), "agent/first");
    assert!(created.head.is_some());

    let listed = worktree::list(repo).expect("list");
    let main: Vec<_> = listed.iter().filter(|entry| entry.main).collect();
    assert_eq!(main.len(), 1);
    assert_eq!(
        main[0].path.canonicalize().expect("main path"),
        repo.canonicalize().expect("repo path")
    );
    assert_eq!(listed.len(), 2);
}

#[test]
fn a_taken_name_gets_the_next_free_number() {
    let dir = repository();
    let repo = dir.path();
    let first = worktree::create(repo, "agent/same").expect("first");
    let second = worktree::create(repo, "agent/same").expect("second");

    assert_eq!(first.branch.as_deref(), Some("agent/same"));
    assert_eq!(second.branch.as_deref(), Some("agent/same-2"));
    assert_ne!(first.path, second.path);
    assert!(second.path.is_dir());
}

#[test]
fn the_parent_stays_clean_and_the_exclude_entry_is_written_once() {
    let dir = repository();
    let repo = dir.path();
    worktree::create(repo, "agent/one").expect("first");
    worktree::create(repo, "agent/two").expect("second");

    let exclude =
        fs::read_to_string(repo.join(".git").join("info").join("exclude")).expect("exclude file");
    assert_eq!(
        exclude
            .lines()
            .filter(|line| line.trim() == worktree::EXCLUDE_ENTRY)
            .count(),
        1,
        "{exclude:?}"
    );
    assert_eq!(git(repo, &["status", "--porcelain"]).trim(), "");
}

#[test]
fn removal_refuses_a_dirty_worktree_until_forced() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/dirty").expect("create");
    fs::write(created.path.join("scratch.txt"), "unsaved\n").expect("write scratch");

    assert!(worktree::remove(repo, &created.path, false).is_err());
    assert!(created.path.is_dir());

    worktree::remove(repo, &created.path, true).expect("forced removal");
    assert!(!created.path.exists());
    assert!(
        !git(repo, &["rev-parse", "--verify", "refs/heads/agent/dirty"])
            .trim()
            .is_empty(),
        "the branch survives its worktree"
    );
}

#[test]
fn removal_leaves_a_clean_worktree_gone_and_its_branch_behind() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/clean").expect("create");

    worktree::remove(repo, &created.path, false).expect("removal");
    assert!(!created.path.exists());
    assert_eq!(worktree::list(repo).expect("list").len(), 1);
}

#[test]
fn prune_drops_an_entry_whose_folder_is_gone() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/lost").expect("create");
    fs::remove_dir_all(&created.path).expect("delete folder");

    let pruned = worktree::prune(repo).expect("prune");
    assert_eq!(pruned, 1);
    assert!(
        !worktree::list(repo)
            .expect("list")
            .iter()
            .any(|entry| entry.path == created.path)
    );
}

#[test]
fn a_folder_that_is_not_a_repository_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let folder = dir.path();

    assert!(!worktree::is_repository(folder));
    assert!(worktree::create(folder, "agent/none").is_err());
    assert!(worktree::list(folder).is_err());
}

#[test]
fn a_repository_without_a_commit_is_refused_and_creates_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    git(dir.path(), &["init", "--initial-branch=main", "--quiet"]);

    assert!(worktree::is_repository(dir.path()));
    assert!(worktree::create(dir.path(), "agent/none").is_err());
    assert!(!dir.path().join(worktree::WORKTREES_DIR).exists());
}

#[test]
fn a_detached_head_starts_the_branch_from_the_detached_commit() {
    let dir = repository();
    let repo = dir.path();
    let first = git(repo, &["rev-parse", "HEAD"]).trim().to_string();
    fs::write(repo.join("second.txt"), "second\n").expect("write second");
    git(repo, &["add", "."]);
    git(repo, &["commit", "--quiet", "-m", "second"]);
    git(repo, &["checkout", "--quiet", "--detach", &first]);

    let created = worktree::create(repo, "agent/detached").expect("create");
    let head = git(&created.path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_eq!(head, first);
}

#[test]
fn the_folder_flattens_the_branch() {
    let dir = repository();
    assert_eq!(
        worktree::path_for(dir.path(), "agent/fix-login"),
        dir.path().join(".worktrees").join("agent-fix-login")
    );
}

#[test]
fn a_linked_worktree_lists_the_real_main_checkout_first() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/from-linked").expect("create");

    // run the same call from inside the worktree: git lists the shared
    // repository, and the main checkout must still be the one marked main
    let listed = worktree::list(&created.path).expect("list from the worktree");
    let main: Vec<_> = listed.iter().filter(|entry| entry.main).collect();
    assert_eq!(main.len(), 1);
    assert_eq!(
        main[0].path.canonicalize().expect("main path"),
        repo.canonicalize().expect("repo path")
    );
    assert!(
        listed
            .iter()
            .any(|entry| entry.path == created.path && !entry.main),
        "the worktree itself is a child of the main checkout"
    );
}

#[test]
fn attach_restores_a_checkout_whose_folder_is_gone() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/restored").expect("create");
    let head = git(&created.path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    fs::remove_dir_all(&created.path).expect("delete the folder");

    let restored = worktree::attach(repo, "agent/restored", &created.path).expect("attach");
    assert_eq!(restored.path, created.path);
    assert_eq!(restored.branch.as_deref(), Some("agent/restored"));
    assert!(restored.path.is_dir(), "the checkout is back");
    assert_eq!(
        git(&restored.path, &["rev-parse", "HEAD"]).trim(),
        head,
        "the branch's own commit is checked out"
    );
}

#[test]
fn attach_refuses_an_unknown_branch_and_an_existing_folder() {
    let dir = repository();
    let repo = dir.path();
    let created = worktree::create(repo, "agent/present").expect("create");

    assert!(
        worktree::attach(
            repo,
            "agent/never-made",
            &repo.join(".worktrees/agent-never")
        )
        .is_err(),
        "a repair needs the branch to exist"
    );
    assert!(
        worktree::attach(repo, "agent/present", &created.path).is_err(),
        "an existing folder is not a repair"
    );
}
