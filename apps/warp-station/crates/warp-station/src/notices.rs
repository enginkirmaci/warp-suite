//! The one notification service: every surfaced result walks in here and the
//! two outlets fan out — the in-app toast stack (rendered by the window, fed
//! by [`Notices::push`]) and the desktop channel (gated by
//! `warp_station_core::desktop_notifications`, published on a detached
//! thread). The store keeps ownership of the state; this module owns the
//! policy, so the mirror semantics live in one place and the store's
//! `notify_*` API stays the one call sites know.

use warp_station_core::desktop_notifications::{
    self as desktop, Category, DesktopNotice, Severity,
};
use warp_station_core::settings::NotificationsSettings;

/// A toast's severity: which glyph + color it shows and how long it stays up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    Success,
    Info,
    Error,
}

impl NoticeKind {
    /// Errors read longer; confirmations clear in the classic few seconds.
    pub fn ttl(self) -> std::time::Duration {
        match self {
            NoticeKind::Error => std::time::Duration::from_secs(8),
            NoticeKind::Info | NoticeKind::Success => std::time::Duration::from_secs(4),
        }
    }

    /// The copy helpers answer fixed static strings ("Password copied…",
    /// "No password stored", "Clipboard copy failed…"); classify those.
    pub fn for_copy_message(message: &str) -> Self {
        if message.starts_with("Clipboard copy failed") {
            NoticeKind::Error
        } else if message == "No password stored" {
            NoticeKind::Info
        } else {
            NoticeKind::Success
        }
    }

    /// Where the toast reads in the settings: errors are their own toggle,
    /// both confirmation tones share the "action confirmations" one.
    fn category(self) -> Category {
        match self {
            NoticeKind::Error => Category::Errors,
            NoticeKind::Info | NoticeKind::Success => Category::ActionConfirmations,
        }
    }

    fn severity(self) -> Severity {
        match self {
            NoticeKind::Error => Severity::Error,
            NoticeKind::Info | NoticeKind::Success => Severity::Info,
        }
    }
}

/// One toast in the window's corner stack.
#[derive(Clone, Debug)]
pub struct Notice {
    pub id: u64,
    pub kind: NoticeKind,
    pub text: String,
    born: std::time::Instant,
}

/// A toast burst never stacks a wall of cards on top of each other; the same
/// cap the desktop channel rides (a desktop burst is one bubble a second at
/// worst once the dedup window is in, so no separate counter is needed).
const TOAST_CAP: usize = 4;

/// Both outlets' shared state: the toast stack and the routing bookkeeping.
/// The store owns one instance; `push` walks every event through the desktop
/// gate, `tick`/`dismiss`/`list` serve the window's per-second beat and
/// renderer.
#[derive(Debug, Default)]
pub struct Notices {
    toasts: Vec<Notice>,
    next_id: u64,
    dedup: desktop::Dedup,
}

impl Notices {
    /// One event, both outlets. The toast always lands (that is the app's
    /// visible feedback and it expires itself); the desktop copy is what the
    /// settings decide: `focused` is the store's tick-tracked window state,
    /// `None` before the first tick reads as focused.
    ///
    /// Returns the notice to publish, if any — the caller spawns the publish
    /// thread so this stays inline in the store's existing call sites.
    pub fn push(
        &mut self,
        kind: NoticeKind,
        text: impl Into<String>,
        focused: Option<bool>,
        settings: &NotificationsSettings,
    ) -> Option<DesktopNotice> {
        let text = text.into();
        // ids start at 0: the probe selectors pin `toast-0`, so keep the
        // generation contract the old toast stack kept
        let id = self.next_id;
        self.next_id += 1;
        self.toasts.push(Notice {
            id,
            kind,
            text: text.clone(),
            born: std::time::Instant::now(),
        });
        if self.toasts.len() > TOAST_CAP {
            self.toasts.remove(0);
        }
        desktop::wants_desktop(
            kind.category(),
            kind.severity(),
            &text,
            focused,
            settings,
            &mut self.dedup,
        )
    }

    /// The desktop-only summary of a background pass (the auto-fetch result):
    /// no toast, since an unfocused window's stack is not on screen it would
    /// have expired by the time the window returns. Same gate, same dedup.
    pub fn push_desktop_only(
        &mut self,
        text: impl Into<String>,
        focused: Option<bool>,
        settings: &NotificationsSettings,
    ) -> Option<DesktopNotice> {
        desktop::wants_desktop(
            Category::FetchSummaries,
            Severity::Info,
            &text.into(),
            focused,
            settings,
            &mut self.dedup,
        )
    }

    /// The toast stack for the window's render pass (newest last).
    pub fn list(&self) -> &[Notice] {
        &self.toasts
    }

    /// The toast's close button; true when a toast actually left.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|notice| notice.id != id);
        self.toasts.len() != before
    }

    /// Drops toasts past their kind's ttl; true when one actually expired (an
    /// idle window stays idle — no render, no notify).
    pub fn tick(&mut self) -> bool {
        let before = self.toasts.len();
        self.toasts
            .retain(|notice| notice.born.elapsed() < notice.kind.ttl());
        self.toasts.len() != before
    }
}

/// The detached publish: one thread per notice, so the Windows side's WinRT
/// notifier spin-up and the Linux one-shot process never cost the store's
/// toasts or the tick their frame. The thread owns the error: a missing
/// desktop is a log line, never a panic in the app.
pub fn publish_desktop(notice: DesktopNotice) {
    let _ = std::thread::Builder::new()
        .name("desktop-notify".into())
        .spawn(move || {
            if let Err(err) = desktop::publish(&notice) {
                tracing::warn!("desktop notification failed: {err}");
            }
        });
}
