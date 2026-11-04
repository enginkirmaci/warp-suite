//! The CLI mutation layer against a scratch repository: stage/unstage/discard/
//! commit through the real `git` binary. Skips when git is unavailable.

use std::path::Path;
use std::process::Command;
use warp_station_core::git::cli;

/// One git invocation in `dir`, asserted only by the caller.
fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
}

fn scratch_repo(tag: &str) -> Option<std::path::PathBuf> {
    if Command::new("git").arg("--version").output().is_err() {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("dt-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .ok()?
        .status
        .success();
    if !ok {
        return None;
    }
    Command::new("git")
        .args(["config", "user.email", "t@t"])
        .current_dir(&dir)
        .output()
        .ok()?;
    Command::new("git")
        .args(["config", "user.name", "T"])
        .current_dir(&dir)
        .output()
        .ok()?;
    // pin line endings so byte-exact assertions hold regardless of the
    // machine's global core.autocrlf (true on stock Git for Windows)
    Command::new("git")
        .args(["config", "core.autocrlf", "false"])
        .current_dir(&dir)
        .output()
        .ok()?;
    std::fs::write(dir.join("base.txt"), "base\n").ok()?;
    Command::new("git")
        .args(["add", "."])
        .current_dir(&dir)
        .output()
        .ok()?;
    Command::new("git")
        .args(["commit", "-qm", "base"])
        .current_dir(&dir)
        .output()
        .ok()?;
    Some(dir)
}

#[tokio::test]
async fn stage_commit_discard_round_trip() {
    let Some(dir) = scratch_repo("rt") else {
        return;
    };

    // new file lands untracked on the unstaged side
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let snapshot = warp_station_core::git::read::change_snapshot(&dir).unwrap();
    assert!(
        snapshot
            .unstaged_files
            .iter()
            .any(|f| f.path == "a.txt" && f.status_code == "?")
    );

    // stage it — moves to the staged side
    assert!(cli::stage(&dir, "a.txt").await.ok);
    let snapshot = warp_station_core::git::read::change_snapshot(&dir).unwrap();
    assert!(snapshot.staged_files.iter().any(|f| f.path == "a.txt"));
    assert!(!snapshot.unstaged_files.iter().any(|f| f.path == "a.txt"));

    // unstage returns it to the unstaged side as untracked
    assert!(cli::unstage(&dir, "a.txt").await.ok);
    let snapshot = warp_station_core::git::read::change_snapshot(&dir).unwrap();
    assert!(
        snapshot
            .unstaged_files
            .iter()
            .any(|f| f.path == "a.txt" && f.status_code == "?")
    );

    // discard deletes the untracked file entirely
    assert!(cli::discard_file(&dir, "a.txt", false).await);
    assert!(!dir.join("a.txt").exists());
    assert!(
        warp_station_core::git::read::change_snapshot(&dir)
            .unwrap()
            .status_files
            .is_empty()
    );

    // modify a tracked file, stage, commit -F with a multiline message
    std::fs::write(dir.join("base.txt"), "base\nmore\n").unwrap();
    assert!(cli::stage(&dir, "base.txt").await.ok);
    let hash = cli::commit(&dir, "subject line\n\nbody with \"quotes\" and\nnewlines\n")
        .await
        .expect("commit hash");
    assert_eq!(hash.len(), 7);
    let snapshot = warp_station_core::git::read::change_snapshot(&dir).unwrap();
    assert!(snapshot.status_files.is_empty());

    let details = warp_station_core::git::read::commit_details(&dir, &hash).unwrap();
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].path, "base.txt");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn branch_create_and_checkout() {
    let Some(dir) = scratch_repo("br") else {
        return;
    };

    let result = cli::create_branch(&dir, "feature/x", None, true).await;
    assert!(result.success, "{result:?}");
    assert_eq!(
        warp_station_core::git::read::probe(&dir)
            .unwrap()
            .branch_name
            .as_deref(),
        Some("feature/x")
    );

    assert!(cli::checkout(&dir, "master").await.ok || cli::checkout(&dir, "main").await.ok);

    // duplicate name fails with git's fatal line surfaced
    let dupe = cli::create_branch(&dir, "feature/x", None, false).await;
    assert!(!dupe.success);
    assert!(dupe.error.as_deref().is_some_and(|e| e.contains("fatal:")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A remote-tracking pick creates (or switches to) its local tracking branch
/// instead of leaving a detached HEAD; a commit hash still detaches, as the
/// history drill-down's checkout expects.
#[tokio::test]
async fn remote_pick_checks_out_a_local_tracking_branch() {
    use warp_station_core::model::GitBranchKind;

    let Some(dir) = scratch_repo("remote-pick") else {
        return;
    };
    let origin = std::env::temp_dir().join(format!("dt-cli-origin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&origin);
    assert!(
        git(&dir, &["init", "--bare", "-q", origin.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        git(&dir, &["remote", "add", "origin", origin.to_str().unwrap()])
            .status
            .success()
    );

    // seed the remote with the starting branch plus a feature branch
    let base = warp_station_core::git::read::probe(&dir)
        .unwrap()
        .branch_name
        .expect("scratch repo has a branch");
    assert!(git(&dir, &["push", "-q", "origin", &base]).status.success());
    assert!(
        git(&dir, &["checkout", "-q", "-b", "feature"])
            .status
            .success()
    );
    std::fs::write(dir.join("feature.txt"), "feature\n").unwrap();
    for args in [
        vec!["add", "."],
        vec!["commit", "-qm", "feature"],
        vec!["push", "-q", "-u", "origin", "feature"],
    ] {
        assert!(git(&dir, &args).status.success(), "{args:?}");
    }

    // back on base with the local feature gone: only origin/feature remains
    assert!(git(&dir, &["checkout", "-q", &base]).status.success());
    assert!(git(&dir, &["branch", "-D", "feature"]).status.success());
    assert!(git(&dir, &["fetch", "-q", "origin"]).status.success());
    let branches = warp_station_core::git::read::branches(&dir).unwrap();
    assert!(
        branches
            .iter()
            .any(|b| b.name == "origin/feature" && b.kind == GitBranchKind::Remote),
        "{branches:?}"
    );
    assert!(
        !branches
            .iter()
            .any(|b| b.name == "feature" && b.kind == GitBranchKind::Local),
        "{branches:?}"
    );

    // the pick lands on a local tracking branch, not a detached HEAD
    let outcome = cli::checkout(&dir, "origin/feature").await;
    assert!(outcome.ok, "{:?}", outcome.error);
    assert_eq!(
        warp_station_core::git::read::probe(&dir)
            .unwrap()
            .branch_name
            .as_deref(),
        Some("feature")
    );
    let upstream = git(&dir, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]);
    assert_eq!(
        String::from_utf8_lossy(&upstream.stdout).trim(),
        "origin/feature"
    );

    // picking the same remote again (local copy now present) just switches
    // back to that local branch — no duplicate creation, no detach
    assert!(git(&dir, &["checkout", "-q", &base]).status.success());
    let again = cli::checkout(&dir, "origin/feature").await;
    assert!(again.ok, "{:?}", again.error);
    assert_eq!(
        warp_station_core::git::read::probe(&dir)
            .unwrap()
            .branch_name
            .as_deref(),
        Some("feature")
    );

    // a commit hash keeps plain-checkout semantics (detached on purpose)
    let hash = git(&dir, &["rev-parse", &base]);
    let hash = String::from_utf8_lossy(&hash.stdout).trim().to_string();
    assert!(cli::checkout(&dir, &hash).await.ok);
    assert_eq!(
        warp_station_core::git::read::probe(&dir)
            .unwrap()
            .branch_name
            .as_deref(),
        Some("(detached)")
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&origin);
}

#[tokio::test]
async fn branch_delete() {
    let Some(dir) = scratch_repo("bd") else {
        return;
    };

    assert!(cli::create_branch(&dir, "keep", None, false).await.success);
    assert!(cli::create_branch(&dir, "gone", None, false).await.success);

    // deleting a plain local branch removes it from the list
    let result = cli::delete_branch(&dir, "gone").await;
    assert!(result.success, "{result:?}");
    let names: Vec<String> = warp_station_core::git::read::branches(&dir)
        .unwrap()
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert!(!names.contains(&"gone".to_string()), "{names:?}");
    assert!(names.contains(&"keep".to_string()), "{names:?}");

    // deleting the checked-out branch is refused by git, not a panic
    let current = warp_station_core::git::read::probe(&dir)
        .unwrap()
        .branch_name
        .expect("scratch repo has a branch");
    let refused = cli::delete_branch(&dir, &current).await;
    assert!(!refused.success, "{refused:?}");
    assert!(
        refused
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("error:"))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn discard_staged_path_returns_to_head() {
    let Some(dir) = scratch_repo("dc") else {
        return;
    };

    std::fs::write(dir.join("base.txt"), "changed\n").unwrap();
    assert!(cli::stage(&dir, "base.txt").await.ok);
    assert!(cli::discard_file(&dir, "base.txt", true).await);
    assert_eq!(
        std::fs::read_to_string(dir.join("base.txt")).unwrap(),
        "base\n"
    );
    assert!(Path::new(&dir).join("base.txt").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

/// A staged rename lists under its new name, edited afterwards or not.
#[test]
fn a_renamed_file_lists_under_its_new_name() {
    let Some(dir) = scratch_repo("renamed") else {
        return;
    };
    let text = "line\n".repeat(40);
    std::fs::write(dir.join("old.txt"), &text).unwrap();
    for args in [
        vec!["add", "."],
        vec!["commit", "-qm", "add"],
        vec!["mv", "old.txt", "new.txt"],
    ] {
        Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
    }
    std::fs::write(dir.join("new.txt"), format!("{text}more\n")).unwrap();

    let snapshot = warp_station_core::git::read::change_snapshot(&dir).expect("a snapshot");

    let paths = |files: &[warp_station_core::model::GitChangedFile]| {
        files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
    };
    assert_eq!(paths(&snapshot.staged_files), ["new.txt"]);
    assert_eq!(paths(&snapshot.unstaged_files), ["new.txt"]);
}
