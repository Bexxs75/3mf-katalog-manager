//! Abbuchen: Gramm berechnen, Spule/Material pruefen, Bestaetigen/Ignorieren.

use std::f64::consts::PI;

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::error::DbError;
use crate::db::models::NewPrintLogEntry;
use crate::db::printer_link::{get_job, mark_job_confirmed, mark_job_ignored};
use crate::db::printers::round_tenth;

/// Erstes Wort in Kleinbuchstaben: "PLA Matt" -> "pla", "PETG-CF" -> "petg".
pub fn normalize_material(m: &str) -> String {
    m.trim()
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '-' || c == ';' || c == '+')
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn density_g_cm3(material: &str) -> f64 {
    match normalize_material(material).as_str() {
        "pla" => 1.24,
        "petg" => 1.27,
        "abs" => 1.04,
        "asa" => 1.07,
        "tpu" => 1.21,
        "pa" => 1.14,
        "pc" => 1.20,
        _ => 1.24,
    }
}

/// Verbrauch in Gramm, auf 0,1 g gerundet. Bevorzugt die Slicer-Angaben
/// (Gewicht x gefoerdert / geschaetzte Laenge), sonst Laenge x Querschnitt x
/// Dichte.
pub fn grams(
    used_mm: f64,
    slicer_total_mm: Option<f64>,
    slicer_weight_g: Option<f64>,
    spool_diameter_mm: f64,
    spool_material: &str,
) -> f64 {
    let raw = match (slicer_weight_g, slicer_total_mm) {
        (Some(w), Some(t)) if w > 0.0 && t > 0.0 => w * used_mm / t,
        _ => {
            let r = spool_diameter_mm / 2.0;
            used_mm * PI * r * r * density_g_cm3(spool_material) / 1000.0
        }
    };
    round_tenth(raw.max(0.0))
}

pub fn partial_percent(used_mm: f64, slicer_total_mm: Option<f64>) -> Option<u8> {
    slicer_total_mm
        .filter(|t| *t > 0.0)
        .map(|t| (used_mm / t * 100.0).round().clamp(0.0, 100.0) as u8)
}

pub fn materials_match(job_material: Option<&str>, spool_material: &str) -> bool {
    match job_material {
        None => true,
        Some(m) => normalize_material(m) == normalize_material(spool_material),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub job_id: i64,
    pub spool_id: i64,
    pub file_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpoolInfo {
    pub diameter_mm: f64,
    pub material: String,
    pub remaining_weight_g: f64,
}

/// Spule im ersten belegten Fach der ersten Einheit dieses Druckers.
pub fn suggest_spool(conn: &Connection, printer_id: i64) -> Result<Option<i64>, DbError> {
    Ok(conn
        .query_row(
            "SELECT s.id FROM filament_spools s JOIN material_units u ON s.unit_id = u.id
             WHERE u.printer_id = ?1 AND s.kind = 'filament'
             ORDER BY u.position, u.id, s.slot_index LIMIT 1",
            params![printer_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Daten einer Spule fuer die Abbuchung. Resin-Flaschen (v0.13.1) sind fuer
/// die Druckeranbindung keine Spulen: dafuer gibt es `None`, wie fuer eine
/// unbekannte ID - `ensure_filament` liefert vorher die passende Meldung.
pub fn spool_info(conn: &Connection, spool_id: i64) -> Result<Option<SpoolInfo>, DbError> {
    Ok(conn
        .query_row(
            "SELECT diameter_mm, material, remaining_weight_g FROM filament_spools WHERE id = ?1 AND kind = 'filament'",
            params![spool_id],
            |r| Ok(SpoolInfo { diameter_mm: r.get(0)?, material: r.get(1)?, remaining_weight_g: r.get(2)? }),
        )
        .optional()?)
}

/// Lehnt eine Resin-Flasche als Abbuch-Ziel ab (Druckeranbindung kennt nur
/// Filament). Eine unbekannte ID laesst sie durch - das meldet `spool_info`.
pub fn ensure_filament(conn: &Connection, spool_id: i64) -> Result<(), DbError> {
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM filament_spools WHERE id = ?1", params![spool_id], |r| r.get(0))
        .optional()?;
    if kind.as_deref() == Some("resin") {
        return Err(DbError::Other("Resin kann nicht über die Druckeranbindung abgebucht werden".into()));
    }
    Ok(())
}

pub fn log_note(printer_name: &str, grams: f64, outcome: &str, percent: Option<u8>) -> String {
    let base = format!("{printer_name} · {grams:.1} g");
    match (outcome, percent) {
        ("completed", _) => base,
        (_, Some(p)) => format!("{base} · {p} %"),
        (_, None) => format!("{base} · ✕"),
    }
}

fn rfc3339_from_unix(secs: f64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

/// Bucht einen offenen Druck in EINER Transaktion ab. Liefert die Gramm.
pub fn confirm_job(conn: &mut Connection, d: &Decision, decided_at: &str) -> Result<f64, DbError> {
    let tx = conn.transaction()?;
    let job = get_job(&tx, d.job_id)?
        .filter(|j| j.state == "open")
        .ok_or_else(|| DbError::Other("Druck ist nicht mehr offen".into()))?;
    ensure_filament(&tx, d.spool_id)?;
    let spool = spool_info(&tx, d.spool_id)?.ok_or_else(|| DbError::Other("Spule nicht gefunden".into()))?;
    let g = grams(job.used_mm, job.slicer_total_mm, job.slicer_weight_g, spool.diameter_mm, &spool.material);
    let rest = round_tenth((spool.remaining_weight_g - g).max(0.0));
    tx.execute("UPDATE filament_spools SET remaining_weight_g = ?2 WHERE id = ?1", params![d.spool_id, rest])?;
    if let Some(file_id) = d.file_id {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE id = ?1 AND deleted_at IS NULL)",
            params![file_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(DbError::Other("Modell nicht gefunden".into()));
        }
        let printer_name: String = tx.query_row("SELECT name FROM printers WHERE id = ?1", params![job.printer_id], |r| r.get(0))?;
        crate::db::insert_print_log_entry(
            &tx,
            &NewPrintLogEntry {
                file_id,
                printed_at: rfc3339_from_unix(job.ended_at),
                note: Some(log_note(&printer_name, g, &job.outcome, partial_percent(job.used_mm, job.slicer_total_mm))),
                photo_png: None,
            },
        )?;
        crate::db::set_print_status(&tx, file_id, "printed")?;
    }
    if mark_job_confirmed(&tx, d.job_id, d.spool_id, d.file_id, g, decided_at)? != 1 {
        return Err(DbError::Other("Druck ist nicht mehr offen".into()));
    }
    tx.commit()?;
    Ok(g)
}

pub fn ignore_job(conn: &Connection, job_id: i64, decided_at: &str) -> Result<(), DbError> {
    if mark_job_ignored(conn, job_id, decided_at)? != 1 {
        return Err(DbError::Other("Druck ist nicht mehr offen".into()));
    }
    Ok(())
}

/// Jede Entscheidung in eigener Transaktion. Liefert (bestätigt, fehlgeschlagen).
pub fn confirm_many(conn: &mut Connection, decisions: &[Decision], decided_at: &str) -> (usize, usize) {
    let mut ok = 0;
    let mut failed = 0;
    for d in decisions {
        match confirm_job(conn, d, decided_at) {
            Ok(_) => ok += 1,
            Err(e) => {
                eprintln!("[printer_link] Bestaetigen von Druck {} fehlgeschlagen: {e}", d.job_id);
                failed += 1;
            }
        }
    }
    (ok, failed)
}

#[cfg(test)]
mod pure_tests {
    use super::*;

    #[test]
    fn grams_follow_the_slicer_weight_of_the_sv08_prints() {
        assert_eq!(grams(303.265800000015, Some(279.17), Some(0.83), 1.75, "PLA"), 0.9);
        assert_eq!(grams(18464.74639999127, Some(18440.65), Some(55.0), 1.75, "PLA"), 55.1);
        assert_eq!(grams(1331.9600600000035, Some(3440.41), Some(10.26), 1.75, "PLA"), 4.0);
    }

    #[test]
    fn grams_fall_back_to_length_diameter_and_density() {
        // 1000 mm * pi * 0.875^2 = 2405.28 mm^3 = 2.405 cm^3 * 1.24 = 2.98 g
        assert_eq!(grams(1000.0, None, None, 1.75, "PLA"), 3.0);
        assert_eq!(grams(1000.0, Some(0.0), Some(5.0), 1.75, "PLA Matt"), 3.0);
        assert_eq!(grams(1000.0, None, None, 1.75, "PETG-CF"), 3.1);
    }

    #[test]
    fn tiny_or_negative_amounts_become_zero() {
        assert_eq!(grams(0.01, None, None, 1.75, "PLA"), 0.0);
        assert_eq!(grams(-5.0, None, None, 1.75, "PLA"), 0.0);
    }

    #[test]
    fn densities_by_material() {
        assert_eq!(density_g_cm3("PETG"), 1.27);
        assert_eq!(density_g_cm3("abs"), 1.04);
        assert_eq!(density_g_cm3("ASA"), 1.07);
        assert_eq!(density_g_cm3("TPU 95A"), 1.21);
        assert_eq!(density_g_cm3("PA-CF"), 1.14);
        assert_eq!(density_g_cm3("PC"), 1.20);
        assert_eq!(density_g_cm3("Holz"), 1.24);
    }

    #[test]
    fn material_matching_ignores_variants() {
        assert!(materials_match(Some("PLA"), "PLA Matt"));
        assert!(materials_match(Some("petg"), "PETG-CF"));
        assert!(!materials_match(Some("PETG"), "PLA"));
        assert!(materials_match(None, "PLA"));
    }

    #[test]
    fn partial_percent_from_slicer_estimate() {
        assert_eq!(partial_percent(1331.96, Some(3440.41)), Some(39));
        assert_eq!(partial_percent(5000.0, Some(3440.41)), Some(100));
        assert_eq!(partial_percent(10.0, None), None);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::db::printer_link::{insert_job_if_new, list_open_jobs};
    use crate::printer_link::{JobOutcome, RemoteJob};

    fn setup() -> Connection {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO printers (id, name, position) VALUES (1, 'Sovol SV08', 0);
             INSERT INTO material_units (id, printer_id, name, kind, slot_count, position) VALUES (10, 1, 'Spulenhalter', 'external', 1, 0);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES (100, 'PLA', 1.75, 1000, 612.4, '2026-09-24', 10, 0);
             INSERT INTO filament_spools (id, material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES (101, 'PETG', 1.75, 1000, 0.5, '2026-09-24');",
        )
        .unwrap();
        let fid = crate::db::test_insert_minimal_file(&conn, "Distanzhülse 13,40mm.3mf", None).unwrap();
        conn.execute("UPDATE files SET queue_position = 1 WHERE id = ?1", [fid]).unwrap();
        insert_job_if_new(&conn, 1, &RemoteJob {
            remote_id: "00003F".into(),
            file_name: "Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode".into(),
            outcome: JobOutcome::Completed,
            raw_status: "completed".into(),
            ended_at: 1789042135.8,
            print_duration_s: 394.65,
            used_mm: 303.2658,
            slicer_total_mm: Some(279.17),
            slicer_weight_g: Some(0.83),
            material: Some("PLA".into()),
            thumbnail_path: None,
        })
        .unwrap();
        conn
    }

    fn first_job(conn: &Connection) -> i64 {
        list_open_jobs(conn).unwrap()[0].id
    }

    fn file_id(conn: &Connection) -> i64 {
        conn.query_row("SELECT id FROM files LIMIT 1", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_spool_in_the_printer_is_suggested() {
        let conn = setup();
        assert_eq!(suggest_spool(&conn, 1).unwrap(), Some(100));
        assert_eq!(suggest_spool(&conn, 99).unwrap(), None);
    }

    #[test]
    fn confirming_books_grams_log_and_printed_status() {
        let mut conn = setup();
        let job = first_job(&conn);
        let fid = file_id(&conn);
        let g = confirm_job(&mut conn, &Decision { job_id: job, spool_id: 100, file_id: Some(fid) }, "2026-09-24T20:00:00Z").unwrap();
        assert_eq!(g, 0.9);
        let rest: f64 = conn.query_row("SELECT remaining_weight_g FROM filament_spools WHERE id = 100", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, 611.5);
        let (status, queue): (String, Option<i64>) =
            conn.query_row("SELECT print_status, queue_position FROM files WHERE id = ?1", [fid], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(status, "printed");
        assert_eq!(queue, None);
        let note: String = conn.query_row("SELECT note FROM print_log WHERE file_id = ?1", [fid], |r| r.get(0)).unwrap();
        assert_eq!(note, "Sovol SV08 · 0.9 g");
        assert!(list_open_jobs(&conn).unwrap().is_empty());
    }

    #[test]
    fn confirming_without_model_only_books_grams() {
        let mut conn = setup();
        let job = first_job(&conn);
        confirm_job(&mut conn, &Decision { job_id: job, spool_id: 100, file_id: None }, "t").unwrap();
        let logs: i64 = conn.query_row("SELECT COUNT(*) FROM print_log", [], |r| r.get(0)).unwrap();
        assert_eq!(logs, 0);
    }

    #[test]
    fn remaining_weight_never_goes_below_zero() {
        let mut conn = setup();
        let job = first_job(&conn);
        confirm_job(&mut conn, &Decision { job_id: job, spool_id: 101, file_id: None }, "t").unwrap();
        let rest: f64 = conn.query_row("SELECT remaining_weight_g FROM filament_spools WHERE id = 101", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, 0.0);
    }

    #[test]
    fn a_failing_step_rolls_everything_back() {
        let mut conn = setup();
        let job = first_job(&conn);
        let err = confirm_job(&mut conn, &Decision { job_id: job, spool_id: 100, file_id: Some(999_999) }, "t");
        assert!(err.is_err());
        let rest: f64 = conn.query_row("SELECT remaining_weight_g FROM filament_spools WHERE id = 100", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, 612.4);
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 1);
    }

    #[test]
    fn missing_spool_is_an_error() {
        let mut conn = setup();
        let job = first_job(&conn);
        assert!(confirm_job(&mut conn, &Decision { job_id: job, spool_id: 555, file_id: None }, "t").is_err());
    }

    #[test]
    fn a_resin_bottle_is_never_booked_and_nothing_changes() {
        let mut conn = setup();
        conn.execute(
            "INSERT INTO filament_spools (id, kind, material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
             VALUES (200, 'resin', 'Standard', 1.75, 1000, 640.5, '2026-09-25')",
            [],
        )
        .unwrap();
        let job = first_job(&conn);
        let err = confirm_job(&mut conn, &Decision { job_id: job, spool_id: 200, file_id: None }, "t").unwrap_err();
        assert!(err.to_string().contains("Resin"), "unerwartete Meldung: {err}");
        let rest: f64 = conn.query_row("SELECT remaining_weight_g FROM filament_spools WHERE id = 200", [], |r| r.get(0)).unwrap();
        assert_eq!(rest, 640.5);
        assert_eq!(list_open_jobs(&conn).unwrap().len(), 1, "der Druck bleibt offen");
        assert_eq!(spool_info(&conn, 200).unwrap(), None);
    }

    #[test]
    fn resin_is_never_suggested_even_if_it_sat_in_a_slot() {
        // Eine Resin-Flasche kommt ueber die App nie in ein Fach; eine
        // praeparierte DB koennte das trotzdem enthalten. Der Vorschlag
        // ueberspringt sie und nimmt die naechste Filament-Spule.
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO material_units (id, printer_id, name, kind, slot_count, position) VALUES (5, 1, 'AMS', 'bambu_ams', 4, 0);
             INSERT INTO filament_spools (id, kind, material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES (200, 'resin', 'Standard', 1.75, 1000, 640.5, '2026-09-25', 5, 0);",
        )
        .unwrap();
        assert_eq!(suggest_spool(&conn, 1).unwrap(), Some(100));
    }

    #[test]
    fn a_job_can_be_decided_only_once() {
        let mut conn = setup();
        let job = first_job(&conn);
        ignore_job(&conn, job, "t").unwrap();
        assert!(ignore_job(&conn, job, "t").is_err());
        assert!(confirm_job(&mut conn, &Decision { job_id: job, spool_id: 100, file_id: None }, "t").is_err());
    }

    #[test]
    fn confirm_many_counts_successes_and_failures() {
        let mut conn = setup();
        let job = first_job(&conn);
        let (ok, failed) = confirm_many(
            &mut conn,
            &[Decision { job_id: job, spool_id: 100, file_id: None }, Decision { job_id: 424242, spool_id: 100, file_id: None }],
            "t",
        );
        assert_eq!((ok, failed), (1, 1));
    }

    #[test]
    fn notes_are_language_neutral() {
        assert_eq!(log_note("SV08", 0.9, "completed", None), "SV08 · 0.9 g");
        assert_eq!(log_note("SV08", 4.0, "partial", Some(39)), "SV08 · 4.0 g · 39 %");
        assert_eq!(log_note("SV08", 4.0, "partial", None), "SV08 · 4.0 g · ✕");
    }
}
