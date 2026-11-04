//! What the interface shows for a connection, and when the next attempt is due. Pure functions
//! on stored values, so every border can be tested without a printer.

use crate::db::printer_link::PrinterConnectionRecord;

/// Failures in a row before "connecting again" turns into "not reachable".
pub const RECONNECT_FAILURES: i64 = 2;
/// A printer that has been unreachable for this long counts as an error (red) instead of "off" (grey).
pub const OFFLINE_ERROR_AFTER_S: f64 = 24.0 * 3600.0;
/// Failures from which the wait between two attempts grows from 10 to 30 minutes.
const SLOW_BACKOFF_FAILURES: i64 = 4;
const BACKOFF_SHORT_S: f64 = 10.0 * 60.0;
const BACKOFF_LONG_S: f64 = 30.0 * 60.0;

/// Error codes that usually pass by themselves. Everything else (login, missing history, address,
/// wrong device, name, container) needs the user and is an error right away.
pub fn is_transient_code(code: &str) -> bool {
    matches!(code, "timeout" | "connection_refused" | "connection_reset" | "server_busy" | "unreachable")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Ok,
    Reconnecting,
    Offline,
    Error,
    Paused,
}

impl ConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionStatus::Ok => "ok",
            ConnectionStatus::Reconnecting => "reconnecting",
            ConnectionStatus::Offline => "offline",
            ConnectionStatus::Error => "error",
            ConnectionStatus::Paused => "paused",
        }
    }
}

/// Status of a connection at `now` (Unix seconds).
pub fn connection_status(c: &PrinterConnectionRecord, now: f64) -> ConnectionStatus {
    if c.paused {
        return ConnectionStatus::Paused;
    }
    let Some(code) = c.last_error.as_deref() else {
        return ConnectionStatus::Ok;
    };
    if !is_transient_code(code) {
        return ConnectionStatus::Error;
    }
    if c.failures < RECONNECT_FAILURES {
        return ConnectionStatus::Reconnecting;
    }
    match c.error_since {
        Some(since) if now - since < OFFLINE_ERROR_AFTER_S => ConnectionStatus::Offline,
        _ => ConnectionStatus::Error,
    }
}

/// Whether the background pass contacts this printer now. The first failures are retried at
/// every pass (the printer may just have restarted), then the pauses grow so a switched-off
/// printer is not knocked on every five minutes. `force` is "Connect now".
pub fn should_attempt(failures: i64, last_attempt_at: Option<f64>, now: f64, force: bool) -> bool {
    if force || failures < RECONNECT_FAILURES {
        return true;
    }
    let Some(last) = last_attempt_at else { return true };
    let wait = if failures < SLOW_BACKOFF_FAILURES { BACKOFF_SHORT_S } else { BACKOFF_LONG_S };
    // A clock that went backwards must not silence a printer for good.
    now - last >= wait || now < last
}

/// Whether the background pass contacts this connection now. The wait only applies to failures
/// that usually pass; a login or address problem is retried at every pass, as before.
pub fn is_due(c: &PrinterConnectionRecord, now: f64, force: bool) -> bool {
    let transient = c.last_error.as_deref().is_some_and(is_transient_code);
    force || !transient || should_attempt(c.failures, c.last_attempt_at, now, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(last_error: Option<&str>, failures: i64, error_since: Option<f64>, paused: bool) -> PrinterConnectionRecord {
        PrinterConnectionRecord {
            printer_id: 1,
            kind: "moonraker".into(),
            address: "192.168.1.60".into(),
            base_url: None,
            remote_version: None,
            connected_since: 0.0,
            last_synced_at: None,
            last_error: last_error.map(str::to_string),
            error_since,
            paused,
            clock_offset_s: 0.0,
            failures,
            last_attempt_at: None,
        }
    }

    #[test]
    fn status_table() {
        let now = 1_000_000.0;
        let day = OFFLINE_ERROR_AFTER_S;
        let cases: Vec<(PrinterConnectionRecord, ConnectionStatus)> = vec![
            (record(None, 0, None, false), ConnectionStatus::Ok),
            (record(Some("auth_required"), 1, Some(now), true), ConnectionStatus::Paused),
            (record(None, 0, None, true), ConnectionStatus::Paused),
            // transient: first failure is "connecting again", from the second on "not reachable"
            (record(Some("timeout"), 1, Some(now), false), ConnectionStatus::Reconnecting),
            (record(Some("timeout"), 0, Some(now), false), ConnectionStatus::Reconnecting),
            (record(Some("timeout"), 2, Some(now), false), ConnectionStatus::Offline),
            (record(Some("connection_refused"), 9, Some(now - 3600.0), false), ConnectionStatus::Offline),
            (record(Some("server_busy"), 3, Some(now - 3600.0), false), ConnectionStatus::Offline),
            (record(Some("connection_reset"), 3, Some(now - 3600.0), false), ConnectionStatus::Offline),
            (record(Some("unreachable"), 3, Some(now - 3600.0), false), ConnectionStatus::Offline),
            // exactly 24 hours is already an error, a second less is not
            (record(Some("timeout"), 5, Some(now - day + 1.0), false), ConnectionStatus::Offline),
            (record(Some("timeout"), 5, Some(now - day), false), ConnectionStatus::Error),
            (record(Some("timeout"), 5, None, false), ConnectionStatus::Error),
            // permanent: error at once, whatever the counter says
            (record(Some("auth_required"), 0, Some(now), false), ConnectionStatus::Error),
            (record(Some("history_missing"), 0, Some(now), false), ConnectionStatus::Error),
            (record(Some("address_not_allowed"), 0, Some(now), false), ConnectionStatus::Error),
            (record(Some("bad_response"), 1, Some(now), false), ConnectionStatus::Error),
            (record(Some("too_many_open_jobs"), 1, Some(now), false), ConnectionStatus::Error),
            (record(Some("name_resolution_failed"), 1, Some(now), false), ConnectionStatus::Error),
            (record(Some("container_loopback"), 1, Some(now), false), ConnectionStatus::Error),
            (record(Some("a_code_from_the_future"), 1, Some(now), false), ConnectionStatus::Error),
        ];
        for (rec, expected) in cases {
            assert_eq!(connection_status(&rec, now), expected, "{:?}", rec);
        }
    }

    #[test]
    fn status_texts_are_stable() {
        let all = [
            (ConnectionStatus::Ok, "ok"),
            (ConnectionStatus::Reconnecting, "reconnecting"),
            (ConnectionStatus::Offline, "offline"),
            (ConnectionStatus::Error, "error"),
            (ConnectionStatus::Paused, "paused"),
        ];
        for (status, text) in all {
            assert_eq!(status.as_str(), text);
        }
    }

    #[test]
    fn backoff_table() {
        let now = 100_000.0;
        // 0 and 1 failures: every pass
        assert!(should_attempt(0, None, now, false));
        assert!(should_attempt(0, Some(now), now, false));
        assert!(should_attempt(1, Some(now - 1.0), now, false));
        // 2 and 3 failures: every 10 minutes
        assert!(!should_attempt(2, Some(now - 599.0), now, false));
        assert!(should_attempt(2, Some(now - 600.0), now, false));
        assert!(!should_attempt(3, Some(now - 599.0), now, false));
        assert!(should_attempt(3, Some(now - 600.0), now, false));
        // 4 and more: every 30 minutes
        assert!(!should_attempt(4, Some(now - 600.0), now, false));
        assert!(!should_attempt(4, Some(now - 1799.0), now, false));
        assert!(should_attempt(4, Some(now - 1800.0), now, false));
        assert!(!should_attempt(40, Some(now - 1799.0), now, false));
        // never tried, or the clock went backwards: try
        assert!(should_attempt(4, None, now, false));
        assert!(should_attempt(4, Some(now + 5000.0), now, false));
        // "Connect now" skips the wait
        assert!(should_attempt(40, Some(now), now, true));
    }

    #[test]
    fn only_waiting_failures_are_postponed() {
        let now = 100_000.0;
        let mut c = record(Some("timeout"), 5, Some(now - 5000.0), false);
        c.last_attempt_at = Some(now - 60.0);
        assert!(!is_due(&c, now, false), "a printer that has been off for a while is left alone");
        assert!(is_due(&c, now, true), "Connect now ignores the wait");
        c.last_attempt_at = Some(now - 1800.0);
        assert!(is_due(&c, now, false));
        let mut open_jobs = record(Some("too_many_open_jobs"), 9, Some(now - 5000.0), false);
        open_jobs.last_attempt_at = Some(now - 60.0);
        assert!(is_due(&open_jobs, now, false), "an error the user must fix is checked at every pass");
        assert!(is_due(&record(None, 0, None, false), now, false));
    }

    #[test]
    fn link_errors_know_whether_they_pass() {
        use crate::printer_link::LinkError;
        for e in [LinkError::Timeout, LinkError::ConnectionRefused, LinkError::ConnectionReset, LinkError::ServerBusy, LinkError::Unreachable] {
            assert!(e.is_transient(), "{e:?}");
        }
        for e in [
            LinkError::AuthRequired,
            LinkError::HistoryMissing,
            LinkError::AddressNotAllowed,
            LinkError::BadResponse("x".into()),
            LinkError::NameResolutionFailed,
            LinkError::ContainerLocalName,
            LinkError::ContainerLoopback,
        ] {
            assert!(!e.is_transient(), "{e:?}");
        }
    }
}
