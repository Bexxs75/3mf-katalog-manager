//! Renaming or moving a folder carries the `_extras` folders of the models inside it
//! (the directory entry moves as one) and rewrites the paths in `companion_files` in the
//! same transaction as `files.path`. Cases the folder cannot vouch for stay refused.
use super::companion_gap_tests::*;
use super::*;
use std::{fs, os::unix::fs::symlink, path::Path};

const DEEP_OLD: &str = "a/b/Deep_extras/pic.png";

fn p(f: &Fx, rel: &str) -> String {
    f.root().join(rel).to_str().unwrap().to_owned()
}

/// Every stored path, so "nothing changed" and "everything changed" are one comparison.
fn db_paths(f: &Fx) -> Vec<String> {
    let mut out = Vec::new();
    for sql in [
        "SELECT 'f:'||path FROM files ORDER BY id",
        "SELECT 'c:'||path||'|'||COALESCE(trash_path,'')||'|'||COALESCE(deleted_at,'') FROM companion_files ORDER BY id",
        "SELECT 'd:'||path FROM folders ORDER BY id",
    ] {
        let mut stmt = f.ops.db.prepare(sql).unwrap();
        out.extend(stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()));
    }
    out
}

fn deep_companion(f: &Fx) -> String {
    f.ops
        .db
        .query_row("SELECT path FROM companion_files WHERE file_id=3", [], |r| r.get(0))
        .unwrap()
}

fn nested() -> Fx {
    let mut f = fixture();
    add_nested(&mut f, true);
    f
}

fn reopen_and_recover(f: &mut Fx) -> RecoveryReport {
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    let report = f.ops.recover().unwrap();
    // A pending manual entry keeps the journal blocked, which `finish_recovery` reports.
    let finished = f.ops.finish_recovery();
    assert_eq!(finished.is_ok(), report.manual.is_empty(), "{finished:?}");
    report
}

// ---- Grundfälle ----

#[test]
fn rename_rewrites_files_and_companions_in_one_go_and_keeps_the_disk_file() {
    let mut f = nested();
    f.ops.rename_folder(2, "neu").unwrap();
    assert_eq!(fs::read(f.root().join("neu/b/Deep_extras/pic.png")).unwrap(), PNG);
    assert!(!exists(&f.root().join("a")));
    assert_eq!(deep_companion(&f), p(&f, "neu/b/Deep_extras/pic.png"));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn move_rewrites_companions_of_every_level() {
    let mut f = nested();
    // A second model one level up, with its own companion.
    let up = f.root().join("a/Up.3mf");
    fs::write(&up, b"m").unwrap();
    fs::create_dir(f.root().join("a/Up_extras")).unwrap();
    fs::write(f.root().join("a/Up_extras/x.png"), PNG).unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,folder_id,file_type,file_size_bytes,imported_at) VALUES (4,'Up.3mf',?1,2,'3mf',5,'2026-01-01T00:00:00Z')",
            [up.to_str().unwrap()],
        )
        .unwrap();
    crate::db::companions::insert_companion(
        &f.ops.db,
        4,
        "x.png",
        f.root().join("a/Up_extras/x.png").to_str().unwrap(),
        "image",
        Some(PNG.len() as i64),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    f.ops.move_folder(2, Some(1)).unwrap();
    assert!(exists(&f.root().join("sub/a/Up_extras/x.png")));
    assert!(exists(&f.root().join("sub/a/b/Deep_extras/pic.png")));
    let rows: Vec<String> = {
        let mut s = f.ops.db.prepare("SELECT path FROM companion_files WHERE file_id IN (3,4) ORDER BY id").unwrap();
        s.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    assert_eq!(rows, vec![p(&f, "sub/a/b/Deep_extras/pic.png"), p(&f, "sub/a/Up_extras/x.png")]);
    // The unrelated model Foo keeps its companion.
    assert_eq!(
        f.ops.db.query_row::<String, _, _>("SELECT path FROM companion_files WHERE file_id=1", [], |r| r.get(0)).unwrap(),
        p(&f, "Foo_extras/pic.png")
    );
}

#[test]
fn models_sharing_one_extras_folder_are_both_rewritten() {
    let mut f = nested();
    let stl = f.root().join("a/b/Deep.stl");
    fs::write(&stl, b"m").unwrap();
    fs::write(f.root().join("a/b/Deep_extras/two.png"), PNG).unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,folder_id,file_type,file_size_bytes,imported_at) VALUES (4,'Deep.stl',?1,3,'stl',5,'2026-01-01T00:00:00Z')",
            [stl.to_str().unwrap()],
        )
        .unwrap();
    crate::db::companions::insert_companion(
        &f.ops.db,
        4,
        "two.png",
        f.root().join("a/b/Deep_extras/two.png").to_str().unwrap(),
        "image",
        Some(PNG.len() as i64),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    f.ops.rename_folder(2, "neu").unwrap();
    assert!(exists(&f.root().join("neu/b/Deep_extras/two.png")));
    let n: i64 = f
        .ops
        .db
        .query_row("SELECT COUNT(*) FROM companion_files WHERE path LIKE ?1", [format!("{}/neu/b/Deep_extras/%", f.root().to_str().unwrap())], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn a_folder_whose_name_is_a_prefix_of_a_sibling_leaves_the_sibling_alone() {
    let mut f = nested();
    let ab = f.root().join("ab");
    fs::create_dir_all(ab.join("S_extras")).unwrap();
    fs::write(ab.join("S.3mf"), b"m").unwrap();
    fs::write(ab.join("S_extras/s.png"), PNG).unwrap();
    f.ops
        .db
        .execute("INSERT INTO folders(id,name,path) VALUES (7,'ab',?1)", [ab.to_str().unwrap()])
        .unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,folder_id,file_type,file_size_bytes,imported_at) VALUES (7,'S.3mf',?1,7,'3mf',5,'2026-01-01T00:00:00Z')",
            [ab.join("S.3mf").to_str().unwrap()],
        )
        .unwrap();
    crate::db::companions::insert_companion(&f.ops.db, 7, "s.png", ab.join("S_extras/s.png").to_str().unwrap(), "image", Some(1), "2026-01-01T00:00:00Z").unwrap();
    f.ops.rename_folder(2, "neu").unwrap();
    assert_eq!(
        f.ops.db.query_row::<String, _, _>("SELECT path FROM companion_files WHERE file_id=7", [], |r| r.get(0)).unwrap(),
        ab.join("S_extras/s.png").to_str().unwrap()
    );
    assert!(exists(&ab.join("S_extras/s.png")));
}

#[test]
fn a_companion_whose_file_is_gone_only_gets_its_row_rewritten() {
    let mut f = nested();
    fs::remove_file(f.root().join(DEEP_OLD)).unwrap();
    f.ops.rename_folder(2, "neu").unwrap();
    assert_eq!(deep_companion(&f), p(&f, "neu/b/Deep_extras/pic.png"));
    assert!(!exists(&f.root().join("neu/b/Deep_extras/pic.png")));
}

#[test]
fn case_only_rename_and_conflicts_still_work() {
    let mut f = nested();
    f.ops.rename_folder(2, "A").unwrap();
    assert_eq!(deep_companion(&f), p(&f, "A/b/Deep_extras/pic.png"));
    assert!(exists(&f.root().join("A/b/Deep_extras/pic.png")));

    // Target directory already on disk: refused, nothing moved or rewritten.
    let mut f = nested();
    fs::create_dir(f.root().join("neu")).unwrap();
    let before = db_paths(&f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::Conflict));
    assert_eq!(db_paths(&f), before);
    assert!(exists(&f.root().join(DEEP_OLD)));
    assert!(f.ops.journal.is_empty());

    // A companion row already holds a path the move would produce.
    let mut f = nested();
    let held = p(&f, "neu/b/Deep_extras/pic.png");
    f.ops
        .db
        .execute("UPDATE companion_files SET path=?1 WHERE file_id=1", [&held])
        .unwrap();
    let before = db_paths(&f);
    // The pre-check refuses it before anything moves; the unique column is only the last net.
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::Conflict));
    assert_eq!(db_paths(&f), before);
    assert!(exists(&f.root().join(DEEP_OLD)));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn web_trashed_model_in_the_tree_keeps_its_companions_and_restores_to_the_new_place() {
    let mut f = nested();
    f.ops.delete_file(3).unwrap();
    let (trash, deleted): (String, String) = f
        .ops
        .db
        .query_row("SELECT trash_path,deleted_at FROM companion_files WHERE file_id=3", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    f.ops.rename_folder(2, "neu").unwrap();
    let (path, trash_after, deleted_after): (String, String, String) = f
        .ops
        .db
        .query_row("SELECT path,trash_path,deleted_at FROM companion_files WHERE file_id=3", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!(path, p(&f, "neu/b/Deep_extras/pic.png"));
    assert_eq!((trash_after.as_str(), deleted_after.as_str()), (trash.as_str(), deleted.as_str()));
    f.ops.restore_file(3).unwrap();
    assert!(exists(&f.root().join("neu/b/Deep.3mf")));
    assert!(exists(&f.root().join("neu/b/Deep_extras/pic.png")));
    assert_eq!(deep_companion(&f), p(&f, "neu/b/Deep_extras/pic.png"));
}

// ---- Rest-Sperren ----

fn assert_blocked(f: &mut Fx) {
    let before = db_paths(f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert_eq!(f.ops.move_folder(2, Some(1)), Err(Error::HasCompanions));
    assert_eq!(db_paths(f), before, "database untouched");
    assert!(exists(&f.root().join("a")) && !exists(&f.root().join("neu")) && !exists(&f.root().join("sub/a")));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn symlinked_extras_folder_blocks() {
    let mut f = nested();
    let real = f.root().join("elsewhere");
    fs::rename(f.root().join("a/b/Deep_extras"), &real).unwrap();
    symlink(&real, f.root().join("a/b/Deep_extras")).unwrap();
    assert_blocked(&mut f);
    assert!(exists(&real.join("pic.png")));
}

#[test]
fn symlink_in_the_folder_path_blocks() {
    let mut f = nested();
    let real = f.root().join("elsewhere_b");
    fs::rename(f.root().join("a/b"), &real).unwrap();
    symlink(&real, f.root().join("a/b")).unwrap();
    assert_blocked(&mut f);
}

#[test]
fn symlinked_or_hardlinked_companion_file_blocks() {
    let mut f = nested();
    let pic = f.root().join(DEEP_OLD);
    fs::remove_file(&pic).unwrap();
    symlink(f.root().join("Foo.3mf"), &pic).unwrap();
    assert_blocked(&mut f);

    let mut f = nested();
    fs::hard_link(f.root().join(DEEP_OLD), f.root().join("a/b/Deep_extras/second-name")).unwrap();
    assert_blocked(&mut f);
}

#[test]
fn row_outside_the_folder_or_of_a_foreign_model_blocks() {
    // The tree's model has a row whose path is elsewhere.
    let mut f = nested();
    let outside = p(&f, "Foo_extras/other.png");
    f.ops.db.execute("UPDATE companion_files SET path=?1 WHERE file_id=3", [&outside]).unwrap();
    assert_blocked(&mut f);

    // A model outside the tree has a row whose path is inside it.
    let mut f = nested();
    let inside = p(&f, "a/b/Deep_extras/foreign.png");
    f.ops.db.execute("UPDATE companion_files SET path=?1 WHERE file_id=1", [&inside]).unwrap();
    assert_blocked(&mut f);
}

#[test]
fn desktop_trash_row_blocks_but_a_row_without_file_does_not() {
    let mut f = nested();
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/nonexistent/desktop-trash/companion-2-pic.png' WHERE file_id=3",
            [],
        )
        .unwrap();
    assert_blocked(&mut f);

    // Removed on the desktop without a file in the trash: only a restore path is stored.
    let mut f = nested();
    f.ops.db.execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z' WHERE file_id=3", []).unwrap();
    f.ops.rename_folder(2, "neu").unwrap();
    assert_eq!(deep_companion(&f), p(&f, "neu/b/Deep_extras/pic.png"));
}

#[test]
fn trash_file_of_another_row_id_blocks() {
    let mut f = nested();
    f.ops.delete_file(3).unwrap();
    let trash: String = f.ops.db.query_row("SELECT trash_path FROM companion_files WHERE file_id=3", [], |r| r.get(0)).unwrap();
    let other = Path::new(&trash).with_file_name("companion-99-pic.png");
    fs::rename(&trash, &other).unwrap();
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE file_id=3", [other.to_str().unwrap()]).unwrap();
    let before = db_paths(&f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert_eq!(db_paths(&f), before);
}

// ---- Rennen ----

#[test]
fn row_added_after_the_pre_check_rolls_the_folder_back() {
    let (mut f, db_path) = fixture_on_file();
    add_nested(&mut f, true);
    let extra = f.root().join("a/b/Deep_extras/late.png");
    fs::write(&extra, PNG).unwrap();
    super::tests::BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || insert_companion_via_second_connection(&db_path, 3, &extra)));
    });
    // The folder is renamed on disk first, then the commit sees the new row and fails.
    let before: Vec<String> = db_paths(&f);
    let r = f.ops.rename_folder(2, "neu");
    assert_eq!(r, Err(Error::Changed));
    assert!(exists(&f.root().join(DEEP_OLD)) && !exists(&f.root().join("neu")));
    let after = db_paths(&f);
    assert_eq!(after.len(), before.len() + 1);
    assert!(after.iter().filter(|s| s.starts_with("c:")).all(|s| s.contains("/a/b/")), "{after:?}");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn row_added_after_the_directory_rename_rolls_it_back() {
    let (mut f, db_path) = fixture_on_file();
    add_nested(&mut f, true);
    let extra = f.root().join("a/b/Deep_extras/late.png");
    super::tests::AFTER_DIRECTORY_RENAME.with(|h| {
        let extra = extra.clone();
        *h.borrow_mut() = Some(Box::new(move || {
            // The file now lives under the new name; the row still names the old one.
            insert_companion_via_second_connection(&db_path, 3, &extra)
        }));
    });
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::Changed));
    assert!(exists(&f.root().join(DEEP_OLD)) && !exists(&f.root().join("neu")));
    assert!(f.ops.journal.is_empty());
}

// ---- Absturzpunkte ----

#[test]
fn crash_at_every_durable_phase_recovers_to_all_old_or_all_new() {
    for phase in [Phase::Prepared, Phase::Renamed, Phase::Committed] {
        let mut f = nested();
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::RestartRequired), "{phase:?}");
        let report = reopen_and_recover(&mut f);
        assert!(report.manual.is_empty(), "{phase:?}");
        let (dir, ok) = if phase == Phase::Prepared { ("a", "old") } else { ("neu", "new") };
        assert!(exists(&f.root().join(dir).join("b/Deep_extras/pic.png")), "{phase:?} {ok}");
        assert_eq!(deep_companion(&f), p(&f, &format!("{dir}/b/Deep_extras/pic.png")), "{phase:?}");
        let file: String = f.ops.db.query_row("SELECT path FROM files WHERE id=3", [], |r| r.get(0)).unwrap();
        assert_eq!(file, p(&f, &format!("{dir}/b/Deep.3mf")), "{phase:?}");
        assert!(f.ops.journal.is_empty(), "{phase:?}");
    }
}

#[test]
fn crash_during_move_recovers_forward_and_backward() {
    for phase in [Phase::Prepared, Phase::Renamed, Phase::Committed] {
        let mut f = nested();
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(f.ops.move_folder(2, Some(1)), Err(Error::RestartRequired));
        let report = reopen_and_recover(&mut f);
        assert!(report.manual.is_empty());
        let dir = if phase == Phase::Prepared { "a" } else { "sub/a" };
        assert_eq!(deep_companion(&f), p(&f, &format!("{dir}/b/Deep_extras/pic.png")), "{phase:?}");
        assert!(exists(&f.root().join(dir).join("b/Deep_extras/pic.png")));
    }
}

#[test]
fn journal_with_folder_companions_is_version_two_and_older_readers_refuse_it() {
    let mut f = nested();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    let _ = f.ops.rename_folder(2, "neu");
    let file = f.dir.join("journal/web-fsops.json");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"version\":2"), "{text}");
    // A version-1 header on a file that carries companion data was not written by us.
    fs::write(&file, text.replace("\"version\":2", "\"version\":1")).unwrap();
    assert!(Journal::open(&f.dir.join("journal")).err() == Some(Error::RestartRequired));
    fs::write(&file, text).unwrap();
    assert!(Journal::open(&f.dir.join("journal")).is_ok());

    // Without companions the format stays as before.
    let mut f = fixture();
    add_nested(&mut f, false);
    f.ops.journal.crash_at = Some(Phase::Prepared);
    let _ = f.ops.rename_folder(2, "neu");
    let text = fs::read_to_string(f.dir.join("journal/web-fsops.json")).unwrap();
    assert!(text.contains("\"version\":1") && !text.contains("companions"), "{text}");
}

#[test]
fn recovery_refuses_when_the_rows_changed_during_the_crash() {
    // A row appears under the folder while the journal is pending.
    let mut f = nested();
    f.ops.journal.crash_at = Some(Phase::Renamed);
    let _ = f.ops.rename_folder(2, "neu");
    crate::db::companions::insert_companion(
        &f.ops.db,
        3,
        "late.png",
        f.root().join("a/b/Deep_extras/late.png").to_str().unwrap(),
        "image",
        Some(1),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let report = reopen_and_recover(&mut f);
    assert_eq!(report.manual.len(), 1);
    assert!(exists(&f.root().join("neu/b/Deep_extras/pic.png")), "disk left alone");

    // A row's path was edited to something that is neither the old nor the new state.
    let mut f = nested();
    f.ops.journal.crash_at = Some(Phase::Renamed);
    let _ = f.ops.rename_folder(2, "neu");
    f.ops.db.execute("UPDATE companion_files SET path=?1 WHERE file_id=3", [p(&f, "somewhere/else.png")]).unwrap();
    let report = reopen_and_recover(&mut f);
    assert_eq!(report.manual.len(), 1);
}

#[test]
fn recovery_does_not_follow_a_tampered_journal() {
    let mut f = nested();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    let _ = f.ops.rename_folder(2, "neu");
    let file = f.dir.join("journal/web-fsops.json");
    let text = fs::read_to_string(&file).unwrap();
    // Point the stored companion target at a path outside the renamed tree.
    let tampered = text.replacen("/neu/b/Deep_extras/pic.png", "/elsewhere/pic.png", 1);
    assert_ne!(tampered, text);
    fs::write(&file, tampered).unwrap();
    let report = reopen_and_recover(&mut f);
    assert_eq!(report.manual.len(), 1);
    assert!(exists(&f.root().join(DEEP_OLD)));
}

#[test]
fn recovery_refuses_a_journal_that_lists_one_companion_row_twice() {
    let mut f = nested();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    let _ = f.ops.rename_folder(2, "neu");
    let file = f.dir.join("journal/web-fsops.json");
    let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    for side in ["before", "after"] {
        let rows = doc["folders"][0][side]["companions"].as_array_mut().unwrap();
        let first = rows[0].clone();
        rows.push(first);
    }
    fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let report = reopen_and_recover(&mut f);
    assert_eq!(report.manual.len(), 1);
    assert!(exists(&f.root().join(DEEP_OLD)));
}

#[test]
fn orphan_model_row_under_the_destination_is_a_conflict() {
    let mut f = nested();
    let orphan = p(&f, "neu/x/stale.3mf");
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (9,'stale.3mf',?1,'3mf',1,'2026-01-01T00:00:00Z')",
            [&orphan],
        )
        .unwrap();
    let before = db_paths(&f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::Conflict));
    assert_eq!(db_paths(&f), before);
    assert!(exists(&f.root().join(DEEP_OLD)) && !exists(&f.root().join("neu")));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn orphan_companion_row_leaves_everything_in_place() {
    let mut f = nested();
    let orphan = p(&f, "neu/x_extras/pic.png");
    f.ops
        .db
        .execute("UPDATE companion_files SET path=?1 WHERE file_id=1", [&orphan])
        .unwrap();
    let before = db_paths(&f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::Conflict));
    assert_eq!(db_paths(&f), before);
    assert!(exists(&f.root().join(DEEP_OLD)) && !exists(&f.root().join("neu")));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn trash_file_in_another_roots_trash_blocks() {
    let mut f = nested();
    f.ops.delete_file(3).unwrap();
    let root2 = f.dir.join("root2");
    fs::create_dir(&root2).unwrap();
    f.ops.context = FsContext::new(&[f.dir.join("root"), root2.clone()], &[]).unwrap();
    let trash2 = root2.join(".katalog-papierkorb");
    assert!(trash2.is_dir(), "the context prepared the second root's trash");
    let (id, old): (i64, String) = f
        .ops
        .db
        .query_row("SELECT id,trash_path FROM companion_files WHERE file_id=3", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let moved = trash2.join(format!("companion-{id}-pic.png"));
    fs::copy(&old, &moved).unwrap();
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=?2", rusqlite::params![moved.to_str().unwrap(), id]).unwrap();
    let before = db_paths(&f);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert_eq!(db_paths(&f), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn folder_tree_rejects_unknown_fields_and_old_shape_still_loads() {
    assert!(serde_json::from_str::<FolderTree>(r#"{"folders":[],"files":[],"extra":1}"#).is_err());
    let t: FolderTree = serde_json::from_str(r#"{"folders":[],"files":[]}"#).unwrap();
    assert!(!serde_json::to_string(&t).unwrap().contains("companions"));
}
