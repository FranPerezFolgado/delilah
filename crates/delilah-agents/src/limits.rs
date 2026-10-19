//! Usage-limit detection for Claude Code subscriptions.
//!
//! Claude pipes `rate_limits` to the status line on every turn
//! (`{"five_hour": {"used_percentage": 82, "resets_at": <epoch s>}, "seven_day": …}`),
//! and ends a turn that failed on an API error with `StopFailure` instead of `Stop`.
//! The exact `StopFailure` payload for a limit has not been observed yet, so the rule
//! relies on the windows first and on the payload's wording only as a fallback.

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;

use crate::claude::Signal;

/// When a turn failed on a limit but no reset time is known: try again after this.
pub const UNKNOWN_RESET_RETRY: Duration = Duration::minutes(30);
/// Windows this full are the likely culprit of a limit failure.
const NEARLY_FULL: f64 = 80.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub name: String,
    pub used_percentage: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// The usage windows in a status line `rate_limits` object.
pub fn windows(rate_limits: &Value) -> Vec<Window> {
    let Some(map) = rate_limits.as_object() else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(name, w)| {
            // `utilization` is the usage endpoint's spelling; accept it if the CLI drifts.
            let used = w
                .get("used_percentage")
                .or_else(|| w.get("utilization"))?
                .as_f64()?;
            Some(Window {
                name: name.clone(),
                used_percentage: used,
                resets_at: w.get("resets_at").and_then(parse_reset),
            })
        })
        .collect()
}

fn parse_reset(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Number(n) => Utc.timestamp_opt(n.as_i64()?, 0).single(),
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        _ => None,
    }
}

/// For a turn that just ended: `Some(resume_at)` if it ended because the subscription
/// ran out, `None` if it ended normally or for another reason.
pub fn limit_reset(
    signal: Signal,
    hook: &Value,
    rate_limits: Option<&Value>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if !matches!(signal, Signal::TurnEnded | Signal::TurnFailed) {
        return None;
    }
    let windows = rate_limits.map(windows).unwrap_or_default();

    // A full window blocks the next turn whatever this one did: wait for the latest
    // reset among the full ones, since all of them must have reset.
    let full: Vec<&Window> = windows
        .iter()
        .filter(|w| w.used_percentage >= 100.0)
        .collect();
    if !full.is_empty() {
        let reset = full.iter().filter_map(|w| w.resets_at).max();
        return Some(
            reset
                .filter(|r| *r > now)
                .unwrap_or(now + UNKNOWN_RESET_RETRY),
        );
    }

    if signal == Signal::TurnFailed && mentions_limit(hook) {
        // The status line may lag behind: the nearly full window is the best guess.
        let reset = windows
            .iter()
            .filter(|w| w.used_percentage >= NEARLY_FULL)
            .filter_map(|w| w.resets_at)
            .filter(|r| *r > now)
            .min();
        return Some(reset.unwrap_or(now + UNKNOWN_RESET_RETRY));
    }
    None
}

fn mentions_limit(hook: &Value) -> bool {
    let text = hook.to_string().to_lowercase();
    [
        "rate_limit",
        "rate limit",
        "usage limit",
        "limit reached",
        "429",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 14, 0, 0).unwrap()
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, hour, 0, 0).unwrap()
    }

    fn limits(five: f64, five_reset: DateTime<Utc>, week: f64, week_reset: DateTime<Utc>) -> Value {
        json!({
            "five_hour": { "used_percentage": five, "resets_at": five_reset.timestamp() },
            "seven_day": { "used_percentage": week, "resets_at": week_reset.timestamp() },
        })
    }

    fn hook(name: &str) -> Value {
        json!({ "hook_event_name": name })
    }

    #[test]
    fn parses_the_status_line_windows() {
        let w = windows(&limits(49.0, at(15), 51.0, at(20)));
        let five = w.iter().find(|w| w.name == "five_hour").unwrap();
        assert_eq!(five.used_percentage, 49.0);
        assert_eq!(five.resets_at, Some(at(15)));
        assert!(windows(&json!(null)).is_empty());
    }

    #[test]
    fn a_normal_turn_is_not_a_limit() {
        let l = limits(82.0, at(15), 51.0, at(20));
        assert_eq!(
            limit_reset(Signal::TurnEnded, &hook("Stop"), Some(&l), now()),
            None
        );
        assert_eq!(
            limit_reset(Signal::TurnFailed, &hook("StopFailure"), Some(&l), now()),
            None
        );
    }

    #[test]
    fn a_full_window_pauses_until_it_resets() {
        let l = limits(100.0, at(15), 51.0, at(20));
        assert_eq!(
            limit_reset(Signal::TurnEnded, &hook("Stop"), Some(&l), now()),
            Some(at(15))
        );
    }

    #[test]
    fn with_several_full_windows_waits_for_the_last_reset() {
        let l = limits(100.0, at(15), 100.0, at(20));
        assert_eq!(
            limit_reset(Signal::TurnFailed, &hook("StopFailure"), Some(&l), now()),
            Some(at(20))
        );
    }

    #[test]
    fn a_failure_that_mentions_the_limit_uses_the_nearly_full_window() {
        let failed =
            json!({ "hook_event_name": "StopFailure", "error": "Claude usage limit reached" });
        let l = limits(97.0, at(15), 51.0, at(20));
        assert_eq!(
            limit_reset(Signal::TurnFailed, &failed, Some(&l), now()),
            Some(at(15))
        );
        // No usable window: try again later.
        assert_eq!(
            limit_reset(Signal::TurnFailed, &failed, None, now()),
            Some(now() + UNKNOWN_RESET_RETRY)
        );
    }

    #[test]
    fn a_reset_in_the_past_retries_later() {
        let l = limits(100.0, at(13), 51.0, at(20));
        assert_eq!(
            limit_reset(Signal::TurnEnded, &hook("Stop"), Some(&l), now()),
            Some(now() + UNKNOWN_RESET_RETRY)
        );
    }

    #[test]
    fn other_hooks_are_ignored() {
        let l = limits(100.0, at(15), 51.0, at(20));
        assert_eq!(
            limit_reset(
                Signal::PromptSubmitted,
                &hook("UserPromptSubmit"),
                Some(&l),
                now()
            ),
            None
        );
    }
}
