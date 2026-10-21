//! The disk comparison leaves companion folders to the database: their files are not
//! "new", their folder is not a "new folder", and a missing companion file is no
//! catalog change.
use super::*;

const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OBJ_B: &str = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
const OBJ_C: &str = "v 0 0 0\nv 3 0 0\nv 0 3 0\nf 1 2 3\n";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

struct Fixture {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
    folder: i64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "disk-companions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let root = base.join("models");
        std::fs::create_dir_all(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let folder = db::ensure_folder_path(&conn, &root, &root).unwrap();
        Self { base, conn, root, folder }
    }
    fn model(&self, name: &str, content: &str) -> i64 {
        let path = self.root.join(name);
        std::fs::write(&path, content).unwrap();
        db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), Some(self.folder)).unwrap()
    }
    fn companion(&self, model: i64, model_name: &str, name: &str) -> PathBuf {
        let stem = model_name.rsplit_once('.').unwrap().0;
        let dir = self.root.join(format!("{stem}_extras"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, PNG).unwrap();
        db::companions::insert_companion(&self.conn, model, name, &path.to_string_lossy(), "image", None, "now").unwrap();
        path
    }
}

#[test]
fn files_in_the_companion_folder_of_a_catalog_model_are_not_offered() {
    let f = Fixture::new();
    let id = f.model("Foo.obj", OBJ_A);
    f.model("Bar.obj", OBJ_C);
    f.companion(id, "Foo.obj", "front.png");
    // A model-type file someone put into the companion folder by hand.
    std::fs::write(f.root.join("Foo_extras").join("stray.obj"), OBJ_B).unwrap();

    let scan = scan(&f.conn).unwrap();

    assert!(scan.new_files.is_empty(), "{:?}", scan.new_files);
    assert!(scan.new_folders.is_empty(), "{:?}", scan.new_folders);
}

#[test]
fn the_same_stray_file_elsewhere_is_still_offered() {
    // Counter-check: the scan does find model files and folders that are not companions.
    let f = Fixture::new();
    f.model("Foo.obj", OBJ_A);
    std::fs::create_dir(f.root.join("Other")).unwrap();
    std::fs::write(f.root.join("Other").join("stray.obj"), OBJ_B).unwrap();
    std::fs::create_dir(f.root.join("Orphan_extras")).unwrap();
    std::fs::write(f.root.join("Orphan_extras").join("unrelated.obj"), OBJ_C).unwrap();

    let scan = scan(&f.conn).unwrap();

    let mut files: Vec<_> = scan.new_files.iter().map(|e| e.relative_path.clone()).collect();
    files.sort();
    assert_eq!(files, vec!["Orphan_extras/unrelated.obj", "Other/stray.obj"], "an _extras folder without a catalog model is a normal folder");
    assert_eq!(scan.new_folders.len(), 2);
}

#[test]
fn a_missing_companion_file_is_no_catalog_change() {
    let f = Fixture::new();
    let id = f.model("Foo.obj", OBJ_A);
    f.model("Keep.obj", OBJ_C);
    let path = f.companion(id, "Foo.obj", "front.png");
    std::fs::remove_file(&path).unwrap();

    let scan = scan(&f.conn).unwrap();

    assert!(scan.missing_files.is_empty() && scan.new_files.is_empty() && scan.missing_folders.is_empty());
    assert!(scan.unreachable_roots.is_empty());
    let row = &db::companions::list_companions(&f.conn, id).unwrap()[0];
    assert!(!Path::new(&row.path).exists(), "the entry stays; it is only reported as missing by the companion list");
}

#[test]
fn applying_the_comparison_leaves_companions_and_their_rows_alone() {
    let mut f = Fixture::new();
    let id = f.model("Foo.obj", OBJ_A);
    f.model("Keep.obj", OBJ_C);
    let path = f.companion(id, "Foo.obj", "front.png");
    std::fs::write(f.root.join("New.obj"), OBJ_B).unwrap();
    let conn = &mut f.conn;
    let preview = scan(conn).unwrap();
    let selection = DiskSelection {
        new_paths: preview.new_folders.iter().chain(&preview.new_files).map(|e| e.path.clone()).collect(),
        ..Default::default()
    };
    apply(conn, selection).unwrap();
    assert!(path.is_file());
    assert_eq!(db::companions::list_companions(conn, id).unwrap().len(), 1);
    assert_eq!(db::list_files(conn).unwrap().len(), 3);
}
