//! Settings of the Thingiverse area, stored as rows of `app_settings` (no schema
//! change). The key is only readable through `get_token`, which hands out the
//! `Token` type that cannot be printed.

use rusqlite::{params, Connection};

use super::error::DbError;
use super::printer_link::{get_setting, set_setting};
use crate::thingiverse::Token;

pub const SETTING_ENABLED: &str = "thingiverse_enabled";
pub const SETTING_TOKEN: &str = "thingiverse_token";
pub const SETTING_USAGE: &str = "thingiverse_usage";

/// Rows that must never end up in a copy of the database (backup, web export,
/// update backup, restore).
const SECRET_KEYS: &[&str] = &[SETTING_TOKEN];

pub fn get_enabled(conn: &Connection) -> Result<bool, DbError> {
    Ok(get_setting(conn, SETTING_ENABLED)?.as_deref() == Some("1"))
}

pub fn set_enabled(conn: &Connection, enabled: bool) -> Result<(), DbError> {
    set_setting(conn, SETTING_ENABLED, if enabled { "1" } else { "0" })
}

/// A stored value that is no longer a valid key counts as no key.
pub fn get_token(conn: &Connection) -> Result<Option<Token>, DbError> {
    Ok(get_setting(conn, SETTING_TOKEN)?.and_then(|raw| Token::parse(&raw).ok()))
}

/// Runs `f` with `secure_delete` on, so overwritten or deleted key bytes do not stay
/// readable in free pages of the working database; restores the previous setting.
pub(super) fn with_secure_delete<T>(conn: &Connection, f: impl FnOnce() -> Result<T, DbError>) -> Result<T, DbError> {
    let before: i64 = conn.query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA secure_delete = ON")?;
    let result = f();
    let _ = conn.execute_batch(&format!("PRAGMA secure_delete = {before}"));
    result
}

pub fn set_token(conn: &Connection, token: &Token) -> Result<(), DbError> {
    with_secure_delete(conn, || set_setting(conn, SETTING_TOKEN, token.expose()))
}

pub fn clear_token(conn: &Connection) -> Result<(), DbError> {
    with_secure_delete(conn, || {
        conn.execute("DELETE FROM app_settings WHERE key = ?1", params![SETTING_TOKEN])?;
        Ok(())
    })
}

/// Removes every secret row from a COPY of the database, and the freed pages with it.
/// Never call it on the working database: `VACUUM` rewrites the whole file.
///
/// Must run outside a transaction.
pub fn scrub_secrets(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch("PRAGMA secure_delete = ON")?;
    // A copy made from a database without the table (a foreign or minimal one) has no key.
    let has_settings: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'app_settings')",
        [],
        |r| r.get(0),
    )?;
    if has_settings {
        for key in SECRET_KEYS {
            conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
        }
    }
    super::printer_link::scrub_api_keys(conn)?;
    conn.execute_batch("VACUUM")?;
    Ok(())
}

/// The machine-local part of the area's settings that a restore keeps: the key and the
/// switch of the working database. A backup never holds the key (see `scrub_secrets`),
/// so without this a restore would silently drop it.
pub struct LocalSettings {
    enabled: Option<String>,
    token: Option<Token>,
}

/// Reads what [`keep_local`] puts back. The key is read only when it is a valid one.
pub fn local_settings(conn: &Connection) -> Result<LocalSettings, DbError> {
    Ok(LocalSettings { enabled: get_setting(conn, SETTING_ENABLED)?, token: get_token(conn)? })
}

/// Puts the local key (and the local switch, when there was one) into a database that
/// is about to become the working one. Without a local key the incoming switch stays.
pub fn keep_local(conn: &Connection, local: &LocalSettings) -> Result<(), DbError> {
    if let Some(token) = &local.token {
        set_token(conn, token)?;
        if let Some(enabled) = &local.enabled {
            set_setting(conn, SETTING_ENABLED, enabled)?;
        }
    }
    Ok(())
}

/// What the day's counter says: only calls of this program, estimated, and local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    /// The request was counted; this is the count including it.
    Counted(u32),
    /// The day's cap was reached; nothing was counted.
    AtLimit,
}

fn parse_usage(raw: Option<String>, today: &str) -> u32 {
    raw.and_then(|v| v.split_once(':').map(|(d, n)| (d.to_string(), n.to_string())))
        .filter(|(d, _)| d == today)
        .and_then(|(_, n)| n.parse().ok())
        .unwrap_or(0)
}

pub fn usage_today(conn: &Connection, today: &str) -> Result<u32, DbError> {
    Ok(parse_usage(get_setting(conn, SETTING_USAGE)?, today))
}

/// Counts one request for `today` ("YYYY-MM-DD", local date) unless `cap` is reached.
/// A new day starts at 0.
pub fn bump_usage(conn: &Connection, today: &str, cap: u32) -> Result<Usage, DbError> {
    let used = usage_today(conn, today)?;
    if used >= cap {
        return Ok(Usage::AtLimit);
    }
    set_setting(conn, SETTING_USAGE, &format!("{today}:{}", used + 1))?;
    Ok(Usage::Counted(used + 1))
}

pub fn local_today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "abcd1234efgh5678a3f9";

    fn conn() -> Connection {
        super::super::connect_in_memory().unwrap()
    }

    #[test]
    fn enabled_defaults_to_off() {
        let c = conn();
        assert!(!get_enabled(&c).unwrap());
        set_enabled(&c, true).unwrap();
        assert!(get_enabled(&c).unwrap());
        set_enabled(&c, false).unwrap();
        assert!(!get_enabled(&c).unwrap());
    }

    #[test]
    fn the_key_round_trips_and_clears() {
        let c = conn();
        assert!(get_token(&c).unwrap().is_none());
        set_token(&c, &Token::parse(KEY).unwrap()).unwrap();
        assert_eq!(get_token(&c).unwrap().unwrap().expose(), KEY);
        clear_token(&c).unwrap();
        assert!(get_token(&c).unwrap().is_none());
    }

    #[test]
    fn a_stored_value_that_is_not_a_key_counts_as_none() {
        let c = conn();
        set_setting(&c, SETTING_TOKEN, "bad value with spaces").unwrap();
        assert!(get_token(&c).unwrap().is_none());
    }

    #[test]
    fn secure_delete_is_restored_after_set_and_clear() {
        let c = conn();
        let before: i64 = c.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
        set_token(&c, &Token::parse(KEY).unwrap()).unwrap();
        clear_token(&c).unwrap();
        let after: i64 = c.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn the_counter_counts_per_day_and_stops_at_the_cap() {
        let c = conn();
        assert_eq!(usage_today(&c, "2026-11-04").unwrap(), 0);
        assert_eq!(bump_usage(&c, "2026-11-04", 3).unwrap(), Usage::Counted(1));
        assert_eq!(bump_usage(&c, "2026-11-04", 3).unwrap(), Usage::Counted(2));
        assert_eq!(bump_usage(&c, "2026-11-04", 3).unwrap(), Usage::Counted(3));
        assert_eq!(bump_usage(&c, "2026-11-04", 3).unwrap(), Usage::AtLimit);
        assert_eq!(usage_today(&c, "2026-11-04").unwrap(), 3, "a refused request is not counted");
        // The next day starts again.
        assert_eq!(usage_today(&c, "2026-11-05").unwrap(), 0);
        assert_eq!(bump_usage(&c, "2026-11-05", 3).unwrap(), Usage::Counted(1));
    }

    #[test]
    fn a_damaged_counter_value_starts_at_zero() {
        let c = conn();
        for bad in ["", "garbage", "2026-11-04:x", "2026-11-04"] {
            set_setting(&c, SETTING_USAGE, bad).unwrap();
            assert_eq!(usage_today(&c, "2026-11-04").unwrap(), 0, "{bad}");
        }
    }

    #[test]
    fn scrub_copes_with_a_database_that_has_no_settings_table() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE a(n INTEGER)").unwrap();
        scrub_secrets(&c).unwrap();
    }

    #[test]
    fn scrub_removes_the_key_row_and_keeps_the_rest() {
        let c = conn();
        set_enabled(&c, true).unwrap();
        set_token(&c, &Token::parse(KEY).unwrap()).unwrap();
        set_setting(&c, SETTING_USAGE, "2026-11-04:5").unwrap();
        scrub_secrets(&c).unwrap();
        assert!(get_token(&c).unwrap().is_none());
        assert!(get_enabled(&c).unwrap());
        assert_eq!(usage_today(&c, "2026-11-04").unwrap(), 5);
    }
}
