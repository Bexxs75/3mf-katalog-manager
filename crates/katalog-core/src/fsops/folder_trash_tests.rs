use super::companion_carry_tests::{Fx, PNG};
use super::tests::{GenerousBatchBudget, PointAction, POINT};
use super::*;
use std::fs;

/// a -> {b, c}; models a/m0, a/b/m1 (with a companion picture), a/c/m2; an unknown file in b.
fn tree(f: &mut Fx) {
    for leaf in ["a/b", "a/c"] {
        crate::db::ensure_folder_path(&f.ops.db, &f.p("a"), &f.p(leaf)).unwrap();
    }
    for (id, rel, folder) in [(10, "a/m0.3mf", "a"), (11, "a/b/m1.3mf", "a/b"), (12, "a/c/m2.3mf", "a/c")] {
        f.model(id, rel);
        let folder_id = folder_id(f, folder);
        f.ops.db.execute("UPDATE files SET folder_id=?1 WHERE id=?2", rusqlite::params![folder_id, id]).unwrap();
    }
    f.companion(11, "a/b/m1_extras/pic.png");
    fs::write(f.p("a/b/notes.txt"), b"keep me").unwrap();
}
fn folder_id(f: &Fx, rel: &str) -> i64 {
    f.ops.db.query_row("SELECT id FROM folders WHERE path=?1", [f.p(rel).to_str().unwrap()], |r| r.get(0)).unwrap()
}
fn count(f: &Fx, sql: &str) -> i64 {
    f.ops.db.query_row(sql, [], |r| r.get(0)).unwrap()
}
fn markers(f: &Fx) -> i64 {
    count(f, "SELECT COUNT(*) FROM folder_trash_origin")
}
fn tree_folder_rows(f: &Fx) -> i64 {
    count(f, "SELECT COUNT(*) FROM folders WHERE path LIKE '%/a' OR path LIKE '%/a/%'")
}
fn trash_all(f: &mut Fx, id: i64) -> FolderTrashBlock {
    let mut snapshot = f.ops.folder_trash_snapshot(id).unwrap();
    let mut last = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    while last.remaining > 0 {
        last = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    }
    last
}
fn recover(f: &mut Fx) {
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    f.ops.finish_recovery().unwrap();
    assert!(f.ops.journal.is_empty());
}

#[test]
fn folder_trash_moves_models_with_companions_marks_origin_and_drops_the_folder_rows() {
    let mut f = Fx::new();
    tree(&mut f);
    let a = folder_id(&f, "a");
    let block = trash_all(&mut f, a);
    assert_eq!(block.results.len(), 3);
    assert!(block.results.iter().all(|r| r.ok), "{:?}", block.results);
    assert_eq!(block.removed_folder_count, Some(3));
    assert_eq!(block.kept_folder_count, Some(0));
    assert_eq!(block.folder_error, None);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL AND trash_path IS NOT NULL"), 3);
    assert_eq!(markers(&f), 3);
    assert_eq!(tree_folder_rows(&f), 0);
    // Only the unrelated folder row is left; the directories and unknown files stay on disk.
    assert_eq!(count(&f, "SELECT COUNT(*) FROM folders"), 1);
    assert_eq!(fs::read(f.p("a/b/notes.txt")).unwrap(), b"keep me");
    assert!(!f.p("a/m0.3mf").exists() && !f.p("a/b/m1.3mf").exists());
    assert!(!f.p("a/b/m1_extras/pic.png").exists(), "the companion goes to the trash with its model");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn trashed_folder_branch_comes_back_model_by_model_after_it_was_removed_from_disk() {
    let mut f = Fx::new();
    tree(&mut f);
    let a = folder_id(&f, "a");
    trash_all(&mut f, a);
    fs::remove_dir_all(f.p("a")).unwrap();
    for id in [11, 10, 12] {
        f.ops.restore_file(id).unwrap();
    }
    assert_eq!(fs::read(f.p("a/b/m1.3mf")).unwrap(), b"model 11");
    assert_eq!(fs::read(f.p("a/m0.3mf")).unwrap(), b"model 10");
    assert_eq!(fs::read(f.p("a/c/m2.3mf")).unwrap(), b"model 12");
    assert_eq!(fs::read(f.p("a/b/m1_extras/pic.png")).unwrap(), PNG);
    for (id, rel) in [(10, "a"), (11, "a/b"), (12, "a/c")] {
        let s = f.ops.state(id).unwrap();
        assert!(s.deleted_at.is_none() && s.trash_path.is_none());
        assert_eq!(s.folder_id, Some(folder_id(&f, rel)), "model {id} must be linked to {rel}");
    }
    for (path, parent) in [("a/b", "a"), ("a/c", "a")] {
        let actual: String = f.ops.db.query_row(
            "SELECT p.path FROM folders f JOIN folders p ON p.id=f.parent_id WHERE f.path=?1",
            [f.p(path).to_str().unwrap()], |r| r.get(0)).unwrap();
        assert_eq!(actual, f.p(parent).to_str().unwrap());
    }
    assert_eq!(markers(&f), 0, "the marker is cleared with each restore");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_crash_while_restoring_a_trashed_folder_branch_recovers_and_the_restore_can_be_repeated() {
    for index in 0..2 {
        let mut f = Fx::new();
        tree(&mut f);
        let a = folder_id(&f, "a");
        trash_all(&mut f, a);
        fs::remove_dir_all(f.p("a")).unwrap();
        POINT.with(|p| *p.borrow_mut() = Some(("restore_after_mkdir", index, PointAction::Crash)));
        assert_eq!(f.ops.restore_file(11), Err(Error::RestartRequired));
        recover(&mut f);
        // Rolled back: still in the trash, still marked, no directory left over.
        assert!(f.ops.state(11).unwrap().deleted_at.is_some());
        assert_eq!(markers(&f), 3);
        assert!(!f.p("a").exists());
        for id in [11, 10, 12] {
            f.ops.restore_file(id).unwrap();
        }
        assert_eq!(markers(&f), 0);
        assert_eq!(tree_folder_rows(&f), 3);
        assert_eq!(fs::read(f.p("a/b/m1_extras/pic.png")).unwrap(), PNG);
    }
}

#[test]
fn a_model_that_cannot_move_keeps_its_folder_and_ancestors_in_the_catalog() {
    let mut f = Fx::new();
    tree(&mut f);
    // A second hard link makes the file unsafe to move.
    fs::hard_link(f.p("a/b/m1.3mf"), f.p("a/b/m1-link.3mf")).unwrap();
    let a = folder_id(&f, "a");
    let block = trash_all(&mut f, a);
    assert_eq!(block.results.iter().filter(|r| !r.ok).count(), 1);
    assert_eq!(block.kept_folder_count, Some(2));
    assert_eq!(block.removed_folder_count, Some(1));
    assert_eq!(tree_folder_rows(&f), 2, "a and a/b stay, a/c goes");
    assert!(f.ops.state(11).unwrap().deleted_at.is_none());
    assert_eq!(markers(&f), 2, "the model that stayed has no marker");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM folder_trash_origin WHERE file_id=11"), 0);
}

#[test]
fn large_folders_run_in_blocks_of_at_most_200_and_remove_the_folders_after_the_last() {
    let _budget = GenerousBatchBudget::new();
    let mut f = Fx::new();
    crate::db::ensure_folder_path(&f.ops.db, &f.p("a"), &f.p("a/b")).unwrap();
    let b = folder_id(&f, "a/b");
    for id in 1..=405 {
        let path = f.p(&format!("a/b/missing-{id}.3mf"));
        f.ops.db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at,folder_id) VALUES (?1,?2,?3,'3mf',1,'2026-01-01T00:00:00Z',?4)",
            rusqlite::params![id, format!("missing-{id}.3mf"), path.to_str().unwrap(), b]).unwrap();
    }
    let a = folder_id(&f, "a");
    let mut snapshot = f.ops.folder_trash_snapshot(a).unwrap();
    assert_eq!(snapshot.remaining(), 405);
    let first = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    assert_eq!((first.results.len(), first.remaining, first.removed_folder_count), (200, 205, None));
    assert_eq!(tree_folder_rows(&f), 2, "folder rows stay until the last block");
    let second = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    assert_eq!((second.results.len(), second.remaining), (200, 5));
    let third = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    assert_eq!((third.results.len(), third.remaining, third.removed_folder_count), (5, 0, Some(2)));
    assert_eq!(markers(&f), 405);
    assert_eq!(tree_folder_rows(&f), 0);
}

#[test]
fn a_block_that_is_stopped_hands_the_untouched_models_to_the_next_block() {
    let mut f = Fx::new();
    tree(&mut f);
    let a = folder_id(&f, "a");
    let mut snapshot = f.ops.folder_trash_snapshot(a).unwrap();
    let mut calls = 0;
    let first = f.ops.folder_trash_block(&mut snapshot, || { calls += 1; calls <= 1 }).unwrap();
    assert_eq!(first.results.iter().filter(|r| r.ok).count(), 1);
    assert_eq!(first.remaining, 2);
    assert_eq!(first.removed_folder_count, None);
    assert_eq!(markers(&f), 1, "stopped models carry no marker");
    let rest = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
    assert_eq!((rest.remaining, rest.removed_folder_count), (0, Some(3)));
    assert_eq!(markers(&f), 3);
}

#[test]
fn a_crash_in_each_phase_of_a_model_move_recovers_and_the_folder_can_be_trashed_again() {
    for phase in [Phase::Prepared, Phase::Renamed, Phase::Committed] {
        let mut f = Fx::new();
        tree(&mut f);
        let a = folder_id(&f, "a");
        let mut snapshot = f.ops.folder_trash_snapshot(a).unwrap();
        f.ops.journal.crash_at = Some(phase);
        let block = f.ops.folder_trash_block(&mut snapshot, || true).unwrap();
        assert_eq!(block.results[0].kind, Some(Error::RestartRequired), "{phase:?}");
        assert!(block.removed_folder_count.is_none() && tree_folder_rows(&f) == 3, "no folder row may go after a crash");
        // The process is gone: only the journal and the database survive.
        recover(&mut f);
        let trashed = f.ops.state(10).unwrap().deleted_at.is_some();
        assert_eq!(trashed, phase != Phase::Prepared, "{phase:?}");
        // A rolled-forward model keeps its marker; a rolled-back one has none after the next start.
        trash_all(&mut f, a);
        assert_eq!(count(&f, "SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL"), 3, "{phase:?}");
        assert_eq!(markers(&f), 3, "{phase:?}");
        assert_eq!(tree_folder_rows(&f), 0, "{phase:?}");
        fs::remove_dir_all(f.p("a")).unwrap();
        for id in [10, 11, 12] {
            f.ops.restore_file(id).unwrap();
        }
        assert_eq!(markers(&f), 0, "{phase:?}");
        assert_eq!(tree_folder_rows(&f), 3, "{phase:?}");
    }
}

#[test]
fn the_catalog_base_and_anything_above_it_are_refused_before_a_model_moves() {
    let mut f = Fx::new();
    tree(&mut f);
    let a = folder_id(&f, "a");
    f.ops.db.execute("INSERT INTO app_settings(key,value) VALUES('catalog_base_dir',?1)", [f.p("a/b").to_str().unwrap()]).unwrap();
    assert_eq!(f.ops.folder_trash_snapshot(a).unwrap_err(), Error::Protected);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL"), 0);
    assert_eq!(f.ops.folder_trash_snapshot(999).unwrap_err(), Error::NotFound);
}

#[test]
fn desktop_format_entries_in_a_mixed_trash_are_left_alone() {
    let mut f = Fx::new();
    tree(&mut f);
    // An entry the desktop trashed from this folder earlier: its file sits in the desktop trash folder.
    fs::create_dir_all(f.dir.join("trash")).unwrap();
    let desktop_file = f.dir.join("trash/20-old.3mf");
    fs::write(&desktop_file, b"desktop").unwrap();
    f.ops.db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at,deleted_at,trash_path) VALUES (20,'old.3mf',?1,'3mf',7,'2026-01-01T00:00:00Z','2026-01-02T00:00:00Z',?2)",
        rusqlite::params![f.p("a/old.3mf").to_str().unwrap(), desktop_file.to_str().unwrap()]).unwrap();
    f.ops.db.execute("INSERT INTO folder_trash_origin(file_id) VALUES(20)", []).unwrap();
    let a = folder_id(&f, "a");
    let block = trash_all(&mut f, a);
    assert!(block.results.iter().all(|r| r.ok) && block.results.len() == 3);
    assert_eq!(fs::read(&desktop_file).unwrap(), b"desktop");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM folder_trash_origin WHERE file_id=20"), 1);
    assert_eq!(markers(&f), 4);
    let ids = desktop_trash_ids(&f.ops.db).unwrap();
    assert_eq!(ids.iter().copied().collect::<Vec<_>>(), vec![20]);
    // The web restores its own entries; the desktop entry stays for the desktop.
    fs::remove_dir_all(f.p("a")).unwrap();
    for id in [10, 11, 12] {
        f.ops.restore_file(id).unwrap();
    }
    assert!(f.ops.restore_file(20).is_err());
    assert_eq!(fs::read(&desktop_file).unwrap(), b"desktop");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM folder_trash_origin WHERE file_id=20"), 1);
    assert_eq!(markers(&f), 1);
}
