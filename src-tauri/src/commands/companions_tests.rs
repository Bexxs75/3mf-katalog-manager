//! Companion files: type and content checks, copy rules, the trash cycle, and moving
//! or renaming the model they belong to. Real files in throw-away folders.
use super::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const JPG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF";
const GIF: &[u8] = b"GIF89a\x01\0\x01\0";
const WEBP: &[u8] = b"RIFF\x10\0\0\0WEBPVP8 ";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";
const TEXT: &[u8] = "Lizenz: CC-BY, ünï\n".as_bytes();
const EXE: &[u8] = b"MZ\x90\0\x03\0\0\0";

struct Fixture {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
    folder: i64,
    trash: PathBuf,
    outside: PathBuf,
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
            "companions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (root, trash, outside) = (base.join("catalog"), base.join("trash"), base.join("outside"));
        for dir in [&root, &trash, &outside] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let conn = db::connect_in_memory().unwrap();
        let folder = db::insert_folder_with_parent(&conn, "catalog", None, &root.to_string_lossy()).unwrap();
        Self { base, conn, root, folder, trash, outside }
    }
    /// A model file in the catalog folder (or a folder below it, given a folder row).
    fn model_in(&self, dir: &Path, folder: Option<i64>, name: &str) -> (i64, PathBuf) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, b"model").unwrap();
        let id = db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), folder).unwrap();
        (id, path)
    }
    fn model(&self, name: &str) -> (i64, PathBuf) {
        self.model_in(&self.root.clone(), Some(self.folder), name)
    }
    fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.outside.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
    fn add(&self, model: i64, name: &str, bytes: &[u8]) -> CompanionRecord {
        let source = self.source(name, bytes);
        add_companion_with_conn(&self.conn, model, &source, &[]).unwrap()
    }
    fn rows(&self, model: i64) -> Vec<CompanionRecord> {
        db::companions::list_companions(&self.conn, model).unwrap()
    }
    fn trash_entries(&self) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(&self.trash).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
    fn trash_model(&self, id: i64) {
        let file = db::get_file(&self.conn, id).unwrap().unwrap();
        trash::delete_file_with_conn(&self.conn, &file, id, &self.trash).unwrap();
    }
    fn restore_model(&self, id: i64) {
        let file = db::get_file(&self.conn, id).unwrap().unwrap();
        trash::restore_file_with_conn(&self.conn, &file, id, &[]).unwrap();
    }
}

fn sparse_file(path: &Path, header: &[u8], len: u64) {
    let file = std::fs::File::create(path).unwrap();
    std::io::Write::write_all(&mut &file, header).unwrap();
    file.set_len(len).unwrap();
}

// ---- type and content checks ----

#[test]
fn every_allowed_type_is_accepted_with_matching_content() {
    for (name, bytes, kind) in [
        ("a.png", PNG, CompanionKind::Image),
        ("a.jpg", JPG, CompanionKind::Image),
        ("a.JPEG", JPG, CompanionKind::Image),
        ("a.gif", GIF, CompanionKind::Image),
        ("a.webp", WEBP, CompanionKind::Image),
        ("a.pdf", PDF, CompanionKind::Pdf),
        ("a.txt", TEXT, CompanionKind::Text),
        ("a.md", TEXT, CompanionKind::Text),
        ("empty.txt", b"", CompanionKind::Text),
    ] {
        assert_eq!(classify(name, bytes), Ok(kind), "{name}");
    }
}

#[test]
fn an_executable_with_an_image_extension_is_refused() {
    assert_eq!(classify("holiday.png", EXE), Err(Refusal::Content));
    assert_eq!(classify("holiday.pdf", b"\x7fELF\x02\x01\x01"), Err(Refusal::Content));
    assert_eq!(classify("notes.txt", EXE), Err(Refusal::Content));
}

#[test]
fn a_png_with_the_wrong_extension_is_refused() {
    assert_eq!(classify("a.jpg", PNG), Err(Refusal::Content), "the types must agree");
    assert_eq!(classify("a.pdf", PNG), Err(Refusal::Content));
    assert_eq!(classify("a.exe", PNG), Err(Refusal::Type));
    assert_eq!(classify("a", PNG), Err(Refusal::Type), "no extension at all");
}

#[test]
fn scripts_archives_and_unknown_types_are_never_allowed() {
    for name in ["run.sh", "tool.exe", "setup.msi", "x.bat", "x.ps1", "x.zip", "x.7z", "x.docx", "x.svg", "x.html", "x.FCStd", "x.png.exe", "png", ".png"] {
        assert_eq!(classify(name, PNG), Err(Refusal::Type), "{name}");
        assert_eq!(classify(name, TEXT), Err(Refusal::Type), "{name}");
    }
}

#[test]
fn text_with_a_nul_byte_or_invalid_utf8_is_refused() {
    assert_eq!(classify("a.txt", b"hello\0world"), Err(Refusal::Content));
    assert_eq!(classify("a.md", b"\xff\xfe broken"), Err(Refusal::Content));
    assert_eq!(classify("a.txt", b"fine"), Ok(CompanionKind::Text), "counter-check: plain text passes");
}

#[test]
fn a_multibyte_character_cut_by_the_inspected_block_is_not_an_error() {
    let mut cut = vec![b'a'; CONTENT_CHECK_BYTES - 1];
    cut.push(0xC3);
    assert_eq!(classify("big.txt", &cut), Ok(CompanionKind::Text), "the block ends inside a character");
    cut.truncate(100);
    cut.push(0xC3);
    assert_eq!(classify("small.txt", &cut), Err(Refusal::Content), "a short file ending inside a character is damaged");
}

// ---- adding ----

#[test]
fn adding_copies_the_file_next_to_the_model_and_keeps_the_source() {
    let f = Fixture::new();
    let (id, model) = f.model("Foo.3mf");
    let source = f.source("pic.png", PNG);
    let row = add_companion_with_conn(&f.conn, id, &source, &[]).unwrap();
    let expected = f.root.join("Foo_extras").join("pic.png");
    assert_eq!(row.path, expected.to_string_lossy());
    assert_eq!((row.kind.as_str(), row.name.as_str(), row.size_bytes), ("image", "pic.png", Some(PNG.len() as i64)));
    assert_eq!(std::fs::read(&expected).unwrap(), PNG);
    assert_eq!(std::fs::read(&source).unwrap(), PNG, "the chosen file is only read");
    assert!(model.exists());
    assert_eq!(f.rows(id).len(), 1);
}

#[test]
fn adding_never_overwrites_and_takes_a_free_name() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let extras = f.root.join("Foo_extras");
    std::fs::create_dir(&extras).unwrap();
    std::fs::write(extras.join("pic.png"), b"someone else's file").unwrap();
    let first = f.add(id, "pic.png", PNG);
    let second = f.add(id, "pic.png", PNG);
    assert_eq!(first.name, "pic (2).png");
    assert_eq!(second.name, "pic (3).png");
    assert_eq!(std::fs::read(extras.join("pic.png")).unwrap(), b"someone else's file");
}

#[test]
fn refused_files_leave_nothing_behind() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    sparse_file(&f.outside.join("huge.pdf"), PDF, MAX_COMPANION_BYTES + 1);
    let cases = [
        ("disguised.png", EXE.to_vec()),
        ("really-a-png.exe", PNG.to_vec()),
        ("script.sh", b"#!/bin/sh\n".to_vec()),
        ("archive.zip", b"PK\x03\x04".to_vec()),
        ("nul.txt", b"a\0b".to_vec()),
        ("wrong.jpg", PNG.to_vec()),
    ];
    for (name, bytes) in cases {
        let source = f.source(name, &bytes);
        assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err(), "{name} must be refused");
    }
    assert!(add_companion_with_conn(&f.conn, id, &f.outside.join("huge.pdf"), &[]).is_err(), "over the size limit");
    assert!(!f.root.join("Foo_extras").exists(), "no folder is created for a refused file");
    assert!(f.rows(id).is_empty());
    // Counter-check: the same checks let a proper file through.
    f.add(id, "ok.png", PNG);
    assert_eq!(f.rows(id).len(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_is_not_accepted_as_a_source() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let real = f.source("real.png", PNG);
    let link = f.outside.join("link.png");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(add_companion_with_conn(&f.conn, id, &link, &[]).is_err());
    assert!(!f.root.join("Foo_extras").exists());
    assert!(add_companion_with_conn(&f.conn, id, &real, &[]).is_ok(), "counter-check: the real file works");
}

#[test]
fn unsafe_source_paths_are_refused() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let source = f.source("pic.png", PNG);
    let sensitive = vec![f.outside.clone()];
    assert!(add_companion_with_conn(&f.conn, id, &source, &sensitive).is_err(), "inside a protected folder");
    assert!(add_companion_with_conn(&f.conn, id, Path::new("pic.png"), &[]).is_err(), "relative");
    let dotdot = f.outside.join("..").join("outside").join("pic.png");
    assert!(add_companion_with_conn(&f.conn, id, &dotdot, &[]).is_err(), "with ..");
    assert!(add_companion_with_conn(&f.conn, id, &f.outside, &[]).is_err(), "a folder");
    assert!(add_companion_with_conn(&f.conn, id, &f.outside.join("missing.png"), &[]).is_err(), "missing");
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_ok(), "counter-check");
}

#[test]
fn awkward_names_are_made_harmless() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let a = f.add(id, ".hidden.png", PNG);
    let b = f.add(id, "a:b*c?.PNG", PNG);
    assert_eq!(a.name, "hidden.png");
    assert_eq!(b.name, "a_b_c_.png");
    for row in [a, b] {
        assert_eq!(Path::new(&row.path).parent().unwrap(), f.root.join("Foo_extras"));
    }
}

#[test]
fn a_model_outside_every_catalog_folder_gets_no_companions() {
    let f = Fixture::new();
    let (id, _) = f.model_in(&f.outside.clone(), None, "Loose.3mf");
    let source = f.source("pic.png", PNG);
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err());
    assert!(!f.outside.join("Loose_extras").exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_extras_folder_is_not_used() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let elsewhere = f.base.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, f.root.join("Foo_extras")).unwrap();
    let source = f.source("pic.png", PNG);
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err());
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0, "nothing was written through the link");
}

#[test]
fn a_model_holds_at_most_100_companions_and_trashed_ones_do_not_count() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    for n in 0..MAX_COMPANIONS_PER_MODEL {
        db::companions::insert_companion(&f.conn, id, &format!("n{n}.txt"), &format!("/x/Foo_extras/n{n}.txt"), "text", None, "now").unwrap();
    }
    let source = f.source("one-more.txt", TEXT);
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err());
    let first = f.rows(id)[0].id;
    db::companions::soft_delete_companion(&f.conn, first, None, "now").unwrap();
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_ok());
}

#[test]
fn a_model_in_the_trash_gets_no_companions() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    f.trash_model(id);
    let source = f.source("pic.png", PNG);
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err());
}

// ---- trash cycle ----

#[test]
fn trashing_a_model_takes_its_companions_along_and_restoring_brings_them_back() {
    let f = Fixture::new();
    let (id, model) = f.model("Foo.3mf");
    let png = f.add(id, "pic.png", PNG);
    let txt = f.add(id, "license.txt", TEXT);

    f.trash_model(id);

    assert!(!model.exists());
    assert!(!f.root.join("Foo_extras").exists(), "the emptied folder is removed");
    let mut expected = vec![format!("companion-{}-license.txt", txt.id), format!("companion-{}-pic.png", png.id), format!("{id}-Foo.3mf")];
    expected.sort();
    assert_eq!(f.trash_entries(), expected);
    let model_row = db::get_file(&f.conn, id).unwrap().unwrap();
    for row in f.rows(id) {
        assert_eq!(row.deleted_at, model_row.deleted_at, "stamped with the model's time");
        assert!(row.trash_path.is_some());
    }

    f.restore_model(id);

    assert!(model.exists());
    assert_eq!(std::fs::read(f.root.join("Foo_extras").join("pic.png")).unwrap(), PNG);
    assert_eq!(std::fs::read(f.root.join("Foo_extras").join("license.txt")).unwrap(), TEXT);
    assert!(f.trash_entries().is_empty());
    for row in f.rows(id) {
        assert!(row.deleted_at.is_none() && row.trash_path.is_none(), "{row:?}");
        assert!(Path::new(&row.path).is_file());
    }
}

#[test]
fn restoring_a_model_leaves_a_separately_removed_companion_in_the_trash() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let gone = f.add(id, "old.png", PNG);
    let stays = f.add(id, "keep.png", PNG);
    remove_companion_with_conn(&f.conn, gone.id, &f.trash).unwrap();
    f.trash_model(id);
    f.restore_model(id);
    let rows = f.rows(id);
    let by_name = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    assert!(by_name("old.png").deleted_at.is_some(), "it was removed on its own earlier");
    assert!(by_name(&stays.name).deleted_at.is_none());
    assert!(Path::new(&by_name(&stays.name).path).is_file());
}

#[test]
fn removing_a_companion_moves_it_to_the_trash_and_restoring_puts_it_back_under_a_free_name() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    remove_companion_with_conn(&f.conn, row.id, &f.trash).unwrap();
    assert!(!Path::new(&row.path).exists());
    assert!(!f.root.join("Foo_extras").exists());
    assert_eq!(f.trash_entries(), vec![format!("companion-{}-pic.png", row.id)]);
    assert!(remove_companion_with_conn(&f.conn, row.id, &f.trash).is_err(), "not twice");

    // Something else took the name in the meantime: the restore never replaces it.
    std::fs::create_dir(f.root.join("Foo_extras")).unwrap();
    std::fs::write(f.root.join("Foo_extras").join("pic.png"), b"newer").unwrap();
    let restored = restore_companion_with_conn(&f.conn, row.id, &[]).unwrap();
    assert_eq!(restored.name, "pic (2).png");
    assert_eq!(std::fs::read(&restored.path).unwrap(), PNG);
    assert_eq!(std::fs::read(f.root.join("Foo_extras").join("pic.png")).unwrap(), b"newer");
    assert!(restored.deleted_at.is_none() && restored.trash_path.is_none());
    assert!(f.trash_entries().is_empty());
}

#[test]
fn a_companion_cannot_be_restored_while_its_model_is_in_the_trash() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    remove_companion_with_conn(&f.conn, row.id, &f.trash).unwrap();
    f.trash_model(id);
    assert!(restore_companion_with_conn(&f.conn, row.id, &[]).is_err());
}

#[test]
fn removing_a_companion_whose_file_is_gone_just_drops_the_entry() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    std::fs::remove_file(&row.path).unwrap();
    remove_companion_with_conn(&f.conn, row.id, &f.trash).unwrap();
    assert!(f.rows(id).is_empty());
}

#[test]
fn deleting_a_model_for_good_deletes_the_trash_files_of_its_companions_too() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    f.add(id, "pic.png", PNG);
    f.trash_model(id);
    assert_eq!(f.trash_entries().len(), 2);
    trash::delete_file_permanently_with_conn(&f.conn, id, &f.trash).unwrap();
    assert!(f.trash_entries().is_empty(), "{:?}", f.trash_entries());
    assert!(f.rows(id).is_empty());
    assert!(db::get_file(&f.conn, id).unwrap().is_none());
}

#[test]
fn emptying_the_trash_removes_companions_of_trashed_models_and_removed_ones() {
    let f = Fixture::new();
    let (trashed, _) = f.model("Foo.3mf");
    f.add(trashed, "a.png", PNG);
    let (kept, _) = f.model("Bar.3mf");
    let alone = f.add(kept, "b.png", PNG);
    let stays = f.add(kept, "c.png", PNG);
    remove_companion_with_conn(&f.conn, alone.id, &f.trash).unwrap();
    f.trash_model(trashed);
    trash::empty_trash_with_conn(&f.conn, &f.trash).unwrap();
    assert!(f.trash_entries().is_empty(), "{:?}", f.trash_entries());
    let left = f.rows(kept);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, stays.id);
    assert!(Path::new(&left[0].path).is_file(), "an active companion is never touched");
}

#[test]
fn expired_trash_is_purged_with_its_companions_and_fresh_trash_is_kept() {
    let f = Fixture::new();
    let (old, _) = f.model("Old.3mf");
    f.add(old, "a.png", PNG);
    let (fresh, _) = f.model("Fresh.3mf");
    f.add(fresh, "b.png", PNG);
    let (alone_model, _) = f.model("Alone.3mf");
    let alone = f.add(alone_model, "c.png", PNG);
    f.trash_model(old);
    f.trash_model(fresh);
    remove_companion_with_conn(&f.conn, alone.id, &f.trash).unwrap();
    let long_ago = (chrono::Utc::now() - chrono::Duration::days(8)).to_rfc3339();
    f.conn.execute("UPDATE files SET deleted_at = ?1 WHERE id = ?2", rusqlite::params![long_ago, old]).unwrap();
    f.conn.execute("UPDATE companion_files SET deleted_at = ?1 WHERE file_id IN (?2, ?3)", rusqlite::params![long_ago, old, alone_model]).unwrap();

    trash::purge_expired_trash_on_startup(&f.conn, &f.trash);

    assert!(db::get_file(&f.conn, old).unwrap().is_none());
    assert!(f.rows(old).is_empty() && f.rows(alone_model).is_empty());
    assert_eq!(f.trash_entries().len(), 2, "only the fresh model and its companion remain: {:?}", f.trash_entries());
    assert_eq!(f.rows(fresh).len(), 1);
}

#[test]
fn a_companion_row_pointing_outside_the_trash_never_causes_a_delete() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let precious = f.source("precious.png", PNG);
    let planted = |f: &Fixture| {
        let row = db::companions::insert_companion(&f.conn, id, "x.png", &f.root.join("Foo_extras").join("x.png").to_string_lossy(), "image", None, "now").unwrap();
        f.conn
            .execute(
                "UPDATE companion_files SET deleted_at = '2000-01-01T00:00:00+00:00', trash_path = ?1 WHERE id = ?2",
                rusqlite::params![precious.to_string_lossy(), row],
            )
            .unwrap();
    };
    planted(&f);
    trash::empty_trash_with_conn(&f.conn, &f.trash).unwrap();
    assert!(precious.exists(), "emptying the trash");
    planted(&f);
    trash::purge_expired_trash_on_startup(&f.conn, &f.trash);
    assert!(precious.exists(), "the startup cleanup");
}

#[test]
fn trashing_a_folder_takes_the_companions_along_so_the_folder_can_go() {
    let f = Fixture::new();
    let sub = f.root.join("sub");
    let sub_id = db::insert_folder_with_parent(&f.conn, "sub", Some(f.folder), &sub.to_string_lossy()).unwrap();
    let (id, _) = f.model_in(&sub, Some(sub_id), "Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    assert_eq!(folder_trash::trash_folder_for_test(&f.conn, sub_id, &f.trash), 1);
    assert!(!sub.exists(), "the folder is empty once model and companions are in the trash");
    assert!(f.trash_entries().contains(&format!("companion-{}-pic.png", row.id)));
}

// ---- moving and renaming the model ----

fn second_folder(f: &Fixture, name: &str) -> (i64, PathBuf) {
    let dir = f.root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    (db::insert_folder_with_parent(&f.conn, name, Some(f.folder), &dir.to_string_lossy()).unwrap(), dir)
}

#[test]
fn moving_a_model_moves_its_companions_into_the_new_folder() {
    let f = Fixture::new();
    let (a, a_dir) = second_folder(&f, "A");
    let (b, b_dir) = second_folder(&f, "B");
    let (id, _) = f.model_in(&a_dir, Some(a), "Foo.3mf");
    f.add(id, "pic.png", PNG);
    f.add(id, "license.txt", TEXT);

    files::move_file_to_folder_with_conn(&f.conn, id, Some(b), &[]).unwrap();

    assert!(b_dir.join("Foo.3mf").is_file());
    assert_eq!(std::fs::read(b_dir.join("Foo_extras").join("pic.png")).unwrap(), PNG);
    assert!(!a_dir.join("Foo_extras").exists(), "the old folder is gone");
    for row in f.rows(id) {
        assert!(Path::new(&row.path).starts_with(b_dir.join("Foo_extras")), "{}", row.path);
        assert!(Path::new(&row.path).is_file());
    }
}

#[test]
fn a_clash_in_the_new_folder_stops_the_move_and_loses_nothing() {
    let f = Fixture::new();
    let (a, a_dir) = second_folder(&f, "A");
    let (b, b_dir) = second_folder(&f, "B");
    let (id, model) = f.model_in(&a_dir, Some(a), "Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    std::fs::create_dir(b_dir.join("Foo_extras")).unwrap();
    std::fs::write(b_dir.join("Foo_extras").join("pic.png"), b"in the way").unwrap();

    assert!(files::move_file_to_folder_with_conn(&f.conn, id, Some(b), &[]).is_err());

    assert!(model.is_file() && !b_dir.join("Foo.3mf").exists(), "the model is back where it was");
    assert_eq!(std::fs::read(&row.path).unwrap(), PNG, "the companion never left");
    assert_eq!(std::fs::read(b_dir.join("Foo_extras").join("pic.png")).unwrap(), b"in the way", "nothing was overwritten");
    assert_eq!(f.rows(id)[0].path, row.path);
    assert_eq!(db::get_file(&f.conn, id).unwrap().unwrap().folder_id, Some(a));
}

#[test]
fn moving_a_model_does_not_take_the_companions_of_a_model_with_the_same_name_stem() {
    let f = Fixture::new();
    let (a, a_dir) = second_folder(&f, "A");
    let (b, b_dir) = second_folder(&f, "B");
    let (one, _) = f.model_in(&a_dir, Some(a), "Foo.3mf");
    let (two, _) = f.model_in(&a_dir, Some(a), "Foo.stl");
    f.add(one, "one.png", PNG);
    let other = f.add(two, "two.png", PNG);

    files::move_file_to_folder_with_conn(&f.conn, one, Some(b), &[]).unwrap();

    assert!(b_dir.join("Foo_extras").join("one.png").is_file());
    assert!(Path::new(&other.path).is_file(), "the other model's companion stays");
    assert_eq!(f.rows(two)[0].path, other.path);
}

#[test]
fn moving_a_model_whose_companion_file_is_gone_only_updates_the_entry() {
    let f = Fixture::new();
    let (a, a_dir) = second_folder(&f, "A");
    let (b, b_dir) = second_folder(&f, "B");
    let (id, _) = f.model_in(&a_dir, Some(a), "Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    std::fs::remove_file(&row.path).unwrap();
    files::move_file_to_folder_with_conn(&f.conn, id, Some(b), &[]).unwrap();
    assert_eq!(f.rows(id)[0].path, b_dir.join("Foo_extras").join("pic.png").to_string_lossy());
}

#[test]
fn renaming_a_model_renames_its_companion_folder() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    f.add(id, "pic.png", PNG);
    files::rename_file_with_conn(&f.conn, id, "Bar.3mf".into(), &[]).unwrap();
    assert!(f.root.join("Bar.3mf").is_file());
    assert_eq!(std::fs::read(f.root.join("Bar_extras").join("pic.png")).unwrap(), PNG);
    assert!(!f.root.join("Foo_extras").exists());
    assert_eq!(f.rows(id)[0].path, f.root.join("Bar_extras").join("pic.png").to_string_lossy());
}

#[test]
fn renaming_to_a_name_whose_companion_folder_is_taken_stops_and_loses_nothing() {
    let f = Fixture::new();
    let (id, model) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    std::fs::create_dir(f.root.join("Bar_extras")).unwrap();
    std::fs::write(f.root.join("Bar_extras").join("pic.png"), b"in the way").unwrap();
    assert!(files::rename_file_with_conn(&f.conn, id, "Bar.3mf".into(), &[]).is_err());
    assert!(model.is_file() && !f.root.join("Bar.3mf").exists());
    assert_eq!(std::fs::read(&row.path).unwrap(), PNG);
    assert_eq!(db::get_file(&f.conn, id).unwrap().unwrap().name, "Foo.3mf");
}

#[test]
fn changing_only_the_extension_keeps_the_companion_folder() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    files::rename_file_with_conn(&f.conn, id, "Foo.stl".into(), &[]).unwrap();
    assert_eq!(f.rows(id)[0].path, row.path);
    assert!(Path::new(&row.path).is_file());
}

// ---- opening and previews ----

#[test]
fn open_info_and_image_data_work_only_for_allowed_content() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    let doc = f.add(id, "doc.pdf", PDF);
    let info = open_info_with_conn(&f.conn, pic.id, &[]).unwrap();
    assert_eq!((info.kind.as_str(), info.path.as_str()), ("image", pic.path.as_str()));
    assert_eq!(open_info_with_conn(&f.conn, doc.id, &[]).unwrap().kind, "pdf");
    let data = image_data_with_conn(&f.conn, pic.id, &[]).unwrap();
    assert!(data.starts_with("data:image/png;base64,"));
    assert!(image_data_with_conn(&f.conn, doc.id, &[]).is_err(), "only pictures");
}

#[test]
fn a_swapped_file_is_refused_when_it_is_opened_or_previewed() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    std::fs::write(&pic.path, EXE).unwrap();
    assert!(open_info_with_conn(&f.conn, pic.id, &[]).is_err());
    assert!(image_data_with_conn(&f.conn, pic.id, &[]).is_err());
}

#[test]
fn a_crafted_row_cannot_be_used_to_open_or_read_an_arbitrary_file() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let secret = f.source("secret.png", PNG);
    let wrong_folder = db::companions::insert_companion(&f.conn, id, "secret.png", &secret.to_string_lossy(), "image", None, "now").unwrap();
    assert!(open_info_with_conn(&f.conn, wrong_folder, &[]).is_err(), "not inside an _extras folder");
    assert!(image_data_with_conn(&f.conn, wrong_folder, &[]).is_err());
    let extras = f.base.join("somewhere_extras");
    std::fs::create_dir(&extras).unwrap();
    std::fs::write(extras.join("tool.exe"), EXE).unwrap();
    let wrong_kind = db::companions::insert_companion(&f.conn, id, "tool.exe", &extras.join("tool.exe").to_string_lossy(), "image", None, "now").unwrap();
    assert!(open_info_with_conn(&f.conn, wrong_kind, &[]).is_err(), "an executable is never opened");
    let dotdot = db::companions::insert_companion(&f.conn, id, "secret.png", &format!("{}/somewhere_extras/../outside/secret.png", f.base.display()), "image", None, "now").unwrap();
    assert!(open_info_with_conn(&f.conn, dotdot, &[]).is_err());
    assert!(remove_companion_with_conn(&f.conn, wrong_folder, &f.trash).is_err(), "nor moved to the trash");
    assert!(secret.exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_companion_is_neither_opened_nor_previewed() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    let real = f.source("real.png", PNG);
    std::fs::remove_file(&pic.path).unwrap();
    std::os::unix::fs::symlink(&real, &pic.path).unwrap();
    assert!(open_info_with_conn(&f.conn, pic.id, &[]).is_err());
    assert!(image_data_with_conn(&f.conn, pic.id, &[]).is_err());
    assert!(remove_companion_with_conn(&f.conn, pic.id, &f.trash).is_err());
    assert!(real.exists());
}

#[test]
fn a_big_picture_is_refused_for_the_preview_but_still_listed() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    sparse_file(Path::new(&pic.path), PNG, MAX_PREVIEW_BYTES + 1);
    assert!(image_data_with_conn(&f.conn, pic.id, &[]).is_err());
    assert!(open_info_with_conn(&f.conn, pic.id, &[]).is_ok(), "opening externally has no preview limit");
}

#[test]
fn the_listing_flags_a_missing_file_without_changing_the_catalog() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    let before = f.rows(id);
    assert!(!to_dto(before[0].clone()).missing);
    std::fs::remove_file(&pic.path).unwrap();
    assert!(to_dto(f.rows(id)[0].clone()).missing);
    assert_eq!(f.rows(id), before, "the entry itself is unchanged");
    assert!(db::get_file(&f.conn, id).unwrap().is_some());
}

#[test]
fn stored_rows_are_validated_like_the_app_writes_them() {
    let good = |name: &str, kind: &str| validate_stored_fields(name, &format!("/c/Foo_extras/{name}"), kind);
    assert!(good("a.png", "image").is_ok());
    assert!(good("a.png", "pdf").is_err(), "kind must match the extension");
    assert!(good("a.exe", "image").is_err());
    assert!(good("a.png", "exe").is_err());
    assert!(validate_stored_fields("a.png", "/c/Foo/a.png", "image").is_err(), "not an _extras folder");
    assert!(validate_stored_fields("a.png", "/c/Foo_extras/../a.png", "image").is_err());
    assert!(validate_stored_fields("a.png", "Foo_extras/a.png", "image").is_err(), "relative");
    assert!(validate_stored_fields("b.png", "/c/Foo_extras/a.png", "image").is_err(), "name must match the path");
}

/// Replaces the extras folder of `Foo.3mf` by a link to a foreign folder that holds a
/// file of the same name, the way a swapped folder would look.
#[cfg(unix)]
fn swap_extras_for_link_to_foreign_folder(f: &Fixture, name: &str) -> (PathBuf, PathBuf) {
    let real = f.root.join("Foo_extras");
    std::fs::rename(&real, f.base.join("Foo_extras_real")).unwrap();
    let foreign = f.base.join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let foreign_file = foreign.join(name);
    std::fs::write(&foreign_file, PNG).unwrap();
    std::os::unix::fs::symlink(&foreign, &real).unwrap();
    (foreign, foreign_file)
}

#[cfg(unix)]
#[test]
fn a_companion_reached_through_a_linked_extras_folder_is_never_touched() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let pic = f.add(id, "pic.png", PNG);
    let (foreign, foreign_file) = swap_extras_for_link_to_foreign_folder(&f, "pic.png");
    assert!(remove_companion_with_conn(&f.conn, pic.id, &f.trash).is_err());
    assert!(open_info_with_conn(&f.conn, pic.id, &[]).is_err());
    assert!(image_data_with_conn(&f.conn, pic.id, &[]).is_err());
    let source = f.source("other.png", PNG);
    assert!(add_companion_with_conn(&f.conn, id, &source, &[]).is_err());
    assert!(foreign_file.exists(), "the foreign file stays where it is");
    assert_eq!(std::fs::read_dir(&foreign).unwrap().count(), 1);
    assert!(f.trash_entries().is_empty());
    assert!(f.rows(id)[0].deleted_at.is_none());
}

#[cfg(unix)]
#[test]
fn a_linked_extras_folder_stops_the_companions_following_a_model() {
    let f = Fixture::new();
    let (id, old_model) = f.model("Foo.3mf");
    f.add(id, "pic.png", PNG);
    let (_, foreign_file) = swap_extras_for_link_to_foreign_folder(&f, "pic.png");
    let new_model = f.root.join("Bar.3mf");
    assert!(carry(&f.conn, id, &old_model, &new_model).is_err());
    assert!(foreign_file.exists());
    assert!(!f.root.join("Bar_extras").exists());
}

#[cfg(unix)]
#[test]
fn a_linked_extras_folder_is_not_trashed_with_the_model() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    f.add(id, "pic.png", PNG);
    let (_, foreign_file) = swap_extras_for_link_to_foreign_folder(&f, "pic.png");
    f.trash_model(id);
    assert!(foreign_file.exists());
    assert_eq!(f.trash_entries().len(), 1, "only the model itself");
}

#[test]
fn a_companion_name_that_a_trashed_entry_still_holds_is_not_reused() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let first = f.add(id, "a.txt", TEXT);
    remove_companion_with_conn(&f.conn, first.id, &f.trash).unwrap();
    let second = f.add(id, "a.txt", TEXT);
    assert_eq!(second.name, "a (2).txt");
    assert_eq!(f.rows(id).len(), 2);
    // The restored entry may take its own old path back; the two never clash.
    let restored = restore_companion_with_conn(&f.conn, first.id, &[]).unwrap();
    assert_eq!(restored.name, "a.txt");
    assert_eq!(std::fs::read(&restored.path).unwrap(), TEXT);
}

#[test]
fn a_failed_restore_leaves_name_path_and_trash_state_as_they_were() {
    let f = Fixture::new();
    let (id, _) = f.model("Foo.3mf");
    let row = f.add(id, "pic.png", PNG);
    remove_companion_with_conn(&f.conn, row.id, &f.trash).unwrap();
    // Occupy the first free name with a row whose file is gone, so the row update is
    // the step that can fail; the file move must be undone and the row stay untouched.
    std::fs::create_dir(f.root.join("Foo_extras")).unwrap();
    std::fs::write(f.root.join("Foo_extras").join("pic.png"), b"newer").unwrap();
    f.conn
        .execute_batch("CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
        .unwrap();
    let before = db::companions::get_companion(&f.conn, row.id).unwrap().unwrap();
    assert!(restore_companion_with_conn(&f.conn, row.id, &[]).is_err());
    let after = db::companions::get_companion(&f.conn, row.id).unwrap().unwrap();
    assert_eq!((&after.name, &after.path, &after.trash_path, &after.deleted_at), (&before.name, &before.path, &before.trash_path, &before.deleted_at));
    assert_eq!(f.trash_entries(), vec![format!("companion-{}-pic.png", row.id)]);
    assert_eq!(std::fs::read_dir(f.root.join("Foo_extras")).unwrap().count(), 1, "only the other file");
}

#[test]
fn a_row_outside_the_models_folder_is_not_moved_along_with_the_model() {
    let f = Fixture::new();
    let (id, old_model) = f.model("Foo.3mf");
    let own = f.add(id, "pic.png", PNG);
    let stray = f.source("stray.png", PNG);
    let row = db::companions::insert_companion(&f.conn, id, "stray.png", &stray.to_string_lossy(), "image", Some(1), "now").unwrap();
    let new_model = f.root.join("Bar.3mf");
    std::fs::rename(&old_model, &new_model).unwrap();
    let carried = carry(&f.conn, id, &old_model, &new_model).unwrap();
    carried.finish();
    assert!(stray.exists(), "a file outside the folder stays where it is");
    assert_eq!(db::companions::get_companion(&f.conn, row).unwrap().unwrap().path, stray.to_string_lossy());
    assert!(f.root.join("Bar_extras").join("pic.png").exists(), "counter-check: the model's own file moves");
    assert_ne!(db::companions::get_companion(&f.conn, own.id).unwrap().unwrap().path, own.path);
}

#[test]
fn a_case_only_rename_leaves_a_folder_shared_with_another_model_alone() {
    let f = Fixture::new();
    let (a, stl) = f.model("a.stl");
    let (b, _) = f.model("a.3mf");
    let first = f.add(a, "one.png", PNG);
    let second = f.add(b, "two.png", PNG);
    assert_eq!(Path::new(&first.path).parent(), Path::new(&second.path).parent(), "precondition: one shared folder");
    let new_model = f.root.join("A.stl");
    let carried = carry_with(&f.conn, a, &stl, &new_model, true).unwrap();
    carried.finish();
    assert!(Path::new(&second.path).exists(), "the other model's file is still where its row says");
    assert_eq!(f.rows(b)[0].path, second.path);
    assert_eq!(f.rows(a)[0].path, first.path);
}

#[test]
fn a_case_only_rename_still_renames_a_folder_nobody_shares() {
    let f = Fixture::new();
    let (a, stl) = f.model("a.stl");
    f.add(a, "one.png", PNG);
    let new_model = f.root.join("A.stl");
    let carried = carry_with(&f.conn, a, &stl, &new_model, true).unwrap();
    carried.finish();
    let row = &f.rows(a)[0];
    assert!(row.path.contains("A_extras"), "{}", row.path);
    assert!(Path::new(&row.path).exists());
}
