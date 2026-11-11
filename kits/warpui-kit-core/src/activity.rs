//! The window's activity line.
//!
//! The titlebar chip and the OS window title say the same thing: what the
//! app is doing right now, or nothing when it is idle. One formatter keeps the
//! two from drifting (see `docs/agents/design-system.md`, *Motion* › *Where it
//! shows*).

/// `2 running — Warp Agent`; the app name alone when there is no activity.
///
/// Activity comes first so a truncated taskbar label still carries it. The
/// em dash matches the notes title's `name — Warp Notes`.
pub fn activity_title(app: &str, activity: Option<&str>) -> String {
    match activity
        .map(str::trim)
        .filter(|activity| !activity.is_empty())
    {
        Some(activity) => format!("{activity} — {app}"),
        None => app.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_window_is_just_the_app_name() {
        assert_eq!(activity_title("Warp Agent", None), "Warp Agent");
        assert_eq!(activity_title("Warp Station", Some("  ")), "Warp Station");
    }

    #[test]
    fn activity_leads_so_truncation_keeps_it() {
        assert_eq!(
            activity_title("Warp Agent", Some("2 running")),
            "2 running — Warp Agent"
        );
        assert_eq!(
            activity_title("Warp Station", Some("Generating alpha")),
            "Generating alpha — Warp Station"
        );
    }
}
