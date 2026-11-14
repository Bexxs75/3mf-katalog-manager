//! Reports of sliced files that a slicer sent to the program (see `crate::slicer_report`). The program
//! only remembers and suggests: a report never changes a file, and a link to a model is the user's
//! decision. Reports are machine-local facts (paths on this computer), shown until handled.

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::printer_link::{get_setting, set_setting};
use crate::error_code::ErrorCode;

/// Switch of the report intake (`1` = on). Off by default.
pub const SETTING_SLICER_REPORT_ENABLED: &str = "slicer_report_enabled";
/// Program path that was shown in the command lines when the intake was switched on.
pub const SETTING_SLICER_REPORT_EXE: &str = "slicer_report_exe";

/// Largest `summary_json` that is stored.
pub const MAX_SUMMARY_BYTES: usize = 16 * 1024;
/// Handled reports that stay in the list (and the table).
pub const KEEP_DECIDED: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct ReportRecord {
    pub id: i64,
    pub reported_at: String,
    pub reported_path: String,
    pub file_name: String,
    pub file_size_bytes: i64,
    pub slicer: Option<String>,
    pub summary_json: String,
    pub state: String,
    pub suggested_file_id: Option<i64>,
    pub model_file_id: Option<i64>,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewReport {
    pub reported_at: String,
    pub reported_path: String,
    pub file_name: String,
    pub file_size_bytes: i64,
    pub slicer: Option<String>,
    pub summary_json: String,
}

const COLUMNS: &str = "id, reported_at, reported_path, file_name, file_size_bytes, slicer, summary_json, state,
                       suggested_file_id, model_file_id, decided_at";

fn record(row: &rusqlite::Row) -> rusqlite::Result<ReportRecord> {
    Ok(ReportRecord {
        id: row.get(0)?,
        reported_at: row.get(1)?,
        reported_path: row.get(2)?,
        file_name: row.get(3)?,
        file_size_bytes: row.get(4)?,
        slicer: row.get(5)?,
        summary_json: row.get(6)?,
        state: row.get(7)?,
        suggested_file_id: row.get(8)?,
        model_file_id: row.get(9)?,
        decided_at: row.get(10)?,
    })
}

pub fn is_enabled(conn: &Connection) -> Result<bool, DbError> {
    Ok(get_setting(conn, SETTING_SLICER_REPORT_ENABLED)?.as_deref() == Some("1"))
}

/// Switching on remembers the program path the command lines were made for (`exe`).
pub fn set_enabled(conn: &Connection, enabled: bool, exe: Option<&str>) -> Result<(), DbError> {
    set_setting(conn, SETTING_SLICER_REPORT_ENABLED, if enabled { "1" } else { "0" })?;
    if enabled {
        if let Some(exe) = exe {
            set_setting(conn, SETTING_SLICER_REPORT_EXE, exe)?;
        }
    }
    Ok(())
}

pub fn recorded_exe(conn: &Connection) -> Result<Option<String>, DbError> {
    get_setting(conn, SETTING_SLICER_REPORT_EXE)
}

/// Stores a report. A second report of the same path that is still open (the same file sliced again, or
/// a duplicate call) updates that row instead of adding one. Returns `(id, merged)`.
pub fn insert_or_merge(conn: &Connection, new: &NewReport) -> Result<(i64, bool), DbError> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM slicer_reports WHERE reported_path = ?1 AND state = 'open' ORDER BY id DESC LIMIT 1",
            [&new.reported_path],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        conn.execute(
            "UPDATE slicer_reports SET reported_at = ?2, file_size_bytes = ?3, slicer = ?4, summary_json = ?5 WHERE id = ?1",
            params![id, new.reported_at, new.file_size_bytes, new.slicer, new.summary_json],
        )?;
        return Ok((id, true));
    }
    conn.execute(
        "INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, slicer, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new.reported_at, new.reported_path, new.file_name, new.file_size_bytes, new.slicer, new.summary_json],
    )?;
    Ok((conn.last_insert_rowid(), false))
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<ReportRecord>, DbError> {
    Ok(conn
        .query_row(&format!("SELECT {COLUMNS} FROM slicer_reports WHERE id = ?1"), [id], record)
        .optional()?)
}

/// Open reports (newest first) followed by the last handled ones.
pub fn list(conn: &Connection) -> Result<Vec<ReportRecord>, DbError> {
    let mut out = Vec::new();
    let mut open = conn.prepare(&format!("SELECT {COLUMNS} FROM slicer_reports WHERE state = 'open' ORDER BY reported_at DESC, id DESC"))?;
    for row in open.query_map([], record)? {
        out.push(row?);
    }
    let mut decided = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM slicer_reports WHERE state <> 'open' ORDER BY decided_at DESC, id DESC LIMIT ?1"
    ))?;
    for row in decided.query_map([KEEP_DECIDED as i64], record)? {
        out.push(row?);
    }
    Ok(out)
}

pub fn set_suggestion(conn: &Connection, id: i64, file_id: Option<i64>) -> Result<(), DbError> {
    conn.execute("UPDATE slicer_reports SET suggested_file_id = ?2 WHERE id = ?1 AND state = 'open'", params![id, file_id])?;
    Ok(())
}

/// Active model of the catalog a report may be linked to: not in the trash, not G-code or resin.
pub fn is_linkable_model(conn: &Connection, file_id: i64) -> Result<bool, DbError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM files WHERE id = ?1 AND deleted_at IS NULL AND file_type NOT IN ('gcode', 'resin')",
            [file_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// The user's decision: link to a model. Only an open report can be decided.
pub fn link(conn: &Connection, id: i64, model_file_id: i64, now: &str) -> Result<(), DbError> {
    if !is_linkable_model(conn, model_file_id)? {
        return Err(DbError::coded(ErrorCode::SlicerReportModelInvalid));
    }
    let changed = conn.execute(
        "UPDATE slicer_reports SET state = 'linked', model_file_id = ?2, decided_at = ?3 WHERE id = ?1 AND state = 'open'",
        params![id, model_file_id, now],
    )?;
    if changed == 0 {
        return Err(DbError::coded(ErrorCode::SlicerReportNotFound));
    }
    prune_decided(conn)
}

pub fn dismiss(conn: &Connection, id: i64, now: &str) -> Result<(), DbError> {
    let changed = conn.execute(
        "UPDATE slicer_reports SET state = 'dismissed', decided_at = ?2 WHERE id = ?1 AND state = 'open'",
        params![id, now],
    )?;
    if changed == 0 {
        return Err(DbError::coded(ErrorCode::SlicerReportNotFound));
    }
    prune_decided(conn)
}

/// Keeps the newest [`KEEP_DECIDED`] handled reports; linked ones stay as long as the printer link may
/// look them up, so the cut only removes the oldest.
fn prune_decided(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM slicer_reports WHERE state <> 'open' AND id NOT IN (
             SELECT id FROM slicer_reports WHERE state <> 'open' ORDER BY decided_at DESC, id DESC LIMIT ?1)",
        [KEEP_DECIDED as i64],
    )?;
    Ok(())
}

/// The model a confirmed report links the sliced file `file_name` to (case ignored, newest decision
/// first). Only a model that is still active counts.
pub fn confirmed_model_for_name(conn: &Connection, file_name: &str) -> Result<Option<(i64, String)>, DbError> {
    let name = last_component(file_name).to_lowercase();
    if name.is_empty() {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT f.id, f.name FROM slicer_reports r JOIN files f ON f.id = r.model_file_id
              WHERE r.state = 'linked' AND lower(r.file_name) = ?1
                AND f.deleted_at IS NULL AND f.file_type NOT IN ('gcode', 'resin')
              ORDER BY r.decided_at DESC, r.id DESC LIMIT 1",
            [&name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

/// File name part of a path in either separator style.
pub fn last_component(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        conn.execute_batch(super::super::repository::SCHEMA_SQL).unwrap();
        conn
    }

    fn file(conn: &Connection, name: &str, file_type: &str) -> i64 {
        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES (?1, ?2, ?3, 1, '2026-11-01T00:00:00Z')",
            params![name, format!("/c/{name}"), file_type],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn new(path: &str) -> NewReport {
        NewReport {
            reported_at: "2026-11-05T10:00:00Z".into(),
            reported_path: path.into(),
            file_name: last_component(path).into(),
            file_size_bytes: 10,
            slicer: Some("PrusaSlicer 2.8.1".into()),
            summary_json: "{}".into(),
        }
    }

    #[test]
    fn the_same_open_path_is_merged_and_a_decided_one_is_not() {
        let conn = conn();
        let (a, merged) = insert_or_merge(&conn, &new("/s/Box.gcode")).unwrap();
        assert!(!merged);
        let (b, merged) = insert_or_merge(&conn, &new("/s/Box.gcode")).unwrap();
        assert!(merged && a == b);
        dismiss(&conn, a, "2026-11-05T11:00:00Z").unwrap();
        let (c, merged) = insert_or_merge(&conn, &new("/s/Box.gcode")).unwrap();
        assert!(!merged && c != a, "a handled report stays; the new slicing is a new report");
    }

    #[test]
    fn linking_needs_an_active_model_and_an_open_report() {
        let conn = conn();
        let model = file(&conn, "Box.3mf", "3mf");
        let gcode = file(&conn, "Box.gcode", "gcode");
        let (id, _) = insert_or_merge(&conn, &new("/s/Box.gcode")).unwrap();
        let err = link(&conn, id, gcode, "t").unwrap_err();
        assert!(matches!(err, DbError::Coded { code: ErrorCode::SlicerReportModelInvalid, .. }));
        assert!(matches!(link(&conn, id, 9999, "t").unwrap_err(), DbError::Coded { code: ErrorCode::SlicerReportModelInvalid, .. }));
        link(&conn, id, model, "t").unwrap();
        assert!(matches!(dismiss(&conn, id, "t").unwrap_err(), DbError::Coded { code: ErrorCode::SlicerReportNotFound, .. }));
        conn.execute("UPDATE files SET deleted_at = 'x' WHERE id = ?1", [model]).unwrap();
        assert!(confirmed_model_for_name(&conn, "BOX.gcode").unwrap().is_none(), "a model in the trash is not offered");
    }

    #[test]
    fn a_confirmed_report_answers_by_file_name_ignoring_case_and_directory() {
        let conn = conn();
        let model = file(&conn, "Box.3mf", "3mf");
        let (id, _) = insert_or_merge(&conn, &new("C:\\out\\Box_PETG.gcode")).unwrap();
        assert!(confirmed_model_for_name(&conn, "box_petg.GCODE").unwrap().is_none(), "open reports do not count");
        link(&conn, id, model, "t").unwrap();
        assert_eq!(confirmed_model_for_name(&conn, "gcodes/BOX_PETG.gcode").unwrap(), Some((model, "Box.3mf".to_string())));
        assert!(confirmed_model_for_name(&conn, "other.gcode").unwrap().is_none());
    }

    #[test]
    fn only_the_newest_handled_reports_are_kept() {
        let conn = conn();
        for i in 0..(KEEP_DECIDED + 5) {
            let (id, _) = insert_or_merge(&conn, &new(&format!("/s/{i}.gcode"))).unwrap();
            dismiss(&conn, id, &format!("2026-11-05T10:{i:02}:00Z")).unwrap();
        }
        let kept: i64 = conn.query_row("SELECT COUNT(*) FROM slicer_reports", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, KEEP_DECIDED as i64);
    }

    #[test]
    fn deleting_a_model_leaves_the_report() {
        let conn = conn();
        let model = file(&conn, "Box.3mf", "3mf");
        let (id, _) = insert_or_merge(&conn, &new("/s/Box.gcode")).unwrap();
        link(&conn, id, model, "t").unwrap();
        conn.execute("DELETE FROM files WHERE id = ?1", [model]).unwrap();
        let r = get(&conn, id).unwrap().unwrap();
        assert_eq!((r.state.as_str(), r.model_file_id), ("linked", None));
    }
}
