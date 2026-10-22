//! Files of an archive that are not models: allowed ones join the model they belong to,
//! everything else stays where it was extracted.
use super::*;
use std::cell::RefCell;
use std::io::Write;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const PDF: &[u8] = b"%PDF-1.7\n";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "archive_companions_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("catalog")).unwrap();
        Self(dir)
    }
    fn target(&self) -> PathBuf {
        self.0.join("catalog")
    }
    fn zip(&self, entries: &[(&str, Vec<u8>)]) -> PathBuf {
        let path = self.0.join("archive.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
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
    for value in [0.0f32, 0.0, 1.0, 0.0, 0.0, s, 10.0, 0.0, s, 0.0, 10.0, s + 1.0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn request(path: &Path) -> ArchiveRequest {
    let info = inspect_one(path);
    ArchiveRequest {
        path: info.path,
        folder_name: "pack".into(),
        on_conflict: ConflictMode::New,
        expected_size: info.file_size,
        expected_modified_unix_ms: info.modified_unix_ms,
    }
}

/// Extracts and imports with the companion assignment wired in, like the import job does.
fn run(fixture: &Fixture, conn: &RefCell<Connection>, entries: &[(&str, Vec<u8>)]) -> ArchiveOutcomeDto {
    let archive = fixture.zip(entries);
    let mut result = extract_archives_controlled(
        &[],
        &fixture.target(),
        vec![request(&archive)],
        false,
        CatalogSteps {
            import_dir: |dir: &Path, created: &[PathBuf]| import_extracted_dir(&mut conn.borrow_mut(), dir, created),
            assign_companions: |_: &Path, written: &[PathBuf], imported: &[ModelFileDto]| {
                companions::assign_from_archive(&conn.borrow(), written, imported, &[])
            },
        },
        |_, _| {},
        None,
    )
    .unwrap();
    assert!(result.archives[0].error.is_none(), "{:?}", result.archives[0].error);
    result.archives.remove(0)
}

fn companions_of(conn: &RefCell<Connection>, name: &str) -> Vec<String> {
    let conn = conn.borrow();
    let id: i64 = conn.query_row("SELECT id FROM files WHERE name = ?1", [name], |r| r.get(0)).unwrap();
    let mut names: Vec<String> = db::companions::list_companions(&conn, id).unwrap().into_iter().map(|c| c.name).collect();
    names.sort();
    names
}

fn total_companions(conn: &RefCell<Connection>) -> i64 {
    conn.borrow().query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap()
}

#[test]
fn with_one_model_all_allowed_files_belong_to_it_and_the_rest_stays() {
    let fixture = Fixture::new();
    let conn = RefCell::new(db::connect_in_memory().unwrap());
    let outcome = run(
        &fixture,
        &conn,
        &[
            ("Model.stl", stl(1)),
            ("images/front.png", PNG.to_vec()),
            ("docs/manual.pdf", PDF.to_vec()),
            ("license.txt", "CC-BY".as_bytes().to_vec()),
            ("drawing.svg", b"<svg/>".to_vec()),
            ("data.bin", vec![1, 2, 3]),
            ("fake.png", b"this is not a png".to_vec()),
        ],
    );
    assert_eq!((outcome.companions_assigned, outcome.companions_ignored), (3, 3));
    assert_eq!(companions_of(&conn, "Model.stl"), vec!["front.png", "license.txt", "manual.pdf"]);
    let dest = fixture.target().join("pack");
    for name in ["front.png", "license.txt", "manual.pdf"] {
        assert!(dest.join("Model_extras").join(name).is_file(), "{name}");
    }
    assert!(!dest.join("images").join("front.png").exists(), "assigned files are moved, not copied");
    for left in ["drawing.svg", "data.bin", "fake.png"] {
        assert!(dest.join(left).is_file(), "{left} stays where it was extracted");
    }
    assert_eq!(outcome.other_files_extracted, 3, "the summary only counts what is left over");
}

#[test]
fn with_several_models_a_file_belongs_to_the_model_in_its_own_folder() {
    let fixture = Fixture::new();
    let conn = RefCell::new(db::connect_in_memory().unwrap());
    let outcome = run(
        &fixture,
        &conn,
        &[
            ("a/First.stl", stl(1)),
            ("a/first.png", PNG.to_vec()),
            ("b/Second.stl", stl(2)),
            ("b/second.txt", b"notes".to_vec()),
            ("readme.txt", b"general".to_vec()),
        ],
    );
    assert_eq!((outcome.companions_assigned, outcome.companions_ignored), (2, 0));
    assert_eq!(companions_of(&conn, "First.stl"), vec!["first.png"]);
    assert_eq!(companions_of(&conn, "Second.stl"), vec!["second.txt"]);
    assert!(fixture.target().join("pack").join("readme.txt").is_file(), "no model in that folder: no assignment");
    assert_eq!(outcome.other_files_extracted, 1);
}

#[test]
fn two_models_in_one_folder_leave_ambiguous_files_alone() {
    let fixture = Fixture::new();
    let conn = RefCell::new(db::connect_in_memory().unwrap());
    let outcome = run(
        &fixture,
        &conn,
        &[("One.stl", stl(1)), ("Two.stl", stl(2)), ("shared.png", PNG.to_vec()), ("sub/Three.stl", stl(3)), ("sub/three.txt", b"x".to_vec())],
    );
    assert_eq!(outcome.companions_assigned, 1, "only the file in the folder with a single model");
    assert_eq!(companions_of(&conn, "Three.stl"), vec!["three.txt"]);
    assert_eq!(total_companions(&conn), 1);
    assert!(fixture.target().join("pack").join("shared.png").is_file());
}

#[test]
fn without_the_assignment_nothing_is_attached() {
    // Counter-check for the tests above: the plain extraction leaves every file in place.
    let fixture = Fixture::new();
    let conn = RefCell::new(db::connect_in_memory().unwrap());
    let archive = fixture.zip(&[("Model.stl", stl(1)), ("front.png", PNG.to_vec())]);
    let result = extract_archives_controlled(
        &[],
        &fixture.target(),
        vec![request(&archive)],
        false,
        CatalogSteps {
            import_dir: |dir: &Path, created: &[PathBuf]| import_extracted_dir(&mut conn.borrow_mut(), dir, created),
            assign_companions: |_: &Path, _: &[PathBuf], _: &[ModelFileDto]| (0, 0),
        },
        |_, _| {},
        None,
    )
    .unwrap();
    assert_eq!(result.archives[0].companions_assigned, 0);
    assert_eq!(total_companions(&conn), 0);
    assert!(fixture.target().join("pack").join("front.png").is_file());
}

#[test]
fn the_outcome_reports_the_counters_to_the_ui() {
    let outcome = ArchiveOutcomeDto { companions_assigned: 2, companions_ignored: 1, ..Default::default() };
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["companionsAssigned"], 2);
    assert_eq!(json["companionsIgnored"], 1);
}
