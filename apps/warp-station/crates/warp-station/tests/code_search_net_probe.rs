//! Live-path probes for the code sweep: real rg over this repo's sources,
//! and the real net thread settling the sweep. The in-file unit tests cover
//! the fixture tree; these two exist because a parser that reads wrong rg
//! field names still passes a vacuously-empty suite.

use std::time::Duration;

fn manifest() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::test]
async fn real_rg_sweep_over_this_repo_produces_usable_hits() {
    let Some(rg) = warp_station_core::exe::locate("rg") else {
        println!("skipping code-search probe: rg not found on PATH");
        return;
    };
    let hits = warp_station_core::code_search::run_code_search(
        &rg,
        &[manifest()],
        "fn run_git",
        warp_station_core::code_search::CODE_SEARCH_CAP,
    )
    .await;
    assert!(
        !hits.is_empty(),
        "this repo's own sources must match 'fn run_git'"
    );
    for hit in &hits {
        assert!(
            hit.excerpt[hit.match_start..hit.match_end].eq_ignore_ascii_case("fn run_git"),
            "offsets must slice exactly the matched text: {}",
            hit.excerpt
        );
    }
}

#[test]
fn net_thread_settles_the_code_sweep() {
    let Some(rg) = warp_station_core::exe::locate("rg") else {
        println!("skipping code-search probe: rg not found on PATH");
        return;
    };
    let (handle, cmd_tx, evt_rx) = warp_station::net::spawn_net_thread();

    cmd_tx
        .send_blocking(warp_station::net::NetCommand::CodeSearch {
            generation: 7,
            term: "fn run_git".into(),
            folders: vec![manifest()],
            rg_exe: rg,
        })
        .expect("send");

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "no CodeSearchSettled within 15s — net-thread path broken"
        );
        match evt_rx.recv_blocking() {
            Ok(warp_station::net::UiEvent::CodeSearchSettled { generation, hits }) => {
                assert_eq!(generation, 7);
                assert!(!hits.is_empty(), "settled sweep must carry hits");
                break;
            }
            Ok(_) => continue,
            Err(e) => panic!("event channel closed: {e}"),
        }
    }
    drop(cmd_tx);
    let _ = handle.join();
}
