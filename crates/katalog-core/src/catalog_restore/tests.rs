use super::*;
use crate::db::models::{FileType, MaterialRecord, NewFile};
use rusqlite::params;
use std::collections::BTreeMap;

fn unique_test_dir(name: &str) -> PathBuf {
    let dir = test_temp_dir().join(format!(
        "{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn test_temp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.target/restore-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn validate_catalog_db_bytes(
    bytes: &[u8],
    sensitive: &[PathBuf],
    trash: &Path,
    known: &[String],
) -> CmdResult<()> {
    super::validate_catalog_db_bytes(
        bytes,
        sensitive,
        &TrashPolicy {
            desktop_trash: Some(trash),
            catalog_roots: &[],
        },
        known,
        &StdRestoreFs::new(test_temp_dir()),
    )
}

#[cfg(not(windows))]
#[test]
fn imported_names_preserve_native_characters_and_trailing_spaces() {
    let dir = unique_test_dir("native_import_names");
    for name in [
        "Benchy: v2.stl",
        "what?.stl",
        "a*b|c\"<d>",
        "model.",
        "model ",
    ] {
        let bytes = catalog_db_bytes_with_file_row(name, dir.join(name).to_str().unwrap(), None);
        assert!(
            validate_catalog_db_bytes(&bytes, &[], &dir, &[]).is_ok(),
            "{name}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(windows)]
#[test]
fn imported_names_reject_drive_syntax_and_devices_on_windows() {
    for name in ["C:x.stl", "CON.stl", "LPT¹.stl"] {
        assert!(validate_file_name(name).is_err());
    }
}

#[test]
fn imported_empty_trash_paths_are_ignored() {
    let dir = unique_test_dir("empty_import_trash");
    for trash in ["", "   ", "\t\n", "\u{2003}"] {
        let bytes = catalog_db_bytes_with_file_row(
            "x.stl",
            dir.join("x.stl").to_str().unwrap(),
            Some(trash),
        );
        assert!(
            validate_catalog_db_bytes(&bytes, &[], &dir, &[]).is_ok(),
            "{trash:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imported_identity_is_recomputed_locally() {
    let conn = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&conn, "/tmp/x.3mf", None).unwrap();
    conn.execute(
        "UPDATE files SET content_hash = 'forged', imported_at = '1900'",
        [],
    )
    .unwrap();
    sanitize_imported_file_identity(&conn).unwrap();
    let file = db::get_file(&conn, id).unwrap().unwrap();
    assert!(file.content_hash.is_none());
    assert_eq!(file.imported_at, "1900");
}

#[test]
fn imported_paths_reject_network_and_relative_strings_without_io() {
    for path in [
        r"\\192.0.2.10\probe\x",
        r"\\?\UNC\host\share\x",
        r"\\host@SSL\share\x",
        r"\\.\device",
        "//host/share/x",
        "relative/x",
        r"C:x",
    ] {
        assert!(validate_imported_local_path(path, &[]).is_err(), "{path}");
    }
    assert!(validate_imported_local_path(test_temp_dir().to_str().unwrap(), &[]).is_ok());
    let bytes = catalog_db_bytes_with_file_row("x.3mf", r"\\192.0.2.10\probe\x", None);
    assert!(validate_catalog_db_bytes(&bytes, &[], &test_temp_dir(), &[]).is_err());
}

#[cfg(windows)]
#[test]
fn imported_windows_paths_require_an_absolute_local_drive() {
    assert!(validate_imported_local_path(r"C:\models\x.3mf", &[]).is_ok());
    for path in [r"\models\x", r"C:x", r"\\?\GLOBALROOT\Device\x"] {
        assert!(validate_imported_local_path(path, &[]).is_err());
    }
}

#[test]
fn imported_folder_names_use_portable_component_rules() {
    let dir = unique_test_dir("import_folder_names");
    let path = dir.join("catalog.db");
    {
        let conn = db::connect(&path).unwrap();
        db::insert_folder_with_parent(&conn, "CON", None, dir.to_str().unwrap()).unwrap();
    }
    assert_eq!(
        validate_catalog_db_bytes(&std::fs::read(&path).unwrap(), &[], &dir, &[]).is_err(),
        cfg!(windows)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn imported_spool_print_temperatures_are_checked() {
    for (min, max) in [
        (Some(9999), None),
        (Some(-1), None),
        (None, Some(501)),
        (Some(240), Some(200)),
    ] {
        for (min_column, max_column) in [
            ("nozzle_temp_min", "nozzle_temp_max"),
            ("bed_temp_min", "bed_temp_max"),
        ] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
                conn.execute(
                    &format!("UPDATE filament_spools SET {min_column} = ?1, {max_column} = ?2"),
                    rusqlite::params![min, max],
                )
                .unwrap();
            });
            let error = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).unwrap_err();
            assert!(
                (error
                    == CmdError::coded_with(
                        ErrorCode::BackupInvalidDatabase,
                        std::iter::empty::<&str>()
                    )),
                "{error:?}"
            );
        }
    }
}

#[test]
fn imported_spool_color_lists_are_checked() {
    for (value, valid) in [
        (r##"["#ff0000","#00ff00","#0000ff","#ffffff"]"##, true),
        (r##"["#ff0000","#ff0000"]"##, true),
        (
            r##"["#ff0000","#00ff00","#0000ff","#ffffff","#000000"]"##,
            false,
        ),
        (r##"["#ff0000","red"]"##, false),
        (r##"["#ff00001"]"##, false),
        (r##"{"a":1}"##, false),
        ("not json", false),
    ] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
            conn.execute(
                "UPDATE filament_spools SET color_hexes = ?1, color_hex = '#ff0000'",
                [value],
            )
            .unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert_eq!(result.is_ok(), valid, "{value}: {result:?}");
    }
}

#[test]
fn imported_source_values_are_checked() {
    for (column, value, valid) in [
        ("source_platform", "makerworld".to_string(), true),
        ("source_model_id", "é".repeat(128), true),
        ("source_model_id", "é".repeat(129), false),
        ("source_profile_id", "12\u{7}34".to_string(), false),
        ("source_profile_id", "12\u{202E}34".to_string(), false),
    ] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
            conn.execute(
                &format!("UPDATE files SET {column} = ?1 WHERE id = ?2"),
                rusqlite::params![value, id],
            )
            .unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert_eq!(result.is_ok(), valid, "{column}: {result:?}");
    }
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
        conn.execute(
            "UPDATE files SET source_model_id = X'1234' WHERE id = ?1",
            [id],
        )
        .unwrap();
    });
    assert!(
        validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(),
        "a BLOB is the wrong storage class"
    );
}

#[test]
fn imported_legacy_text_fields_remain_valid() {
    for (column, limit) in [("manufacturer", 150), ("model", 150), ("tag", 120)] {
        for length in [limit, limit + 1] {
            let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
                let value = "a".repeat(length);
                if column == "tag" {
                    conn.execute(
                        "INSERT INTO tags (name, color_hue) VALUES (?1, 0)",
                        [&value],
                    )
                    .unwrap();
                } else {
                    conn.execute(
                        &format!("INSERT INTO printers (name, {column}) VALUES ('Printer', ?1)"),
                        [&value],
                    )
                    .unwrap();
                }
            });
            let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
            assert!(result.is_ok(), "{column}: {result:?}");
        }
    }
}

#[test]
fn imported_paths_have_a_length_limit() {
    for length in [4096, 4097] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute(
                "INSERT INTO folders (name, path) VALUES ('root', ?1)",
                [format!("/{}", "a".repeat(length - 1))],
            )
            .unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert_eq!(result.is_ok(), length == 4096, "{result:?}");
    }
}

#[test]
fn validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table() {
    let tmp_path = test_temp_dir().join(format!(
        "validate_catalog_db_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
        drop(conn);
    }
    let bytes = std::fs::read(&tmp_path).expect("read temp db");

    let result = validate_catalog_db_bytes(&bytes, &[], &test_temp_dir(), &[]);

    let _ = std::fs::remove_file(&tmp_path);
    assert!(
        result.is_ok(),
        "expected valid catalog db to pass validation: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_printer_link_tables() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "INSERT INTO printers (id, name, position) VALUES (1, 'SV08', 0);
                 INSERT INTO app_settings (key, value) VALUES ('printer_link_enabled', '1');
                 INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (1, 'moonraker', '192.168.1.60', 1000.0);
                 INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm)
                     VALUES (1, 'A', 'a.gcode', 'completed', 'completed', 2000.0, 10.0, 5.0);",
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
}

#[test]
fn validate_catalog_db_bytes_rejects_printer_jobs_with_wrong_types() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "INSERT INTO printers (id, name, position) VALUES (1, 'SV08', 0);
                 INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm)
                     VALUES (1, 'A', 'a.gcode', 'completed', 'completed', 'gestern', 10.0, 5.0);",
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn sanitize_drops_foreign_addresses_and_pauses_the_rest() {
    let conn = crate::db::connect_in_memory().unwrap();
    conn.execute_batch(
            "INSERT INTO printers (id, name, position) VALUES (1, 'A', 0), (2, 'B', 1), (3, 'C', 2);
             INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES
                 (1, 'moonraker', '192.168.1.60', 1.0), (2, 'moonraker', '8.8.8.8', 1.0), (3, 'moonraker', 'sv08.local', 1.0);",
        )
        .unwrap();
    sanitize_printer_connections(&conn).unwrap();
    let rows: Vec<(i64, i64)> = conn
        .prepare("SELECT printer_id, paused FROM printer_connections ORDER BY printer_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, 1), (3, 1)]);
}

#[test]
fn sanitize_resets_the_failure_counter() {
    let conn = crate::db::connect_in_memory().unwrap();
    conn.execute_batch(
            "INSERT INTO printers (id, name, position) VALUES (1, 'A', 0);
             INSERT INTO printer_connections (printer_id, kind, address, connected_since, failures, last_attempt_at)
                 VALUES (1, 'moonraker', '192.168.1.60', 1.0, 7, 99.0);",
        )
        .unwrap();
    sanitize_printer_connections(&conn).unwrap();
    let row: (i64, Option<f64>) = conn
        .query_row(
            "SELECT failures, last_attempt_at FROM printer_connections",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, (0, None));
}

#[test]
fn a_negative_failure_counter_is_rejected() {
    let conn = Connection::open_in_memory().unwrap();
    // A table without the CHECK, as a crafted backup could bring it.
    conn.execute_batch(
            "CREATE TABLE printers (id INTEGER, kind TEXT);
             CREATE TABLE printer_jobs (outcome TEXT, state TEXT, auto_closed INTEGER, failure_reason TEXT, failure_note TEXT);
             CREATE TABLE printer_connections (printer_id INTEGER, kind TEXT, paused INTEGER, failures INTEGER, api_key TEXT);
             INSERT INTO printer_connections VALUES (1, 'moonraker', 0, -3, NULL);",
        )
        .unwrap();
    assert!(validate_printer_link_rows(&conn).is_err());
    conn.execute("UPDATE printer_connections SET failures = 0", [])
        .unwrap();
    assert!(validate_printer_link_rows(&conn).is_ok());
}

#[test]
fn the_failure_fields_of_a_print_are_accepted_when_they_follow_the_rules() {
    let conn = jobs_table(
        "INSERT INTO printer_jobs VALUES
                 ('completed', 'open', 0, NULL, NULL),
                 ('partial', 'open', 0, 'clog', 'nozzle'),
                 ('partial', 'confirmed', 0, 'other', NULL),
                 ('partial', 'ignored', 1, NULL, NULL),
                 ('partial', 'ignored', 1, 'intentional', NULL),
                 ('completed', 'ignored', 1, NULL, NULL);",
    );
    assert!(validate_printer_link_rows(&conn).is_ok());
    for code in crate::printer_link::failure::FAILURE_REASONS {
        conn.execute(
            "UPDATE printer_jobs SET failure_reason = ?1 WHERE failure_reason = 'clog'",
            [code],
        )
        .unwrap();
        assert!(validate_printer_link_rows(&conn).is_ok(), "{code}");
    }
    let at_limit = "\u{e4}".repeat(crate::printer_link::failure::MAX_NOTE_CHARS);
    conn.execute(
        "UPDATE printer_jobs SET failure_note = ?1 WHERE failure_note = 'nozzle'",
        [at_limit],
    )
    .unwrap();
    assert!(
        validate_printer_link_rows(&conn).is_ok(),
        "200 characters, though more than 200 bytes"
    );
}

#[test]
fn a_print_with_a_broken_failure_field_makes_the_backup_unusable() {
    let too_long = "x".repeat(crate::printer_link::failure::MAX_NOTE_CHARS + 1);
    let cases: Vec<(&str, String)> = vec![
        (
            "unknown code",
            "('partial', 'open', 0, 'made_up', NULL)".into(),
        ),
        (
            "code in another case",
            "('partial', 'open', 0, 'Clog', NULL)".into(),
        ),
        ("empty code", "('partial', 'open', 0, '', NULL)".into()),
        (
            "reason on a completed print",
            "('completed', 'open', 0, 'clog', NULL)".into(),
        ),
        (
            "note too long",
            format!("('partial', 'open', 0, 'clog', '{too_long}')"),
        ),
        (
            "flag out of range",
            "('partial', 'ignored', 2, NULL, NULL)".into(),
        ),
        (
            "closed by the program but still open",
            "('partial', 'open', 1, NULL, NULL)".into(),
        ),
        (
            "closed by the program but confirmed",
            "('completed', 'confirmed', 1, NULL, NULL)".into(),
        ),
    ];
    for (what, row) in cases {
        let conn = jobs_table(&format!("INSERT INTO printer_jobs VALUES {row};"));
        assert!(validate_printer_link_rows(&conn).is_err(), "{what}");
    }
}

#[test]
fn a_damaged_api_key_value_is_rejected_and_an_absent_one_is_not() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
            "CREATE TABLE printers (id INTEGER, kind TEXT);
             CREATE TABLE printer_jobs (outcome TEXT, state TEXT, auto_closed INTEGER, failure_reason TEXT, failure_note TEXT);
             CREATE TABLE printer_connections (printer_id INTEGER, kind TEXT, paused INTEGER, failures INTEGER, api_key TEXT);
             INSERT INTO printer_connections VALUES (1, 'moonraker', 0, 0, NULL);",
        )
        .unwrap();
    assert!(validate_printer_link_rows(&conn).is_ok(), "no key is fine");
    for bad in ["abc\ndef", "abc def", "tab\there", "\u{e4}bc"] {
        conn.execute("UPDATE printer_connections SET api_key = ?1", [bad])
            .unwrap();
        assert!(validate_printer_link_rows(&conn).is_err(), "{bad:?}");
    }
    conn.execute(
        "UPDATE printer_connections SET api_key = ?1",
        ["a".repeat(257)],
    )
    .unwrap();
    assert!(validate_printer_link_rows(&conn).is_err(), "too long");
    conn.execute(
        "UPDATE printer_connections SET api_key = ?1",
        ["a".repeat(256)],
    )
    .unwrap();
    assert!(validate_printer_link_rows(&conn).is_ok(), "at the limit");
}

#[test]
fn validate_catalog_db_bytes_accepts_fractional_spool_weights() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000.0, 612.4, '2026-09-24')",
                [],
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
}

#[test]
fn validate_catalog_db_bytes_rejects_garbage_bytes() {
    let result =
        validate_catalog_db_bytes(b"this is not a sqlite database", &[], &test_temp_dir(), &[]);
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_corrupted_sqlite_file() {
    let bytes = b"SQLite format 3\x00this-is-not-actually-a-valid-database-body".to_vec();
    let sensitive_dirs = vec![];
    let trash_dir = unique_test_dir("validate_db_corrupt_trash");
    let result = validate_catalog_db_bytes(&bytes, &sensitive_dirs, &trash_dir, &[]);
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_foreign_key_violation() {
    let tmp_path = unique_test_db_path("validate_db_fk_violation");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        // File with a folder_id that points to no existing row in folders.
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
                "INSERT INTO files (name, path, file_type, folder_id, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 999999, 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_cyclic_folder_graph() {
    let tmp_path = unique_test_db_path("validate_db_folder_cycle");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute(
            "INSERT INTO folders (name, path) VALUES ('A', '/tmp/A')",
            [],
        )
        .unwrap();
        let a_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO folders (name, path) VALUES ('B', '/tmp/B')",
            [],
        )
        .unwrap();
        let b_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
            params![b_id, a_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
            params![a_id, b_id],
        )
        .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_accepts_a_legitimate_pre_migration_backup() {
    // An older backup without migration columns (e.g. folders.parent_id) must be
    // accepted thanks to the migration up front.
    let tmp_path = unique_test_db_path("validate_db_pre_migration_backup");
    {
        let conn = rusqlite::Connection::open(&tmp_path).unwrap();
        conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
        // Deliberately NO run_migrations() here - simulates a DB exported before the
        // parent_id/path migration.
        conn.execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();

    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);

    assert!(result.is_ok(), "a legitimate pre-migration backup must be accepted after internal migration, got: {result:?}");
}

#[test]
fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_trigger() {
    let tmp_path = unique_test_db_path("validate_db_injected_trigger");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute_batch(
                "CREATE TRIGGER evil AFTER DELETE ON folders
                 BEGIN
                     INSERT INTO folders (name, path) VALUES ('Injected', '/tmp/injected-by-trigger');
                 END;",
            ).unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine importierte Datenbank mit einem eingeschleusten Trigger muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_view() {
    let tmp_path = unique_test_db_path("validate_db_injected_view");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute_batch("CREATE VIEW evil_view AS SELECT * FROM folders;")
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine importierte Datenbank mit einer eingeschleusten View muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_an_endless_view_named_files_without_running_it() {
    let tmp_path = unique_test_db_path("validate_db_endless_view");
    {
        let conn = rusqlite::Connection::open(&tmp_path).unwrap();
        conn.execute_batch(
                "CREATE VIEW files AS WITH RECURSIVE r(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM r) SELECT x FROM r;",
            )
            .unwrap();
    }
    let result = validate_with_timeout(std::fs::read(&tmp_path).unwrap());
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_virtual_table() {
    let tmp_path = unique_test_db_path("validate_db_virtual_table");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE evil USING json_each('[1,2]');")
            .ok();
        conn.execute_batch("CREATE VIRTUAL TABLE evil2 USING fts5(x);")
            .ok();
        let virtual_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE sql LIKE 'CREATE VIRTUAL TABLE%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if virtual_tables == 0 {
            return; // this SQLite build offers no virtual table module to test with
        }
    }
    let result = validate_with_timeout(std::fs::read(&tmp_path).unwrap());
    assert!(result.is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_database_missing_a_required_table() {
    // Valid DB with exactly one defect. `tags` instead of `files`, because `files`
    // is queried earlier via COUNT; this way only validate_expected_schema can
    // find the error.
    let tmp_path = unique_test_db_path("validate_db_missing_table");
    {
        let conn = crate::db::connect(&tmp_path).unwrap(); // complete, current schema
        conn.execute_batch("DROP TABLE tags;").unwrap();
        assert!(validate_expected_schema(&conn)
            .unwrap_err()
            .contains("tags"));
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let err = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]).unwrap_err();
    assert!(
        (err == CmdError::coded_with(ErrorCode::BackupInvalidDatabase, std::iter::empty::<&str>())),
        "{err:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_database_missing_a_required_column() {
    // Same as above, but with exactly one column removed.
    let tmp_path = unique_test_db_path("validate_db_missing_column");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        // 'imported_at', because DROP COLUMN won't remove columns with UNIQUE or an index.
        conn.execute_batch("ALTER TABLE files DROP COLUMN imported_at;")
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine 'files'-Tabelle ohne die erforderliche Spalte 'imported_at' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_an_unknown_kind() {
    let tmp_path = unique_test_db_path("validate_db_unit_bad_kind");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (printer_id, _) = printer_with_one_ams_unit(&conn);
        conn.execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Toaster', 'toaster', 4, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit unbekanntem 'kind' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_an_ams_2_pro_unit() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (printer, _) = printer_with_one_ams_unit(conn);
        crate::db::printers::insert_unit(conn, printer, "bambu_ams_2_pro", "AMS 2 Pro", None)
            .unwrap();
    });
    let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
    assert!(
        result.is_ok(),
        "ein Katalog mit AMS-2-Pro-Einheit muss gehen: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_slot_count_out_of_range() {
    let tmp_path = unique_test_db_path("validate_db_unit_bad_slot_count");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (printer_id, _) = printer_with_one_ams_unit(&conn);
        conn.execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Riesig', 'custom', 1000000000, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
            result.is_err(),
            "eine Einheit mit riesiger 'slot_count' muss abgelehnt werden (sonst haengt PrinterColumn.tsx beim Rendern)"
        );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_bambu_ams_index_out_of_range() {
    let tmp_path = unique_test_db_path("validate_db_unit_bad_ams_index");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (printer_id, _) = printer_with_one_ams_unit(&conn);
        conn.execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'AMS X', 'bambu_ams', 4, 99, 1)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit 'bambu_ams_index' ausserhalb 0..=3 muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_dangling_printer_id() {
    let tmp_path = unique_test_db_path("validate_db_unit_dangling_printer");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (999999, 'Verwaist', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit nicht existierendem 'printer_id' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_kind() {
    // `kind NOT IN (...)` is UNKNOWN for NULL, so it needs `kind IS NULL`.
    let tmp_path = unique_test_db_path("validate_db_unit_null_kind");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
        drop_material_units_constraints(&conn);
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', NULL, 4, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit NULL 'kind' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_slot_count() {
    let tmp_path = unique_test_db_path("validate_db_unit_null_slot_count");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
        drop_material_units_constraints(&conn);
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit NULL 'slot_count' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_printer_id() {
    let tmp_path = unique_test_db_path("validate_db_unit_null_printer_id");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        drop_material_units_constraints(&conn);
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (NULL, 'Kaputt', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit NULL 'printer_id' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_printer_with_a_null_name() {
    let tmp_path = unique_test_db_path("validate_db_printer_null_name");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute_batch(
            "DROP TABLE printers;
                 CREATE TABLE printers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT,
                    position INTEGER
                 );",
        )
        .unwrap();
        conn.execute("INSERT INTO printers (name, position) VALUES (NULL, 0)", [])
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "ein Drucker mit NULL 'name' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_spool_in_a_slot_of_a_unit_with_a_null_slot_count() {
    // Covers the COALESCE guard in `bad_spools`.
    let tmp_path = unique_test_db_path("validate_db_spool_slot_in_null_slot_count_unit");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
        drop_material_units_constraints(&conn);
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        let unit_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 999)",
                params![unit_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(result.is_err(), "eine Spule mit slot_index 999 in einer Einheit ohne gueltige Fachanzahl muss abgelehnt werden");
}

#[test]
fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_set_but_no_unit() {
    let tmp_path = unique_test_db_path("validate_db_spool_half_placement");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', NULL, 0)",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Spule mit gesetztem 'slot_index' aber ohne 'unit_id' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_spool_with_a_dangling_unit_id() {
    let tmp_path = unique_test_db_path("validate_db_spool_dangling_unit");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 999999, 0)",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Spule mit nicht existierender 'unit_id' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_outside_the_units_range() {
    let tmp_path = unique_test_db_path("validate_db_spool_slot_out_of_range");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (_, unit_id) = printer_with_one_ams_unit(&conn); // 4 slots, valid are 0..=3
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 4)",
                params![unit_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Spule mit 'slot_index' >= der Fachanzahl ihrer Einheit muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_two_spools_in_the_same_slot() {
    let tmp_path = unique_test_db_path("validate_db_duplicate_slot");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (_, unit_id) = printer_with_one_ams_unit(&conn);
        // Drop the unique index, as it may be missing in a crafted DB.
        conn.execute("DROP INDEX idx_filament_spools_slot", [])
            .unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PETG', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "zwei Spulen im selben Fach muessen abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_an_invalid_color_hex() {
    let tmp_path = unique_test_db_path("validate_db_bad_color_hex");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, color_hex)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 'rot')",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "ein 'color_hex' ausserhalb des #rrggbb-Formats muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_a_valid_backup_with_loaded_spools() {
    let tmp_path = unique_test_db_path("validate_db_valid_with_loaded_spools");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let (_, unit_id) = printer_with_one_ams_unit(&conn);
        let spool_id = crate::db::insert_filament_spool(
            &conn,
            &crate::db::models::NewFilamentSpool {
                nozzle_temp_min: None,
                nozzle_temp_max: None,
                bed_temp_min: None,
                bed_temp_max: None,
                material: "PLA".to_string(),
                manufacturer: None,
                color: None,
                location: Some("Regal 2".to_string()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 800.0,
                price: None,
                image_png: None,
                color_hex: Some("#1a1a1a".to_string()),
                color_hexes: Vec::new(),
                kind: "filament".to_string(),
            },
        )
        .unwrap();
        crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_ok(),
        "ein gueltiges Backup mit belegten Faechern muss akzeptiert werden: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_real_slot_count() {
    // 4.5 stays REAL despite INTEGER affinity and lies "between" 1 and 16.
    let tmp_path = unique_test_db_path("validate_db_unit_real_slot_count");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', 4.5, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Einheit mit 'slot_count' als REAL-Speicherklasse muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_printer_with_an_integer_name() {
    let tmp_path = unique_test_db_path("validate_db_printer_integer_name");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        drop_printers_constraints(&conn);
        conn.execute(
            "INSERT INTO printers (name, position, kind) VALUES (12345, 0, 'filament')",
            [],
        )
        .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "ein Drucker mit 'name' als INTEGER-Speicherklasse muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_filament_spool_with_remaining_weight_g_stored_as_text() {
    // No constraint rebuild needed: INTEGER affinity only converts a TEXT value
    // if it LOOKS like a number - 'viel' stays TEXT.
    let tmp_path = unique_test_db_path("validate_db_spool_text_remaining_weight");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000, 'viel', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Spule mit 'remaining_weight_g' als TEXT-Speicherklasse muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_filament_spool_with_a_null_material() {
    // `material` is a non-optional String in Rust; only the type check catches NULL.
    let tmp_path = unique_test_db_path("validate_db_spool_null_material");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        drop_filament_spools_constraints(&conn);
        conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES (NULL, 1.75, 1000, 1000, '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "eine Spule mit NULL 'material' muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_folder_with_name_stored_as_blob() {
    // A BLOB stays BLOB despite TEXT affinity. `folders.name` instead of
    // `files.name`, because the latter is read earlier; this way the test only
    // covers validate_column_types.
    let tmp_path = unique_test_db_path("validate_db_folder_blob_name");
    {
        let conn = crate::db::connect(&tmp_path).unwrap();
        conn.execute(
            "INSERT INTO folders (name, path) VALUES (x'00010203', '/tmp/blob-name')",
            [],
        )
        .unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
        result.is_err(),
        "ein Ordner mit 'name' als BLOB-Speicherklasse muss abgelehnt werden"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_a_full_catalog_with_data_in_every_table() {
    // A normally filled catalog (data in every table) must not be rejected.
    let tmp_path = unique_test_db_path("validate_db_full_catalog");
    {
        let mut conn = crate::db::connect(&tmp_path).unwrap();

        let folder_id =
            crate::db::insert_folder_with_parent(&conn, "Projekte", None, "/tmp/Projekte").unwrap();

        let mut metadata = BTreeMap::new();
        metadata.insert("Slicer".to_string(), "Bambu Studio".to_string());
        let new_file = NewFile {
            license: None,
            source: Default::default(),
            name: "Gehaeuse.3mf".to_string(),
            path: "/tmp/Projekte/Gehaeuse.3mf".to_string(),
            file_type: FileType::ThreeMf,
            folder_id: Some(folder_id),
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 12345,
            dimensions_mm: Some([10.0, 20.0, 30.0]),
            volume_cm3: Some(123.4),
            object_count: Some(3),
            thumbnail_png: Some(vec![1, 2, 3]),
            imported_at: "2026-09-23T00:00:00Z".to_string(),
            file_modified_at: Some("2026-09-23T00:00:00Z".to_string()),
            materials: vec![MaterialRecord {
                name: "PLA".to_string(),
                display_color: Some("#ff0000".to_string()),
            }],
            metadata,
            tags: vec!["Prototyp".to_string()],
            print_status: "printed".to_string(),
            last_viewed_at: Some("2026-09-23T00:00:00Z".to_string()),
            creator: Some("Max".to_string()),
            content_hash: Some("abc123".to_string()),
            render_snapshot_png: Some(vec![4, 5, 6]),
            custom_image_png: Some(vec![7, 8, 9]),
            source_url: Some("https://example.com/model".to_string()),
            queue_position: Some(1),
            favorite: true,
            plate_count: Some(2),
            plate_dimensions_json: None,
            slice_info_json: Some("{}".to_string()),
            print_info: None,
        };
        let file_id = crate::db::insert_file(&mut conn, &new_file).unwrap();

        let created_at = chrono::Utc::now().to_rfc3339();
        let collection_id = crate::db::create_collection(&conn, "Sammlung", &created_at).unwrap();
        crate::db::add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();

        let slicer_executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .to_string();
        crate::db::insert_registered_slicer(&conn, "Bambu Studio", &slicer_executable, true)
            .unwrap();

        let (_, unit_id) = printer_with_one_ams_unit(&conn);
        let spool_id = crate::db::insert_filament_spool(
            &conn,
            &crate::db::models::NewFilamentSpool {
                nozzle_temp_min: None,
                nozzle_temp_max: None,
                bed_temp_min: None,
                bed_temp_max: None,
                material: "PLA".to_string(),
                manufacturer: Some("Bambu".to_string()),
                color: Some("Schwarz".to_string()),
                location: Some("Regal 2".to_string()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 800.0,
                price: Some(19.99),
                image_png: Some(vec![9, 9, 9]),
                color_hex: Some("#1a1a1a".to_string()),
                color_hexes: Vec::new(),
                kind: "filament".to_string(),
            },
        )
        .unwrap();
        crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
    }
    let bytes = std::fs::read(&tmp_path).unwrap();
    let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"), &[]);
    assert!(
            result.is_ok(),
            "ein regulaer befuellter Katalog mit Daten in jeder Tabelle muss weiterhin akzeptiert werden: {result:?}"
        );
}

#[test]
fn validate_catalog_db_bytes_rejects_folder_path_in_sensitive_directory() {
    // A technically valid backup with `folders.path` in a protected directory must be rejected.
    let tmp_path = test_temp_dir().join(format!(
        "validate_catalog_db_sensitive_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // A real directory: otherwise only the checked path gets resolved (macOS:
    // /tmp -> /private/tmp) and the prefix comparison fails because of the test setup.
    let sensitive_root = unique_test_dir("3mf-test-sensitive-root");
    {
        let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
        db::insert_folder_with_parent(
            &conn,
            "evil",
            None,
            &sensitive_root.join("evil").to_string_lossy(),
        )
        .expect("insert malicious folder row");
        drop(conn);
    }
    let bytes = std::fs::read(&tmp_path).expect("read temp db");

    let result = validate_catalog_db_bytes(
        &bytes,
        std::slice::from_ref(&sensitive_root),
        &test_temp_dir(),
        &[],
    );

    let _ = std::fs::remove_file(&tmp_path);
    let _ = std::fs::remove_dir_all(&sensitive_root);
    assert!(
        result.is_err(),
        "must reject an imported catalog whose folder path lies in a sensitive directory"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_harmless_file_rows() {
    let dir = unique_test_dir("validate_catalog_db_files_ok");
    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&dir.join("1-modell.3mf").to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[std::path::PathBuf::from("/etc")], &dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_ok(),
        "a legitimate backup must still import: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_file_name_with_path_traversal() {
    // `files.name` ends up in `trash_dir.join(...)`; "../" would write outside.
    let dir = unique_test_dir("validate_catalog_db_files_name");
    let bytes = catalog_db_bytes_with_file_row(
        "../../../.config/autostart/evil.desktop",
        &dir.join("modell.3mf").to_string_lossy(),
        None,
    );
    let result = validate_catalog_db_bytes(&bytes, &[], &dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "must reject an imported file row whose name escapes the trash dir"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_file_path_in_sensitive_directory() {
    // Created as a real directory, not just a path string - see
    // `..._rejects_folder_path_in_sensitive_directory`.
    let sensitive_root = unique_test_dir("3mf-test-sensitive-file-path");
    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &sensitive_root.join("modell.3mf").to_string_lossy(),
        None,
    );
    let result = validate_catalog_db_bytes(
        &bytes,
        std::slice::from_ref(&sensitive_root),
        &test_temp_dir(),
        &[],
    );
    let _ = std::fs::remove_dir_all(&sensitive_root);
    assert!(
        result.is_err(),
        "must reject an imported file row pointing into a sensitive directory"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_trash_path_outside_the_real_trash_dir() {
    // `~/Documents/...` is on no denylist, but `purge_expired_trash_on_startup`
    // would still have deleted the file on the next start.
    let dir = unique_test_dir("validate_catalog_db_trash_containment");
    let trash_dir = dir.join("trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");
    let victim = dir.join("irgendwas-wichtiges.pdf");
    std::fs::write(&victim, b"wichtig").expect("create victim file");

    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&victim.to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "must reject a trash_path outside the app's real trash directory: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_trash_path_inside_the_real_trash_dir() {
    // Counter-check: a genuine backup of this app must stay importable.
    let dir = unique_test_dir("validate_catalog_db_trash_containment_ok");
    let trash_dir = dir.join("trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");

    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&trash_dir.join("1-modell.3mf").to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_ok(),
        "a legitimate backup must still import: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_accepts_trash_path_under_sensitive_ancestor() {
    let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_ok");
    let sensitive_root = dir.join("sensitive");
    let trash_dir = sensitive_root.join("app/trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");

    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&trash_dir.join("1-modell.3mf").to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_ok(),
        "a legitimate backup must import with a sensitive trash ancestor: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_trash_path_outside_trash_under_sensitive_ancestor() {
    let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_outside");
    let sensitive_root = dir.join("sensitive");
    let trash_dir = sensitive_root.join("app/trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");

    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&sensitive_root.join("other/x.conf").to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "must reject a trash_path outside trash even under a sensitive ancestor: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_trash_path_escaping_to_sensitive_ancestor() {
    let dir = unique_test_dir("validate_catalog_db_trash_sensitive_ancestor_dotdot");
    let sensitive_root = dir.join("sensitive");
    let trash_dir = sensitive_root.join("app/trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");

    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&trash_dir.join("../../x.conf").to_string_lossy()),
    );
    let result = validate_catalog_db_bytes(&bytes, &[sensitive_root], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "must reject a trash_path escaping to a sensitive ancestor via parent components: {result:?}");
}

#[test]
fn validate_catalog_db_bytes_rejects_trash_path_escaping_the_trash_dir_via_parent_components() {
    // `<trash_dir>/../victim.pdf` passes the raw prefix comparison; the path is
    // therefore resolved first.
    let dir = unique_test_dir("validate_catalog_db_trash_containment_dotdot");
    let trash_dir = dir.join("trash");
    std::fs::create_dir_all(&trash_dir).expect("create trash dir");

    let escaping = format!("{}/../opfer.pdf", trash_dir.to_string_lossy());
    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&escaping),
    );
    let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir, &[]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "must reject a trash_path that escapes the trash dir via \"..\": {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_trash_path_in_sensitive_directory() {
    // `files.trash_path` goes straight to `fs::remove_file` - among others in
    // `purge_expired_trash_on_startup`, which runs at startup without any user
    // interaction.
    let dir = unique_test_dir("validate_catalog_db_trash_path");
    // Created as a real directory, not just a path string - see
    // `..._rejects_folder_path_in_sensitive_directory`.
    let sensitive_root = unique_test_dir("3mf-test-sensitive-trash-path");
    let bytes = catalog_db_bytes_with_file_row(
        "modell.3mf",
        &dir.join("modell.3mf").to_string_lossy(),
        Some(&sensitive_root.join("wichtig.conf").to_string_lossy()),
    );
    // trash_dir deliberately set to temp_dir: the crafted trash_path lies inside,
    // so the containment check does NOT apply - this test still covers exactly
    // the denylist.
    let result = validate_catalog_db_bytes(
        &bytes,
        std::slice::from_ref(&sensitive_root),
        &test_temp_dir(),
        &[],
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&sensitive_root);
    assert!(
        result.is_err(),
        "must reject an imported trash_path pointing into a sensitive directory"
    );
}

#[test]
fn validate_file_name_rejects_separators_and_parent_dir_sequences() {
    assert!(validate_file_name("modell.3mf").is_ok());
    assert!(validate_file_name("Modell mit Leerzeichen (v2).stl").is_ok());
    assert!(validate_file_name("").is_err());
    assert!(validate_file_name("   ").is_err());
    assert!(validate_file_name("../evil.3mf").is_err());
    assert!(validate_file_name("sub/evil.3mf").is_err());
    assert!(validate_file_name("sub\\evil.3mf").is_err());
    assert!(validate_file_name("..").is_err());
}

#[test]
fn reject_oversized_zip_entry_uses_the_declared_uncompressed_size() {
    assert!(reject_oversized_zip_entry("catalog.db", 10, 100).is_ok());
    assert!(reject_oversized_zip_entry("catalog.db", 100, 100).is_ok());
    let err = reject_oversized_zip_entry("catalog.db", 101, 100).unwrap_err();
    assert!(
        err.expected,
        "an oversized entry is about the user's archive, not a fault"
    );
    assert!(
        (err == CmdError::coded_with(
            ErrorCode::BackupEntryTooLarge,
            ["catalog.db", "1", "0"].iter().copied()
        )),
        "{err:?}"
    );
    // The size is rounded up to whole megabytes, so a 1.5 MB entry reads as 2 MB.
    let err =
        reject_oversized_zip_entry("settings.json", 3 * 1024 * 1024 / 2, 1024 * 1024).unwrap_err();
    assert!(
        (err == CmdError::coded_with(
            ErrorCode::BackupEntryTooLarge,
            ["settings.json", "2", "1"].iter().copied()
        )),
        "{err:?}"
    );
}

#[test]
fn read_catalog_archive_rejects_a_file_that_is_not_a_zip_at_all() {
    let err = read_catalog_archive(std::io::Cursor::new(b"not a zip file".to_vec())).unwrap_err();
    assert!(
        err.expected,
        "a non-ZIP file is the user's file, not a fault"
    );
    assert!(
        (err == CmdError::coded_with(ErrorCode::BackupNotZip, std::iter::empty::<&str>())),
        "{err:?}"
    );
}

#[test]
fn read_catalog_archive_rejects_a_zip_without_catalog_db() {
    let buf = zip_bytes_with(&[("settings.json", b"{}")]);
    let err = read_catalog_archive(std::io::Cursor::new(buf)).unwrap_err();
    assert!(
        err.expected,
        "a ZIP missing catalog.db is the user's file, not a fault"
    );
    assert!(
        (err == CmdError::coded_with(
            ErrorCode::BackupMissingEntry,
            ["catalog.db"].iter().copied()
        )),
        "{err:?}"
    );
}

#[test]
fn read_catalog_archive_rejects_a_zip_without_settings_json() {
    let buf = zip_bytes_with(&[("catalog.db", b"fake db bytes")]);
    let err = read_catalog_archive(std::io::Cursor::new(buf)).unwrap_err();
    assert!(
        err.expected,
        "a ZIP missing settings.json is the user's file, not a fault"
    );
    assert!(
        (err == CmdError::coded_with(
            ErrorCode::BackupMissingEntry,
            ["settings.json"].iter().copied()
        )),
        "{err:?}"
    );
}

/// Fixture written with the previous ZIP library and its default options,
/// exactly like `write_zip_atomically` produced it.
#[test]
fn read_catalog_archive_reads_a_backup_written_by_the_previous_library() {
    let bytes =
        include_bytes!("../../../../src-tauri/src/commands/testdata/backup_zip2.zip").to_vec();
    let (db_bytes, settings_json) =
        read_catalog_archive(std::io::Cursor::new(bytes)).expect("legacy backup readable");
    assert!(db_bytes.starts_with(b"SQLite format 3\0"));
    assert_eq!(settings_json, r#"{"language":"de","theme":"dark"}"#);
}

#[test]
fn read_catalog_archive_enforces_the_size_limit_on_real_entries() {
    let at_limit = vec![b' '; MAX_IMPORT_SETTINGS_BYTES as usize];
    let ok = zip_bytes_with(&[("catalog.db", b"x"), ("settings.json", &at_limit)]);
    assert!(read_catalog_archive(std::io::Cursor::new(ok)).is_ok());

    let too_big = vec![b' '; MAX_IMPORT_SETTINGS_BYTES as usize + 1];
    let bad = zip_bytes_with(&[("catalog.db", b"x"), ("settings.json", &too_big)]);
    let err = read_catalog_archive(std::io::Cursor::new(bad)).unwrap_err();
    assert!(err.expected);
    assert!(
        (err == CmdError::coded_with(
            ErrorCode::BackupEntryTooLarge,
            ["settings.json", "2", "1"].iter().copied()
        )),
        "{err:?}"
    );
}

#[test]
fn read_catalog_archive_returns_both_entries_when_present() {
    let buf = zip_bytes_with(&[
        ("catalog.db", b"fake db bytes"),
        ("settings.json", br#"{"a":1}"#),
    ]);
    let (db_bytes, settings_json) =
        read_catalog_archive(std::io::Cursor::new(buf)).expect("both entries present");
    assert_eq!(db_bytes, b"fake db bytes");
    assert_eq!(settings_json, r#"{"a":1}"#);
}

#[test]
fn validate_catalog_db_bytes_accepts_an_old_backup_without_the_kind_column() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL,
                    unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL,
                    slot_index INTEGER, home_location TEXT, color_hex TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01');",
            )
            .unwrap();
        // Before step 36 (catch-up step for `kind`); step 37 (resin printers) came
        // later and runs after it.
        assert!(version >= 36);
        conn.pragma_update(None, "user_version", 35).unwrap();
    });
    let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
    assert!(
        result.is_ok(),
        "aeltere Sicherung ohne kind muss gehen: {result:?}"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_resin_in_a_slot() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "INSERT INTO printers (name) VALUES ('X1C');
                 INSERT INTO material_units (printer_id, name, kind, slot_count) VALUES (1, 'AMS', 'bambu_ams', 4);
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind, unit_id, slot_index)
                    VALUES ('Standard', 1.75, 1000, 500, '2026-09-25', 'resin', 1, 0);",
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_an_unknown_kind_without_check_constraint() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL, unit_id INTEGER, slot_index INTEGER,
                    home_location TEXT, color_hex TEXT, kind TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01', 'pla');",
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn validate_catalog_db_bytes_accepts_a_resin_printer_with_a_bottle_in_its_vat() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (_, vat) = saturn_with_vat(conn);
        let bottle = resin_bottle(conn);
        crate::db::printers::load_spool(conn, bottle, vat, 0).unwrap();
    });
    let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
    assert!(
        result.is_ok(),
        "gueltiger Resin-Drucker muss durchgehen: {result:?}"
    );
}

/// Backup from v0.13.x/early v0.14.0 (user_version 36): `printers` without
/// `kind`, `material_units` with the old CHECK, one spool in a slot. Must be
/// migrated and accepted by step 37; the table rebuild must not throw the spool
/// out of its slot.
#[test]
fn validate_catalog_db_bytes_accepts_an_old_backup_without_printer_kind_and_keeps_loaded_spools() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE printer_jobs;
                 DROP TABLE printer_connections;
                 DROP TABLE material_units;
                 DROP TABLE printers;
                 CREATE TABLE printers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE material_units (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                                       'prusa_mmu3', 'anycubic_ace', 'external', 'custom')),
                    slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
                    bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
                    position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE UNIQUE INDEX idx_filament_spools_slot ON filament_spools (unit_id, slot_index) WHERE unit_id IS NOT NULL;
                 CREATE TABLE printer_connections (
                    printer_id INTEGER PRIMARY KEY REFERENCES printers(id) ON DELETE CASCADE,
                    kind TEXT NOT NULL CHECK (kind IN ('moonraker')),
                    address TEXT NOT NULL, base_url TEXT, remote_version TEXT,
                    connected_since REAL NOT NULL, last_synced_at REAL, last_error TEXT, error_since REAL,
                    paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1))
                 );
                 CREATE TABLE printer_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
                    remote_id TEXT NOT NULL, file_name TEXT NOT NULL,
                    outcome TEXT NOT NULL CHECK (outcome IN ('completed', 'partial')),
                    raw_status TEXT NOT NULL, ended_at REAL NOT NULL, print_duration_s REAL NOT NULL,
                    used_mm REAL NOT NULL, slicer_total_mm REAL, slicer_weight_g REAL, material TEXT,
                    thumbnail_path TEXT,
                    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'confirmed', 'ignored')),
                    booked_spool_id INTEGER REFERENCES filament_spools(id) ON DELETE SET NULL,
                    booked_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
                    booked_g REAL, decided_at TEXT,
                    UNIQUE (printer_id, remote_id)
                 );
                 INSERT INTO printers (id, name, position) VALUES (1, 'X1C', 0);
                 INSERT INTO material_units (id, printer_id, name, kind, slot_count, bambu_ams_index, position)
                     VALUES (10, 1, 'AMS A', 'bambu_ams', 4, 0, 0);
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index, home_location)
                     VALUES ('PLA', 1.75, 1000, 600, '2026-09-01', 10, 1, 'Regal 2');
                 INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (1, 'moonraker', '192.168.1.60', 1.0);",
            )
            .unwrap();
        conn.pragma_update(None, "user_version", 36).unwrap();
    });
    let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
    assert!(
        result.is_ok(),
        "alte Sicherung ohne printers.kind muss gehen: {result:?}"
    );

    // And the restore (same path: run_migrations on the copy) keeps the spool in
    // its slot.
    let path = unique_test_db_path("old_backup_restore_resin_step");
    std::fs::write(&path, &bytes).unwrap();
    let mut conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    crate::db::run_migrations(&mut conn).unwrap();
    let (kind, unit_id): (String, Option<i64>) = conn
            .query_row(
                "SELECT p.kind, s.unit_id FROM printers p, filament_spools s WHERE p.id = 1 AND s.material = 'PLA'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
    assert_eq!((kind.as_str(), unit_id), ("filament", Some(10)));
    drop(conn);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn validate_catalog_db_bytes_rejects_a_resin_bottle_in_a_filament_unit() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (_, unit_id) = printer_with_one_ams_unit(conn);
        let bottle = resin_bottle(conn);
        conn.execute(
            "UPDATE filament_spools SET unit_id = ?1, slot_index = 0 WHERE id = ?2",
            params![unit_id, bottle],
        )
        .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_filament_spool_in_a_vat() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (_, vat) = saturn_with_vat(conn);
        let spool = crate::db::insert_filament_spool(
            conn,
            &crate::db::models::NewFilamentSpool {
                kind: "filament".to_string(),
                material: "PLA".to_string(),
                ..resin_new()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE filament_spools SET unit_id = ?1, slot_index = 0 WHERE id = ?2",
            params![vat, spool],
        )
        .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_a_vat_on_a_filament_printer() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (printer, _) = printer_with_one_ams_unit(conn);
        conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (?1, 'Wanne', 'resin_vat', 1, 1)",
                params![printer],
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn validate_catalog_db_bytes_rejects_other_units_or_a_second_vat_on_a_resin_printer() {
    for extra in ["'AMS', 'bambu_ams', 4", "'Wanne 2', 'resin_vat', 1"] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let (printer, _) = saturn_with_vat(conn);
            conn.execute(
                    &format!("INSERT INTO material_units (printer_id, name, kind, slot_count, position) VALUES (?1, {extra}, 1)"),
                    params![printer],
                )
                .unwrap();
        });
        assert!(
            validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(),
            "{extra}"
        );
    }
}

#[test]
fn validate_catalog_db_bytes_rejects_a_vat_with_more_than_one_place_or_an_unknown_printer_kind() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (_, vat) = saturn_with_vat(conn);
        conn.execute(
            "UPDATE material_units SET slot_count = 2 WHERE id = ?1",
            params![vat],
        )
        .unwrap();
    });
    assert!(
        validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(),
        "Wanne mit 2 Plaetzen"
    );

    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        conn.execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        conn.execute(
            "INSERT INTO printers (name, kind, position) VALUES ('X', 'toast', 0)",
            [],
        )
        .unwrap();
    });
    assert!(
        validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err(),
        "unbekannte Druckerart"
    );
}

#[test]
fn validate_catalog_db_bytes_rejects_a_connection_for_a_resin_printer() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let (printer, _) = saturn_with_vat(conn);
        conn.execute(
                "INSERT INTO printer_connections (printer_id, kind, address, connected_since) VALUES (?1, 'moonraker', '192.168.1.60', 1.0)",
                params![printer],
            )
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

/// Valid catalog DB with exactly one adjusted `files` row, as raw bytes.
fn catalog_db_bytes_with_file_row(name: &str, path: &str, trash_path: Option<&str>) -> Vec<u8> {
    let tmp_path = test_temp_dir().join(format!(
        "validate_catalog_db_files_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
        let new_file = sample_new_file_for_rescan_test(std::path::Path::new(path));
        let id = db::insert_file(&mut conn, &new_file).expect("insert file row");
        conn.execute(
            "UPDATE files SET name = ?1, path = ?2, trash_path = ?3 WHERE id = ?4",
            rusqlite::params![name, path, trash_path, id],
        )
        .expect("patch file row");
        drop(conn);
    }
    let bytes = std::fs::read(&tmp_path).expect("read temp db");
    let _ = std::fs::remove_file(&tmp_path);
    bytes
}

fn colour_spool(colors: &[&str]) -> db::models::NewFilamentSpool {
    db::models::NewFilamentSpool {
        nozzle_temp_min: None,
        nozzle_temp_max: None,
        bed_temp_min: None,
        bed_temp_max: None,
        material: "PLA".into(),
        manufacturer: None,
        color: None,
        location: None,
        diameter_mm: 1.75,
        original_weight_g: 1000.0,
        remaining_weight_g: 500.0,
        price: None,
        image_png: None,
        color_hex: None,
        color_hexes: colors.iter().map(|c| c.to_string()).collect(),
        kind: "filament".into(),
    }
}

/// Freshly migrated catalog DB; `setup` can insert rows first.
fn backup_test_db_with(setup: impl FnOnce(&Connection)) -> (Vec<u8>, Vec<PathBuf>, PathBuf) {
    let tmp_path = test_temp_dir().join(format!(
        "backup_test_db_with_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
        setup(&conn);
    }
    let bytes = std::fs::read(&tmp_path).expect("read temp db");
    let _ = std::fs::remove_file(&tmp_path);
    (bytes, Vec::new(), test_temp_dir())
}

/// Prints as a crafted backup could bring them: a table without the CHECK clauses of ours.
fn jobs_table(rows: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
            "CREATE TABLE printers (id INTEGER, kind TEXT);
             CREATE TABLE printer_connections (printer_id INTEGER, kind TEXT, paused INTEGER, failures INTEGER, api_key TEXT);
             CREATE TABLE printer_jobs (outcome TEXT, state TEXT, auto_closed INTEGER, failure_reason TEXT, failure_note TEXT);
             {rows}"
        ))
        .unwrap();
    conn
}

fn unique_test_db_path(name: &str) -> std::path::PathBuf {
    test_temp_dir().join(format!(
        "{name}_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Runs the validation on a thread; a query that never ends fails the test
/// instead of hanging it.
fn validate_with_timeout(bytes: Vec<u8>) -> CmdResult<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(validate_catalog_db_bytes(
            &bytes,
            &[],
            &unique_test_dir("trash"),
            &[],
        ));
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("the validation must reject the database before running its SQL")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Migrated DB with one printer and one unit ("AMS A", 4 slots). Tests insert
/// invalid rows via raw SQL, the API would reject them.
fn printer_with_one_ams_unit(conn: &Connection) -> (i64, i64) {
    let printer_id = crate::db::printers::insert_printer(conn, "X1C").unwrap();
    let unit_id =
        crate::db::printers::insert_unit(conn, printer_id, "bambu_ams", "AMS A", None).unwrap();
    (printer_id, unit_id)
}

/// Recreate `material_units` without NOT NULL and CHECK, as in a crafted DB;
/// otherwise the INSERT itself would reject the test data.
fn drop_material_units_constraints(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE material_units;
             CREATE TABLE material_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                printer_id INTEGER,
                name TEXT,
                kind TEXT,
                slot_count INTEGER,
                bambu_ams_index INTEGER,
                position INTEGER
             );",
    )
    .unwrap();
}

/// Recreate `printers` with an untyped `name`: without affinity an INTEGER value
/// stays INTEGER instead of becoming TEXT.
fn drop_printers_constraints(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE printers;
             CREATE TABLE printers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name,
                position INTEGER,
                kind TEXT
             );",
    )
    .unwrap();
}

/// Recreate `filament_spools` without NOT NULL (see `drop_material_units_constraints`).
fn drop_filament_spools_constraints(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE filament_spools;
             CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL, original_weight_g INTEGER, remaining_weight_g INTEGER,
                price REAL, image_png BLOB, created_at TEXT,
                unit_id INTEGER, slot_index INTEGER, home_location TEXT, color_hex TEXT
             );",
    )
    .unwrap();
}

fn zip_bytes_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn resin_bottle(conn: &Connection) -> i64 {
    crate::db::insert_filament_spool(
        conn,
        &crate::db::models::NewFilamentSpool {
            nozzle_temp_min: None,
            nozzle_temp_max: None,
            bed_temp_min: None,
            bed_temp_max: None,
            material: "Standard".to_string(),
            manufacturer: None,
            color: Some("Grau".to_string()),
            location: Some("Resin-Schrank".to_string()),
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 620.0,
            price: None,
            image_png: None,
            color_hex: None,
            color_hexes: Vec::new(),
            kind: "resin".to_string(),
        },
    )
    .unwrap()
}

fn saturn_with_vat(conn: &Connection) -> (i64, i64) {
    let printer = crate::db::printers::insert_printer_of_kind(conn, "Saturn 4", "resin").unwrap();
    let vat = crate::db::printers::insert_resin_vat(conn, printer, "Harzwanne").unwrap();
    (printer, vat)
}

fn resin_new() -> crate::db::models::NewFilamentSpool {
    crate::db::models::NewFilamentSpool {
        nozzle_temp_min: None,
        nozzle_temp_max: None,
        bed_temp_min: None,
        bed_temp_max: None,
        material: "Standard".to_string(),
        manufacturer: None,
        color: None,
        location: Some("Regal".to_string()),
        diameter_mm: 1.75,
        original_weight_g: 1000.0,
        remaining_weight_g: 500.0,
        price: None,
        image_png: None,
        color_hex: None,
        color_hexes: Vec::new(),
        kind: "resin".to_string(),
    }
}

fn sample_new_file_for_rescan_test(path: &std::path::Path) -> NewFile {
    NewFile {
        name: "missing.3mf".to_string(),
        path: path.to_string_lossy().to_string(),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes: 0,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: "2026-09-13T00:00:00Z".to_string(),
        file_modified_at: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: None,
        content_hash: None,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        source: Default::default(),
        queue_position: None,
        favorite: false,
        plate_count: None,
        plate_dimensions_json: None,
        slice_info_json: None,
        print_info: None,
    }
}

#[test]
fn catalog_license_validation_accepts_old_exports_and_validates_new_values() {
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
        conn.execute_batch("ALTER TABLE files DROP COLUMN license")
            .unwrap();
        // The license step is the one before the source-column step.
        conn.pragma_update(None, "user_version", 46).unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_ok());
    for (value, valid) in [
        (None, true),
        (Some("cc-by".to_string()), true),
        (Some(" Free license ".to_string()), true),
        (Some("é".repeat(200)), true),
        (Some("a".repeat(201)), false),
        (Some("bad\0text".to_string()), false),
    ] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
            conn.execute(
                "UPDATE files SET license = ?1 WHERE id = ?2",
                rusqlite::params![value, id],
            )
            .unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert_eq!(result.is_ok(), valid, "{result:?}");
    }
    let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
        let id = crate::db::test_insert_minimal_file(conn, "/model.3mf", None).unwrap();
        conn.execute("UPDATE files SET license = X'1234' WHERE id = ?1", [id])
            .unwrap();
    });
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
}

#[test]
fn an_imported_color_list_must_start_with_the_single_color() {
    for (single, valid) in [
        (Some("#ff0000"), true),
        (Some("#FF0000"), true),
        (Some("#0000ff"), false),
        (None, false),
    ] {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            db::insert_filament_spool(conn, &colour_spool(&[])).unwrap();
            conn.execute(
                    "UPDATE filament_spools SET color_hexes = '[\"#ff0000\",\"#00ff00\"]', color_hex = ?1",
                    [single],
                )
                .unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]);
        assert_eq!(result.is_ok(), valid, "{single:?}: {result:?}");
    }
}

#[test]
fn validate_backup_for_import_reports_a_rejected_backup_as_expected() {
    // Exercises the real boundary `import_catalog` calls, not a hand-copied
    // conversion: a bad backup file is the user's problem, not a fault, so it
    // must come out `expected`, not the default `expected: false` of a plain
    // String, and it is reported by its code.
    let err =
        validate_catalog_db_bytes(b"this is not a sqlite database", &[], &test_temp_dir(), &[])
            .unwrap_err();
    assert!(
        err.expected,
        "a rejected backup file must be reported as expected, not a fault"
    );
    assert!(
        (err == CmdError::coded_with(ErrorCode::BackupInvalidDatabase, std::iter::empty::<&str>())),
        "{err:?}"
    );
}

/// A backup whose print data tables carry no primary key, foreign key, NOT NULL or CHECK
/// (as a crafted file can), with one valid 3MF file (id 1) and one STL file (id 2); `setup`
/// adds the rows under test.
fn print_data_template() -> &'static [u8] {
    // Creating a catalog is slow, so the base database is built once and copied per case.
    static TEMPLATE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    TEMPLATE.get_or_init(|| {
            backup_test_db_with(|conn| {
                conn.execute_batch(
                    "DROP TABLE file_print_plates;
                     DROP TABLE file_print_info;
                     CREATE TABLE file_print_info (
                        file_id INTEGER, reader_version INTEGER, read_at TEXT, slice_state TEXT, settings_state TEXT,
                        printer_model TEXT, bed_type TEXT, layer_height_mm REAL, nozzle_mm REAL, nozzle_mixed INTEGER,
                        bed_temp_c REAL, bed_temp_mixed INTEGER, nozzle_temp_c REAL, nozzle_temp_mixed INTEGER,
                        filament_colors TEXT);
                     CREATE TABLE file_print_plates (
                        file_id INTEGER, plate_index INTEGER, plate_name TEXT, print_seconds INTEGER,
                        first_layer_seconds REAL, plate_bed_type TEXT);",
                )
                .unwrap();
                crate::db::test_insert_minimal_file(conn, "/tmp/a.3mf", None).unwrap();
                crate::db::test_insert_minimal_file(conn, "/tmp/b.stl", None).unwrap();
                conn.execute("UPDATE files SET file_type = 'stl' WHERE id = 2", []).unwrap();
            })
            .0
        })
}

/// Runs `setup` on a copy of the template and returns (the explicit print data check on
/// that database alone, the verdict of the whole restore validation on its bytes).
fn print_data_case(setup: impl FnOnce(&Connection)) -> (Result<(), String>, Result<(), String>) {
    let path = unique_test_db_path("print_data_case");
    std::fs::write(&path, print_data_template()).unwrap();
    let explicit = {
        let conn = Connection::open(&path).unwrap();
        setup(&conn);
        validate_print_info_rows(&conn)
    };
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let restore =
        validate_catalog_db_bytes(&bytes, &[], &test_temp_dir(), &[]).map_err(|e| e.message);
    (explicit, restore)
}

/// `INSERT INTO file_print_info` with valid defaults, some columns overridden.
fn info_row(conn: &Connection, file_id: i64, overrides: &[(&str, &str)]) {
    let mut columns: Vec<(&str, String)> = vec![
        ("file_id", file_id.to_string()),
        ("reader_version", "1".into()),
        ("read_at", "'2026-10-31T10:00:00+00:00'".into()),
        ("slice_state", "'sliced'".into()),
        ("settings_state", "'read'".into()),
        ("nozzle_mixed", "0".into()),
        ("bed_temp_mixed", "0".into()),
        ("nozzle_temp_mixed", "0".into()),
    ];
    for (name, value) in overrides {
        match columns.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = value.to_string(),
            None => columns.push((name, value.to_string())),
        }
    }
    let names: Vec<_> = columns.iter().map(|(n, _)| *n).collect();
    let values: Vec<_> = columns.iter().map(|(_, v)| v.as_str()).collect();
    conn.execute(
        &format!(
            "INSERT INTO file_print_info ({}) VALUES ({})",
            names.join(", "),
            values.join(", ")
        ),
        [],
    )
    .unwrap();
}

fn plate_row(conn: &Connection, file_id: i64, index: &str, overrides: &[(&str, &str)]) {
    let mut columns: Vec<(&str, String)> = vec![
        ("file_id", file_id.to_string()),
        ("plate_index", index.to_string()),
    ];
    for (name, value) in overrides {
        columns.push((name, value.to_string()));
    }
    let names: Vec<_> = columns.iter().map(|(n, _)| *n).collect();
    let values: Vec<_> = columns.iter().map(|(_, v)| v.as_str()).collect();
    conn.execute(
        &format!(
            "INSERT INTO file_print_plates ({}) VALUES ({})",
            names.join(", "),
            values.join(", ")
        ),
        [],
    )
    .unwrap();
}

#[test]
fn a_backup_with_valid_print_data_but_no_constraints_is_accepted() {
    let (explicit, restore) = print_data_case(|conn| {
        info_row(
            conn,
            1,
            &[
                ("printer_model", "'X1'"),
                ("layer_height_mm", "0.2"),
                ("nozzle_temp_c", "210"),
                ("bed_temp_c", "60"),
                ("nozzle_mixed", "1"),
                ("filament_colors", "'#ff0000 #00ff00'"),
            ],
        );
        plate_row(
            conn,
            1,
            "1",
            &[
                ("print_seconds", "600"),
                ("first_layer_seconds", "12.5"),
                ("plate_name", "'A'"),
            ],
        );
        plate_row(conn, 1, "2", &[]);
    });
    assert!(explicit.is_ok(), "{explicit:?}");
    assert!(restore.is_ok(), "{restore:?}");
}

#[test]
fn a_backup_with_a_gcode_file_and_its_print_data_is_accepted() {
    let (explicit, restore) = print_data_case(|conn| {
        crate::db::test_insert_minimal_file(conn, "/tmp/c.gcode", None).unwrap();
        conn.execute("UPDATE files SET file_type = 'gcode' WHERE id = 3", [])
            .unwrap();
        info_row(
            conn,
            3,
            &[("printer_model", "'MK4'"), ("layer_height_mm", "0.2")],
        );
        plate_row(conn, 3, "1", &[("print_seconds", "4200")]);
    });
    assert!(explicit.is_ok(), "{explicit:?}");
    assert!(restore.is_ok(), "{restore:?}");
}

#[test]
fn a_gcode_row_with_a_reader_version_from_a_newer_build_is_rejected_but_a_3mf_row_of_that_version_is_not(
) {
    // The G-code reader and the 3MF reader count their versions separately: version 2 does
    // not exist for G-code yet, but it does for 3MF.
    let too_new = (crate::gcode::GCODE_READER_VERSION + 1).to_string();
    let (explicit, restore) = print_data_case(|conn| {
        crate::db::test_insert_minimal_file(conn, "/tmp/e.gcode", None).unwrap();
        conn.execute("UPDATE files SET file_type = 'gcode' WHERE id = 3", [])
            .unwrap();
        info_row(conn, 3, &[("reader_version", too_new.as_str())]);
    });
    assert!(explicit.is_err(), "{explicit:?}");
    assert!(restore.is_err());
    let (explicit, restore) =
        print_data_case(|conn| info_row(conn, 1, &[("reader_version", too_new.as_str())]));
    assert!(explicit.is_ok(), "{explicit:?}");
    assert!(restore.is_ok(), "{restore:?}");
}

#[test]
fn a_backup_with_resin_and_gcode_rows_but_print_data_for_a_resin_file_is_rejected() {
    let (explicit, restore) = print_data_case(|conn| {
        crate::db::test_insert_minimal_file(conn, "/tmp/d.ctb", None).unwrap();
        conn.execute("UPDATE files SET file_type = 'resin' WHERE id = 3", [])
            .unwrap();
        info_row(conn, 3, &[]);
    });
    assert!(
        explicit.is_err(),
        "print data belongs to 3MF and G-Code files only"
    );
    assert!(restore.is_err());
}

#[test]
fn a_backup_with_damaged_print_data_is_rejected_by_the_explicit_check() {
    let many_plates = |conn: &Connection| {
        for i in 1..=1025 {
            plate_row(conn, 1, &i.to_string(), &[]);
        }
    };
    type Setup = Box<dyn Fn(&Connection)>;
    let cases: Vec<(&str, Setup)> = vec![
        (
            "info row of a file that does not exist",
            Box::new(|c| info_row(c, 999, &[])),
        ),
        (
            "plate row of a file that does not exist",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 999, "1", &[])
            }),
        ),
        (
            "info row of a file that is no 3MF or G-Code",
            Box::new(|c| info_row(c, 2, &[])),
        ),
        (
            "two info rows for one file",
            Box::new(|c| {
                info_row(c, 1, &[]);
                info_row(c, 1, &[])
            }),
        ),
        (
            "two plate rows with the same index",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "2", &[]);
                plate_row(c, 1, "2", &[])
            }),
        ),
        (
            "plate index 0",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "0", &[])
            }),
        ),
        (
            "plate index -3",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "-3", &[])
            }),
        ),
        (
            "more plates than allowed",
            Box::new(move |c| {
                info_row(c, 1, &[]);
                many_plates(c)
            }),
        ),
        (
            "unknown slice state",
            Box::new(|c| info_row(c, 1, &[("slice_state", "'x'")])),
        ),
        (
            "unknown settings state",
            Box::new(|c| info_row(c, 1, &[("settings_state", "'x'")])),
        ),
        (
            "reader version from a newer build",
            Box::new(|c| info_row(c, 1, &[("reader_version", "99")])),
        ),
        (
            "reader version 0",
            Box::new(|c| info_row(c, 1, &[("reader_version", "0")])),
        ),
        (
            "flag outside 0 and 1",
            Box::new(|c| info_row(c, 1, &[("nozzle_mixed", "2")])),
        ),
        (
            "nozzle temperature without print data",
            Box::new(|c| {
                info_row(
                    c,
                    1,
                    &[("slice_state", "'no_slice_data'"), ("nozzle_temp_c", "210")],
                )
            }),
        ),
        (
            "bed temperature in an unreadable file",
            Box::new(|c| {
                info_row(
                    c,
                    1,
                    &[("slice_state", "'unreadable'"), ("bed_temp_c", "60")],
                )
            }),
        ),
        (
            "mixed temperature flag without print data",
            Box::new(|c| {
                info_row(
                    c,
                    1,
                    &[
                        ("slice_state", "'no_slice_data'"),
                        ("nozzle_temp_mixed", "1"),
                    ],
                )
            }),
        ),
        (
            "print time without print data",
            Box::new(|c| {
                info_row(c, 1, &[("slice_state", "'no_slice_data'")]);
                plate_row(c, 1, "1", &[("print_seconds", "600")])
            }),
        ),
        (
            "mixed flag with a number",
            Box::new(|c| info_row(c, 1, &[("nozzle_mixed", "1"), ("nozzle_mm", "0.4")])),
        ),
        (
            "layer height below zero",
            Box::new(|c| info_row(c, 1, &[("layer_height_mm", "-1")])),
        ),
        (
            "layer height 0",
            Box::new(|c| info_row(c, 1, &[("layer_height_mm", "0")])),
        ),
        (
            "nozzle 9 mm",
            Box::new(|c| info_row(c, 1, &[("nozzle_mm", "9")])),
        ),
        (
            "temperature 600",
            Box::new(|c| info_row(c, 1, &[("bed_temp_c", "600")])),
        ),
        (
            "print time 0",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "1", &[("print_seconds", "0")])
            }),
        ),
        (
            "print time below zero",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "1", &[("print_seconds", "-5")])
            }),
        ),
        (
            "first layer time 0",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "1", &[("first_layer_seconds", "0")])
            }),
        ),
        (
            "bed type with a control character",
            Box::new(|c| info_row(c, 1, &[("bed_type", "'x' || char(7)")])),
        ),
        (
            "printer model with 10000 characters",
            Box::new(|c| info_row(c, 1, &[("printer_model", "printf('%.*c', 10000, 'a')")])),
        ),
        (
            "plate name with a control character",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(c, 1, "1", &[("plate_name", "'a' || char(10)")])
            }),
        ),
        (
            "plate bed type with 10000 characters",
            Box::new(|c| {
                info_row(c, 1, &[]);
                plate_row(
                    c,
                    1,
                    "1",
                    &[("plate_bed_type", "printf('%.*c', 10000, 'a')")],
                )
            }),
        ),
        (
            "read time that is no date",
            Box::new(|c| info_row(c, 1, &[("read_at", "'garbage'")])),
        ),
        (
            "filament colors stored as a blob",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "x'23666630303030'")])),
        ),
        (
            "filament colors that are only a number",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "5")])),
        ),
        (
            "filament colors with 300 characters",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "printf('%.*c', 300, 'a')")])),
        ),
        (
            "filament colors that are no color",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'red'")])),
        ),
        (
            "filament colors with an empty text",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "''")])),
        ),
        (
            "33 filament colors",
            Box::new(|c| {
                info_row(
                    c,
                    1,
                    &[(
                        "filament_colors",
                        "trim(replace(printf('%.*c', 33, 'x'), 'x', '#aaaaaa '))",
                    )],
                )
            }),
        ),
        (
            "filament colors with a comma",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'#aaaaaa,#bbbbbb'")])),
        ),
        (
            "filament colors with a double space",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'#aaaaaa  #bbbbbb'")])),
        ),
        (
            "filament colors with a trailing space",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'#aaaaaa '")])),
        ),
        (
            "filament colors with alpha",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'#aaaaaaff'")])),
        ),
        (
            "filament colors in capitals",
            Box::new(|c| info_row(c, 1, &[("filament_colors", "'#AAAAAA'")])),
        ),
    ];
    for (label, setup) in &cases {
        // The restore refuses the backup, and it is the explicit check that does it
        // (the other checks of the restore know nothing about these rules).
        let (explicit, restore) = print_data_case(|conn| setup(conn));
        assert!(restore.is_err(), "restore must refuse: {label}");
        let message = explicit.expect_err(label);
        assert!(message.contains("Druckdaten"), "{label}: {message}");
    }
}

#[test]
fn foreign_key_check_alone_does_not_see_orphans_in_a_table_without_keys() {
    // Why `validate_print_info_rows` exists: the table of a crafted backup declares no key.
    let (bytes, _, _) = backup_test_db_with(|conn| {
        conn.execute_batch(
                "DROP TABLE file_print_plates;
                 CREATE TABLE file_print_plates (file_id INTEGER, plate_index INTEGER, plate_name TEXT, print_seconds INTEGER, first_layer_seconds REAL, plate_bed_type TEXT);
                 INSERT INTO file_print_plates (file_id, plate_index) VALUES (999, 1);",
            )
            .unwrap();
    });
    let path = unique_test_db_path("print_data_fk_probe");
    std::fs::write(&path, &bytes).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0, "foreign_key_check sees nothing");
    let _ = std::fs::remove_file(path);
    assert!(
        validate_print_info_rows(&conn).is_err(),
        "the explicit check does"
    );
}

/// A backup whose bundle tables carry no key and no CHECK (as a crafted file can), with two
/// files (ids 1 and 2) and one valid bundle (id 1) of both; `setup` adds the rows under test.
fn bundle_template() -> &'static [u8] {
    static TEMPLATE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    TEMPLATE.get_or_init(|| {
            backup_test_db_with(|conn| {
                conn.execute_batch(
                    "DROP TABLE bundle_members;
                     DROP TABLE bundles;
                     CREATE TABLE bundles (id INTEGER, name TEXT, kind TEXT, created_at TEXT);
                     CREATE TABLE bundle_members (file_id INTEGER, bundle_id INTEGER, position INTEGER, label TEXT);",
                )
                .unwrap();
                crate::db::test_insert_minimal_file(conn, "/tmp/a.3mf", None).unwrap();
                crate::db::test_insert_minimal_file(conn, "/tmp/b.3mf", None).unwrap();
            })
            .0
        })
}

/// Runs `setup` on a copy of the template and returns (the explicit bundle check on that
/// database alone, the verdict of the whole restore validation on its bytes).
fn bundle_case(setup: &str) -> (Result<(), String>, Result<(), String>) {
    let path = unique_test_db_path("bundle_case");
    std::fs::write(&path, bundle_template()).unwrap();
    let explicit = {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(setup).unwrap();
        validate_bundle_rows(&conn)
    };
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let restore =
        validate_catalog_db_bytes(&bytes, &[], &test_temp_dir(), &[]).map_err(|e| e.message);
    (explicit, restore)
}

#[test]
fn the_bundle_template_with_a_valid_bundle_is_accepted() {
    let (explicit, restore) = bundle_case(GOOD_BUNDLE);
    assert_eq!(explicit, Ok(()));
    assert_eq!(restore, Ok(()));
}

#[test]
fn a_bundle_with_fewer_than_two_members_is_accepted_because_the_app_can_leave_one() {
    for setup in [
            "INSERT INTO bundles VALUES (1, 'Alone', 'parts', 't'); INSERT INTO bundle_members VALUES (1, 1, 0, NULL);",
            "INSERT INTO bundles VALUES (1, 'Empty', 'variants', 't');",
        ] {
            let (explicit, restore) = bundle_case(setup);
            assert_eq!(explicit, Ok(()), "{setup}");
            assert_eq!(restore, Ok(()), "{setup}");
        }
}

#[test]
fn crafted_bundle_rows_are_rejected() {
    let many = {
        let members: Vec<String> = (0..201)
            .map(|i| format!("({}, 1, {i}, NULL)", 100 + i))
            .collect();
        format!("INSERT INTO bundles VALUES (1, 'Big', 'parts', 't'); INSERT INTO bundle_members VALUES {};", members.join(", "))
    };
    let cases: Vec<(&str, String)> = vec![
            ("unknown kind", "INSERT INTO bundles VALUES (1, 'A', 'other', 't');".into()),
            ("kind NULL", "INSERT INTO bundles VALUES (1, 'A', NULL, 't');".into()),
            ("member without a file", "INSERT INTO bundles VALUES (1, 'A', 'parts', 't'); INSERT INTO bundle_members VALUES (999, 1, 0, NULL);".into()),
            ("member without a bundle", "INSERT INTO bundle_members VALUES (1, 77, 0, NULL);".into()),
            ("file without a number", "INSERT INTO bundles VALUES (1, 'A', 'parts', 't'); INSERT INTO bundle_members VALUES (NULL, 1, 0, NULL);".into()),
            ("name empty", "INSERT INTO bundles VALUES (1, '  ', 'parts', 't');".into()),
            ("name with a control character", "INSERT INTO bundles VALUES (1, 'A' || char(7), 'parts', 't');".into()),
            ("name too long", format!("INSERT INTO bundles VALUES (1, '{}', 'parts', 't');", "n".repeat(121))),
            ("label too long", format!("{GOOD_BUNDLE} UPDATE bundle_members SET label = '{}' WHERE file_id = 1;", "l".repeat(61))),
            ("label with a control character", format!("{GOOD_BUNDLE} UPDATE bundle_members SET label = 'a' || char(10) || 'b' WHERE file_id = 1;")),
            ("file in two bundles", format!("{GOOD_BUNDLE} INSERT INTO bundles VALUES (2, 'B', 'parts', 't'); INSERT INTO bundle_members VALUES (1, 2, 0, NULL);")),
            ("negative position", format!("{GOOD_BUNDLE} UPDATE bundle_members SET position = -1 WHERE file_id = 2;")),
            ("201 members", many),
        ];
    for (what, setup) in cases {
        let (_, restore) = bundle_case(&setup);
        assert!(
            restore.is_err(),
            "{what}: the restore validation must refuse it"
        );
    }
}

/// A backup whose `slicer_reports` carries no CHECK (as a crafted file can); `setup` adds the rows under test.
fn slicer_report_case(setup: &str) -> Result<(), String> {
    static TEMPLATE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let template = TEMPLATE.get_or_init(|| {
            backup_test_db_with(|conn| {
                conn.execute_batch(
                    "DROP TABLE slicer_reports;
                     CREATE TABLE slicer_reports (id INTEGER PRIMARY KEY, reported_at TEXT, reported_path TEXT, file_name TEXT,
                         file_size_bytes INTEGER, slicer TEXT, summary_json TEXT, state TEXT, suggested_file_id INTEGER,
                         model_file_id INTEGER, decided_at TEXT);",
                )
                .unwrap();
            })
            .0
        });
    let path = unique_test_db_path("slicer_report_case");
    std::fs::write(&path, template).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(setup).unwrap();
    }
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    validate_catalog_db_bytes(&bytes, &[], &test_temp_dir(), &[]).map_err(|e| e.message)
}

#[test]
fn slicer_reports_survive_the_restore_check_and_crafted_ones_do_not() {
    assert_eq!(slicer_report_case(GOOD_REPORT), Ok(()));
    let cases: Vec<(&str, String)> = vec![
        (
            "unknown state",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET state = 'done';"),
        ),
        (
            "negative size",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET file_size_bytes = -1;"),
        ),
        (
            "size NULL",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET file_size_bytes = NULL;"),
        ),
        (
            "control character in the path",
            format!(
                "{GOOD_REPORT} UPDATE slicer_reports SET reported_path = '/o/a' || char(7) || 'b';"
            ),
        ),
        (
            "name empty",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET file_name = '';"),
        ),
        (
            "path too long",
            format!(
                "{GOOD_REPORT} UPDATE slicer_reports SET reported_path = '/{}';",
                "p".repeat(1100)
            ),
        ),
        (
            "time not a time",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET reported_at = 'gestern';"),
        ),
        (
            "summary not JSON",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET summary_json = 'x';"),
        ),
        (
            "summary too large",
            format!(
                "{GOOD_REPORT} UPDATE slicer_reports SET summary_json = '{{\"x\":\"{}\"}}';",
                "a".repeat(17_000)
            ),
        ),
        (
            "name stored as a BLOB",
            format!("{GOOD_REPORT} UPDATE slicer_reports SET file_name = x'4142';"),
        ),
    ];
    for (what, setup) in cases {
        assert!(
            slicer_report_case(&setup).is_err(),
            "{what}: the restore validation must refuse it"
        );
    }
}

#[test]
fn crafted_bundle_rows_are_refused_by_the_explicit_check_itself() {
    for (what, setup) in [
            ("unknown kind", "INSERT INTO bundles VALUES (1, 'A', 'other', 't');"),
            ("member without a file", "INSERT INTO bundles VALUES (1, 'A', 'parts', 't'); INSERT INTO bundle_members VALUES (999, 1, 0, NULL);"),
            ("file in two bundles", "INSERT INTO bundles VALUES (1, 'A', 'parts', 't'), (2, 'B', 'parts', 't'); INSERT INTO bundle_members VALUES (1, 1, 0, NULL), (1, 2, 0, NULL);"),
            ("negative position", "INSERT INTO bundles VALUES (1, 'A', 'parts', 't'); INSERT INTO bundle_members VALUES (1, 1, -3, NULL);"),
            ("control character in the name", "INSERT INTO bundles VALUES (1, 'A' || char(7), 'parts', 't');"),
        ] {
            let (explicit, _) = bundle_case(setup);
            let message = explicit.expect_err(what);
            assert!(message.contains("Buendel"), "{what}: {message}");
        }
}

#[test]
fn a_label_stored_as_a_blob_is_refused_by_the_column_types() {
    let setup =
        format!("{GOOD_BUNDLE} UPDATE bundle_members SET label = x'4c6964' WHERE file_id = 2;");
    let path = unique_test_db_path("bundle_blob_label");
    std::fs::write(&path, bundle_template()).unwrap();
    let types = {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&setup).unwrap();
        validate_column_types(&conn)
    };
    let _ = std::fs::remove_file(&path);
    let message = types.expect_err("a BLOB label");
    assert!(
        message.contains("bundle_members") && message.contains("label"),
        "{message}"
    );
    assert!(
        bundle_case(&setup).1.is_err(),
        "and the whole restore validation refuses it"
    );
}

#[test]
fn column_types_covers_every_table_in_required_columns() {
    for (table, _) in REQUIRED_COLUMNS {
        assert!(
                COLUMN_TYPES.iter().any(|(t, _, _, _)| t == table),
                "COLUMN_TYPES hat keinen Eintrag fuer Tabelle '{table}' aus REQUIRED_COLUMNS - neue Tabellen muessen hier ergaenzt werden"
            );
    }
}

#[test]
fn column_types_covers_known_non_optional_columns_read_as_rust_values() {
    // COLUMN_TYPES must contain at least these typed columns.
    let must_have: &[(&str, &str)] = &[
        ("files", "name"),
        ("files", "path"),
        ("files", "file_type"),
        ("files", "imported_at"),
        ("files", "favorite"),
        ("folders", "name"),
        ("folders", "path"),
        ("tags", "name"),
        ("tags", "color_hue"),
        ("file_metadata", "label"),
        ("file_metadata", "value"),
        ("filament_spools", "material"),
        ("filament_spools", "diameter_mm"),
        ("filament_spools", "original_weight_g"),
        ("filament_spools", "remaining_weight_g"),
        ("printers", "name"),
        ("printers", "kind"),
        ("material_units", "printer_id"),
        ("material_units", "name"),
        ("material_units", "kind"),
        ("material_units", "slot_count"),
        ("collections", "name"),
        ("registered_slicers", "name"),
        ("registered_slicers", "executable_path"),
        ("registered_slicers", "is_auto_detected"),
        ("file_tags", "file_id"),
        ("collection_files", "file_id"),
    ];
    for (table, column) in must_have {
        assert!(
            COLUMN_TYPES
                .iter()
                .any(|(t, c, _, _)| t == table && c == column),
            "COLUMN_TYPES hat keinen Eintrag fuer '{table}.{column}'"
        );
    }
}

/// The bytes of a catalog database after `setup` ran on it, plus the trash folder to validate against.
fn catalog_bytes(setup: impl FnOnce(&Connection)) -> (Vec<u8>, PathBuf) {
    let dir = unique_test_dir("backup_companion_bytes");
    let path = dir.join("catalog.db");
    {
        let conn = crate::db::connect(&path).unwrap();
        setup(&conn);
    }
    (std::fs::read(&path).unwrap(), dir.join("trash"))
}

fn with_companion(conn: &Connection, name: &str, path: &str, kind: &str, trash_path: Option<&str>) {
    // A crafted backup need not carry the CHECK constraints.
    conn.pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    let id = crate::db::test_insert_minimal_file(conn, "/catalog/Foo.3mf", None).unwrap();
    conn.execute(
            "INSERT INTO companion_files (file_id, name, path, kind, added_at, deleted_at, trash_path) VALUES (?1, ?2, ?3, ?4, 'now', ?5, ?6)",
            rusqlite::params![id, name, path, kind, trash_path.map(|_| "2026-01-01T00:00:00+00:00"), trash_path],
        )
        .unwrap();
}

fn validate(bytes: &[u8], trash: &Path) -> CmdResult<()> {
    validate_catalog_db_bytes(bytes, &[], trash, &[])
}

#[test]
fn a_well_formed_companion_row_is_accepted() {
    let (bytes, trash) = catalog_bytes(|conn| {
        with_companion(
            conn,
            "pic.png",
            "/catalog/Foo_extras/pic.png",
            "image",
            None,
        )
    });
    assert!(validate(&bytes, &trash).is_ok());
}

#[test]
fn crafted_companion_rows_are_rejected() {
    let cases: [(&str, &str, &str, &str); 7] = [
        (
            "not an _extras folder",
            "secret.png",
            "/home/user/.ssh/secret.png",
            "image",
        ),
        (
            "name does not match the path",
            "other.png",
            "/catalog/Foo_extras/pic.png",
            "image",
        ),
        (
            "a kind that does not exist",
            "pic.png",
            "/catalog/Foo_extras/pic.png",
            "exe",
        ),
        (
            "kind and extension disagree",
            "pic.png",
            "/catalog/Foo_extras/pic.png",
            "pdf",
        ),
        (
            "an executable as an image",
            "tool.exe",
            "/catalog/Foo_extras/tool.exe",
            "image",
        ),
        (
            "a parent reference",
            "pic.png",
            "/catalog/Foo_extras/../pic.png",
            "image",
        ),
        ("a relative path", "pic.png", "Foo_extras/pic.png", "image"),
    ];
    for (why, name, path, kind) in cases {
        let (bytes, trash) = catalog_bytes(|conn| with_companion(conn, name, path, kind, None));
        assert!(validate(&bytes, &trash).is_err(), "{why}");
    }
}

#[test]
fn a_companion_row_in_a_protected_folder_is_rejected() {
    let (bytes, trash) = catalog_bytes(|conn| {
        with_companion(
            conn,
            "a.png",
            "/home/user/.config/autostart_extras/a.png",
            "image",
            None,
        )
    });
    assert!(
        validate_catalog_db_bytes(&bytes, &[PathBuf::from("/home/user/.config")], &trash, &[])
            .is_err()
    );
    assert!(
        validate(&bytes, &trash).is_ok(),
        "counter-check: allowed when nothing is protected there"
    );
}

#[test]
fn a_companion_trash_path_must_lie_inside_the_trash_folder() {
    let (bytes, trash) = catalog_bytes(|conn| {
        with_companion(
            conn,
            "pic.png",
            "/catalog/Foo_extras/pic.png",
            "image",
            Some("/home/user/Documents/precious.png"),
        )
    });
    assert!(
        validate(&bytes, &trash).is_err(),
        "a restored backup must not make the app delete a document"
    );
    let inside = trash.join("companion-1-pic.png");
    let (bytes, trash) = catalog_bytes(|conn| {
        with_companion(
            conn,
            "pic.png",
            "/catalog/Foo_extras/pic.png",
            "image",
            Some(&inside.to_string_lossy()),
        )
    });
    let _ = trash;
    assert!(
        validate(&bytes, inside.parent().unwrap()).is_ok(),
        "counter-check: inside the trash it is fine"
    );
}

#[test]
fn crafted_blocked_tag_rows_are_rejected() {
    let long = "x".repeat(101);
    for (why, name) in [
        ("too long", long.as_str()),
        ("control character", "a\u{7}b"),
        ("empty", "  "),
    ] {
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute(
                "INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (?1, 't')",
                [name],
            )
            .unwrap();
        });
        assert!(validate(&bytes, &trash).is_err(), "{why}");
    }
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute(
            "INSERT INTO blocked_auto_tags (name, blocked_at) VALUES ('miniatur', 't')",
            [],
        )
        .unwrap();
    });
    assert!(
        validate(&bytes, &trash).is_ok(),
        "counter-check: a normal entry passes"
    );
}

#[test]
fn a_backup_with_too_many_blocked_tags_is_rejected() {
    let limit = crate::db::blocked_tags::MAX_BLOCKED_TAGS;
    for (rows, accepted) in [(limit, true), (limit + 1, false)] {
        let (bytes, trash) = catalog_bytes(|conn| {
            // One transaction: a commit per row makes this test take a minute.
            conn.execute_batch("BEGIN").unwrap();
            for i in 0..rows {
                conn.execute(
                    "INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (?1, 't')",
                    [format!("tag{i}")],
                )
                .unwrap();
            }
            conn.execute_batch("COMMIT").unwrap();
        });
        assert_eq!(validate(&bytes, &trash).is_ok(), accepted, "{rows} rows");
    }
}

#[test]
fn wrong_storage_classes_in_blocked_auto_tags_are_rejected() {
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute(
            "INSERT INTO blocked_auto_tags (name, blocked_at) VALUES (X'1234', 't')",
            [],
        )
        .unwrap();
    });
    assert!(validate(&bytes, &trash).is_err());
}

#[test]
fn wrong_storage_classes_in_the_new_tables_are_rejected() {
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute("INSERT INTO source_rejected (path, size_bytes, rejected_at) VALUES ('/d/x.stl', 'big', 't')", []).unwrap();
    });
    assert!(validate(&bytes, &trash).is_err());
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute(
            "INSERT INTO source_folders (path, added_at) VALUES (X'1234', 't')",
            [],
        )
        .unwrap();
    });
    assert!(validate(&bytes, &trash).is_err());
}

fn insert_known_copy(conn: &Connection, path: &str, size: i64, hash: &str) {
    conn.execute(
            "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES (?1, ?2, ?3, 't')",
            rusqlite::params![path, size, hash],
        )
        .unwrap();
}

#[test]
fn crafted_known_copy_rows_are_rejected() {
    let long = format!("/{}", "x".repeat(4100));
    let cases: [(&str, &str, i64, &str); 8] = [
        ("parent component", "/models/../etc/x.3mf", 1, COPY_HASH),
        ("control character", "/models/a\u{7}.3mf", 1, COPY_HASH),
        ("relative path", "models/a.3mf", 1, COPY_HASH),
        ("negative size", "/models/a.3mf", -1, COPY_HASH),
        ("short hash", "/models/a.3mf", 1, "abcd"),
        (
            "non-hex hash",
            "/models/a.3mf",
            1,
            "zz23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ),
        ("empty path", "", 1, COPY_HASH),
        ("endless path", long.as_str(), 1, COPY_HASH),
    ];
    for (why, path, size, hash) in cases {
        let (bytes, trash) = catalog_bytes(|conn| insert_known_copy(conn, path, size, hash));
        assert!(validate(&bytes, &trash).is_err(), "{why}");
    }
    let (bytes, trash) =
        catalog_bytes(|conn| insert_known_copy(conn, "/models/a.3mf", 1, COPY_HASH));
    assert!(
        validate(&bytes, &trash).is_ok(),
        "counter-check: a normal row passes"
    );
}

#[test]
fn a_known_copy_in_a_protected_location_is_rejected() {
    let (bytes, trash) =
        catalog_bytes(|conn| insert_known_copy(conn, "/protected/dir/a.3mf", 1, COPY_HASH));
    let sensitive = [PathBuf::from("/protected")];
    assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash, &[]).is_err());
    assert!(
        validate_catalog_db_bytes(&bytes, &[], &trash, &[]).is_ok(),
        "counter-check: without the protection the row is fine"
    );
}

#[test]
fn a_backup_with_too_many_known_copies_is_rejected() {
    let limit = crate::db::known_copies::MAX_KNOWN_COPIES;
    for (rows, accepted) in [(limit, true), (limit + 1, false)] {
        let (bytes, trash) = catalog_bytes(|conn| {
            conn.execute_batch("BEGIN").unwrap();
            for i in 0..rows {
                insert_known_copy(conn, &format!("/models/{i}.3mf"), 1, COPY_HASH);
            }
            conn.execute_batch("COMMIT").unwrap();
        });
        assert_eq!(validate(&bytes, &trash).is_ok(), accepted, "{rows} rows");
    }
}

#[test]
fn wrong_storage_classes_in_known_copies_are_rejected() {
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute("INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES ('/m/a.3mf', 'big', ?1, 't')", [COPY_HASH]).unwrap();
    });
    assert!(validate(&bytes, &trash).is_err());
    let (bytes, trash) = catalog_bytes(|conn| {
        conn.execute("INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at) VALUES (X'1234', 1, ?1, 't')", [COPY_HASH]).unwrap();
    });
    assert!(validate(&bytes, &trash).is_err());
}

const GOOD_BUNDLE: &str = "INSERT INTO bundles VALUES (1, 'Vase', 'parts', 't');
        INSERT INTO bundle_members VALUES (1, 1, 0, NULL), (2, 1, 1, 'Lid');";

const GOOD_REPORT: &str = "INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json, state)
        VALUES ('2026-11-05T10:00:00+00:00', '/o/Box.gcode', 'Box.gcode', 5, '{\"printSeconds\":60}', 'open');";

#[test]
fn trash_policy_accepts_both_formats_and_rejects_escape_cases() {
    let dir = unique_test_dir("policy");
    let desktop = dir.join("data/trash");
    let root = dir.join("models");
    let web = root.join(".katalog-papierkorb");
    std::fs::create_dir_all(&desktop).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    let roots = vec![root.clone()];
    let policy = TrashPolicy {
        desktop_trash: Some(&desktop),
        catalog_roots: &roots,
    };
    let fs = StdRestoreFs::new(dir.clone());
    for companion in [false, true] {
        let prefix = if companion { "companion-" } else { "" };
        for path in [
            desktop.join(format!("{prefix}7-model.3mf")),
            web.join(format!("{prefix}7-deadbeef-model.3mf")),
        ] {
            assert!(
                validate_trash_path(&fs, &path, 7, companion, &policy, &[]).is_ok(),
                "{path:?}"
            );
        }
        for path in [
            dir.join(format!(".katalog-papierkorb/{prefix}7-deadbeef-model.3mf")),
            web.join(format!("sub/{prefix}7-deadbeef-model.3mf")),
            web.join(format!("{prefix}8-deadbeef-model.3mf")),
            desktop.join(format!("{prefix}8-model.3mf")),
            desktop.join(format!("sub/{prefix}7-model.3mf")),
            web.join(format!("{prefix}7-model.3mf")),
            web.join(format!("{prefix}7-nothex00-model.3mf")),
            web.join(format!(
                "../.katalog-papierkorb/{prefix}7-deadbeef-model.3mf"
            )),
        ] {
            assert!(
                validate_trash_path(&fs, &path, 7, companion, &policy, &[]).is_err(),
                "{path:?}"
            );
        }
        assert!(validate_trash_path(
            &fs,
            &web.join(format!("{prefix}7-deadbeef-model.3mf")),
            7,
            companion,
            &policy,
            std::slice::from_ref(&root)
        )
        .is_err());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn desktop_and_web_archives_roundtrip_with_both_trash_formats() {
    let dir = unique_test_dir("roundtrip");
    let desktop = dir.join("trash");
    let root = dir.join("models");
    std::fs::create_dir_all(&desktop).unwrap();
    std::fs::create_dir_all(root.join(".katalog-papierkorb")).unwrap();
    let path = dir.join("source.db");
    let conn = db::connect(&path).unwrap();
    for (index, trash) in [desktop.clone(), root.join(".katalog-papierkorb")]
        .iter()
        .enumerate()
    {
        let id = db::test_insert_minimal_file(
            &conn,
            &root.join(format!("{index}.3mf")).to_string_lossy(),
            None,
        )
        .unwrap();
        let op = if index == 0 { "" } else { "deadbeef-" };
        conn.execute(
            "UPDATE files SET deleted_at = '2026-01-01', trash_path = ?1 WHERE id = ?2",
            params![
                trash
                    .join(format!("{id}-{op}{index}.3mf"))
                    .to_string_lossy(),
                id
            ],
        )
        .unwrap();
        conn.execute("INSERT INTO companion_files (id, file_id, name, path, kind, size_bytes, added_at, deleted_at, trash_path) VALUES (?1, ?1, 'notes.txt', ?2, 'text', 1, 't', 't', ?3)",
            params![id, root.join(format!("{index}_extras/notes.txt")).to_string_lossy(), trash.join(format!("companion-{id}-{op}notes.txt")).to_string_lossy()]).unwrap();
    }
    drop(conn);
    let roots = vec![root];
    let policy = TrashPolicy {
        desktop_trash: Some(&desktop),
        catalog_roots: &roots,
    };
    let local = db::connect_in_memory().unwrap();
    let local_settings = db::thingiverse::local_settings(&local).unwrap();
    let fs = StdRestoreFs::new(dir.clone());
    for web_export in [false, true] {
        let bytes = if web_export {
            let zip_path = dir.join("web.zip");
            crate::catalog_export::write_zip(
                &path,
                "{}",
                std::fs::File::create(&zip_path).unwrap(),
            )
            .unwrap();
            std::fs::read(zip_path).unwrap()
        } else {
            zip_bytes_with(&[
                ("catalog.db", &std::fs::read(&path).unwrap()),
                ("settings.json", b"{}"),
            ])
        };
        let (db_bytes, settings) = read_catalog_archive(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(settings, "{}");
        let mut ready =
            prepare_catalog(&db_bytes, &[], &policy, &[], &fs, &[], &local_settings).unwrap();
        for table in ["files", "companion_files"] {
            let count: i64 = ready
                .connection()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE trash_path IS NOT NULL"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 2);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preparation_scrubs_foreign_secrets_and_preserves_local_state_and_source_bytes() {
    let dir = unique_test_dir("prepare");
    let path = dir.join("incoming.db");
    let incoming = db::connect(&path).unwrap();
    let foreign_key = "FOREIGN-0123456789-abcdefghijkl";
    db::thingiverse::set_token(
        &incoming,
        &crate::thingiverse::Token::parse(foreign_key).unwrap(),
    )
    .unwrap();
    db::test_insert_minimal_file(&incoming, "/models/a.3mf", None).unwrap();
    incoming
        .execute(
            "UPDATE files SET content_hash = 'forged', imported_at = '1900'",
            [],
        )
        .unwrap();
    db::insert_registered_slicer(&incoming, "Foreign", "/foreign/slicer", false).unwrap();
    db::printer_link::set_setting(&incoming, "catalog_base_dir", "/foreign/base").unwrap();
    drop(incoming);
    let bytes = std::fs::read(&path).unwrap();
    let local = db::connect_in_memory().unwrap();
    db::insert_registered_slicer(&local, "Local", "/local/slicer", true).unwrap();
    let local_key = "LOCAL-0123456789-abcdefghijkl";
    db::thingiverse::set_token(
        &local,
        &crate::thingiverse::Token::parse(local_key).unwrap(),
    )
    .unwrap();
    let settings = db::thingiverse::local_settings(&local).unwrap();
    let slicers = db::list_registered_slicers(&local).unwrap();
    let policy = TrashPolicy {
        desktop_trash: None,
        catalog_roots: &[],
    };
    let mut ready = prepare_catalog(
        &bytes,
        &[],
        &policy,
        &[],
        &StdRestoreFs::new(dir.clone()),
        &slicers,
        &settings,
    )
    .unwrap();
    let conn = ready.connection();
    assert_eq!(
        db::list_registered_slicers(conn).unwrap()[0].executable_path,
        "/local/slicer"
    );
    assert!(db::thingiverse::get_token(conn).unwrap().is_some());
    assert_eq!(
        db::printer_link::get_setting(conn, "catalog_base_dir").unwrap(),
        None
    );
    let identity: (Option<String>, String) = conn
        .query_row("SELECT content_hash, imported_at FROM files", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(identity, (None, "1900".into()));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let prepared_path = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("catalog-restore-")
        })
        .unwrap();
    let sanitized = std::fs::read(prepared_path).unwrap();
    assert!(!contains(&sanitized, foreign_key.as_bytes()));
    assert!(contains(&sanitized, local_key.as_bytes()));
    drop(ready);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

const COPY_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn temporary_copies_are_removed_after_rejection_and_are_private() {
    let dir = unique_test_dir("temporary");
    let fs = StdRestoreFs::new(dir.clone());
    let policy = TrashPolicy {
        desktop_trash: None,
        catalog_roots: &[],
    };
    assert!(super::validate_catalog_db_bytes(b"garbage", &[], &policy, &[], &fs).is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let temporary = fs.temporary_catalog(b"").unwrap();
    let entry = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            entry.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(entry);
    drop(temporary);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn trash_policy_rejects_symlink_escapes() {
    use std::os::unix::fs::symlink;
    let dir = unique_test_dir("symlinks");
    let root = dir.join("models");
    let outside = dir.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, root.join(".katalog-papierkorb")).unwrap();
    let roots = vec![root.clone()];
    let fs = StdRestoreFs::new(dir.clone());
    let policy = TrashPolicy {
        desktop_trash: None,
        catalog_roots: &roots,
    };
    assert!(validate_trash_path(
        &fs,
        &root.join(".katalog-papierkorb/1-deadbeef-a.3mf"),
        1,
        false,
        &policy,
        &[]
    )
    .is_err());
    let desktop = dir.join("trash");
    std::fs::create_dir(&desktop).unwrap();
    std::fs::write(outside.join("a.3mf"), b"").unwrap();
    symlink(outside.join("a.3mf"), desktop.join("1-a.3mf")).unwrap();
    let policy = TrashPolicy {
        desktop_trash: Some(&desktop),
        catalog_roots: &[],
    };
    assert!(validate_trash_path(&fs, &desktop.join("1-a.3mf"), 1, false, &policy, &[]).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn trash_policy_rejects_linked_subdirectories_and_redirected_row_ids() {
    use std::os::unix::fs::symlink;
    let dir = unique_test_dir("trash-alias");
    let desktop = dir.join("trash");
    std::fs::create_dir(&desktop).unwrap();
    symlink(&desktop, desktop.join("sub")).unwrap();
    std::fs::write(desktop.join("2-a.3mf"), b"").unwrap();
    symlink(desktop.join("2-a.3mf"), desktop.join("1-a.3mf")).unwrap();
    let fs = StdRestoreFs::new(dir.clone());
    let policy = TrashPolicy {
        desktop_trash: Some(&desktop),
        catalog_roots: &[],
    };
    for path in [desktop.join("sub/2-a.3mf"), desktop.join("1-a.3mf")] {
        let id = if path.starts_with(desktop.join("sub")) {
            2
        } else {
            1
        };
        assert!(
            validate_trash_path(&fs, &path, id, false, &policy, &[]).is_err(),
            "{path:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
