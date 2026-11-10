//! `-V` answers with the same build line the About screen shows, so a support
//! report and the About card never disagree.

#[test]
fn the_version_line_carries_the_build() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_warp-snapit"))
        .arg("--version")
        .output()
        .expect("run warp-snapit --version");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.trim();
    assert_eq!(
        line,
        format!(
            "warp-snapit {} ({}) · {}",
            env!("CARGO_PKG_VERSION"),
            env!("WARP_SUITE_BUILD"),
            env!("WARP_SUITE_CHANNEL")
        ),
        "the version line is the build line"
    );
    // A plain `cargo test` build has no release script behind it and says so.
    assert!(
        line.ends_with(") · dev"),
        "a test build reports dev: {line:?}"
    );
}
