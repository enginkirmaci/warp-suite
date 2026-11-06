//! OS-level desktop notifications: one entry point per notice, one branch per
//! platform. The headless side of the app's notification service (`notices.rs`
//! in the UI crate owns the routing policy — settings, focus, dedup); this
//! module only knows how to hand one severity + text pair to the desktop.
//!
//! - Linux (and the other Unix desktops): spawn `notify-send`, which every
//!   notification daemon ships (the same command-gated pattern
//!   [super::terminal] uses to reach the user's own terminal). A missing
//!   binary is the caller's log line, never a panic.
//! - Windows: a WinRT toast through `tauri-winrt-notification`, addressed by
//!   PowerShell's whitelisted AppUserModelID so Action Center accepts it
//!   without a Start-menu registration.
//! - macOS: a best-effort stub so shared code never blocks on Apple specifics.

use crate::settings::NotificationsSettings;

/// A notification's severity on its way to the desktop: picks the urgency and
/// sound level, mirrors the toast kinds one to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Success,
    Info,
    Error,
}

/// One ready-to-publish notification: a short summary line and the body, the
/// same text the toast showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopNotice {
    pub severity: Severity,
    pub summary: String,
    pub body: String,
}

impl DesktopNotice {
    /// The routing gate's construction lives in [wants_desktop]; tests and
    /// tools that already know the text go through here.
    pub fn body(severity: Severity, text: impl Into<String>) -> Self {
        Self {
            severity,
            summary: "Warp Station".into(),
            body: text.into(),
        }
    }
}

/// Publishes on the calling thread (every platform's call is one shot; the
/// service already dedups, so this is at most a handful of notices a minute).
/// Failures are returned, never logged twice or turned into a panic — a
/// missing notification daemon must not take the app down.
pub fn publish(notice: &DesktopNotice) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = notice;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = std::process::Command::new("notify-send");
        command.arg("--app-name=Warp Station");
        if notice.severity == Severity::Error {
            // errors reach the desktop as urgent: on the daemons the suite's
            // users run (dunst, mako, swaync) critical urgency survives the
            // popup's default timeout until it is read
            command.args(["-u", "critical"]);
        }
        command.arg(&notice.summary).arg(&notice.body);
        command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("notify-send: {err}"))
    }

    #[cfg(windows)]
    {
        use tauri_winrt_notification::{Duration as ToastDuration, Toast};

        let toast = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(&notice.summary)
            .text1(&notice.body)
            // errors stay visible longer; everything else is a confirmation
            .duration(if notice.severity == Severity::Error {
                ToastDuration::Long
            } else {
                ToastDuration::Short
            });
        toast.show().map_err(|err| format!("toast: {err}"))
    }
}

/// The routing gate, shared by every caller of the service so the settings
/// mean the same thing everywhere. Pure and display-free: it maps a toast
/// event onto at most one desktop notice, weighing the origin (a background
/// pass has no toast the user could have seen), the live focus state and the
/// dedup window.
///
/// Kinds and texts are owned by the UI side; here they arrive reduced to what
/// the settings can express: a category tag and the render text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// "Commit failed for alpha …" — the `Errors` toggle.
    Errors,
    /// "Merged #12", "Password copied …" — the `ActionConfirmations` toggle.
    ActionConfirmations,
    /// The auto-fetch aggregate — the `FetchSummaries` toggle.
    FetchSummaries,
}

impl Category {
    fn allowed(self, settings: &NotificationsSettings) -> bool {
        match self {
            Category::Errors => settings.errors,
            Category::ActionConfirmations => settings.action_confirmations,
            Category::FetchSummaries => settings.fetch_summaries,
        }
    }
}

/// Identical categories+texts re-raise inside this window across the whole app
/// collapse to nothing: a burst of identical "git fetch failed for X" toasts
/// would otherwise stack the daemon's history up.
#[derive(Debug)]
pub struct Dedup {
    window: std::time::Duration,
    last: std::collections::HashMap<(Category, String), std::time::Instant>,
}

impl Default for Dedup {
    fn default() -> Self {
        Self {
            window: DEDUP_WINDOW,
            last: std::collections::HashMap::new(),
        }
    }
}

/// One blink of the toast stack is enough time for a burst of identical
/// notices; anything inside this is the same event seen twice.
const DEDUP_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

impl Dedup {
    /// True when this exact notification fired inside the last window; the
    /// bookkeeping happens regardless, so a repeated hit keeps the entry hot.
    fn is_repeat(&mut self, key: &(Category, String)) -> bool {
        let now = std::time::Instant::now();
        self.last
            .retain(|_, seen| now.duration_since(*seen) < self.window);
        let seen = self.last.insert(key.clone(), now);
        seen.is_some()
    }

    /// Test-only knob: shrink the window so a test can prove suppression and
    /// expiry without waiting five seconds.
    #[cfg(test)]
    fn with_window(window: std::time::Duration) -> Self {
        Self {
            window,
            last: std::collections::HashMap::new(),
        }
    }
}

/// The one gate every desktop copy walks through. `focused` is the store's
/// tick-tracked window state (`None` before the first tick reads as focused —
/// a launch that starts visible must not rain notifications).
pub fn wants_desktop(
    category: Category,
    severity: Severity,
    text: &str,
    focused: Option<bool>,
    settings: &NotificationsSettings,
    dedup: &mut Dedup,
) -> Option<DesktopNotice> {
    if !settings.enabled || !category.allowed(settings) {
        return None;
    }
    let focused = focused.unwrap_or(true);
    // the desktop's whole job is catching the developer when the app is not
    // on screen: an on-screen toast and a desktop bubble for the same text
    // is duplication, and the background summaries obey the same gate —
    // turning the unfocused gate off means the mirrors show everywhere
    if settings.only_when_unfocused && focused {
        return None;
    }
    if dedup.is_repeat(&(category, text.to_string())) {
        return None;
    }
    Some(DesktopNotice {
        severity,
        summary: "Warp Station".into(),
        body: text.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn gate() -> (NotificationsSettings, Dedup) {
        (
            NotificationsSettings::default(),
            Dedup::with_window(Duration::from_millis(300)),
        )
    }

    fn settings_with(change: impl FnOnce(&mut NotificationsSettings)) -> NotificationsSettings {
        let mut settings = NotificationsSettings::default();
        change(&mut settings);
        settings
    }

    #[test]
    fn unfocused_toasts_mirror_and_focused_ones_do_not() {
        let (mut settings, mut dedup) = gate();
        // away: the toast is rendered but nobody sees it — mirror it
        let away = wants_desktop(
            Category::ActionConfirmations,
            Severity::Success,
            "Merged #12",
            Some(false),
            &settings,
            &mut dedup,
        );
        assert_eq!(away.map(|n| n.body), Some("Merged #12".into()));

        // on screen: the toast is right there; the desktop copy duplicates it
        let (settings, mut dedup) = gate();
        let here = wants_desktop(
            Category::ActionConfirmations,
            Severity::Success,
            "Merged #12",
            Some(true),
            &settings,
            &mut dedup,
        );
        assert!(here.is_none());

        // before the first tick the state reads focused, so a launch does
        // not open with a wall of bubbles
        let unobserved = wants_desktop(
            Category::ActionConfirmations,
            Severity::Success,
            "Merged #12",
            None,
            &settings,
            &mut dedup,
        );
        assert!(unobserved.is_none());
    }

    #[test]
    fn the_unfocused_gate_turns_off_for_the_whole_channel() {
        let settings = settings_with(|s| s.only_when_unfocused = false);
        let (_settings_ref, mut dedup) = gate();
        let always = wants_desktop(
            Category::ActionConfirmations,
            Severity::Success,
            "Merged #12",
            Some(true),
            &settings,
            &mut dedup,
        );
        assert!(always.is_some(), "gate off means on-screen mirrors too");
    }

    #[test]
    fn identical_notices_inside_the_window_collapse() {
        let (settings, mut dedup) = gate();
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "Fetch failed for /work/alpha: no network",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_some()
        );
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "Fetch failed for /work/alpha: no network",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_none(),
            "the same burst event seen twice stays one bubble"
        );
        // a different text is its own event
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "Fetch failed for /work/beta: no network",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_some()
        );
    }

    #[test]
    fn the_master_switch_and_categories_cut_their_own_rows() {
        // master off silences everything
        let master_off = settings_with(|s| s.enabled = false);
        let (_s, mut dedup) = gate();
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "anything",
                Some(false),
                &master_off,
                &mut dedup,
            )
            .is_none()
        );

        // an opt-out category only cuts its own rows
        let no_fetch = settings_with(|s| s.fetch_summaries = false);
        let (_s, mut dedup) = gate();
        assert!(
            wants_desktop(
                Category::FetchSummaries,
                Severity::Info,
                "3 repos behind",
                Some(true),
                &no_fetch,
                &mut dedup,
            )
            .is_none()
        );
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "still loud",
                Some(false),
                &no_fetch,
                &mut dedup,
            )
            .is_some()
        );
    }

    #[test]
    fn background_summaries_obey_the_gate_like_everything_else() {
        let (settings, mut dedup) = gate();
        // on screen the pass still shows nothing on the desktop
        assert!(
            wants_desktop(
                Category::FetchSummaries,
                Severity::Info,
                "3 repos behind",
                Some(true),
                &settings,
                &mut dedup,
            )
            .is_none()
        );
        // away from the desk the summary lands
        let (_s, mut dedup) = gate();
        let summary = wants_desktop(
            Category::FetchSummaries,
            Severity::Info,
            "3 repos behind",
            Some(false),
            &settings,
            &mut dedup,
        );
        assert_eq!(
            summary.map(|n| (n.severity, n.summary == "Warp Station")),
            Some((Severity::Info, true)),
            "the aggregate reads as info under the app's own name"
        );
    }

    #[test]
    fn the_dedup_window_expires_so_a_rare_repeat_still_fires() {
        let (settings, mut dedup) = gate();
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "once",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_some()
        );
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "once",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_none(),
            "immediately: suppressed"
        );
        std::thread::sleep(Duration::from_millis(350));
        assert!(
            wants_desktop(
                Category::Errors,
                Severity::Error,
                "once",
                Some(false),
                &settings,
                &mut dedup,
            )
            .is_some(),
            "after the window: allowed again"
        );
    }
}
