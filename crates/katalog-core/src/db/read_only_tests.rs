use super::*;

#[test]
fn read_only_opens_populated_catalog_and_rejects_writes() {
    let dir = crate::unique_test_dir("read_only");
    let path = dir.join("catalog.db");
    let conn = connect(&path).unwrap();
    test_insert_minimal_file(&conn, "/cube.stl", None).unwrap();
    drop(conn);
    let conn = open_read_only(&path).unwrap();
    assert_eq!(list_files(&conn).unwrap().len(), 1);
    assert_eq!(
        conn.pragma_query_value(None, "query_only", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    for sql in [
        "INSERT INTO folders(name,path) VALUES ('x','x')",
        "UPDATE files SET name = 'changed'",
        "DELETE FROM files",
        "PRAGMA user_version = 1",
    ] {
        assert!(conn.execute_batch(sql).is_err(), "{sql}");
    }
    drop(conn);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn read_only_rejects_old_schema_without_changing_bytes() {
    let dir = crate::unique_test_dir("read_only_old");
    let path = dir.join("catalog.db");
    let conn = connect(&path).unwrap();
    conn.execute_batch("ALTER TABLE files DROP COLUMN license")
        .unwrap();
    conn.pragma_update(None, "user_version", migrations::CURRENT_SCHEMA_VERSION - 1)
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    let error = open_read_only(&path).unwrap_err();
    assert!(error.to_string().contains("zuerst"));
    assert!(error.to_string().contains("migriert werden"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn migrations_really_attempt_to_write_on_an_old_read_only_catalog() {
    let dir = crate::unique_test_dir("read_only_migration");
    let path = dir.join("catalog.db");
    let conn = connect(&path).unwrap();
    conn.execute_batch("ALTER TABLE files DROP COLUMN license")
        .unwrap();
    conn.pragma_update(None, "user_version", migrations::CURRENT_SCHEMA_VERSION - 1)
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    // Bypass open_read_only's version guard, to exercise an actual migration.
    let mut conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let error = run_migrations(&mut conn).unwrap_err();
    assert!(
        matches!(error, error::DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::ReadOnly)
    );
    drop(conn);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    // Same old schema migrates successfully with write access: no duplicate
    // column or other unrelated SQL error can satisfy the assertion above.
    let mut writable = rusqlite::Connection::open(&path).unwrap();
    run_migrations(&mut writable).unwrap();
    drop(writable);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn read_only_rejects_newer_schema_and_missing_file() {
    let dir = crate::unique_test_dir("read_only_newer");
    let missing = dir.join("missing.db");
    assert!(open_read_only(&missing)
        .unwrap_err()
        .to_string()
        .contains("fehlt"));
    assert!(!missing.exists());
    let path = dir.join("catalog.db");
    let conn = connect(&path).unwrap();
    conn.pragma_update(None, "user_version", migrations::CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        open_read_only(&path),
        Err(error::DbError::NewerSchema { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hot_journal_requires_writer_and_read_only_leaves_files_unchanged() {
    const CHILD: &str = "KATALOG_CORE_HOT_JOURNAL_TEST";
    if let Some(path) = std::env::var_os(CHILD) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "PRAGMA cache_size=1; BEGIN IMMEDIATE; UPDATE files SET name=hex(randomblob(8192));",
        )
        .unwrap();
        // Exit without dropping the connection: emulate a crashed writer after
        // cache spill. SQLite must recover the hot journal before reading.
        std::process::exit(0);
    }
    let dir = crate::unique_test_dir("hot_journal");
    let path = dir.join("catalog.db");
    let conn = connect(&path).unwrap();
    for i in 0..40 {
        test_insert_minimal_file(&conn, &format!("/model-{i}.stl"), None).unwrap();
    }
    drop(conn);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "db::read_only_tests::hot_journal_requires_writer_and_read_only_leaves_files_unchanged",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .status()
        .unwrap();
    assert!(status.success());
    let journal = dir.join("catalog.db-journal");
    let before = std::fs::read(&path).unwrap();
    let journal_before = std::fs::read(&journal).unwrap();
    assert_eq!(
        &journal_before[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    let error = open_read_only(&path).unwrap_err();
    assert!(error.to_string().contains("heißes Journal"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(&journal).unwrap(), journal_before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}
