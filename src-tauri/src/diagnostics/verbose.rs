//! "Detailed log" switch: on for at most seven days, because it is typically
//! turned on for one bug report and then forgotten.

pub const VERBOSE_DAYS: i64 = 7;
pub const SETTING_KEY: &str = "verbose_logging_until";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub fn until_from(now_ms: i64) -> i64 {
    now_ms + VERBOSE_DAYS * DAY_MS
}

pub fn parse_until(raw: Option<String>) -> Option<i64> {
    raw.and_then(|s| s.trim().parse::<i64>().ok())
}

pub fn is_active(until_ms: Option<i64>, now_ms: i64) -> bool {
    matches!(until_ms, Some(u) if now_ms < u)
}

pub fn level(active: bool) -> log::LevelFilter {
    if active { log::LevelFilter::Debug } else { log::LevelFilter::Info }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_000_000_000;

    #[test]
    fn active_until_seven_days_later() {
        let until = until_from(NOW);
        assert!(is_active(Some(until), NOW));
        assert!(is_active(Some(until), until - 1));
        assert!(!is_active(Some(until), until));
    }

    #[test]
    fn off_without_or_with_garbage_setting() {
        assert!(!is_active(parse_until(None), NOW));
        assert!(!is_active(parse_until(Some(String::new())), NOW));
        assert!(!is_active(parse_until(Some("kaputt".into())), NOW));
    }

    #[test]
    fn level_follows_state() {
        assert_eq!(level(true), log::LevelFilter::Debug);
        assert_eq!(level(false), log::LevelFilter::Info);
    }
}
