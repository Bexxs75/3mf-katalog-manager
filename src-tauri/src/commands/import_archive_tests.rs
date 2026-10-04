//! Frozen import contract §9 archive regressions, using real ZIP/STL/SQLite.
use super::*;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "import_archive_contract_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("catalog")).unwrap();
        Self(dir)
    }
    fn target(&self) -> PathBuf {
        self.0.join("catalog")
    }
    fn zip(&self, name: &str, entries: &[(String, Vec<u8>)]) -> PathBuf {
        let path = self.0.join(format!("{name}.zip"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn stl(seed: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let s = seed as f32;
    for value in [
        0.0f32,
        0.0,
        1.0,
        0.0,
        0.0,
        s,
        10.0,
        0.0,
        s,
        0.0,
        10.0,
        s + 1.0,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}
fn request(path: &Path, mode: ConflictMode) -> ArchiveRequest {
    let info = inspect_one(path);
    ArchiveRequest {
        path: info.path,
        folder_name: info.suggested_folder_name,
        on_conflict: mode,
        expected_size: info.file_size,
        expected_modified_unix_ms: info.modified_unix_ms,
    }
}
fn run(
    conn: &mut Connection,
    target: &Path,
    requests: Vec<ArchiveRequest>,
    delete: bool,
) -> ArchiveImportResultDto {
    extract_archives_core(
        &[],
        target,
        requests,
        delete,
        |dir, created| import_extracted_dir(conn, dir, created),
        |_, _| {},
    )
    .unwrap()
}

#[test]
fn contract_10_thirtieth_model_database_failure_rolls_back_whole_archive() {
    let fixture = Fixture::new();
    let archive = fixture.zip(
        "thirty",
        &(0..30)
            .map(|i| (format!("part_{i}.stl"), stl(i)))
            .collect::<Vec<_>>(),
    );
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch("CREATE TRIGGER fail_thirtieth BEFORE INSERT ON files WHEN (SELECT COUNT(*) FROM files) = 29 BEGIN SELECT RAISE(ABORT, 'thirtieth file storage failure'); END;").unwrap();
    let result = run(
        &mut conn,
        &fixture.target(),
        vec![request(&archive, ConflictMode::New)],
        true,
    );
    assert!(result.imported.is_empty());
    assert!(result.archives[0]
        .error
        .as_ref()
        .unwrap()
        .contains("thirtieth file storage failure"));
    assert!(db::list_files(&conn).unwrap().is_empty());
    assert!(db::list_folders(&conn).unwrap().is_empty());
    assert!(conn.is_autocommit());
    assert!(!fixture.target().join("thirty").exists());
    assert!(archive.exists());
}

#[test]
fn contract_10_invalid_thirtieth_model_keeps_other_twenty_nine() {
    let fixture = Fixture::new();
    let mut entries: Vec<_> = (0..29).map(|i| (format!("part_{i}.stl"), stl(i))).collect();
    entries.push(("broken.stl".into(), b"not an STL file".to_vec()));
    let archive = fixture.zip("partial", &entries);
    let mut conn = db::connect_in_memory().unwrap();
    let result = run(
        &mut conn,
        &fixture.target(),
        vec![request(&archive, ConflictMode::New)],
        false,
    );
    assert_eq!(result.imported.len(), 29);
    assert_eq!(db::list_files(&conn).unwrap().len(), 29);
    assert_eq!(result.archives[0].error, None);
    assert_eq!(
        result.archives[0].models.skipped,
        vec![serde_json::json!({"entryPath":"broken.stl", "reason":"invalid"})]
    );
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].reason, files::SkipReason::Invalid);
    assert!(fixture.target().join("partial").exists());
}

#[test]
fn contract_10_cancel_second_archive_preserves_first_and_never_starts_third() {
    let fixture = Fixture::new();
    let archives: Vec<_> = (0..3)
        .map(|i| fixture.zip(&format!("archive_{i}"), &[("part.stl".into(), stl(i))]))
        .collect();
    let requests = archives
        .iter()
        .map(|path| request(path, ConflictMode::New))
        .collect();
    let cancelled = AtomicBool::new(false);
    let mut started = Vec::new();
    let mut conn = db::connect_in_memory().unwrap();
    let result = extract_archives_controlled(
        &[],
        &fixture.target(),
        requests,
        false,
        |dir, created| import_extracted_dir(&mut conn, dir, created),
        |path, phase| {
            if phase == "extracting" {
                started.push(path.to_string());
                if path == archives[1].to_string_lossy() {
                    cancelled.store(true, Ordering::Release);
                }
            }
        },
        Some(&cancelled),
    )
    .unwrap();
    assert_eq!(result.imported.len(), 1);
    assert_eq!(db::list_files(&conn).unwrap().len(), 1);
    assert!(fixture.target().join("archive_0/part.stl").exists());
    assert!(!fixture.target().join("archive_1").exists());
    assert!(!fixture.target().join("archive_2").exists());
    assert_eq!(result.archives[1].error.as_deref(), Some("cancelled"));
    assert_eq!(
        started,
        vec![
            archives[0].to_string_lossy().into_owned(),
            archives[1].to_string_lossy().into_owned()
        ]
    );
}

#[test]
fn contract_17_mixed_archive_accounts_each_entry_exactly_once() {
    let fixture = Fixture::new();
    let existing_dir = fixture.target().join("mixed");
    std::fs::create_dir(&existing_dir).unwrap();
    let existing = existing_dir.join("existing.stl");
    std::fs::write(&existing, stl(42)).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    files::import_many_with_conn(&mut conn, vec![existing.clone()]).unwrap();
    let archive = fixture.zip(
        "mixed",
        &[
            ("valid.stl".into(), stl(1)),
            ("same_hash.stl".into(), stl(1)),
            ("broken.stl".into(), b"not an STL file".to_vec()),
            ("../verloren.stl".into(), stl(2)),
            ("existing.stl".into(), stl(3)),
        ],
    );
    let result = run(
        &mut conn,
        &fixture.target(),
        vec![request(&archive, ConflictMode::Merge)],
        false,
    );
    let outcome = &result.archives[0];
    assert_eq!(outcome.error, None);
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.duplicate_count, 1);
    assert_eq!(outcome.unsafe_skipped, 1);
    assert_eq!(outcome.existing_skipped, 1);
    assert_eq!(outcome.models.imported.len(), 1);
    assert_eq!(
        outcome.models.duplicates.len(),
        1,
        "existing skipped entries must not also be hash duplicates"
    );
    let duplicate = outcome.models.duplicates[0]["entryPath"].as_str().unwrap();
    assert!(duplicate == "valid.stl" || duplicate == "same_hash.stl");
    assert_eq!(outcome.models.duplicates[0]["kind"], "hash");
    assert_eq!(outcome.models.skipped.len(), 3);
    for (path, reason) in [
        ("broken.stl", "invalid"),
        ("../verloren.stl", "unsafe"),
        ("existing.stl", "existing"),
    ] {
        assert!(
            outcome
                .models
                .skipped
                .contains(&serde_json::json!({"entryPath":path,"reason":reason})),
            "missing {path}: {reason}: {:?}",
            outcome.models.skipped
        );
    }
    assert_eq!(std::fs::read(&existing).unwrap(), stl(42));
    assert_eq!(db::list_files(&conn).unwrap().len(), 2);
}

#[test]
fn contract_14_legacy_serialization_preserves_success_with_delete_warning() {
    let fixture = Fixture::new();
    let archive = fixture.zip("warning", &[("part.stl".into(), stl(0))]);
    let mut req = request(&archive, ConflictMode::New);
    req.expected_size += 1; // Real extraction succeeds; original must be retained.
    let mut conn = db::connect_in_memory().unwrap();
    let result = run(&mut conn, &fixture.target(), vec![req], true);
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["imported"].as_array().unwrap().len(), 1);
    assert_eq!(json["duplicateCount"], 0);
    assert_eq!(json["archives"][0]["error"], serde_json::Value::Null);
    assert!(json["archives"][0]["deleteError"]
        .as_str()
        .unwrap()
        .contains("veraendert"));
    assert_eq!(json["archives"][0]["archiveDeleted"], false);
    assert!(
        json["archives"][0].get("models").is_none(),
        "legacy DTO must not expose new job-only models field"
    );
    assert!(archive.exists());
}

#[test]
fn contract_17_merge_existing_uncatalogued_file_remains_skipped() {
    let fixture = Fixture::new();
    let existing_dir = fixture.target().join("merge");
    std::fs::create_dir(&existing_dir).unwrap();
    let existing = existing_dir.join("existing.stl");
    std::fs::write(&existing, stl(42)).unwrap();
    let archive = fixture.zip(
        "merge",
        &[("new.stl".into(), stl(1)), ("existing.stl".into(), stl(2))],
    );
    let mut conn = db::connect_in_memory().unwrap();
    let result = run(
        &mut conn,
        &fixture.target(),
        vec![request(&archive, ConflictMode::Merge)],
        false,
    );
    assert_eq!(result.archives[0].existing_skipped, 1);
    assert_eq!(
        result.imported.len(),
        1,
        "entry skipped by extraction must not be imported from older destination content"
    );
    assert_eq!(result.duplicate_count, 0);
    assert!(result.archives[0].models.duplicates.is_empty());
    assert_eq!(
        result.archives[0].models.skipped,
        vec![serde_json::json!({"entryPath":"existing.stl", "reason":"existing"})]
    );
    assert_eq!(db::list_files(&conn).unwrap().len(), 1);
    assert_eq!(std::fs::read(&existing).unwrap(), stl(42));
}
