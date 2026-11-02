//! The UI scale every app shares: one text size, kept in `~/.warp/ui.json` so
//! a choice made in any app holds for all of them from their next start, times
//! the desktop text scaling GTK apps honor (gsettings
//! org.gnome.desktop.interface text-scaling-factor). The ladder and the kit's
//! rem base both read those two factors, so a change only lands once the app
//! re-runs its kit theme: [`follow_ui_scale`] takes that callback.
//! [`text_size_control`] is the one picker every app shows.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{App, FontWeight, Global, SharedString, div, px};

use crate::a11y::A11y as _;
use crate::{fonts, palette};

/// How often the main thread looks for a change the monitor heard: this gpui
/// rev does not service cross-thread channel wakeups on foreground tasks, so
/// the monitor thread has no way to wake it.
const POLL: Duration = Duration::from_millis(500);

/// The latest factor the monitor heard as f32 bits; 0 = nothing new.
static HEARD: AtomicU32 = AtomicU32::new(0);

const TEXT_SIZE_KEY: &str = "text_size";

/// The choices, in the order the control shows them.
pub const TEXT_SIZES: [(&str, &str); 4] = [
    (fonts::FONT_SMALL, "Small"),
    (fonts::FONT_NORMAL, "Normal"),
    (fonts::FONT_LARGE, "Large"),
    (fonts::FONT_LARGER, "Larger"),
];

/// What re-runs the app's kit theme.
struct ScaleHost {
    on_change: Rc<dyn Fn(&mut App)>,
}

impl Global for ScaleHost {}

// ---- the shared setting ----------------------------------------------------

/// The settings file every app shares, beside the `~/.warp/notes` store.
fn shared_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".warp").join("ui.json"))
}

fn read_text_size(file: &Path) -> Option<&'static str> {
    let text = fs::read_to_string(file).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let size = doc.get(TEXT_SIZE_KEY)?.as_str()?;
    Some(fonts::normalize_font_scale(Some(size)))
}

/// Keeps whatever else the file holds. Goes through a rename: another app
/// starting right now must never read it half-written.
fn write_setting(file: &Path, key: &str, value: &str) -> std::io::Result<()> {
    let mut doc: serde_json::Map<String, serde_json::Value> = fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    doc.insert(key.to_string(), value.into());
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let scratch = file.with_extension(format!("json.{}", std::process::id()));
    fs::write(&scratch, serde_json::to_string_pretty(&doc)?)?;
    fs::rename(&scratch, file)
}

fn write_text_size(file: &Path, size: &str) -> std::io::Result<()> {
    write_setting(file, TEXT_SIZE_KEY, size)
}

fn adopt_from(file: Option<&Path>, legacy: Option<&str>) {
    let saved = file.and_then(read_text_size);
    let size = saved.or(legacy.map(|size| fonts::normalize_font_scale(Some(size))));
    if let (None, Some(file), Some(size)) = (saved, file, size)
        && let Err(err) = write_text_size(file, size)
    {
        tracing::warn!("could not save the text size to {}: {err}", file.display());
    }
    fonts::set_font_scale(size);
}

/// Settles the text size at startup; call before the first kit theme so the
/// first frame is already sized. `legacy` is the app's own old setting: it
/// seeds the shared file on the first run, and is ignored after that.
pub fn adopt_text_size(legacy: Option<&str>) {
    adopt_from(shared_file().as_deref(), legacy);
}

/// Changes the text size: this app rescales now, the others read the new value
/// when they next start.
pub fn set_text_size(size: &str, cx: &mut App) {
    let size = fonts::normalize_font_scale(Some(size));
    if let Some(file) = shared_file()
        && let Err(err) = write_text_size(&file, size)
    {
        tracing::warn!("could not save the text size to {}: {err}", file.display());
    }
    apply_text_size(size, cx);
}

// ---- following -------------------------------------------------------------

/// Hooks the app up to scale changes: a click on [`text_size_control`], and the
/// desktop text scaling. The desktop value applies synchronously so the first
/// frame is already sized right; later changes arrive through a gsettings
/// monitor that dies with the app. `on_change` runs on the main thread after
/// each change: re-run the kit theme there, which refreshes every window.
/// Without gsettings the desktop factor stays 1.0.
pub fn follow_ui_scale(cx: &mut App, on_change: impl Fn(&mut App) + 'static) {
    cx.set_global(ScaleHost {
        on_change: Rc::new(on_change),
    });
    match desktop::read_factor() {
        Some(factor) => apply_factor(factor, cx),
        None => tracing::debug!("system text scaling unreadable; keeping factor 1.0"),
    }
    if !desktop::spawn_monitor() {
        return;
    }
    cx.on_app_quit(|_| {
        desktop::stop_monitor();
        async {}
    })
    .detach();
    poll_heard_factor(cx);
}

fn poll_heard_factor(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL).await;
            let heard = HEARD.swap(0, Ordering::Relaxed);
            if heard != 0 {
                cx.update(|cx| apply_factor(f32::from_bits(heard), cx));
            }
        }
    })
    .detach();
}

fn apply_factor(factor: f32, cx: &mut App) {
    if (fonts::system_factor() - factor).abs() > 0.001 {
        fonts::set_system_factor(factor);
        rescale(cx);
    }
}

fn apply_text_size(size: &'static str, cx: &mut App) {
    if fonts::font_scale() != size {
        fonts::set_font_scale(Some(size));
        rescale(cx);
    }
}

fn rescale(cx: &mut App) {
    let on_change = cx.try_global::<ScaleHost>().map(|h| h.on_change.clone());
    if let Some(on_change) = on_change {
        on_change(cx);
    }
}

// ---- the control -----------------------------------------------------------

/// The text size picker, the same in every app: a click applies at once, so a
/// settings page needs no save path for it.
pub fn text_size_control() -> impl IntoElement {
    let current = fonts::font_scale();
    let mut trough = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(34.))
        .p(px(3.))
        .rounded(px(8.))
        .border_1()
        .border_color(palette::hairline())
        .bg(palette::well());
    for (size, label) in TEXT_SIZES {
        let active = size == current;
        let id = format!("text-size-{size}");
        let selector = id.clone();
        trough = trough.child(
            div()
                .id(SharedString::from(id))
                .debug_selector(move || selector.clone())
                .a11y_toggle(format!("Text size {label}"), active)
                .cursor_pointer()
                .h(px(28.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_size(px(fonts::font_row_meta()))
                .font_weight(FontWeight::MEDIUM)
                .when(active, |d| {
                    d.bg(palette::raised()).text_color(palette::text())
                })
                .when(!active, |d| {
                    d.text_color(palette::text_muted())
                        .hover(|s| s.text_color(palette::text()))
                })
                .on_click(move |_, _, cx| set_text_size(size, cx))
                .child(label),
        );
    }
    trough
}

// ---- the desktop -----------------------------------------------------------

/// gsettings exists only on Linux desktops; elsewhere the factor stays 1.0.
#[cfg(all(unix, not(target_os = "macos")))]
mod desktop {
    use super::{HEARD, Ordering};
    use std::io::BufRead as _;
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;

    const SCHEMA: &str = "org.gnome.desktop.interface";
    const KEY: &str = "text-scaling-factor";

    /// The spawned `gsettings monitor`: kept reachable so the quit hook can
    /// kill it. Dropping a Child leaves it running as an orphan until the next
    /// write to its closed pipe, which on a quiet desktop may never come.
    static MONITOR: Mutex<Option<Child>> = Mutex::new(None);

    pub fn read_factor() -> Option<f32> {
        let output = Command::new("gsettings")
            .args(["get", SCHEMA, KEY])
            .output()
            .ok()?;
        parse_factor(&String::from_utf8_lossy(&output.stdout))
    }

    /// False when nothing started: no gsettings, or a monitor already running.
    pub fn spawn_monitor() -> bool {
        let Ok(mut slot) = MONITOR.lock() else {
            return false;
        };
        if slot.is_some() {
            return false;
        }
        let spawned = Command::new("gsettings")
            .args(["monitor", SCHEMA])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            tracing::debug!(
                "gsettings monitor unavailable; system text scale frozen at startup value"
            );
            return false;
        };
        let Some(stdout) = child.stdout.take() else {
            return false;
        };
        let reader = std::thread::Builder::new()
            .name("gsettings".into())
            .spawn(move || {
                for line in std::io::BufReader::new(stdout)
                    .lines()
                    .map_while(Result::ok)
                {
                    if let Some(factor) = parse_factor(&line) {
                        HEARD.store(factor.to_bits(), Ordering::Relaxed);
                    }
                }
            });
        if reader.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        *slot = Some(child);
        true
    }

    /// Best-effort teardown: an already-dead child kill+waits fine, a live one
    /// dies now.
    pub fn stop_monitor() {
        if let Ok(mut slot) = MONITOR.lock()
            && let Some(mut child) = slot.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Accepts bare "1.1667" (gsettings get) and "key: 1.1667" (monitor) lines.
    fn parse_factor(text: &str) -> Option<f32> {
        let text = match text.find(KEY) {
            Some(idx) => &text[idx + KEY.len()..],
            None => text,
        };
        let text = text.trim_start_matches([':', ' ', '\t']).trim_end();
        text.parse::<f32>().ok().filter(|v| *v > 0.0)
    }

    #[cfg(test)]
    mod tests {
        use super::parse_factor;

        #[test]
        fn reads_both_gsettings_forms() {
            assert_eq!(parse_factor("1.25\n"), Some(1.25));
            assert_eq!(parse_factor("text-scaling-factor: 1.5"), Some(1.5));
        }

        #[test]
        fn ignores_other_keys_and_nonsense() {
            assert_eq!(parse_factor("cursor-size: 24"), None);
            assert_eq!(parse_factor("text-scaling-factor: 0"), None);
            assert_eq!(parse_factor(""), None);
        }
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
mod desktop {
    pub fn read_factor() -> Option<f32> {
        None
    }

    pub fn spawn_monitor() -> bool {
        false
    }

    pub fn stop_monitor() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::factor_lock::serial;
    use std::cell::Cell;

    fn scratch_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("warp-scale-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("ui.json")
    }

    fn count_rescales(cx: &mut gpui::TestAppContext) -> Rc<Cell<u32>> {
        let rescales = Rc::new(Cell::new(0));
        let seen = rescales.clone();
        cx.update(|cx| {
            cx.set_global(ScaleHost {
                on_change: Rc::new(move |_| seen.set(seen.get() + 1)),
            });
        });
        rescales
    }

    fn step(cx: &mut gpui::TestAppContext) {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }

    #[test]
    fn a_write_keeps_the_rest_of_the_file() {
        let file = scratch_file("keeps");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, r#"{"other": 1, "text_size": "normal"}"#).unwrap();

        write_text_size(&file, fonts::FONT_LARGE).unwrap();

        assert_eq!(read_text_size(&file), Some(fonts::FONT_LARGE));
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(doc["other"], 1);
    }

    #[test]
    fn a_broken_file_reads_as_unset_and_can_be_rewritten() {
        let file = scratch_file("broken");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "not json").unwrap();
        assert_eq!(read_text_size(&file), None);

        write_text_size(&file, fonts::FONT_LARGER).unwrap();
        assert_eq!(read_text_size(&file), Some(fonts::FONT_LARGER));
    }

    #[test]
    fn the_shared_value_beats_the_apps_old_setting() {
        let _turn = serial();
        let file = scratch_file("beats");
        write_text_size(&file, fonts::FONT_LARGER).unwrap();

        adopt_from(Some(&file), Some(fonts::FONT_LARGE));

        assert_eq!(fonts::font_scale(), fonts::FONT_LARGER);
        assert_eq!(read_text_size(&file), Some(fonts::FONT_LARGER));
        fonts::set_font_scale(None);
    }

    #[test]
    fn the_first_run_seeds_the_file_from_the_old_setting() {
        let _turn = serial();
        let file = scratch_file("seeds");

        adopt_from(Some(&file), Some(fonts::FONT_LARGE));

        assert_eq!(fonts::font_scale(), fonts::FONT_LARGE);
        assert_eq!(read_text_size(&file), Some(fonts::FONT_LARGE));
        fonts::set_font_scale(None);
    }

    #[test]
    fn no_old_setting_writes_nothing() {
        let _turn = serial();
        let file = scratch_file("nothing");

        adopt_from(Some(&file), None);

        assert_eq!(fonts::font_scale(), fonts::FONT_NORMAL);
        assert!(!file.exists());
    }

    #[gpui::test]
    fn a_heard_factor_reaches_the_app_once(cx: &mut gpui::TestAppContext) {
        let _turn = serial();
        let rescales = count_rescales(cx);
        cx.update(poll_heard_factor);

        HEARD.store(1.25_f32.to_bits(), Ordering::Relaxed);
        step(cx);
        assert_eq!(rescales.get(), 1);
        assert_eq!(fonts::system_factor(), 1.25);

        // the monitor repeats itself on any key of the schema
        HEARD.store(1.25_f32.to_bits(), Ordering::Relaxed);
        step(cx);
        assert_eq!(rescales.get(), 1, "the same factor is not a change");

        fonts::set_system_factor(1.0);
    }

    #[gpui::test]
    fn a_new_size_rescales_the_app_once(cx: &mut gpui::TestAppContext) {
        let _turn = serial();
        fonts::set_font_scale(None);
        let rescales = count_rescales(cx);

        cx.update(|cx| apply_text_size(fonts::FONT_LARGE, cx));
        assert_eq!(rescales.get(), 1);
        assert_eq!(fonts::font_scale(), fonts::FONT_LARGE);

        cx.update(|cx| apply_text_size(fonts::FONT_LARGE, cx));
        assert_eq!(rescales.get(), 1, "the same size is not a change");

        fonts::set_font_scale(None);
    }
}

// ---- the shared palette ----------------------------------------------------

const PALETTE_KEY: &str = "palette";

fn read_palette(file: &Path) -> Option<&'static str> {
    let text = fs::read_to_string(file).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let key = doc.get(PALETTE_KEY)?.as_str()?;
    Some(palette::normalize_ramp(Some(key)).key)
}

/// Settles the palette at startup from the shared file; call before the first
/// kit theme, so the first frame already draws in it. `legacy` seeds the file
/// on the first run and is ignored afterwards.
pub fn adopt_palette(legacy: Option<&str>) {
    let Some(file) = shared_file() else {
        if let Some(legacy) = legacy {
            palette::set_ramp(legacy);
        }
        return;
    };
    let saved = read_palette(&file);
    let key = saved.or_else(|| legacy.map(|k| palette::normalize_ramp(Some(k)).key));
    if let (None, Some(key)) = (saved, key)
        && let Err(err) = write_setting(&file, PALETTE_KEY, key)
    {
        tracing::warn!("could not save the palette to {}: {err}", file.display());
    }
    if let Some(key) = key {
        palette::set_ramp(key);
    }
}

/// Switches the palette: this app repaints on its next frame, the others read
/// the value when they next start. The caller re-runs its kit theme, because
/// the kit's own tokens are derived from the palette too.
pub fn set_palette(key: &str, cx: &mut App) {
    let ramp = palette::set_ramp(key);
    if let Some(file) = shared_file()
        && let Err(err) = write_setting(&file, PALETTE_KEY, ramp.key)
    {
        tracing::warn!("could not save the palette to {}: {err}", file.display());
    }
    // the kit's own tokens (input ring, primary, selection) derive from the
    // palette, so the app re-runs its kit theme through this hook
    if let Some(host) = PALETTE_HOST.with(|slot| slot.borrow().clone()) {
        host(cx);
    }
}

/// The app's kit-theme re-run, installed by [`follow_palette`].
type PaletteHost = std::rc::Rc<dyn Fn(&mut App)>;

thread_local! {
    static PALETTE_HOST: std::cell::RefCell<Option<PaletteHost>> =
        const { std::cell::RefCell::new(None) };
}

/// Hooks the app up to palette switches: `on_change` re-runs the kit theme and
/// repaints. Call once at startup, next to `follow_ui_scale`.
pub fn follow_palette(on_change: impl Fn(&mut App) + 'static) {
    PALETTE_HOST.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(on_change)));
}

/// The palette the settings control shows as selected.
pub fn palette_key() -> &'static str {
    palette::current_ramp().key
}

/// The palette picker: one segment per palette, a click applies at once, so a
/// settings page needs no save path for it. The caller passes the click's app
/// context through, because a switch re-runs the kit theme.
pub fn palette_control() -> impl IntoElement {
    let current = palette_key();
    let mut trough = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(34.))
        .p(px(3.))
        .rounded(px(8.))
        .border_1()
        .border_color(palette::current_ramp().line)
        .bg(palette::current_ramp().well);
    for ramp in &palette::RAMPS {
        let active = ramp.key == current;
        let id = format!("palette-{}", ramp.key);
        let selector = id.clone();
        let key = ramp.key;
        trough = trough.child(
            div()
                .id(SharedString::from(id))
                .debug_selector(move || selector.clone())
                .a11y_toggle(format!("Palette {}", ramp.label), active)
                .cursor_pointer()
                .h(px(28.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(6.))
                .text_size(px(fonts::font_row_meta()))
                .font_weight(FontWeight::MEDIUM)
                .when(active, |d| d.bg(ramp.raised).text_color(ramp.text))
                .when(!active, |d| {
                    d.text_color(palette::current_ramp().text_muted)
                        .hover(|s| s.text_color(palette::current_ramp().text))
                })
                .child(div().size(px(10.)).rounded_full().bg(ramp.accent))
                .on_click(move |_, _, cx| set_palette(key, cx))
                .child(ramp.label),
        );
    }
    trough
}
