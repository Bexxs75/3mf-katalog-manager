use super::companion_carry_tests::{Fx, PNG};
use super::tests::{PointAction, POINT};
use super::*;
use std::{fs, os::unix::fs::symlink};

fn removed_branch(companion: bool) -> Fx {
    let mut f = Fx::new();
    f.model(1, "a/b/c/model.3mf");
    let folder = crate::db::ensure_folder_path(&f.ops.db, &f.p("a"), &f.p("a/b/c")).unwrap();
    f.ops
        .db
        .execute("UPDATE files SET folder_id=?1 WHERE id=1", [folder])
        .unwrap();
    if companion {
        f.companion(1, "a/b/c/model_extras/pic.png");
    }
    f.ops.delete_file(1).unwrap();
    f.ops
        .db
        .execute("INSERT INTO folder_trash_origin(file_id) VALUES(1)", [])
        .unwrap();
    f.ops
        .db
        .execute("UPDATE files SET folder_id=NULL WHERE id=1", [])
        .unwrap();
    f.ops
        .db
        .execute(
            "DELETE FROM folders WHERE path<>?1",
            [f.p("sub").to_str().unwrap()],
        )
        .unwrap();
    fs::remove_dir_all(f.p("a")).unwrap();
    f
}

fn assert_restored(f: &mut Fx, companion: bool) {
    let s = f.ops.state(1).unwrap();
    assert!(s.deleted_at.is_none() && s.trash_path.is_none());
    assert_eq!(fs::read(f.p("a/b/c/model.3mf")).unwrap(), b"model 1");
    let path: String = f
        .ops
        .db
        .query_row(
            "SELECT path FROM folders WHERE id=?1",
            [s.folder_id.unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(path, f.p("a/b/c").to_str().unwrap());
    for (path, parent) in [("a", None), ("a/b", Some("a")), ("a/b/c", Some("a/b"))] {
        let actual: Option<String> = f.ops.db.query_row(
            "SELECT p.path FROM folders f LEFT JOIN folders p ON p.id=f.parent_id WHERE f.path=?1",
            [f.p(path).to_str().unwrap()], |r| r.get(0),
        ).unwrap();
        assert_eq!(actual, parent.map(|p| f.p(p).to_str().unwrap().to_owned()));
    }
    let count: i64 = f
        .ops
        .db
        .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    if companion {
        assert_eq!(fs::read(f.p("a/b/c/model_extras/pic.png")).unwrap(), PNG);
        let active: bool = f.ops.db.query_row("SELECT deleted_at IS NULL AND trash_path IS NULL FROM companion_files WHERE file_id=1", [], |r| r.get(0)).unwrap();
        assert!(active);
    }
}

fn recover(f: &mut Fx) {
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    f.ops.finish_recovery().unwrap();
    assert!(f.ops.journal.is_empty());
}

#[test]
fn restore_recreates_three_levels_and_commits_folders_and_companions() {
    for companion in [false, true] {
        let mut f = removed_branch(companion);
        f.ops.restore_file(1).unwrap();
        assert_restored(&mut f, companion);
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn restore_rejects_an_intermediate_symlink_without_touching_its_target() {
    let mut f = removed_branch(false);
    fs::create_dir(f.p("a")).unwrap();
    symlink(f.p("sub"), f.p("a/b")).unwrap();
    assert!(f.ops.restore_file(1).is_err());
    assert!(!f.p("sub/c").exists());
    assert!(f.ops.state(1).unwrap().deleted_at.is_some());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn recovery_after_each_mkdir_removes_only_the_created_empty_suffix() {
    for index in 0..3 {
        let mut f = removed_branch(true);
        let before = f.ops.state(1).unwrap();
        POINT.with(|p| *p.borrow_mut() = Some(("restore_after_mkdir", index, PointAction::Crash)));
        assert_eq!(f.ops.restore_file(1), Err(Error::RestartRequired));
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(f.dir.join("journal/web-fsops.json")).unwrap())
                .unwrap();
        assert_eq!(document["version"], 3);
        recover(&mut f);
        assert!(!f.p("a").exists());
        assert_eq!(f.ops.state(1).unwrap(), before);
        let count: i64 = f
            .ops
            .db
            .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        f.ops.restore_file(1).unwrap();
        assert_restored(&mut f, true);
    }
}

#[test]
fn recovery_after_rename_or_commit_finishes_the_same_restore_transaction() {
    for phase in [Phase::Renamed, Phase::Committed] {
        for companion in [false, true] {
            let mut f = removed_branch(companion);
            f.ops.journal.crash_at = Some(phase);
            assert_eq!(f.ops.restore_file(1), Err(Error::RestartRequired));
            recover(&mut f);
            assert_restored(&mut f, companion);
            recover(&mut f);
            assert_restored(&mut f, companion);
        }
    }
}

#[test]
fn recovery_keeps_unrecorded_replaced_and_nonempty_directories() {
    for mode in ["unrecorded", "replaced", "nonempty"] {
        let mut f = removed_branch(false);
        let point = if mode == "unrecorded" {
            "restore_mkdir_unrecorded"
        } else {
            "restore_after_mkdir"
        };
        POINT.with(|p| *p.borrow_mut() = Some((point, 0, PointAction::Crash)));
        assert_eq!(f.ops.restore_file(1), Err(Error::RestartRequired));
        if mode == "replaced" {
            fs::rename(f.p("a"), f.p("saved")).unwrap();
            fs::create_dir(f.p("a")).unwrap();
        } else if mode == "nonempty" {
            fs::write(f.p("a/foreign"), b"keep").unwrap();
        }
        recover(&mut f);
        assert!(f.p("a").is_dir());
        if mode == "nonempty" {
            assert_eq!(fs::read(f.p("a/foreign")).unwrap(), b"keep");
        }
        assert!(f.ops.state(1).unwrap().deleted_at.is_some());
    }
}

#[test]
fn failed_commit_rolls_back_files_companions_and_created_parents() {
    let mut f = removed_branch(true);
    f.ops.db.execute_batch("CREATE TRIGGER fail_restore BEFORE DELETE ON folder_trash_origin BEGIN SELECT RAISE(ABORT, 'test'); END;").unwrap();
    assert!(f.ops.restore_file(1).is_err());
    assert!(!f.p("a").exists());
    assert!(f.ops.state(1).unwrap().deleted_at.is_some());
    assert!(f.ops.journal.is_empty());
    let count: i64 = f
        .ops
        .db
        .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn name_conflict_keeps_the_existing_file_and_preexisting_parents() {
    let mut f = removed_branch(false);
    fs::create_dir_all(f.p("a/b/c")).unwrap();
    fs::write(f.p("a/b/c/model.3mf"), b"foreign").unwrap();
    assert_eq!(f.ops.restore_file(1), Err(Error::Conflict));
    assert_eq!(fs::read(f.p("a/b/c/model.3mf")).unwrap(), b"foreign");
    assert!(f.ops.state(1).unwrap().deleted_at.is_some());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn folder_origin_relinks_existing_directories_and_db_only_restore() {
    for db_only in [false, true] {
        let mut f = removed_branch(false);
        fs::create_dir_all(f.p("a/b/c")).unwrap();
        crate::db::printer_link::set_setting(
            &f.ops.db,
            "catalog_base_dir",
            f.p("a").to_str().unwrap(),
        )
        .unwrap();
        if db_only {
            fs::remove_file(f.ops.state(1).unwrap().trash_path.unwrap()).unwrap();
            f.ops
                .db
                .execute("UPDATE files SET trash_path=NULL WHERE id=1", [])
                .unwrap();
        }
        f.ops.restore_file(1).unwrap();
        let state = f.ops.state(1).unwrap();
        assert!(state.folder_id.is_some());
        assert!(state.deleted_at.is_none() && state.trash_path.is_none());
        let count: i64 = f
            .ops
            .db
            .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn recovery_rolls_back_companions_and_preserves_existing_ancestors() {
    let mut f = removed_branch(true);
    fs::create_dir(f.p("a")).unwrap();
    f.ops.journal.crash_at = Some(Phase::CompanionsMoved);
    assert_eq!(f.ops.restore_file(1), Err(Error::RestartRequired));
    recover(&mut f);
    assert!(f.p("a").is_dir());
    assert!(!f.p("a/b").exists());
    assert!(f.ops.state(1).unwrap().deleted_at.is_some());
    let trash: String = f
        .ops
        .db
        .query_row(
            "SELECT trash_path FROM companion_files WHERE file_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fs::read(trash).unwrap(), PNG);
}

#[test]
fn restore_refuses_missing_children_of_a_protected_directory() {
    let mut f = Fx::with_protected(&[("upload", Protection::Upload)]);
    f.model(1, "upload/model.3mf");
    f.ops.delete_file(1).unwrap();
    f.ops
        .db
        .execute(
            "UPDATE files SET path=?1 WHERE id=1",
            [f.p("upload/new/model.3mf").to_str().unwrap()],
        )
        .unwrap();
    assert_eq!(f.ops.restore_file(1), Err(Error::Protected));
    assert!(!f.p("upload/new").exists());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn restore_keeps_the_existing_upload_and_catalog_base_rules() {
    for protection in [Protection::Upload, Protection::CatalogBase] {
        let mut f = Fx::with_protected(&[("base", protection)]);
        f.model(1, "base/model.3mf");
        f.ops.delete_file(1).unwrap();
        f.ops.restore_file(1).unwrap();
        assert!(f.p("base/model.3mf").is_file());
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn restore_parent_metadata_requires_the_third_journal_format() {
    let mut f = removed_branch(false);
    POINT.with(|p| *p.borrow_mut() = Some(("restore_after_mkdir", 0, PointAction::Crash)));
    assert_eq!(f.ops.restore_file(1), Err(Error::RestartRequired));
    let path = f.dir.join("journal/web-fsops.json");
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for version in [1, 2, 4] {
        let mut changed = document.clone();
        changed["version"] = version.into();
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(matches!(
            Journal::open(&f.dir.join("journal")),
            Err(Error::RestartRequired)
        ));
    }
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    recover(&mut f);
    assert!(!f.p("a").exists());
}
