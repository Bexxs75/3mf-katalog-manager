//! Logging helpers and the pure logic behind "Report a bug".
pub mod anonymize;
pub mod export;
pub mod form_url;
pub mod verbose;

use rusqlite::Connection;

/// Start of the first line written on every app start; the export always keeps
/// the most recent one so version and system are known.
pub const HEADER_MARK: &str = "[start] 3MF Katalog Manager";

pub fn startup_line(version: &str, step: bool, os: &str, schema: i64, models: i64) -> String {
    let variant = if step { "mit STEP" } else { "ohne STEP" };
    format!("3MF Katalog Manager {version} ({variant}) · {os} · Schema {schema} · {models} Modelle")
}

pub fn log_startup(conn: &Connection) {
    let schema: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap_or(-1);
    let models: i64 = conn
        .query_row("SELECT COUNT(*) FROM files WHERE deleted_at IS NULL", [], |r| r.get(0))
        .unwrap_or(-1);
    let os = os_info::get().to_string();
    log::info!(
        target: "start",
        "{}",
        startup_line(env!("CARGO_PKG_VERSION"), cfg!(feature = "step-preview"), &os, schema, models)
    );
    if let Ok(folders) = crate::db::list_folders(conn) {
        let roots: Vec<&str> = folders.iter().filter(|f| f.parent_id.is_none()).map(|f| f.path.as_str()).collect();
        log::info!(target: "start", "Katalog: {}", roots.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_line_names_version_variant_os_schema_and_models() {
        assert_eq!(
            startup_line("0.15.0", false, "Linux 7.2", 40, 1284),
            "3MF Katalog Manager 0.15.0 (ohne STEP) · Linux 7.2 · Schema 40 · 1284 Modelle"
        );
    }

    #[test]
    fn a_logged_startup_line_contains_the_header_mark() {
        let line = format!("2026-10-03 14:02:11 INFO  [start] {}", startup_line("0.15.0", true, "x", 1, 2));
        assert!(line.contains(HEADER_MARK));
    }
}
