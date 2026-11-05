use super::*;
use crate::unique_test_dir;
use rusqlite::params;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gcode").join(name)
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn conn(enabled: bool) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
    if enabled {
        slicer_reports::set_enabled(&conn, true, Some("/app")).unwrap();
    }
    conn
}

fn add_file(conn: &Connection, name: &str, file_type: &str) -> i64 {
    conn.execute(
        "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES (?1, ?2, ?3, 1, '2026-11-01T00:00:00Z')",
        params![name, format!("/c/{name}"), file_type],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn ticket_json(path: &str) -> String {
    serde_json::json!({"version": 1, "reportedAt": "2026-11-05T10:00:00+00:00", "reportedPath": path, "fileSizeBytes": 42,
        "slicer": "PrusaSlicer 2.8.1", "summary": {"printSeconds": 4200, "weightG": 12.5}})
    .to_string()
}

fn drop_ticket(inbox: &Path, name: &str, content: &str) -> PathBuf {
    std::fs::create_dir_all(inbox).unwrap();
    let path = inbox.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

// ---------------------------------------------------------------- command line

#[test]
fn the_flag_is_found_and_the_path_follows_it() {
    let a = args(&["app", "--report-sliced", "/tmp/My Model ü.gcode"]);
    assert_eq!(report_request(&a), Some(Ok(PathBuf::from("/tmp/My Model ü.gcode"))));
    let eq = args(&["app", "--report-sliced=C:\\out\\a b.gcode"]);
    assert_eq!(report_request(&eq), Some(Ok(PathBuf::from("C:\\out\\a b.gcode"))));
    let other = args(&["app", "--verbose", "--report-sliced", "--quiet", "x.gcode"]);
    assert_eq!(report_request(&other), Some(Ok(PathBuf::from("x.gcode"))), "options in between are skipped");
    let dashes = args(&["app", "--report-sliced", "--", "-odd.gcode"]);
    assert_eq!(report_request(&dashes), Some(Ok(PathBuf::from("-odd.gcode"))));
}

#[test]
fn without_the_flag_there_is_no_report_call_and_without_a_path_an_error() {
    assert_eq!(report_request(&args(&["app"])), None);
    assert_eq!(report_request(&args(&["app", "model.3mf"])), None);
    assert_eq!(report_request(&args(&["app", "--report-sliced"])), Some(Err(ReportError::PathMissing)));
    assert_eq!(report_request(&args(&["app", "--report-sliced="])), Some(Err(ReportError::PathMissing)));
    assert_eq!(report_request(&args(&["app", "--report-sliced", ""])), Some(Err(ReportError::PathMissing)));
}

// ---------------------------------------------------------------- the report call

#[test]
fn a_real_gcode_file_gives_a_ticket_with_what_the_slicer_wrote() {
    let now = chrono::Utc::now();
    let ticket = build_ticket(&fixture("prusa_one_filament.gcode"), None, now).unwrap();
    assert_eq!(ticket.version, 1);
    assert!(ticket.slicer.as_deref().unwrap_or("").starts_with("PrusaSlicer"));
    assert!(ticket.file_size_bytes > 0);
    assert!(ticket.summary.print_seconds.is_some_and(|s| s > 0));
    assert!(ticket.summary.weight_g.is_some_and(|w| w > 0.0));
    assert!(!ticket.summary.test);
    assert!(Path::new(&ticket.reported_path).is_absolute());
    let binary = build_ticket(&fixture("prusa_binary.bgcode"), None, now).unwrap();
    assert!(binary.summary.print_seconds.is_some());
}

#[test]
fn the_final_name_of_the_slicer_replaces_the_path_of_the_call_only_when_usable() {
    let now = chrono::Utc::now();
    let file = fixture("orca_style.gcode");
    let with_env = build_ticket(&file, Some("/home/u/out/Final Name.gcode"), now).unwrap();
    assert_eq!(with_env.reported_path, "/home/u/out/Final Name.gcode");
    let windows = build_ticket(&file, Some("C:\\Users\\u\\Final.gcode"), now).unwrap();
    assert_eq!(windows.reported_path, "C:\\Users\\u\\Final.gcode");
    for bad in ["relative/name.gcode", "", "/tmp/a\nb.gcode", &format!("/{}", "x".repeat(2000))] {
        let t = build_ticket(&file, Some(bad), now).unwrap();
        assert_eq!(t.reported_path, file.to_string_lossy(), "{bad:?} is ignored");
    }
}

#[test]
fn a_missing_file_a_directory_and_a_foreign_file_give_no_ticket() {
    let dir = unique_test_dir("sr_bad");
    let now = chrono::Utc::now();
    assert_eq!(build_ticket(&dir.join("nope.gcode"), None, now), Err(ReportError::FileMissing));
    assert_eq!(build_ticket(&dir, None, now), Err(ReportError::FileMissing));
    let text = dir.join("notes.gcode");
    std::fs::write(&text, "hello world\nthis is no slicer output\n").unwrap();
    assert_eq!(build_ticket(&text, None, now), Err(ReportError::NotGcode));
    let empty = dir.join("empty.gcode");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(build_ticket(&empty, None, now), Err(ReportError::NotGcode));
    let mut broken = std::fs::read(fixture("prusa_binary.bgcode")).unwrap();
    broken.truncate(broken.len().min(60));
    let cut = dir.join("cut.bgcode");
    std::fs::write(&cut, broken).unwrap();
    assert!(matches!(build_ticket(&cut, None, now), Err(ReportError::Unreadable(_))));
}

#[test]
fn the_call_writes_one_whole_ticket_and_only_into_the_inbox() {
    let data = unique_test_dir("sr_call").join("data");
    // The data directory does not exist yet: the call creates the inbox and nothing else.
    let inbox = data.join(INBOX_DIR);
    let written = report_sliced(Ok(fixture("prusa_one_filament.gcode")), Some("/o/Box.gcode"), &inbox).unwrap();
    let mut names: Vec<_> = std::fs::read_dir(&data).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec![INBOX_DIR.to_string()], "no database, no other file next to the inbox");
    let files: Vec<_> = std::fs::read_dir(&inbox).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(files.len(), 1, "no temporary file stays: {files:?}");
    assert!(files[0].ends_with(".json") && !files[0].starts_with('.'));
    let ticket: Ticket = serde_json::from_slice(&std::fs::read(&written).unwrap()).unwrap();
    assert_eq!(ticket.reported_path, "/o/Box.gcode");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&written).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn a_failed_call_leaves_a_line_for_the_settings_page_and_a_good_one_removes_it() {
    let inbox = unique_test_dir("sr_err").join(INBOX_DIR);
    assert!(report_sliced(Ok(inbox.join("missing.gcode")), None, &inbox).is_err());
    assert!(last_error(&inbox).unwrap().contains("does not exist"));
    assert_eq!(std::fs::read_dir(&inbox).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".json")).count(), 0);
    report_sliced(Ok(fixture("cura_style.gcode")), None, &inbox).unwrap();
    assert_eq!(last_error(&inbox), None);
    assert!(report_sliced(Err(ReportError::PathMissing), None, &inbox).is_err());
    assert!(last_error(&inbox).unwrap().contains("no file path"), "a call without a path is recorded too");
}

#[test]
fn a_full_inbox_takes_no_further_ticket() {
    let inbox = unique_test_dir("sr_full").join(INBOX_DIR);
    for i in 0..MAX_TICKETS {
        drop_ticket(&inbox, &format!("t{i}.json"), "{}");
    }
    let file = fixture("prusa_one_filament.gcode");
    assert_eq!(report_sliced(Ok(file.clone()), None, &inbox), Err(ReportError::InboxFull));
    std::fs::remove_file(inbox.join("t0.json")).unwrap();
    assert!(report_sliced(Ok(file.clone()), None, &inbox).is_ok());
}

#[test]
fn the_test_ticket_is_marked_and_goes_the_normal_way() {
    let inbox = unique_test_dir("sr_test").join(INBOX_DIR);
    let conn = conn(true);
    write_test_ticket(&inbox).unwrap();
    assert_eq!(ingest_inbox(&conn, &inbox).unwrap().added, 1);
    let report = &slicer_reports::list(&conn).unwrap()[0];
    let summary: Summary = serde_json::from_str(&report.summary_json).unwrap();
    assert!(summary.test);
    assert_eq!(report.file_name, "Test report.gcode");
}

// ---------------------------------------------------------------- the intake

#[test]
fn the_intake_stores_a_report_suggests_a_model_and_deletes_the_ticket() {
    let conn = conn(true);
    let model = add_file(&conn, "Box.3mf", "3mf");
    add_file(&conn, "Box_PETG_0.2_1h10m.gcode", "gcode");
    let inbox = unique_test_dir("sr_in").join(INBOX_DIR);
    let ticket = drop_ticket(&inbox, "a.json", &ticket_json("/o/Box_PETG_0.2_1h10m.gcode"));
    let outcome = ingest_inbox(&conn, &inbox).unwrap();
    assert_eq!(outcome, IngestOutcome { added: 1, merged: 0, discarded: 0 });
    assert!(!ticket.exists());
    let report = &slicer_reports::list(&conn).unwrap()[0];
    assert_eq!((report.state.as_str(), report.file_name.as_str(), report.file_size_bytes), ("open", "Box_PETG_0.2_1h10m.gcode", 42));
    assert_eq!(report.suggested_file_id, Some(model), "the G-code entry of the catalog is no candidate");
    assert!(report.summary_json.contains("\"printSeconds\":4200"));
}

#[test]
fn the_same_path_twice_is_one_report() {
    let conn = conn(true);
    let inbox = unique_test_dir("sr_dup").join(INBOX_DIR);
    drop_ticket(&inbox, "a.json", &ticket_json("/o/Box.gcode"));
    drop_ticket(&inbox, "b.json", &ticket_json("/o/Box.gcode"));
    let outcome = ingest_inbox(&conn, &inbox).unwrap();
    assert_eq!((outcome.added, outcome.merged), (1, 1));
    assert_eq!(slicer_reports::list(&conn).unwrap().len(), 1);
}

#[test]
fn with_the_intake_off_tickets_are_deleted_unread() {
    let conn = conn(false);
    let inbox = unique_test_dir("sr_off").join(INBOX_DIR);
    let ticket = drop_ticket(&inbox, "a.json", &ticket_json("/o/Box.gcode"));
    assert_eq!(ingest_inbox(&conn, &inbox).unwrap(), IngestOutcome { added: 0, merged: 0, discarded: 1 });
    assert!(!ticket.exists());
    assert!(slicer_reports::list(&conn).unwrap().is_empty());
}

#[test]
fn broken_oversized_and_foreign_tickets_are_discarded_without_harm() {
    let conn = conn(true);
    let inbox = unique_test_dir("sr_bad_in").join(INBOX_DIR);
    drop_ticket(&inbox, "garbage.json", "not json at all");
    drop_ticket(&inbox, "version.json", &ticket_json("/o/a.gcode").replace("\"version\":1", "\"version\":2"));
    drop_ticket(&inbox, "relative.json", &ticket_json("relative/a.gcode"));
    drop_ticket(&inbox, "control.json", &ticket_json("/o/a\u{7}b.gcode"));
    drop_ticket(&inbox, "date.json", &ticket_json("/o/a.gcode").replace("2026-11-05T10:00:00+00:00", "yesterday"));
    drop_ticket(&inbox, "huge.json", &format!("{} ", ticket_json("/o/huge.gcode")).repeat(2000));
    drop_ticket(&inbox, "negative.json", &ticket_json("/o/n.gcode").replace("\"fileSizeBytes\":42", "\"fileSizeBytes\":-1"));
    let keep = drop_ticket(&inbox, "notes.txt", "stays");
    let good = drop_ticket(&inbox, "good.json", &ticket_json("/o/good.gcode"));
    let outcome = ingest_inbox(&conn, &inbox).unwrap();
    assert_eq!((outcome.added, outcome.discarded), (1, 7));
    assert!(keep.exists() && !good.exists(), "foreign files stay, handled tickets go");
    assert_eq!(std::fs::read_dir(&inbox).unwrap().count(), 1);
}

#[test]
fn the_summary_is_cleaned_and_bounded() {
    let conn = conn(true);
    let inbox = unique_test_dir("sr_clean").join(INBOX_DIR);
    let filaments: Vec<_> = (0..100).map(|i| serde_json::json!({"filamentType": "PLA", "color": "#FF00AA", "weightG": i as f64 + 1.0})).collect();
    let content = serde_json::json!({"version": 1, "reportedAt": "2026-11-05T10:00:00+00:00", "reportedPath": "/o/a.gcode", "fileSizeBytes": 1,
        "summary": {"weightG": -5.0, "printSeconds": 0, "layerHeightMm": 0.2, "unknown": "x", "filaments": filaments}})
    .to_string();
    drop_ticket(&inbox, "a.json", &content);
    ingest_inbox(&conn, &inbox).unwrap();
    let summary: Summary = serde_json::from_str(&slicer_reports::list(&conn).unwrap()[0].summary_json).unwrap();
    assert_eq!((summary.weight_g, summary.print_seconds, summary.layer_height_mm), (None, None, Some(0.2)));
    assert_eq!(summary.filaments.len(), MAX_FILAMENTS);
    assert_eq!(summary.filaments[0].color.as_deref(), Some("#ff00aa"));
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_inbox_is_ignored_and_not_followed() {
    let conn = conn(true);
    let base = unique_test_dir("sr_link");
    let inbox = base.join(INBOX_DIR);
    let target = base.join("secret.json");
    std::fs::write(&target, ticket_json("/o/secret.gcode")).unwrap();
    std::fs::create_dir_all(&inbox).unwrap();
    let link = inbox.join("link.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(ingest_inbox(&conn, &inbox).unwrap(), IngestOutcome::default());
    assert!(link.symlink_metadata().is_ok() && target.exists(), "neither read nor deleted");
    assert!(slicer_reports::list(&conn).unwrap().is_empty());
}

#[test]
fn old_tickets_and_those_over_the_limit_are_dropped() {
    let conn = conn(true);
    let inbox = unique_test_dir("sr_limit").join(INBOX_DIR);
    let old = drop_ticket(&inbox, "old.json", &ticket_json("/o/old.gcode"));
    let aged = std::time::SystemTime::now() - std::time::Duration::from_secs((MAX_AGE_DAYS as u64 + 1) * 86_400);
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(aged).unwrap();
    for i in 0..(MAX_TICKETS + 3) {
        drop_ticket(&inbox, &format!("t{i}.json"), &ticket_json(&format!("/o/{i}.gcode")));
    }
    let outcome = ingest_inbox(&conn, &inbox).unwrap();
    assert_eq!(outcome.added, MAX_TICKETS);
    assert_eq!(outcome.discarded, 4, "the old one and the three over the limit");
    assert!(!slicer_reports::list(&conn).unwrap().iter().any(|r| r.file_name == "old.gcode"));
}

#[test]
fn a_missing_inbox_is_an_empty_intake() {
    let conn = conn(true);
    let inbox = unique_test_dir("sr_none").join("nowhere");
    assert_eq!(ingest_inbox(&conn, &inbox).unwrap(), IngestOutcome::default());
}

#[test]
fn candidates_leave_out_gcode_resin_and_trashed_entries() {
    let conn = conn(true);
    let model = add_file(&conn, "Box.3mf", "3mf");
    add_file(&conn, "Box.gcode", "gcode");
    add_file(&conn, "Box.ctb", "resin");
    let trashed = add_file(&conn, "Old.stl", "stl");
    conn.execute("UPDATE files SET deleted_at = 'x' WHERE id = ?1", [trashed]).unwrap();
    let ids: Vec<i64> = model_candidates(&conn).unwrap().into_iter().map(|c| c.file_id).collect();
    assert_eq!(ids, vec![model]);
}
