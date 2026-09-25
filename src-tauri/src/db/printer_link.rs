//! Datenbankzugriff der Druckeranbindung: Schalter, Verbindungen, abgeholte Drucke.

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::error::DbError;
use crate::printer_link::RemoteJob;

pub const SETTING_PRINTER_LINK_ENABLED: &str = "printer_link_enabled";

#[derive(Debug, Clone, PartialEq)]
pub struct PrinterConnectionRecord {
    pub printer_id: i64,
    pub kind: String,
    pub address: String,
    pub base_url: Option<String>,
    pub remote_version: Option<String>,
    pub connected_since: f64,
    pub last_synced_at: Option<f64>,
    pub last_error: Option<String>,
    pub error_since: Option<f64>,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrinterJobRecord {
    pub id: i64,
    pub printer_id: i64,
    pub remote_id: String,
    pub file_name: String,
    pub outcome: String,
    pub raw_status: String,
    pub ended_at: f64,
    pub print_duration_s: f64,
    pub used_mm: f64,
    pub slicer_total_mm: Option<f64>,
    pub slicer_weight_g: Option<f64>,
    pub material: Option<String>,
    pub thumbnail_path: Option<String>,
    pub state: String,
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, DbError> {
    Ok(conn
        .query_row("SELECT value FROM app_settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn printer_link_enabled(conn: &Connection) -> Result<bool, DbError> {
    Ok(get_setting(conn, SETTING_PRINTER_LINK_ENABLED)?.as_deref() == Some("1"))
}

pub fn set_printer_link_enabled(conn: &Connection, enabled: bool) -> Result<(), DbError> {
    set_setting(conn, SETTING_PRINTER_LINK_ENABLED, if enabled { "1" } else { "0" })
}

const CONNECTION_COLUMNS: &str = "printer_id, kind, address, base_url, remote_version, connected_since,
    last_synced_at, last_error, error_since, paused";

fn connection_from_row(r: &Row) -> rusqlite::Result<PrinterConnectionRecord> {
    Ok(PrinterConnectionRecord {
        printer_id: r.get(0)?,
        kind: r.get(1)?,
        address: r.get(2)?,
        base_url: r.get(3)?,
        remote_version: r.get(4)?,
        connected_since: r.get(5)?,
        last_synced_at: r.get(6)?,
        last_error: r.get(7)?,
        error_since: r.get(8)?,
        paused: r.get::<_, i64>(9)? != 0,
    })
}

/// Speichert eine erfolgreich getestete Verbindung. `connected_since` wird
/// nur beim ersten Mal gesetzt ("ab jetzt"), Fehler und Pause werden gelöscht.
pub fn save_connection_after_test(
    conn: &Connection,
    printer_id: i64,
    kind: &str,
    address: &str,
    base_url: &str,
    version: &str,
    now: f64,
) -> Result<PrinterConnectionRecord, DbError> {
    conn.execute(
        "INSERT INTO printer_connections (printer_id, kind, address, base_url, remote_version, connected_since)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(printer_id) DO UPDATE SET
             kind = excluded.kind, address = excluded.address, base_url = excluded.base_url,
             remote_version = excluded.remote_version, last_error = NULL, error_since = NULL, paused = 0",
        params![printer_id, kind, address, base_url, version, now],
    )?;
    get_connection(conn, printer_id)?.ok_or_else(|| DbError::Other("Verbindung fehlt nach dem Speichern".into()))
}

pub fn get_connection(conn: &Connection, printer_id: i64) -> Result<Option<PrinterConnectionRecord>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {CONNECTION_COLUMNS} FROM printer_connections WHERE printer_id = ?1"),
            params![printer_id],
            connection_from_row,
        )
        .optional()?)
}

pub fn list_connections(conn: &Connection) -> Result<Vec<PrinterConnectionRecord>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CONNECTION_COLUMNS} FROM printer_connections ORDER BY printer_id"
    ))?;
    let rows = stmt.query_map([], connection_from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Entfernt die Verbindung und alle noch offenen Drucke dieses Druckers.
/// Bestätigte und ignorierte Drucke bleiben als Verlauf erhalten.
pub fn delete_connection(conn: &Connection, printer_id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM printer_jobs WHERE printer_id = ?1 AND state = 'open'", params![printer_id])?;
    conn.execute("DELETE FROM printer_connections WHERE printer_id = ?1", params![printer_id])?;
    Ok(())
}

pub fn record_sync_success(conn: &Connection, printer_id: i64, now: f64, base_url: &str, version: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE printer_connections SET last_synced_at = ?2, last_error = NULL, error_since = NULL,
             base_url = ?3, remote_version = ?4
         WHERE printer_id = ?1",
        params![printer_id, now, base_url, version],
    )?;
    Ok(())
}

/// Merkt sich den Fehler; `error_since` bleibt beim ersten Fehler stehen.
/// `auth_required` pausiert die Verbindung bis zum nächsten Test.
pub fn record_sync_error(conn: &Connection, printer_id: i64, code: &str, now: f64) -> Result<(), DbError> {
    conn.execute(
        "UPDATE printer_connections SET last_error = ?2, error_since = COALESCE(error_since, ?3),
             paused = CASE WHEN ?2 = 'auth_required' THEN 1 ELSE paused END
         WHERE printer_id = ?1",
        params![printer_id, code, now],
    )?;
    Ok(())
}

/// Ab diesem Zeitpunkt (Unix-Sekunden) gelten beendete Drucke als neu:
/// der spätere Wert aus "verbunden seit" und dem Ende des jüngsten
/// abgeholten Drucks.
pub fn sync_from(conn: &Connection, printer_id: i64) -> Result<f64, DbError> {
    let since: f64 = conn.query_row(
        "SELECT MAX(c.connected_since, COALESCE((SELECT MAX(j.ended_at) FROM printer_jobs j WHERE j.printer_id = c.printer_id), 0))
         FROM printer_connections c WHERE c.printer_id = ?1",
        params![printer_id],
        |r| r.get(0),
    )?;
    Ok(since)
}

/// Fügt einen Druck ein, wenn es ihn für diesen Drucker noch nicht gibt.
/// Liefert `true`, wenn er neu war.
pub fn insert_job_if_new(conn: &Connection, printer_id: i64, job: &RemoteJob) -> Result<bool, DbError> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at,
             print_duration_s, used_mm, slicer_total_mm, slicer_weight_g, material, thumbnail_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            printer_id,
            job.remote_id,
            job.file_name,
            job.outcome.as_str(),
            job.raw_status,
            job.ended_at,
            job.print_duration_s,
            job.used_mm,
            job.slicer_total_mm,
            job.slicer_weight_g,
            job.material,
            job.thumbnail_path,
        ],
    )?;
    Ok(changed == 1)
}

const JOB_COLUMNS: &str = "id, printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s,
    used_mm, slicer_total_mm, slicer_weight_g, material, thumbnail_path, state";

fn job_from_row(r: &Row) -> rusqlite::Result<PrinterJobRecord> {
    Ok(PrinterJobRecord {
        id: r.get(0)?,
        printer_id: r.get(1)?,
        remote_id: r.get(2)?,
        file_name: r.get(3)?,
        outcome: r.get(4)?,
        raw_status: r.get(5)?,
        ended_at: r.get(6)?,
        print_duration_s: r.get(7)?,
        used_mm: r.get(8)?,
        slicer_total_mm: r.get(9)?,
        slicer_weight_g: r.get(10)?,
        material: r.get(11)?,
        thumbnail_path: r.get(12)?,
        state: r.get(13)?,
    })
}

pub fn list_open_jobs(conn: &Connection) -> Result<Vec<PrinterJobRecord>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {JOB_COLUMNS} FROM printer_jobs WHERE state = 'open' ORDER BY printer_id, ended_at"
    ))?;
    let rows = stmt.query_map([], job_from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_job(conn: &Connection, job_id: i64) -> Result<Option<PrinterJobRecord>, DbError> {
    Ok(conn
        .query_row(&format!("SELECT {JOB_COLUMNS} FROM printer_jobs WHERE id = ?1"), params![job_id], job_from_row)
        .optional()?)
}

/// Nur offene Drucke. Liefert die Anzahl geänderter Zeilen (0 = nicht mehr offen).
pub fn mark_job_confirmed(
    conn: &Connection,
    job_id: i64,
    spool_id: i64,
    file_id: Option<i64>,
    grams: f64,
    decided_at: &str,
) -> Result<usize, DbError> {
    Ok(conn.execute(
        "UPDATE printer_jobs SET state = 'confirmed', booked_spool_id = ?2, booked_file_id = ?3, booked_g = ?4, decided_at = ?5
         WHERE id = ?1 AND state = 'open'",
        params![job_id, spool_id, file_id, grams, decided_at],
    )?)
}

pub fn mark_job_ignored(conn: &Connection, job_id: i64, decided_at: &str) -> Result<usize, DbError> {
    Ok(conn.execute(
        "UPDATE printer_jobs SET state = 'ignored', decided_at = ?2 WHERE id = ?1 AND state = 'open'",
        params![job_id, decided_at],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer_link::{JobOutcome, RemoteJob};

    fn setup() -> Connection {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0)", []).unwrap();
        conn
    }

    fn job(remote_id: &str, ended_at: f64) -> RemoteJob {
        RemoteJob {
            remote_id: remote_id.into(),
            file_name: "Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode".into(),
            outcome: JobOutcome::Completed,
            raw_status: "completed".into(),
            ended_at,
            print_duration_s: 394.65,
            used_mm: 303.2658,
            slicer_total_mm: Some(279.17),
            slicer_weight_g: Some(0.83),
            material: Some("PLA".into()),
            thumbnail_path: Some(".thumbs/a-300x300.png".into()),
        }
    }

    #[test]
    fn printer_link_is_off_by_default_and_can_be_switched() {
        let conn = setup();
        assert!(!printer_link_enabled(&conn).unwrap());
        set_printer_link_enabled(&conn, true).unwrap();
        assert!(printer_link_enabled(&conn).unwrap());
        set_printer_link_enabled(&conn, false).unwrap();
        assert!(!printer_link_enabled(&conn).unwrap());
    }

    #[test]
    fn connected_since_is_kept_when_testing_again() {
        let conn = setup();
        save_connection_after_test(&conn, 1, "moonraker", "192.168.1.60", "http://192.168.1.60", "v0.8.0", 1000.0).unwrap();
        record_sync_error(&conn, 1, "auth_required", 1100.0).unwrap();
        let c = save_connection_after_test(&conn, 1, "moonraker", "192.168.1.61", "http://192.168.1.61", "v0.8.1", 2000.0).unwrap();
        assert_eq!(c.connected_since, 1000.0);
        assert_eq!(c.address, "192.168.1.61");
        assert!(!c.paused);
        assert_eq!(c.last_error, None);
    }

    #[test]
    fn auth_errors_pause_the_connection_and_keep_error_since() {
        let conn = setup();
        save_connection_after_test(&conn, 1, "moonraker", "192.168.1.60", "http://192.168.1.60", "v0.8.0", 1000.0).unwrap();
        record_sync_error(&conn, 1, "unreachable", 1100.0).unwrap();
        record_sync_error(&conn, 1, "auth_required", 1400.0).unwrap();
        let c = get_connection(&conn, 1).unwrap().unwrap();
        assert_eq!(c.error_since, Some(1100.0));
        assert_eq!(c.last_error.as_deref(), Some("auth_required"));
        assert!(c.paused);
        record_sync_success(&conn, 1, 1500.0, "http://192.168.1.60", "v0.8.0").unwrap();
        let c = get_connection(&conn, 1).unwrap().unwrap();
        assert_eq!(c.error_since, None);
        assert_eq!(c.last_synced_at, Some(1500.0));
    }

    #[test]
    fn same_remote_job_is_stored_once() {
        let conn = setup();
        assert!(insert_job_if_new(&conn, 1, &job("00003F", 5000.0)).unwrap());
        assert!(!insert_job_if_new(&conn, 1, &job("00003F", 5000.0)).unwrap());
        assert!(insert_job_if_new(&conn, 1, &job("00003E", 5100.0)).unwrap());
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 2);
    }

    #[test]
    fn sync_from_is_connected_since_or_latest_job_end() {
        let conn = setup();
        save_connection_after_test(&conn, 1, "moonraker", "192.168.1.60", "http://192.168.1.60", "v0.8.0", 1000.0).unwrap();
        assert_eq!(sync_from(&conn, 1).unwrap(), 1000.0);
        insert_job_if_new(&conn, 1, &job("A", 4000.0)).unwrap();
        assert_eq!(sync_from(&conn, 1).unwrap(), 4000.0);
    }

    #[test]
    fn removing_a_connection_deletes_only_open_jobs() {
        let conn = setup();
        save_connection_after_test(&conn, 1, "moonraker", "192.168.1.60", "http://192.168.1.60", "v0.8.0", 1000.0).unwrap();
        insert_job_if_new(&conn, 1, &job("A", 4000.0)).unwrap();
        insert_job_if_new(&conn, 1, &job("B", 4100.0)).unwrap();
        let b = list_open_jobs(&conn).unwrap().into_iter().find(|j| j.remote_id == "B").unwrap();
        assert_eq!(mark_job_ignored(&conn, b.id, "2026-09-24T00:00:00Z").unwrap(), 1);
        delete_connection(&conn, 1).unwrap();
        assert!(get_connection(&conn, 1).unwrap().is_none());
        let remaining: i64 = conn.query_row("SELECT COUNT(*) FROM printer_jobs", [], |r| r.get(0)).unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn deciding_a_job_only_works_once() {
        let conn = setup();
        insert_job_if_new(&conn, 1, &job("A", 4000.0)).unwrap();
        let a = list_open_jobs(&conn).unwrap().remove(0);
        assert_eq!(mark_job_ignored(&conn, a.id, "t").unwrap(), 1);
        assert_eq!(mark_job_ignored(&conn, a.id, "t").unwrap(), 0);
        assert_eq!(mark_job_confirmed(&conn, a.id, 1, None, 0.9, "t").unwrap(), 0);
    }
}
