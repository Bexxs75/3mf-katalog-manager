//! Start-sync switches shared by the desktop app and the web server. Both read and write
//! the same catalog keys, so an exported and re-imported catalog keeps its choice.
use crate::db;
use rusqlite::Connection;

pub const SETTING_ON_START: &str = "sync_on_start";
pub const SETTING_AUTO_ADD: &str = "sync_auto_add";

/// A missing key counts as off.
pub fn flag(conn: &Connection, key: &str) -> Result<bool, String> {
    Ok(db::printer_link::get_setting(conn, key)
        .map_err(|e| format!("database: {e}"))?
        .as_deref()
        == Some("1"))
}

pub fn set_flag(conn: &Connection, key: &str, enabled: bool) -> Result<(), String> {
    db::printer_link::set_setting(conn, key, if enabled { "1" } else { "0" }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_switches_default_to_off_and_are_independent() {
        let conn = db::connect_in_memory().unwrap();
        assert!(!flag(&conn, SETTING_ON_START).unwrap());
        assert!(!flag(&conn, SETTING_AUTO_ADD).unwrap());
        set_flag(&conn, SETTING_ON_START, true).unwrap();
        assert!(flag(&conn, SETTING_ON_START).unwrap());
        assert!(!flag(&conn, SETTING_AUTO_ADD).unwrap());
        set_flag(&conn, SETTING_ON_START, false).unwrap();
        assert!(!flag(&conn, SETTING_ON_START).unwrap());
    }

    #[test]
    fn the_stored_keys_and_values_stay_stable_for_exported_catalogs() {
        let conn = db::connect_in_memory().unwrap();
        set_flag(&conn, SETTING_AUTO_ADD, true).unwrap();
        assert_eq!(db::printer_link::get_setting(&conn, "sync_auto_add").unwrap().as_deref(), Some("1"));
        set_flag(&conn, SETTING_AUTO_ADD, false).unwrap();
        assert_eq!(db::printer_link::get_setting(&conn, "sync_auto_add").unwrap().as_deref(), Some("0"));
    }
}
