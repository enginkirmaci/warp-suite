//! Compact relative age labels ("just now", "5m ago", "2h ago", …): the most
//! significant unit only, no rounding across unit boundaries. Wall-clock
//! difference, so the label ignores the timestamp's UTC offset; future
//! timestamps (clock skew) clamp to "just now".

use chrono::{DateTime, Local};

pub fn format(at: DateTime<Local>) -> String {
    let span = Local::now() - at;
    let minutes = span.num_minutes().max(0);
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 60 * 24 {
        format!("{}h ago", minutes / 60)
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", minutes / (60 * 24))
    } else if minutes < 60 * 24 * 30 {
        format!("{}w ago", minutes / (60 * 24 * 7))
    } else if minutes < 60 * 24 * 365 {
        format!("{}mo ago", minutes / (60 * 24 * 30))
    } else {
        format!("{}y ago", minutes / (60 * 24 * 365))
    }
}

/// Relative label for an ISO-8601 timestamp (the gh pass's updatedAt shape).
pub fn format_rfc3339(iso: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|at| format(at.with_timezone(&Local)))
}

/// The day a timestamp belongs to, for a list grouped by day: "Today",
/// "Yesterday", or the date itself (`2026-11-08`).
pub fn day_label(iso: &str) -> Option<String> {
    let at = DateTime::parse_from_rfc3339(iso)
        .ok()?
        .with_timezone(&Local);
    let today = Local::now().date_naive();
    let day = at.date_naive();
    if day == today {
        Some("Today".into())
    } else if today.pred_opt() == Some(day) {
        Some("Yesterday".into())
    } else {
        Some(day.format("%Y-%m-%d").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn boundaries() {
        let now = Local::now();
        assert_eq!(format(now), "just now");
        assert_eq!(format(now - Duration::minutes(5)), "5m ago");
        assert_eq!(format(now - Duration::minutes(59)), "59m ago");
        assert_eq!(format(now - Duration::hours(2)), "2h ago");
        assert_eq!(format(now - Duration::hours(23)), "23h ago");
        assert_eq!(format(now - Duration::days(3)), "3d ago");
        assert_eq!(format(now - Duration::days(20)), "2w ago");
        assert_eq!(format(now - Duration::days(60)), "2mo ago");
        assert_eq!(format(now - Duration::days(400)), "1y ago");
        // future clamps
        assert_eq!(format(now + Duration::minutes(5)), "just now");
    }

    #[test]
    fn rfc3339_labels() {
        let now = Local::now() - Duration::days(70);
        let iso = now
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(format_rfc3339(&iso).as_deref(), Some("2mo ago"));
        assert_eq!(format_rfc3339("not a date"), None);
    }

    #[test]
    fn a_day_label_names_today_yesterday_or_the_date() {
        let stamp = |at: DateTime<Local>| {
            at.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        };
        let now = Local::now();

        assert_eq!(day_label(&stamp(now)).as_deref(), Some("Today"));
        assert_eq!(
            day_label(&stamp(now - Duration::days(1))).as_deref(),
            Some("Yesterday")
        );
        let old = now - Duration::days(30);
        assert_eq!(
            day_label(&stamp(old)).as_deref(),
            Some(old.format("%Y-%m-%d").to_string().as_str())
        );
        assert_eq!(day_label("not a date"), None);
    }
}
