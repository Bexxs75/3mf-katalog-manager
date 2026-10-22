//! Blocked automatic tags must hold on every path that assigns tags by itself.
use super::*;


struct Fixture {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
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
            "blocked-tags-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("models");
        std::fs::create_dir(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        Self { base, conn, root }
    }
    fn model(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        // Different content per name: an identical file would be skipped as a duplicate.
        let size = 1 + name.len();
        std::fs::write(&path, format!("v 0 0 0\nv {size} 0 0\nv 0 {size} 0\nf 1 2 3\n")).unwrap();
        path
    }
    fn tags_of(&self, name: &str) -> Vec<String> {
        let id = db::list_files(&self.conn).unwrap().into_iter().find(|f| f.name == name).unwrap().id;
        let mut tags: Vec<String> = db::list_all_file_tags(&self.conn)
            .unwrap()
            .into_iter()
            .filter(|(file_id, _)| *file_id == id)
            .map(|(_, tag)| tag)
            .collect();
        tags.sort();
        tags
    }
}

#[test]
fn the_tiny_model_gets_both_tags_while_nothing_is_blocked() {
    let mut f = Fixture::new();
    let path = f.model("Kabelhalter.obj");
    files::import_many_with_conn(&mut f.conn, vec![path]).unwrap();
    assert_eq!(f.tags_of("Kabelhalter.obj"), vec!["kabelhalter", "miniatur"]);
}

#[test]
fn batch_import_skips_blocked_tags() {
    let mut f = Fixture::new();
    db::blocked_tags::block_auto_tag(&f.conn, "Miniature").unwrap();
    let path = f.model("Kabelhalter.obj");
    files::import_many_with_conn(&mut f.conn, vec![path]).unwrap();
    assert_eq!(f.tags_of("Kabelhalter.obj"), vec!["kabelhalter"]);
}

#[test]
fn archive_style_atomic_import_skips_blocked_tags() {
    let mut f = Fixture::new();
    db::blocked_tags::block_auto_tag(&f.conn, "kabelhalter").unwrap();
    let path = f.model("Kabelhalter.obj");
    files::import_many_with_conn_atomic(&mut f.conn, vec![path]).unwrap();
    assert_eq!(f.tags_of("Kabelhalter.obj"), vec!["miniatur"]);
}

#[test]
fn disk_sync_import_skips_blocked_tags() {
    let mut f = Fixture::new();
    db::blocked_tags::block_auto_tag(&f.conn, "miniatur").unwrap();
    f.model("Kabelhalter.obj");
    let preview = scan(&f.conn).unwrap();
    let selection = DiskSelection {
        new_paths: preview.new_folders.into_iter().chain(preview.new_files).map(|e| e.path).collect(),
        ..Default::default()
    };
    apply(&mut f.conn, selection).unwrap();
    assert_eq!(f.tags_of("Kabelhalter.obj"), vec!["kabelhalter"]);
}

#[test]
fn a_tag_added_by_hand_is_never_blocked_and_existing_tags_stay() {
    let mut f = Fixture::new();
    let first = f.model("Erstes.obj");
    files::import_many_with_conn(&mut f.conn, vec![first]).unwrap();
    db::blocked_tags::block_auto_tag(&f.conn, "miniatur").unwrap();
    // The block only stops the automatic assignment: the earlier model keeps its tag ...
    assert_eq!(f.tags_of("Erstes.obj"), vec!["erstes", "miniatur"]);
    // ... and the same tag can still be put on another model by hand.
    let second = f.model("Zweites.obj");
    files::import_many_with_conn(&mut f.conn, vec![second]).unwrap();
    assert_eq!(f.tags_of("Zweites.obj"), vec!["zweites"]);
    let id = db::list_files(&f.conn).unwrap().into_iter().find(|x| x.name == "Zweites.obj").unwrap().id;
    files::add_tag_with_conn(&f.conn, id, "Miniatur").unwrap();
    assert_eq!(f.tags_of("Zweites.obj"), vec!["miniatur", "zweites"]);
}

#[test]
fn unblocking_restores_the_automatic_assignment() {
    let mut f = Fixture::new();
    db::blocked_tags::block_auto_tag(&f.conn, "miniatur").unwrap();
    db::blocked_tags::unblock_auto_tag(&f.conn, "MINIATURE").unwrap();
    let path = f.model("Kabelhalter.obj");
    files::import_many_with_conn(&mut f.conn, vec![path]).unwrap();
    assert_eq!(f.tags_of("Kabelhalter.obj"), vec!["kabelhalter", "miniatur"]);
}

#[test]
fn suggestions_for_a_model_are_recomputed_and_ignore_tags_added_by_hand() {
    let mut f = Fixture::new();
    let path = f.model("Kabelhalter.obj");
    files::import_many_with_conn(&mut f.conn, vec![path]).unwrap();
    let id = db::list_files(&f.conn).unwrap()[0].id;
    files::add_tag_with_conn(&f.conn, id, "mein-projekt").unwrap();
    let mut suggested = files::suggested_auto_tags_with_conn(&f.conn, id).unwrap();
    suggested.sort();
    assert_eq!(suggested, vec!["kabelhalter", "miniatur"]);
    assert!(files::suggested_auto_tags_with_conn(&f.conn, id + 1000).is_err());
}
