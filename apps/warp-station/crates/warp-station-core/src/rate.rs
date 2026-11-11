//! The platforms' own rate-limit signals. GitHub counts requests an hour and
//! answers `resources.core.remaining`/`reset`; Azure DevOps counts TSTUs per
//! five-minute window. Both send `X-RateLimit-Remaining`/`X-RateLimit-Reset`,
//! and a refused call adds `Retry-After`.
//!
//! The Pipelines follow loop turns those into a wait: when the budget says it
//! is spent — or a 429 named the moment it refills — the next poll is parked
//! until then instead of being retried into another 429.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What a platform said about its budget after one call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateBudget {
    /// The fewest requests (GitHub) or TSTUs (Azure) the answers said were
    /// left in the window.
    pub remaining: Option<u32>,
    /// Epoch seconds when the window refills.
    pub reset: Option<i64>,
    /// The call was refused as rate limited (HTTP 429).
    pub limited: bool,
}

impl RateBudget {
    /// Folds one answer's signals into the budget: the smallest `remaining`
    /// wins (the tightest window the calls have seen), a reset replaces the
    /// last one, and a 429 keeps what it names — `Retry-After` when the server
    /// sends it, the reset otherwise.
    pub fn note(
        &mut self,
        status: u16,
        remaining: Option<u32>,
        reset: Option<i64>,
        retry_after: Option<i64>,
        now: i64,
    ) {
        if let Some(remaining) = remaining {
            self.remaining = Some(self.remaining.map_or(remaining, |old| old.min(remaining)));
        }
        if let Some(reset) = reset {
            self.reset = Some(reset);
        }
        if status == 429 {
            self.limited = true;
            if let Some(seconds) = retry_after {
                self.reset = Some(now + seconds.max(0));
            }
        }
    }

    /// How long the next call should wait, when the budget is spent or a 429
    /// named a moment. `reserve` is the slice kept for everything else the app
    /// does — other tabs, the activity pass, the user's own opens — so the
    /// follow loop yields before the last request is gone. `None` means the
    /// caller's own cadence is fine.
    pub fn wait(&self, reserve: u32) -> Option<Duration> {
        let spent = self.limited || self.remaining.is_some_and(|left| left <= reserve);
        if !spent {
            return None;
        }
        let reset = self.reset?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
        Some(Duration::from_secs((reset - now).clamp(1, 60 * 60) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch(delta: i64) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_secs() as i64
            + delta
    }

    /// The answers' signals fold tightest-first: a smaller remaining wins, a
    /// 429's `Retry-After` (or its reset) names the moment, and a plain answer
    /// keeps what earlier ones said.
    #[test]
    fn the_budget_folds_the_answers_tightest_first() {
        let now = epoch(0);
        let mut budget = RateBudget::default();
        budget.note(200, Some(4_000), Some(now + 900), None, now);
        assert_eq!(budget.remaining, Some(4_000));
        assert_eq!(budget.reset, Some(now + 900));

        budget.note(200, Some(120), Some(now + 600), None, now);
        assert_eq!(budget.remaining, Some(120), "the smaller window wins");
        assert_eq!(budget.reset, Some(now + 600));

        budget.note(200, Some(5_000), None, None, now);
        assert_eq!(budget.remaining, Some(120), "a bigger remaining never wins");
        assert_eq!(
            budget.reset,
            Some(now + 600),
            "a missing reset changes nothing"
        );

        budget.note(429, None, Some(now + 30), Some(45), now);
        assert!(budget.limited);
        assert_eq!(
            budget.reset,
            Some(now + 45),
            "Retry-After outranks the reset on a 429"
        );
    }

    /// A healthy budget leaves the cadence alone; a spent one, or a 429, parks
    /// the next poll until the window refills.
    #[test]
    fn a_spent_budget_names_the_wait() {
        let healthy = RateBudget {
            remaining: Some(4_000),
            reset: Some(epoch(600)),
            limited: false,
        };
        assert_eq!(healthy.wait(20), None, "plenty left: no wait");

        let low = RateBudget {
            remaining: Some(5),
            reset: Some(epoch(60)),
            ..Default::default()
        };
        let wait = low.wait(20).expect("a low budget waits");
        assert!(
            wait.as_secs() >= 55 && wait.as_secs() <= 61,
            "the wait runs to the reset, got {wait:?}"
        );

        let limited = RateBudget {
            remaining: None,
            reset: Some(epoch(30)),
            limited: true,
        };
        assert!(
            limited.wait(20).is_some_and(|wait| wait.as_secs() >= 25),
            "a 429 waits for the moment it named"
        );

        // a reset already past is a one-second pause, never a negative one
        let past = RateBudget {
            remaining: Some(0),
            reset: Some(epoch(-90)),
            limited: true,
        };
        assert_eq!(past.wait(20), Some(Duration::from_secs(1)));

        // a spent budget without a reset falls back to the caller's cadence
        let unknown = RateBudget {
            remaining: Some(0),
            reset: None,
            limited: true,
        };
        assert_eq!(unknown.wait(20), None);
    }
}
