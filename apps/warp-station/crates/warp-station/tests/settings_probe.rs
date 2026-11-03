//! Headless draw probe for the Settings page: seeded from the real settings
//! file, two draws without a panic means the card/field/switch tree is sound.

use gpui::TestAppContext;
use gpui::prelude::*;
use warp_station::net::{NetCommand, TemplateOp};
use warp_station::store::Store;
use warp_station::ui::settings::SettingsPage;

/// Serializes the HOME-swapping probes against each other.
static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[gpui_kit::gpui::test]
fn settings_page_draws_all_sections(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // the template rows seed their prompt bodies on first render — keep that
    // off the real ~/.warp
    let home = std::env::temp_dir().join(format!(
        "warp-station-settings-draw-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; serialized against the sibling HOME probes
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| SettingsPage::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "settings-appearance");
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("text-size-normal").is_some(),
        "Appearance carries the shared text size control"
    );
    // the palette is a live picker now, the same control the other apps carry
    for key in ["copper", "plum", "pine", "blue"] {
        let selector: &'static str = Box::leak(format!("palette-{key}").into_boxed_str());
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Appearance offers the {key} palette"
        );
    }
    assert!(
        cx.debug_bounds("settings-fixed-Palette").is_none(),
        "the fixed Copper row is gone"
    );

    // the switch applies at once, with no Save; the design's row puts it at
    // the end of the row, opposite the label
    let reduce = cx
        .debug_bounds("settings-row-Reduce motion")
        .expect("Appearance carries Reduce motion");
    cx.simulate_click(
        gpui::point(reduce.right() - gpui::px(20.), reduce.center().y),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    assert!(cx.update(|_, _| {
        warp_station_core::settings::SettingsService::open()
            .get()
            .general
            .reduce_motion
    }));
    assert!(warpui_kit_core::motion::reduced());
    warpui_kit_core::motion::set_reduced(false);
    assert!(
        cx.update(|_, _| warp_station_core::settings::SettingsService::open()
            .get()
            .repos
            .max_scan_depth)
            >= 1
    );
}

/// The AI card's Model filtered picker: the pseudo "(server default)" entry the
/// catalog door forwards is hidden, the in-dropdown filter narrows the list, a
/// row click selects and closes, and a click outside the picker dismisses it.
#[gpui_kit::gpui::test]
fn filtered_picker_filters_and_dismisses(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-model-picker-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; serialized against the sibling HOME probes
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(3000.)),
        |window, cx| SettingsPage::new(store.clone(), window, cx),
    );
    let mut wcx = gpui::VisualTestContext::from_window(handle.into(), cx);
    wcx.run_until_parked();
    store.update(&mut wcx, |s, _| {
        let mut models: Vec<warp_station_core::opencode::CatalogModel> = vec![
            "(server default)".to_string(),
            "opencode/alpha".into(),
            "other/beta".into(),
        ]
        .into_iter()
        .map(|key| warp_station_core::opencode::CatalogModel {
            key,
            harness: "opencode".into(),
            variants: Vec::new(),
            context_limit: 0,
        })
        .collect();
        models.extend(
            (0..200).map(|ix| warp_station_core::opencode::CatalogModel {
                key: format!("opencode/model-{ix:03}"),
                harness: "opencode".into(),
                variants: Vec::new(),
                context_limit: 0,
            }),
        );
        s.harnesses = warp_station_core::opencode::default_harnesses();
        s.catalog = warp_station_core::opencode::ModelCatalog {
            harnesses: s.harnesses.clone(),
            models,
        };
    });
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(&mut wcx, "settings-ai");

    click(&mut wcx, "model-trigger-Model");
    assert!(
        wcx.debug_bounds("model-panel-Model").is_some(),
        "the picker opens"
    );
    assert!(
        wcx.debug_bounds("model-filter-Model").is_some(),
        "the filter renders inside the picker"
    );
    assert!(wcx.debug_bounds("model-row-opencode/alpha").is_some());
    assert!(
        wcx.debug_bounds("model-row-opencode/model-199").is_none(),
        "the virtualized list only lays out visible rows"
    );
    assert!(
        wcx.debug_bounds("model-row-(server default)").is_none(),
        "the catalog sentinel never becomes a row"
    );

    click(&mut wcx, "model-filter-Model");
    wcx.simulate_input("beta");
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        wcx.debug_bounds("model-row-other/beta").is_some(),
        "the filter keeps a match"
    );
    assert!(
        wcx.debug_bounds("model-row-opencode/alpha").is_none(),
        "the filter drops non-matches"
    );

    click(&mut wcx, "model-row-other/beta");
    assert!(
        wcx.debug_bounds("model-panel-Model").is_none(),
        "a pick closes the picker"
    );

    click(&mut wcx, "model-trigger-Model");
    assert!(
        wcx.debug_bounds("model-row-opencode/alpha").is_some(),
        "reopening clears the filter"
    );
    assert!(
        wcx.debug_bounds("model-row-opencode/model-199").is_none(),
        "the virtualized list still only lays out visible rows"
    );
    // The floating picker covers the window with a catcher, so "outside"
    // means a press on that catcher rather than a stray coordinate.
    assert!(
        wcx.debug_bounds("field-catcher").is_some(),
        "the floating picker lays a catcher over the form"
    );
    click(&mut wcx, "field-catcher");
    assert!(
        wcx.debug_bounds("model-panel-Model").is_none(),
        "an outside click dismisses the picker"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The AI card's harness radio and Variant picker: the door catalog's
/// harnesses render as radios, switching to dsh scopes the Model picker to
/// that catalog, picking a model exposes its variants, and Save persists the
/// harness/model/variant triple — with no CommitModel left behind.
#[gpui_kit::gpui::test]
fn harness_model_and_variant_persist(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-ai-model-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings; the
    // sibling tests serialize on HOME_ENV_LOCK
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(
        &scratch,
        r#"{"OpenCode":{"DefaultModel":"zai/glm"},"WarpAgent":{"Harness":"opencode"}}"#,
    )
    .expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(3000.)),
        |window, cx| SettingsPage::new(store.clone(), window, cx),
    );
    let mut wcx = gpui::VisualTestContext::from_window(handle.into(), cx);
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(&mut wcx, "settings-ai");

    assert!(
        wcx.debug_bounds("warp-harness-opencode").is_some(),
        "the opencode radio renders"
    );
    assert!(
        wcx.debug_bounds("warp-harness-dsh").is_some(),
        "the dsh radio renders"
    );

    store.update(&mut wcx, |s, cx| {
        s.harnesses = warp_station_core::opencode::default_harnesses();
        s.catalog = warp_station_core::opencode::ModelCatalog {
            harnesses: s.harnesses.clone(),
            models: vec![
                warp_station_core::opencode::CatalogModel {
                    key: "zai/glm".into(),
                    harness: "opencode".into(),
                    variants: vec!["low".into(), "high".into()],
                    context_limit: 0,
                },
                warp_station_core::opencode::CatalogModel {
                    key: "deepseek-account/deepseek-flash".into(),
                    harness: "dsh".into(),
                    variants: vec!["max".into()],
                    context_limit: 0,
                },
            ],
        };
        cx.notify();
    });
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));

    click(&mut wcx, "warp-harness-dsh");
    click(&mut wcx, "model-trigger-Model");
    assert!(
        wcx.debug_bounds("model-row-deepseek-account/deepseek-flash")
            .is_some(),
        "the dsh catalog is the only list under the dsh radio"
    );
    assert!(
        wcx.debug_bounds("model-row-zai/glm").is_none(),
        "the opencode model is filtered out"
    );
    click(&mut wcx, "model-row-deepseek-account/deepseek-flash");

    click(&mut wcx, "model-trigger-Variant");
    assert!(
        wcx.debug_bounds("variant-row-max").is_some(),
        "the picked model's variant renders"
    );
    click(&mut wcx, "variant-row-max");

    let save = wcx.debug_bounds("settings-save").expect("save button");
    wcx.simulate_click(save.center(), gpui::Modifiers::default());
    wcx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"Harness\": \"dsh\""),
        "harness persisted: {json}"
    );
    assert!(
        json.contains("\"DefaultModel\": \"deepseek-account/deepseek-flash\""),
        "model persisted: {json}"
    );
    assert!(
        json.contains("\"Variant\": \"max\""),
        "variant persisted: {json}"
    );
    assert!(
        !json.contains("CommitModel"),
        "no commit model remains: {json}"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The Git card's auto-fetch picker: four segmented options render seeded from
/// the settings, and picking 30m + Save persists AutoFetchMinutes to the
/// settings file on disk. The store is scratch-backed; the probe also runs
/// under a temp HOME because Save reconciles the autostart registration, and
/// in a tall window so the Git card (third card down) is clickable.
#[gpui_kit::gpui::test]
fn auto_fetch_row_picks_and_persists(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-auto-fetch-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings; the
    // sibling test tolerates a default-settings read while this runs
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2400.)),
        |window, cx| SettingsPage::new(store, window, cx),
    );
    let cx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "settings-git");

    assert!(
        cx.debug_bounds("auto-fetch-row").is_some(),
        "the auto fetch row renders"
    );
    for option in [
        "auto-fetch-off",
        "auto-fetch-15",
        "auto-fetch-30",
        "auto-fetch-60",
    ] {
        assert!(cx.debug_bounds(option).is_some(), "{option} renders");
    }

    let pick = cx.debug_bounds("auto-fetch-30").expect("30m button");
    cx.simulate_click(pick.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let save = cx.debug_bounds("settings-save").expect("save button");
    cx.simulate_click(save.center(), gpui::Modifiers::default());
    cx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"AutoFetchMinutes\": 30"),
        "the interval persisted (got {json})"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The AI features card's prompt-template manager. Templates live under the
/// scratch HOME: the three rows render un-customized, a body modified
/// straight on disk badges after the page re-opens, Edit round-trips
/// TemplateIo Read and swaps in the editor (a TemplateLoaded hand-set on the
/// store seeds it), Cancel returns to the rows, and Reset is two-click armed
/// with the confirm landing on the net channel.
#[gpui_kit::gpui::test]
fn prompt_templates_list_edit_reset(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-template-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; serialized against the sibling HOME probe
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2600.)),
        |window, cx| SettingsPage::new(store.clone(), window, cx),
    );
    let mut wcx = gpui::VisualTestContext::from_window(handle.into(), cx);
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(&mut wcx, "settings-ai");

    // the three registry rows render; none is customized yet
    for selector in [
        "template-row-commit-message",
        "template-row-readme",
        "template-row-daily-summary-all",
    ] {
        assert!(wcx.debug_bounds(selector).is_some(), "{selector} renders");
    }
    assert!(wcx.debug_bounds("template-customized-readme").is_none());

    // a body modified straight on disk badges once a fresh page re-opens
    std::fs::create_dir_all(home.join(".warp").join("assistant")).unwrap();
    std::fs::write(
        home.join(".warp").join("assistant").join("readme.md"),
        "custom {repo_name}",
    )
    .expect("seed a customized body");
    let reopened = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2600.)),
        |window, cx| SettingsPage::new(store.clone(), window, cx),
    );
    let mut wcx = gpui::VisualTestContext::from_window(reopened.into(), cx);
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(&mut wcx, "settings-ai");
    assert!(
        wcx.debug_bounds("template-customized-readme").is_some(),
        "disk edit badges"
    );
    assert!(
        wcx.debug_bounds("template-customized-commit-message")
            .is_none()
    );

    // Edit swaps the section for the embedded editor and asks for the body
    click(&mut wcx, "template-edit-readme");
    assert!(
        wcx.debug_bounds("template-editor-readme").is_some(),
        "the editor view renders"
    );
    assert!(
        wcx.debug_bounds("template-row-readme").is_none(),
        "the rows are gone"
    );
    assert!(
        matches!(
            last_command(&net_rx),
            Some(NetCommand::TemplateIo { id, op: TemplateOp::Read }) if id == "readme"
        ),
        "Edit sends a readme TemplateIo Read"
    );

    // the async gap: the editor draws empty, then TemplateLoaded landing on
    // the store seeds the buffer (two draws, no panic)
    wcx.update(|window, cx| drop(window.draw(cx)));
    store.update(&mut wcx, |s, _| {
        s.template_loaded = Some(("readme".into(), "custom {repo_name}".into(), true));
    });
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    wcx.update(|window, cx| drop(window.draw(cx)));

    // Cancel exits without writing anything
    click(&mut wcx, "template-cancel-readme");
    assert!(
        wcx.debug_bounds("template-editor-readme").is_none(),
        "cancel exits the editor"
    );
    assert!(
        wcx.debug_bounds("template-row-readme").is_some(),
        "the rows are back"
    );

    // Reset is two-click armed; only the confirm reaches the net channel
    click(&mut wcx, "template-reset-readme");
    assert!(net_rx.is_empty(), "the arming click sends nothing");
    click(&mut wcx, "template-reset-readme");
    assert!(
        matches!(
            last_command(&net_rx),
            Some(NetCommand::TemplateIo { id, op: TemplateOp::Reset }) if id == "readme"
        ),
        "the confirm sends a readme TemplateIo Reset"
    );

    let _ = std::fs::remove_dir_all(&home);
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing"));
    cx.simulate_click(
        gpui::Point {
            x: bounds.origin.x + bounds.size.width / 2.,
            y: bounds.origin.y + bounds.size.height / 2.,
        },
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// Drains the net channel and returns the last command (None when idle).
fn last_command(net_rx: &async_channel::Receiver<NetCommand>) -> Option<NetCommand> {
    let mut last = None;
    while let Ok(command) = net_rx.try_recv() {
        last = Some(command);
    }
    last
}

/// The scanning card's "Group repos by folder" switch: it renders seeded
/// from the settings, a click flips it, and Save persists GroupReposByRoot
/// into the scratch file — grouping is a projection toggle, so saving must
/// not send a second rescan.
#[gpui_kit::gpui::test]
fn group_repos_switch_toggles_and_persists(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home =
        std::env::temp_dir().join(format!("warp-station-group-switch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings;
    // serialized against the sibling HOME probes by HOME_ENV_LOCK
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2400.)),
        |window, cx| SettingsPage::new(store, window, cx),
    );
    let wcx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(wcx, "settings-repositories");

    assert!(
        wcx.debug_bounds("settings-row-Group repos by folder")
            .is_some(),
        "the grouping switch row renders in the scanning card"
    );

    // the design's row ends with its control: a click near the right edge
    // flips the switch
    let row = wcx
        .debug_bounds("settings-row-Group repos by folder")
        .expect("grouping switch row bounds");
    wcx.simulate_click(
        gpui::point(row.right() - gpui::px(20.), row.center().y),
        gpui::Modifiers::default(),
    );
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));

    let save = wcx.debug_bounds("settings-save").expect("save button");
    wcx.simulate_click(save.center(), gpui::Modifiers::default());
    wcx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"GroupReposByRoot\": true"),
        "the grouping toggle persisted (got {json})"
    );

    // grouping applies by reprojecting: the only Scan ever sent is the
    // construction one
    let mut scans = 0;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::Scan { .. }) {
            scans += 1;
        }
    }
    assert_eq!(scans, 1, "saving the grouping toggle must not rescan");

    let _ = std::fs::remove_dir_all(&home);
}

/// The group-depth picker: it renders only while grouping is on, a segment
/// click selects that depth, and Save persists GroupRootDepth — still
/// without a rescan, like the grouping toggle.
#[gpui_kit::gpui::test]
fn group_depth_picker_renders_grouped_and_persists(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home =
        std::env::temp_dir().join(format!("warp-station-group-depth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings;
    // serialized against the sibling HOME probes by HOME_ENV_LOCK
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(&scratch, r#"{"Repos": {"GroupReposByRoot": true}}"#).expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2400.)),
        |window, cx| SettingsPage::new(store, window, cx),
    );
    let wcx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(wcx, "settings-repositories");

    assert!(
        wcx.debug_bounds("group-depth-row").is_some(),
        "the depth picker renders while grouping is on"
    );

    let segment = wcx.debug_bounds("group-depth-2").expect("depth-2 segment");
    wcx.simulate_click(segment.center(), gpui::Modifiers::default());
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));

    let save = wcx.debug_bounds("settings-save").expect("save button");
    wcx.simulate_click(save.center(), gpui::Modifiers::default());
    wcx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"GroupRootDepth\": 2"),
        "the picked depth persisted (got {json})"
    );

    let mut scans = 0;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::Scan { .. }) {
            scans += 1;
        }
    }
    assert_eq!(scans, 1, "saving the depth must not rescan");

    let _ = std::fs::remove_dir_all(&home);
}

/// The External tools card's cross-repo code search pair: the switch renders
/// (absent settings = enabled, the absence-tolerant opt-out), the ripgrep
/// field seeds with the default, and Save persists the opt-out flag plus the
/// typed executable name into the scratch file.
#[gpui_kit::gpui::test]
fn code_search_rows_render_and_persist(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home =
        std::env::temp_dir().join(format!("warp-station-code-search-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings;
    // serialized against the sibling HOME probes by HOME_ENV_LOCK
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2400.)),
        |window, cx| SettingsPage::new(store, window, cx),
    );
    let wcx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(wcx, "settings-tools");

    assert!(
        wcx.debug_bounds("settings-row-Cross-repo code search")
            .is_some(),
        "the code search switch row renders in the External tools card"
    );
    assert!(
        wcx.debug_bounds("settings-field-ripgrep executable")
            .is_some(),
        "the ripgrep executable field row renders"
    );

    // the design's row ends with its control: a click near the right edge
    // flips it off
    let row = wcx
        .debug_bounds("settings-row-Cross-repo code search")
        .expect("code search switch row bounds");
    wcx.simulate_click(
        gpui::point(row.right() - gpui::px(20.), row.center().y),
        gpui::Modifiers::default(),
    );
    wcx.run_until_parked();

    // the field seeds "rg"; select-all + type replaces it with a custom name
    let field = wcx
        .debug_bounds("settings-field-ripgrep executable")
        .expect("ripgrep field bounds");
    // the design's `.setting .field`: 340 wide at the 36 control height
    assert_eq!(
        field.size.width,
        gpui::px(340.),
        "a settings field is 340 wide"
    );
    assert_eq!(
        field.size.height,
        gpui::px(warpui_kit_core::shell::scaled(36.)),
        "a settings field is the shared field height"
    );
    // the design's row puts the field at the end, opposite its label
    wcx.simulate_click(
        gpui::point(field.right() - gpui::px(120.), field.center().y),
        gpui::Modifiers::default(),
    );
    wcx.run_until_parked();
    wcx.simulate_keystrokes("ctrl-a");
    wcx.simulate_input("rgz");
    wcx.run_until_parked();

    let save = wcx.debug_bounds("settings-save").expect("save button");
    wcx.simulate_click(save.center(), gpui::Modifiers::default());
    wcx.run_until_parked();

    let json = std::fs::read_to_string(&scratch).expect("saved settings");
    assert!(
        json.contains("\"EnableCodeSearch\": false"),
        "the opt-out persisted (got {json})"
    );
    assert!(
        json.contains("\"CodeSearchExecutable\": \"rgz\""),
        "the executable persisted (got {json})"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The enable switches are independent cards: the OpenCode and DeepSeek CLI
/// switches (with their executable fields) and the warp-agent AI switch each
/// render while the *other* features are off. Fusing them again would show up
/// here.
#[gpui_kit::gpui::test]
fn opencode_cli_and_warp_agent_switches_are_separate(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home =
        std::env::temp_dir().join(format!("warp-station-switch-split-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; set before the store reads settings;
    // serialized against the sibling HOME probes by HOME_ENV_LOCK
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    // the CLIs on, warp-agent off: each switch must own its own controls
    let scratch = home.join(".warp").join("settings.json");
    std::fs::write(
        &scratch,
        r#"{"OpenCode": {"EnableOpenCode": true}, "DeepSeek": {"EnableDeepSeek": true}, "WarpAgent": {"EnableWarpAgent": false, "Executable": "warp-agent"}}"#,
    )
    .expect("scratch settings");

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let handle = cx.open_window(
        gpui::size(gpui::px(1400.), gpui::px(2400.)),
        |window, cx| SettingsPage::new(store, window, cx),
    );
    let wcx = gpui::VisualTestContext::from_window(handle.into(), cx).into_mut();
    wcx.run_until_parked();
    wcx.update(|window, cx| drop(window.draw(cx)));
    click(wcx, "settings-tools");

    assert!(
        wcx.debug_bounds("settings-row-Enable OpenCode").is_some(),
        "the OpenCode CLI switch renders with warp-agent off"
    );
    assert!(
        wcx.debug_bounds("settings-field-OpenCode executable")
            .is_some(),
        "the OpenCode executable field sits in the CLI card"
    );
    assert!(
        wcx.debug_bounds("settings-row-Enable DeepSeek").is_some(),
        "the DeepSeek CLI switch renders with warp-agent off"
    );
    assert!(
        wcx.debug_bounds("settings-field-DeepSeek executable")
            .is_some(),
        "the DeepSeek executable field sits in the CLI card"
    );
    click(wcx, "settings-ai");
    assert!(
        wcx.debug_bounds("settings-row-Enable warp-agent").is_some(),
        "the warp-agent switch renders independently of the CLI switch"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The head keeps its actions inside the pane on every section: a long hint
/// used to widen the row and push Save past the window edge.
#[gpui_kit::gpui::test]
fn the_settings_head_keeps_save_inside_the_pane(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-settings-head-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; serialized against the sibling HOME probes
    unsafe {
        std::env::set_var("HOME", &home);
        // USERPROFILE too: Windows resolves the data root from it first, so a
        // HOME-only scratch leaves the real ~/.warp live for anything that
        // saves through the store's default service
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| SettingsPage::new(store, window, cx));
    cx.run_until_parked();
    // the window the app actually opens at, at the owner's desktop scale: the
    // head only overflowed at this size, not at the maximized test default
    cx.simulate_scale_factor_change(1.1667);
    cx.simulate_resize(gpui::size(gpui::px(1280.), gpui::px(820.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    for section in [
        "settings-section-Git",
        "settings-section-External tools",
        "settings-section-AI",
        "settings-section-Appearance",
    ] {
        if cx.debug_bounds(section).is_some() {
            click(cx, section);
            cx.update(|window, cx| drop(window.draw(cx)));
        }
        let pane = cx.debug_bounds("settings-detail").expect("detail pane");
        let save = cx.debug_bounds("settings-save").expect("Save");
        assert!(
            save.right() <= pane.right(),
            "{section}: Save {save:?} runs past the pane {pane:?}"
        );
    }
}
