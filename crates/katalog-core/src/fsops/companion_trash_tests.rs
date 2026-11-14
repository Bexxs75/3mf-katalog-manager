//! Papierkorb, Wiederherstellen, endgueltiges Loeschen und Papierkorb leeren fuer
//! Modelle mit Begleitdateien: Sollverhalten, Randfaelle, Reste der Sperre und ein
//! Absturz an jedem Schritt.
use super::companion_carry_tests::{standard, walk, Fx, PNG};
use super::tests::{PointAction, BEFORE_MUTATION, POINT};
use super::*;
use std::{fs, path::Path};

/// Trash entry name without the random operation segment, so two runs compare equal.
fn norm(name: &str) -> String {
    let (parts, drop) = if name.starts_with("companion-") { (4, 2) } else { (3, 1) };
    let mut v: Vec<&str> = name.splitn(parts, '-').collect();
    if v.len() > drop + 1 {
        v.remove(drop);
    }
    v.join("-")
}

pub(super) fn trash_listing(f: &Fx) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(f.p(TRASH))
        .unwrap()
        .flatten()
        .map(|e| norm(e.file_name().to_str().unwrap()))
        .collect();
    names.sort();
    names
}

pub(super) fn raw_trash_names(f: &Fx) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(f.p(TRASH))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// Disk, trash and both tables, with timestamps reduced to "set or not" and trash
/// names to their stable part.
pub(super) fn view(f: &Fx) -> String {
    let mut lines = Vec::new();
    walk(&f.root(), &f.root(), &mut lines);
    for n in trash_listing(f) {
        lines.push(format!("t {n}"));
    }
    let strip = |s: String| s.replace(f.root().to_str().unwrap(), "");
    let base = |p: Option<String>| p.map(|p| norm(Path::new(&p).file_name().unwrap().to_str().unwrap()));
    let mut stmt = f.ops.db.prepare("SELECT id,name,path,folder_id,deleted_at,trash_path FROM files ORDER BY id").unwrap();
    for r in stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .unwrap()
    {
        let (id, name, path, folder, deleted, trash) = r.unwrap();
        lines.push(strip(format!("F {id} {name} {path} {folder:?} {} {:?}", deleted.is_some(), base(trash))));
    }
    let mut stmt = f
        .ops
        .db
        .prepare("SELECT id,file_id,name,path,deleted_at,trash_path FROM companion_files ORDER BY id")
        .unwrap();
    for r in stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .unwrap()
    {
        let (id, file, name, path, deleted, trash) = r.unwrap();
        lines.push(strip(format!("C {id} {file} {name} {path} {} {:?}", deleted.is_some(), base(trash))));
    }
    lines.join("\n")
}

pub(super) fn crow(f: &Fx, id: i64) -> (String, String, Option<String>, Option<String>) {
    f.ops
        .db
        .query_row("SELECT name,path,deleted_at,trash_path FROM companion_files WHERE id=?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
}

pub(super) fn model_deleted_at(f: &Fx, id: i64) -> Option<String> {
    f.ops.db.query_row("SELECT deleted_at FROM files WHERE id=?1", [id], |r| r.get(0)).unwrap()
}

pub(super) fn count(f: &Fx, table: &str) -> i64 {
    f.ops.db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

/// A model and one companion row that the desktop moved to its own trash.
fn desktop_trashed(f: &mut Fx, model: i64, rel: &str, companion: &str) -> i64 {
    f.model(model, rel);
    let row = f.companion(model, companion);
    let at = "2026-01-02T00:00:00Z";
    f.ops
        .db
        .execute(
            "UPDATE files SET deleted_at=?1, trash_path=?2 WHERE id=?3",
            rusqlite::params![at, format!("/desktop/trash/{model}-x"), model],
        )
        .unwrap();
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET deleted_at=?1, trash_path=?2 WHERE id=?3",
            rusqlite::params![at, format!("/desktop/trash/companion-{row}-x.png"), row],
        )
        .unwrap();
    row
}

// ---- Papierkorb ----

#[test]
fn trash_takes_every_companion_along_with_flat_names_and_the_models_timestamp() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    assert!(!f.p("Foo.3mf").exists());
    assert!(!f.p("Foo_extras").exists(), "the emptied folder is gone");
    let stamp = model_deleted_at(&f, 1).expect("model trashed");
    let raw = raw_trash_names(&f);
    assert_eq!(raw.len(), 3);
    for (id, name) in [(1, "pic.png"), (2, "zzz.png")] {
        let (n, path, deleted, trash) = crow(&f, id);
        assert_eq!((n.as_str(), deleted.as_deref()), (name, Some(stamp.as_str())));
        assert_eq!(path, f.p(&format!("Foo_extras/{name}")).to_str().unwrap(), "the old path stays for the restore");
        let trash = trash.expect("trash path");
        assert!(Path::new(&trash).is_file());
        assert_eq!(Path::new(&trash).parent().unwrap(), f.p(TRASH));
        let file = Path::new(&trash).file_name().unwrap().to_str().unwrap();
        assert!(file.starts_with(&format!("companion-{id}-")) && file.ends_with(&format!("-{name}")), "{file}");
        assert!(file.len() <= 255);
    }
    assert!(f.ops.journal.is_empty());
    assert_eq!(f.ops.outcome.moved, 2);
}

#[test]
fn trash_and_restore_round_trip_restores_disk_and_database() {
    let mut f = standard();
    let before = view(&f);
    f.ops.delete_file(1).unwrap();
    assert_ne!(view(&f), before);
    f.ops.restore_file(1).unwrap();
    assert_eq!(view(&f), before);
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(model_deleted_at(&f, 1).is_none());
    assert!(f.ops.journal.is_empty());
    f.assert_rows_match_disk();
}

#[test]
fn companions_with_the_same_stem_stay_with_their_own_model() {
    let mut f = standard();
    f.model(2, "Foo.stl");
    f.companion(2, "Foo_extras/other.png");
    f.ops.delete_file(1).unwrap();
    assert!(f.p("Foo_extras/other.png").is_file(), "the other model's file is not ours");
    assert!(!f.p("Foo_extras/pic.png").exists());
    assert_eq!(crow(&f, 3).2, None);
    f.ops.restore_file(1).unwrap();
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/other.png").is_file());
}

#[test]
fn trash_with_a_missing_companion_file_marks_only_the_row() {
    let mut f = standard();
    fs::remove_file(f.p("Foo_extras/pic.png")).unwrap();
    f.ops.delete_file(1).unwrap();
    let (_, path, deleted, trash) = crow(&f, 1);
    assert!(deleted.is_some() && trash.is_none());
    assert_eq!(path, f.p("Foo_extras/pic.png").to_str().unwrap());
    assert_eq!(f.ops.outcome.moved, 2);
    assert_eq!(raw_trash_names(&f).len(), 2, "model and the one companion that existed");
    // The row comes back active without a file, like at the desktop.
    f.ops.restore_file(1).unwrap();
    let (_, path, deleted, trash) = crow(&f, 1);
    assert!((deleted, trash) == (None, None));
    assert_eq!(path, f.p("Foo_extras/pic.png").to_str().unwrap());
    assert!(!f.p("Foo_extras/pic.png").exists());
    assert!(f.p("Foo_extras/zzz.png").is_file());
}

#[test]
fn a_model_whose_companion_files_are_all_gone_makes_no_folder_on_restore() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.row_for(1, &f.p("Foo_extras/pic.png"));
    f.ops.delete_file(1).unwrap();
    f.ops.restore_file(1).unwrap();
    assert!(!f.p("Foo_extras").exists());
    assert_eq!(crow(&f, 1).2, None);
}

#[test]
fn a_missing_model_file_with_all_companions_gone_is_a_database_only_trash() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.row_for(1, &f.p("Foo_extras/pic.png"));
    fs::remove_file(f.p("Foo.3mf")).unwrap();
    f.ops.delete_file(1).unwrap();
    let stamp = model_deleted_at(&f, 1).unwrap();
    let (_, _, deleted, trash) = crow(&f, 1);
    assert_eq!((deleted, trash), (Some(stamp), None));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_missing_model_file_with_a_companion_file_left_stays_blocked() {
    let mut f = standard();
    fs::remove_file(f.p("Foo.3mf")).unwrap();
    let before = view(&f);
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn remaining_blocks_leave_everything_untouched() {
    // An active row outside the model's own folder would strand its file.
    let mut f = standard();
    f.companion(1, "Elsewhere_extras/loose.png");
    let before = view(&f);
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
    assert_eq!(view(&f), before);

    // The model's folder is a link.
    let mut g = Fx::new();
    g.model(1, "Foo.3mf");
    g.companion(1, "Real_extras/pic.png");
    std::os::unix::fs::symlink(g.p("Real_extras"), g.p("Foo_extras")).unwrap();
    g.ops.db.execute("UPDATE companion_files SET path=?1", [g.p("Foo_extras/pic.png").to_str().unwrap()]).unwrap();
    let before = view(&g);
    assert_eq!(g.ops.delete_file(1), Err(Error::HasCompanions));
    assert_eq!(view(&g), before);

    // A companion that is a link.
    let mut h = Fx::new();
    h.model(1, "Foo.3mf");
    fs::create_dir(h.p("Foo_extras")).unwrap();
    fs::write(h.p("secret.png"), PNG).unwrap();
    std::os::unix::fs::symlink(h.p("secret.png"), h.p("Foo_extras/pic.png")).unwrap();
    h.row_for(1, &h.p("Foo_extras/pic.png"));
    let before = view(&h);
    assert_eq!(h.ops.delete_file(1), Err(Error::HasCompanions));
    assert_eq!(view(&h), before);
}

#[test]
fn a_separately_removed_row_neither_blocks_nor_travels() {
    let mut f = standard();
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET deleted_at='2026-01-01T00:00:00Z', trash_path='/desktop/trash/companion-2-zzz.png' WHERE id=2",
            [],
        )
        .unwrap();
    f.ops.delete_file(1).unwrap();
    let removed = crow(&f, 2);
    assert_eq!(removed.2.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert_eq!(removed.3.as_deref(), Some("/desktop/trash/companion-2-zzz.png"));
    assert!(f.p("Foo_extras/zzz.png").is_file(), "its file is not ours");
    // Restoring the model brings back only the files of this deletion.
    f.ops.restore_file(1).unwrap();
    assert_eq!(f.ops.outcome.kept_in_trash, 0, "a row of another deletion is not even looked at");
    assert_eq!(crow(&f, 2).2.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert!(model_deleted_at(&f, 1).is_none());
    assert_eq!(crow(&f, 1).2, None);
}

#[test]
fn repeating_an_operation_is_refused_and_changes_nothing() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let after = view(&f);
    assert_eq!(f.ops.delete_file(1), Err(Error::Conflict));
    assert_eq!(view(&f), after);
    f.ops.restore_file(1).unwrap();
    let restored = view(&f);
    assert_eq!(f.ops.restore_file(1), Err(Error::Conflict));
    assert_eq!(view(&f), restored);
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::NotFound));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_failing_second_companion_puts_the_first_back() {
    let mut f = standard();
    // After the first file moved, the second source is swapped for another inode.
    let second = f.p("Foo_extras/zzz.png");
    POINT.with(|c| {
        *c.borrow_mut() = Some((
            "after_companion",
            0,
            PointAction::Run(Box::new(move || {
                fs::remove_file(&second).unwrap();
                fs::write(&second, b"other").unwrap();
            })),
        ))
    });
    assert_eq!(f.ops.delete_file(1), Err(Error::Changed));
    assert!(f.p("Foo.3mf").exists());
    assert!(f.p("Foo_extras/pic.png").is_file(), "the first file is back");
    assert_eq!(fs::read(f.p("Foo_extras/zzz.png")).unwrap(), b"other");
    assert_eq!(crow(&f, 1).2, None);
    assert!(raw_trash_names(&f).is_empty());
    assert!(f.ops.journal.is_empty());
}

// ---- Wiederherstellen ----

#[test]
fn restore_uses_a_free_name_and_never_replaces_a_file() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    // A loose file now sits at the old name, and a row holds the first replacement name.
    fs::create_dir(f.p("Foo_extras")).unwrap();
    fs::write(f.p("Foo_extras/pic.png"), b"loose").unwrap();
    f.model(2, "Foo.stl");
    f.row_for(2, &f.p("Foo_extras/pic (2).png"));
    f.ops.restore_file(1).unwrap();
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"loose");
    let (name, path, deleted, trash) = crow(&f, 1);
    assert_eq!(name, "pic (3).png");
    assert_eq!(path, f.p("Foo_extras/pic (3).png").to_str().unwrap());
    assert!((deleted, trash) == (None, None));
    assert_eq!(fs::read(f.p("Foo_extras/pic (3).png")).unwrap(), PNG);
    assert!(f.p("Foo_extras/zzz.png").is_file());
}

#[test]
fn restore_into_a_link_or_a_file_is_a_conflict_and_changes_nothing() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    fs::write(f.p("Foo_extras"), b"a file").unwrap();
    let before = view(&f);
    assert_eq!(f.ops.restore_file(1), Err(Error::CompanionsConflict));
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
    fs::remove_file(f.p("Foo_extras")).unwrap();
    fs::create_dir(f.p("elsewhere")).unwrap();
    std::os::unix::fs::symlink(f.p("elsewhere"), f.p("Foo_extras")).unwrap();
    let before = view(&f);
    assert_eq!(f.ops.restore_file(1), Err(Error::CompanionsConflict));
    assert_eq!(view(&f), before);
}

#[test]
fn restore_with_a_taken_model_path_moves_no_companion() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    fs::write(f.p("Foo.3mf"), b"squatter").unwrap();
    let before = view(&f);
    assert_eq!(f.ops.restore_file(1), Err(Error::Conflict));
    assert_eq!(view(&f), before);
    assert!(!f.p("Foo_extras").exists());
}

#[test]
fn a_row_in_the_desktop_trash_stays_there_when_the_model_comes_back() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let stamp = model_deleted_at(&f, 1).unwrap();
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET trash_path='/desktop/trash/companion-2-zzz.png' WHERE id=2",
            [],
        )
        .unwrap();
    f.ops.restore_file(1).unwrap();
    assert_eq!(f.ops.outcome.kept_in_trash, 1);
    let (_, _, deleted, trash) = crow(&f, 2);
    assert_eq!((deleted, trash.as_deref()), (Some(stamp), Some("/desktop/trash/companion-2-zzz.png")));
    assert!(f.p("Foo_extras/pic.png").is_file() && !f.p("Foo_extras/zzz.png").exists());
    // The file of the kept row is still in the web trash: nothing was deleted.
    assert!(raw_trash_names(&f).iter().any(|n| n.starts_with("companion-2-")));
}

#[test]
fn a_trash_file_of_another_row_or_a_link_is_not_restored() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    // Row 2 now claims row 1's trash file.
    let t1 = crow(&f, 1).3.unwrap();
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=2", [&t1]).unwrap();
    f.ops.restore_file(1).unwrap();
    assert!(f.p("Foo_extras/pic.png").is_file());
    assert!(!f.p("Foo_extras/zzz.png").exists());
    assert_eq!(f.ops.outcome.kept_in_trash, 1);
}

#[test]
fn restore_of_a_desktop_trashed_model_is_still_refused() {
    let mut f = Fx::new();
    desktop_trashed(&mut f, 3, "Desk.3mf", "Desk_extras/d.png");
    let before = view(&f);
    assert_eq!(f.ops.restore_file(3), Err(Error::TrashForeign));
    assert_eq!(view(&f), before);
}

// ---- Endgueltig loeschen ----

#[test]
fn permanent_delete_removes_the_trash_files_and_cascades_the_rows() {
    let mut f = standard();
    f.model(2, "Free.3mf");
    f.ops.delete_file(1).unwrap();
    let out = f.ops.delete_file_permanently(1).unwrap();
    assert!(!out.kept_on_disk);
    assert!(raw_trash_names(&f).is_empty());
    assert_eq!(count(&f, "companion_files"), 0);
    assert_eq!(count(&f, "files"), 1);
    assert!(f.p("Free.3mf").exists());
    assert!(f.ops.journal.is_empty());
    assert_eq!(f.ops.outcome.moved, 2);
}

#[test]
fn permanent_delete_leaves_other_models_companions_alone() {
    let mut f = standard();
    f.model(2, "Bar.3mf");
    f.companion(2, "Bar_extras/b.png");
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file(2).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert_eq!(trash_listing(&f), vec!["2-Bar.3mf".to_owned(), "companion-3-b.png".to_owned()]);
    assert_eq!(count(&f, "companion_files"), 1);
    f.ops.restore_file(2).unwrap();
    assert!(f.p("Bar_extras/b.png").is_file());
}

#[test]
fn permanent_delete_with_gone_trash_files_only_drops_the_rows() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    for e in fs::read_dir(f.p(TRASH)).unwrap().flatten() {
        if e.file_name().to_str().unwrap().starts_with("companion-") {
            fs::remove_file(e.path()).unwrap();
        }
    }
    f.ops.delete_file_permanently(1).unwrap();
    assert_eq!(count(&f, "companion_files"), 0);
    assert!(raw_trash_names(&f).is_empty());
}

#[test]
fn desktop_trash_entries_stay_blocked_and_untouched() {
    let mut f = Fx::new();
    let row = desktop_trashed(&mut f, 3, "Desk.3mf", "Desk_extras/d.png");
    let before = view(&f);
    assert_eq!(f.ops.delete_file_permanently(3), Err(Error::HasCompanions));
    assert_eq!(view(&f), before);
    assert_eq!(count(&f, "companion_files"), 1);
    assert_eq!(f.ops.blocked_by_companions(3), Ok(true));
    assert!(crow(&f, row).3.unwrap().starts_with("/desktop/trash/"));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn an_active_row_of_a_trashed_model_blocks_the_delete() {
    // The legacy state: the model was marked trashed without moving its companion.
    let mut f = standard();
    f.ops.db.execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path=NULL WHERE id=1", []).unwrap();
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
    assert!(f.p("Foo_extras/pic.png").is_file());
    assert_eq!(count(&f, "companion_files"), 2);
    // Restoring it just flips the model back; the active rows are not ours to touch.
    f.ops.restore_file(1).unwrap();
    assert_eq!(crow(&f, 1).2, None);
}

#[test]
fn a_trash_file_that_is_a_link_is_kept_and_reported() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let trash_file = crow(&f, 1).3.unwrap();
    fs::remove_file(&trash_file).unwrap();
    fs::write(f.p("victim.png"), PNG).unwrap();
    std::os::unix::fs::symlink(f.p("victim.png"), &trash_file).unwrap();
    let out = f.ops.delete_file_permanently(1).unwrap();
    assert!(out.kept_on_disk);
    assert!(f.p("victim.png").is_file(), "a link's target is never followed");
    assert_eq!(count(&f, "companion_files"), 0);
}

#[test]
fn a_model_with_its_trash_file_gone_still_deletes_its_companion_files() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    for e in fs::read_dir(f.p(TRASH)).unwrap().flatten() {
        if !e.file_name().to_str().unwrap().starts_with("companion-") {
            fs::remove_file(e.path()).unwrap();
        }
    }
    // The summary, the snapshot and the delete agree that this entry can be emptied.
    assert_eq!(f.ops.blocked_by_companions(1), Ok(false));
    let s = f.ops.empty_trash_summary().unwrap();
    assert_eq!((s.count, s.blocked_by_companions, s.companion_count), (1, 0, 2));
    f.ops.delete_file_permanently(1).unwrap();
    assert!(raw_trash_names(&f).is_empty());
    assert_eq!((count(&f, "files"), count(&f, "companion_files")), (0, 0));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_crash_while_deleting_a_model_whose_file_is_gone_is_finished_by_recovery() {
    for k in [0usize, 1] {
        let mut f = standard();
        f.ops.delete_file(1).unwrap();
        for e in fs::read_dir(f.p(TRASH)).unwrap().flatten() {
            if !e.file_name().to_str().unwrap().starts_with("companion-") {
                fs::remove_file(e.path()).unwrap();
            }
        }
        POINT.with(|c| *c.borrow_mut() = Some(("after_unlink", k, PointAction::Crash)));
        assert_eq!(f.ops.delete_file_permanently(1).map(|_| ()), Err(Error::RestartRequired));
        assert!(f.reopen_and_recover().manual.is_empty());
        assert!(raw_trash_names(&f).is_empty());
        assert_eq!((count(&f, "files"), count(&f, "companion_files")), (0, 0));
    }
}

#[test]
fn a_row_added_after_the_check_stops_the_delete_before_any_unlink() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let db_path = f.db_path.clone();
    let late = f.p("Foo_extras/late.png");
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = crate::db::connect(&db_path).unwrap();
            crate::db::companions::insert_companion(&other, 1, "late.png", late.to_str().unwrap(), "image", None, "t").unwrap();
        }))
    });
    let before = raw_trash_names(&f);
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::CompanionsChanged));
    assert_eq!(raw_trash_names(&f), before);
    assert_eq!(count(&f, "files"), 1);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_row_added_before_the_commit_of_a_trash_undoes_it() {
    let mut f = standard();
    let db_path = f.db_path.clone();
    let late = f.p("Foo_extras/late.png");
    POINT.with(|c| {
        *c.borrow_mut() = Some((
            "before_commit",
            0,
            PointAction::Run(Box::new(move || {
                let other = crate::db::connect(&db_path).unwrap();
                crate::db::companions::insert_companion(&other, 1, "late.png", late.to_str().unwrap(), "image", None, "t").unwrap();
            })),
        ))
    });
    assert_eq!(f.ops.delete_file(1), Err(Error::CompanionsChanged));
    assert!(f.p("Foo.3mf").exists());
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(raw_trash_names(&f).is_empty());
    assert!(model_deleted_at(&f, 1).is_none());
    assert_eq!(crow(&f, 1).2, None);
    assert_eq!(crow(&f, 1).3, None);
    assert!(f.ops.journal.is_empty());
}

// ---- Papierkorb leeren ----

#[test]
fn empty_trash_removes_web_entries_with_their_companions_and_skips_desktop_ones() {
    let mut f = standard();
    f.model(2, "Free.3mf");
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file(2).unwrap();
    desktop_trashed(&mut f, 3, "Desk.3mf", "Desk_extras/d.png");
    let summary = f.ops.empty_trash_summary().unwrap();
    assert_eq!((summary.count, summary.blocked_by_companions, summary.companion_count), (2, 1, 2));
    let mut snap = f.ops.empty_trash_snapshot(2, summary.max_deleted_at.as_deref()).unwrap();
    assert_eq!(snap.ids(), &[1, 2]);
    let results = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.ok));
    assert_eq!((results[0].companions_moved, results[1].companions_moved), (2, 0));
    assert!(raw_trash_names(&f).is_empty());
    // Only the desktop entry and its row are left.
    assert_eq!(count(&f, "files"), 1);
    assert_eq!(count(&f, "companion_files"), 1);
    assert!(f.p("Desk.3mf").exists());
    assert!(f.ops.journal.is_empty());
    let after = f.ops.empty_trash_summary().unwrap();
    assert_eq!((after.count, after.blocked_by_companions, after.companion_count), (0, 1, 0));
}

#[test]
fn the_summary_counts_the_bytes_of_the_companions_too() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let s = f.ops.empty_trash_summary().unwrap();
    assert_eq!(s.companion_count, 2);
    assert_eq!(s.bytes, 5 + 2 * PNG.len() as u64);
}

#[test]
fn empty_trash_batch_reports_a_stranded_entry_as_blocked() {
    let mut f = standard();
    f.model(2, "Free.3mf");
    f.ops.delete_file(2).unwrap();
    f.ops.delete_file(1).unwrap();
    // A row appears between the snapshot and the run that makes entry 1 undeletable.
    let mut snap = f.ops.empty_trash_snapshot(2, Some("2030-01-01T00:00:00Z")).unwrap();
    f.ops.db.execute("UPDATE companion_files SET trash_path='/desktop/trash/x' WHERE id=1", []).unwrap();
    let results = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert_eq!(results[0].kind, Some(Error::HasCompanions));
    assert!(results[1].ok);
    assert_eq!(count(&f, "files"), 1);
    assert_eq!(count(&f, "companion_files"), 2);
}

// ---- Absturz an jedem Schritt ----

#[derive(Clone, Copy, Debug)]
enum Crash {
    Phase(Phase),
    Point(&'static str, usize),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Before,
    After,
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Trash,
    Restore,
    Permanent,
}

/// A fresh fixture in the state the operation starts from.
fn start(op: Op) -> Fx {
    let mut f = standard();
    if matches!(op, Op::Restore | Op::Permanent) {
        f.ops.delete_file(1).unwrap();
    }
    f
}
fn run(f: &mut Fx, op: Op) -> Result<()> {
    match op {
        Op::Trash => f.ops.delete_file(1),
        Op::Restore => f.ops.restore_file(1),
        Op::Permanent => f.ops.delete_file_permanently(1).map(|_| ()),
    }
}

fn crash_and_recover(op: Op, crash: Crash) -> (Fx, String, String) {
    let mut twin = start(op);
    run(&mut twin, op).unwrap();
    let after = view(&twin);
    let mut f = start(op);
    let before = view(&f);
    match crash {
        Crash::Phase(p) => f.ops.journal.crash_at = Some(p),
        Crash::Point(name, k) => POINT.with(|c| *c.borrow_mut() = Some((name, k, PointAction::Crash))),
    }
    assert_eq!(run(&mut f, op), Err(Error::RestartRequired), "{op:?} {crash:?}");
    f.ops.journal.crash_at = None;
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty(), "{op:?} {crash:?}");
    assert!(f.ops.journal.is_empty(), "{op:?} {crash:?}");
    (f, before, after)
}

#[test]
fn a_crash_at_any_step_of_a_trash_ends_completely_before_or_completely_after() {
    let points: &[(Crash, Outcome)] = &[
        (Crash::Phase(Phase::Prepared), Outcome::Before),
        (Crash::Point("after_companion", 0), Outcome::Before),
        (Crash::Point("after_companion", 1), Outcome::Before),
        (Crash::Phase(Phase::CompanionsMoved), Outcome::Before),
        (Crash::Phase(Phase::Renamed), Outcome::After),
        (Crash::Point("before_commit", 0), Outcome::After),
        (Crash::Phase(Phase::Committed), Outcome::After),
    ];
    for (crash, expected) in points {
        let (f, before, after) = crash_and_recover(Op::Trash, *crash);
        let now = view(&f);
        assert_eq!(now, if *expected == Outcome::Before { before } else { after }, "{crash:?}");
        f.assert_rows_match_disk();
        let stamp_model = model_deleted_at(&f, 1);
        if *expected == Outcome::After {
            // The companions carry the model's own timestamp in every outcome.
            assert_eq!(crow(&f, 1).2, stamp_model, "{crash:?}");
        }
    }
}

#[test]
fn a_crash_at_any_step_of_a_restore_ends_completely_before_or_completely_after() {
    let points: &[(Crash, Outcome)] = &[
        (Crash::Phase(Phase::Prepared), Outcome::Before),
        (Crash::Point("after_mkdir", 0), Outcome::Before),
        (Crash::Point("after_companion", 0), Outcome::Before),
        (Crash::Point("after_companion", 1), Outcome::Before),
        (Crash::Phase(Phase::CompanionsMoved), Outcome::Before),
        (Crash::Phase(Phase::Renamed), Outcome::After),
        (Crash::Point("before_commit", 0), Outcome::After),
        (Crash::Phase(Phase::Committed), Outcome::After),
    ];
    for (crash, expected) in points {
        let (f, before, after) = crash_and_recover(Op::Restore, *crash);
        assert_eq!(view(&f), if *expected == Outcome::Before { before } else { after }, "{crash:?}");
        f.assert_rows_match_disk();
    }
}

#[test]
fn a_crash_at_any_step_of_a_permanent_delete_ends_completely_before_or_completely_after() {
    let points: &[(Crash, Outcome)] = &[
        (Crash::Phase(Phase::Prepared), Outcome::Before),
        (Crash::Point("after_unlink", 0), Outcome::After),
        (Crash::Point("after_unlink", 1), Outcome::After),
        (Crash::Phase(Phase::CompanionsMoved), Outcome::After),
        (Crash::Phase(Phase::Unlinked), Outcome::After),
        (Crash::Phase(Phase::Committed), Outcome::After),
    ];
    for (crash, expected) in points {
        let (f, before, after) = crash_and_recover(Op::Permanent, *crash);
        assert_eq!(view(&f), if *expected == Outcome::Before { before } else { after }, "{crash:?}");
    }
}

#[test]
fn a_crash_inside_empty_trash_finishes_the_started_entry_and_leaves_the_rest() {
    let mut f = Fx::new();
    f.model(1, "A.3mf");
    f.companion(1, "A_extras/a.png");
    f.model(2, "B.3mf");
    f.companion(2, "B_extras/b1.png");
    f.companion(2, "B_extras/b2.png");
    f.model(3, "C.3mf");
    f.ops.delete_files(&[1, 2, 3], || true).unwrap();
    let mut snap = f.ops.empty_trash_snapshot(3, Some("2030-01-01T00:00:00Z")).unwrap();
    // A has one companion (k = 0 only); the crash hits B's second file.
    POINT.with(|c| *c.borrow_mut() = Some(("after_unlink", 1, PointAction::Crash)));
    let results = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert!(results[0].ok);
    assert_eq!(results[1].kind, Some(Error::RestartRequired));
    assert_eq!(results[2].kind, Some(Error::NotStarted));
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty());
    assert_eq!(count(&f, "files"), 1, "A and B are gone, C is untouched");
    assert_eq!(count(&f, "companion_files"), 0);
    assert_eq!(trash_listing(&f), vec!["3-C.3mf".to_owned()]);
}

#[test]
fn a_foreign_file_in_the_trash_after_a_crash_needs_manual_review_and_loses_nothing() {
    let mut f = standard();
    POINT.with(|c| *c.borrow_mut() = Some(("after_companion", 0, PointAction::Crash)));
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
    let trash_file = crow(&f, 1).3;
    assert!(trash_file.is_none(), "the database still shows the state before");
    let moved = fs::read_dir(f.p(TRASH))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_str().unwrap().starts_with("companion-1-"))
        .unwrap()
        .path();
    fs::remove_file(&moved).unwrap();
    fs::write(&moved, b"foreign").unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1);
    assert_eq!(fs::read(&moved).unwrap(), b"foreign");
    assert!(f.p("Foo.3mf").exists() && f.p("Foo_extras/zzz.png").is_file());
}

#[test]
fn a_foreign_file_in_place_of_a_companion_stops_a_delete_for_review() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    POINT.with(|c| *c.borrow_mut() = Some(("after_unlink", 0, PointAction::Crash)));
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::RestartRequired));
    // The second companion is swapped for another file before recovery looks.
    let second = f
        .ops
        .journal
        .intents[0]
        .companions[1]
        .source
        .clone()
        .unwrap();
    let path = f.p(&second);
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"foreign").unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1);
    assert_eq!(fs::read(&path).unwrap(), b"foreign", "nothing foreign is deleted");
    assert_eq!(count(&f, "files"), 1);
}

#[test]
fn an_old_trash_entry_without_companion_data_is_not_finished_over_a_row_added_since() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
    // Written like the previous release: no companion data at all.
    f.companion(1, "Foo_extras/new.png");
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1, "finishing would strand the new file");
    assert!(model_deleted_at(&f, 1).is_none());
}

#[test]
fn the_model_with_no_companions_still_trashes_restores_and_deletes_as_before() {
    let mut f = Fx::new();
    f.model(1, "Free.3mf");
    f.ops.delete_file(1).unwrap();
    f.ops.restore_file(1).unwrap();
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert_eq!(count(&f, "files"), 0);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn trash_names_for_companions_respect_the_name_budget() {
    let long = format!("{}.png", "x".repeat(300));
    let n = companion_trash_name(12, "abcdef0123456789", &long).unwrap();
    assert!(n.len() <= 255 && n.starts_with("companion-12-abcdef01-") && n.ends_with(".png"), "{n}");
    assert_eq!(companion_trash_name(1, "abcdef0123456789", "a/b.png").unwrap(), "companion-1-abcdef01-a_b.png");
    assert_eq!(companion_trash_name(1, "xyz", "a.png"), Err(Error::Internal));
}

#[test]
fn a_database_only_trash_and_restore_survive_a_crash_at_either_end() {
    for (crash, committed) in [(Phase::Prepared, false), (Phase::Committed, true)] {
        let mut f = Fx::new();
        f.model(1, "Foo.3mf");
        f.row_for(1, &f.p("Foo_extras/pic.png"));
        fs::remove_file(f.p("Foo.3mf")).unwrap();
        let before = view(&f);
        f.ops.journal.crash_at = Some(crash);
        assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
        f.ops.journal.crash_at = None;
        assert!(f.reopen_and_recover().manual.is_empty(), "{crash:?}");
        assert!(f.ops.journal.is_empty());
        assert_eq!(model_deleted_at(&f, 1).is_some(), committed, "{crash:?}");
        assert_eq!(crow(&f, 1).2.is_some(), committed, "model and row move together: {crash:?}");
        if !committed {
            assert_eq!(view(&f), before);
        }
    }
    // Restoring such an entry flips model and row back in one step.
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.row_for(1, &f.p("Foo_extras/pic.png"));
    fs::remove_file(f.p("Foo.3mf")).unwrap();
    f.ops.delete_file(1).unwrap();
    f.ops.restore_file(1).unwrap();
    assert_eq!((model_deleted_at(&f, 1), crow(&f, 1).2), (None, None));
}

#[test]
fn trash_restore_and_delete_journals_carry_companion_data_as_version_2() {
    for (op, crash) in [(Op::Trash, Phase::CompanionsMoved), (Op::Restore, Phase::CompanionsMoved), (Op::Permanent, Phase::CompanionsMoved)] {
        let mut f = start(op);
        f.ops.journal.crash_at = Some(crash);
        assert_eq!(run(&mut f, op), Err(Error::RestartRequired));
        let text = f.journal_text();
        assert!(text.contains("\"version\":2") && text.contains("\"companions\"") && text.contains("\"companion_rows\""), "{op:?}");
    }
}

#[test]
fn a_failing_rollback_of_a_trash_blocks_the_journal_and_deletes_nothing() {
    let mut f = standard();
    let (trash, second) = (f.p(TRASH), f.p("Foo_extras/zzz.png"));
    // After the first file moved: swap it in the trash for another file, and the second source too.
    POINT.with(|c| {
        *c.borrow_mut() = Some((
            "after_companion",
            0,
            PointAction::Run(Box::new(move || {
                let moved = fs::read_dir(&trash)
                    .unwrap()
                    .flatten()
                    .find(|e| e.file_name().to_str().unwrap().starts_with("companion-1-"))
                    .unwrap()
                    .path();
                fs::remove_file(&moved).unwrap();
                fs::write(&moved, b"foreign").unwrap();
                fs::remove_file(&second).unwrap();
                fs::write(&second, b"other").unwrap();
            })),
        ))
    });
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
    assert!(f.ops.journal.is_blocked());
    assert!(f.p("Foo.3mf").exists());
    assert_eq!(fs::read(f.p("Foo_extras/zzz.png")).unwrap(), b"other");
    let foreign = raw_trash_names(&f).into_iter().find(|n| n.starts_with("companion-1-")).unwrap();
    assert_eq!(fs::read(f.p(&format!("{TRASH}/{foreign}"))).unwrap(), b"foreign");
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
}

#[test]
fn a_row_added_before_a_database_only_trash_commits_undoes_it() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    fs::remove_file(f.p("Foo.3mf")).unwrap();
    let db_path = f.db_path.clone();
    let late = f.p("Foo_extras/late.png");
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = crate::db::connect(&db_path).unwrap();
            crate::db::companions::insert_companion(&other, 1, "late.png", late.to_str().unwrap(), "image", None, "t").unwrap();
        }))
    });
    assert_eq!(f.ops.delete_file(1), Err(Error::CompanionsChanged));
    assert!(model_deleted_at(&f, 1).is_none());
    assert_eq!(crow(&f, 1).2, None);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_row_with_a_harmful_name_stays_in_the_trash_on_restore() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    // As from a restored backup: the stored name would decide the new file name.
    f.ops.db.execute("UPDATE companion_files SET name='x:y.png' WHERE id=1", []).unwrap();
    f.ops.restore_file(1).unwrap();
    assert_eq!(f.ops.outcome.kept_in_trash, 1);
    assert!(f.p("Foo_extras/zzz.png").is_file());
    assert_eq!(fs::read_dir(f.p("Foo_extras")).unwrap().count(), 1);
    assert!(crow(&f, 1).2.is_some(), "the row stays in the trash");
    assert!(raw_trash_names(&f).iter().any(|n| n.starts_with("companion-1-")));
}

#[test]
fn the_upload_area_keeps_trash_with_companions_blocked() {
    let mut f = Fx::with_protected(&[("up", Protection::Upload)]);
    f.model(1, "up/Foo.3mf");
    f.companion(1, "up/Foo_extras/pic.png");
    let before = view(&f);
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_companion_removed_on_its_own_stays_in_the_web_trash_when_the_model_comes_back() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    // Row 2 was sent to the trash separately, at another time, into the same web trash.
    f.ops.db.execute("UPDATE companion_files SET deleted_at='2026-01-01T00:00:00Z' WHERE id=2", []).unwrap();
    f.ops.restore_file(1).unwrap();
    assert!(f.p("Foo_extras/pic.png").is_file());
    assert!(!f.p("Foo_extras/zzz.png").exists(), "not part of this deletion");
    assert_eq!(crow(&f, 2).2.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert!(raw_trash_names(&f).iter().any(|n| n.starts_with("companion-2-")));
    // Deleting the model for good takes the file along, since its row goes with the model.
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert!(raw_trash_names(&f).is_empty());
}

fn unlink_facts() -> RecoveryFacts {
    RecoveryFacts {
        trusted: true,
        db_before: true,
        db_after: false,
        source_matches: true,
        source_absent: false,
        target_matches: false,
        temporary_matches: false,
        unlink: true,
        db_only: false,
        companions_db_before: true,
        companions_db_after: true,
        companions_all_source: false,
        companions_all_target: false,
        companions_mixed: true,
        companions_any: true,
    }
}

#[test]
fn recovery_decisions_for_a_delete_with_companions_only_go_forward() {
    use RecoveryDecision::*;
    let base = unlink_facts();
    // Some file is gone, the model is still there: finish.
    assert_eq!(recovery_decision(base), FinishUnlink);
    // All files gone, model gone: finish.
    assert_eq!(
        recovery_decision(RecoveryFacts { source_matches: false, source_absent: true, companions_all_target: true, companions_mixed: false, ..base }),
        FinishUnlink
    );
    // Nothing happened yet: give up.
    assert_eq!(
        recovery_decision(RecoveryFacts { companions_all_source: true, companions_mixed: false, ..base }),
        Discard
    );
    // The rows changed while the server was down: not ours to finish.
    assert_eq!(recovery_decision(RecoveryFacts { companions_db_before: false, ..base }), Manual);
    // Something foreign among the files: neither all-source, all-target nor mixed.
    assert_eq!(recovery_decision(RecoveryFacts { companions_mixed: false, ..base }), Manual);
    // A delete never rolls companions back.
    for facts in [base, RecoveryFacts { companions_all_target: true, companions_mixed: false, ..base }] {
        assert_ne!(recovery_decision(facts), RollbackCompanions);
    }
    // The model neither at its source nor gone (cannot be told apart): review.
    assert_eq!(recovery_decision(RecoveryFacts { source_matches: false, source_absent: false, ..base }), Manual);
}

#[test]
fn a_trash_file_swapped_for_another_inode_before_the_delete_is_never_unlinked() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let trash = f.p(TRASH);
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let ours = fs::read_dir(&trash)
                .unwrap()
                .flatten()
                .find(|e| e.file_name().to_str().unwrap().starts_with("companion-2-"))
                .unwrap()
                .path();
            fs::remove_file(&ours).unwrap();
            fs::write(&ours, b"someone else's").unwrap();
        }))
    });
    // The first file is already gone when the second turns out to be foreign: the entry is
    // left for review instead of guessing.
    assert!(matches!(
        f.ops.delete_file_permanently(1),
        Err(Error::Changed) | Err(Error::RestartRequired)
    ));
    let foreign = raw_trash_names(&f).into_iter().find(|n| n.starts_with("companion-2-")).unwrap();
    assert_eq!(fs::read(f.p(&format!("{TRASH}/{foreign}"))).unwrap(), b"someone else's");
    assert_eq!(count(&f, "files"), 1);
}

// ---- Nachbesserungen der Pruefung ----

#[test]
fn an_active_row_blocks_the_delete_even_with_a_trash_path_set() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    f.ops.db.execute("UPDATE companion_files SET deleted_at=NULL WHERE id=2", []).unwrap();
    let before = raw_trash_names(&f);
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
    assert_eq!(raw_trash_names(&f), before);
    assert_eq!(f.ops.blocked_by_companions(1), Ok(true));
    let s = f.ops.empty_trash_summary().unwrap();
    assert_eq!((s.count, s.blocked_by_companions), (0, 1), "the summary agrees with the delete");
}

#[test]
fn a_model_row_changed_before_the_first_unlink_stops_the_delete() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let db_path = f.db_path.clone();
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = crate::db::connect(&db_path).unwrap();
            other.execute("UPDATE files SET name='Renamed.3mf' WHERE id=1", []).unwrap();
        }))
    });
    let before = raw_trash_names(&f);
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::Changed));
    assert_eq!(raw_trash_names(&f), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn the_companion_rows_go_even_when_the_connection_has_foreign_keys_off() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    f.ops.db.execute("PRAGMA foreign_keys = OFF", []).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert_eq!((count(&f, "files"), count(&f, "companion_files")), (0, 0));
}

/// Two roots: the second root's trash is not the model's trash.
#[test]
fn a_trash_file_in_another_roots_trash_is_not_ours_to_delete_or_restore() {
    let mut f = standard();
    let root2 = f.dir.join("root2");
    fs::create_dir(&root2).unwrap();
    f.ops.context = FsContext::new(&[f.root(), root2.clone()], &[]).unwrap();
    f.ops.delete_file(1).unwrap();
    // Row 2's file now sits in the other root's trash, under its own valid name.
    let t2 = crow(&f, 2).3.unwrap();
    let moved = root2.join(TRASH).join(Path::new(&t2).file_name().unwrap());
    fs::rename(&t2, &moved).unwrap();
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=2", [moved.to_str().unwrap()]).unwrap();
    // The read-only view judges it the same way as the delete does.
    let s = f.ops.empty_trash_summary().unwrap();
    assert_eq!((s.count, s.blocked_by_companions), (0, 1));
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
    assert!(moved.is_file());
    f.ops.restore_file(1).unwrap();
    assert_eq!(f.ops.outcome.kept_in_trash, 1);
    assert!(moved.is_file(), "nothing is moved out of the other root's trash");
    assert!(f.p("Foo_extras/pic.png").is_file() && !f.p("Foo_extras/zzz.png").exists());
}

#[test]
fn the_summary_tells_ours_from_foreign_trash_paths_like_the_delete_does() {
    let mut f = standard();
    f.ops.delete_file(1).unwrap();
    let t1 = crow(&f, 1).3.unwrap();
    let name = Path::new(&t1).file_name().unwrap().to_str().unwrap().to_owned();
    let summary = |f: &mut Fx| {
        let s = f.ops.empty_trash_summary().unwrap();
        (s.count, s.blocked_by_companions, s.companion_count)
    };
    assert_eq!(summary(&mut f), (1, 0, 2));
    // A link or an extra name for the file is not a plain file of ours: kept, not counted, not blocking.
    fs::remove_file(&t1).unwrap();
    fs::write(f.p("victim.png"), PNG).unwrap();
    std::os::unix::fs::symlink(f.p("victim.png"), &t1).unwrap();
    assert_eq!(summary(&mut f), (1, 0, 1));
    fs::remove_file(&t1).unwrap();
    // Another row's name, or a folder that merely has the trash's name, belongs to nobody here.
    let wrong_id = f.p(&format!("{TRASH}/{}", name.replace("companion-1-", "companion-77-")));
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=1", [wrong_id.to_str().unwrap()]).unwrap();
    assert_eq!(summary(&mut f), (0, 1, 0));
    fs::create_dir_all(f.p(&format!("sub/{TRASH}"))).unwrap();
    let lookalike = f.p(&format!("sub/{TRASH}/{name}"));
    f.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=1", [lookalike.to_str().unwrap()]).unwrap();
    assert_eq!(summary(&mut f), (0, 1, 0));
}

/// Rows that want the same name from different old places, and one that already has a numbered name.
#[test]
fn names_chosen_together_in_one_restore_never_collide() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    for n in ["a.png", "b.png", "c.png"] {
        f.companion(1, &format!("Foo_extras/{n}"));
    }
    f.ops.delete_file(1).unwrap();
    // As the rows can look after a backup or a desktop rename: same names from different old places.
    for (id, name, dir) in [(1, "a.png", "One"), (2, "a.png", "Two"), (3, "a (2).png", "Three")] {
        let path = f.p(&format!("{dir}_extras/{name}"));
        f.ops
            .db
            .execute("UPDATE companion_files SET name=?1,path=?2 WHERE id=?3", rusqlite::params![name, path.to_str().unwrap(), id])
            .unwrap();
    }
    f.ops.restore_file(1).unwrap();
    let mut names: Vec<String> = (1..=3).map(|id| crow(&f, id).0).collect();
    names.sort();
    assert_eq!(names, ["a (2) (2).png", "a (2).png", "a.png"]);
    let files: Vec<String> = fs::read_dir(f.p("Foo_extras")).unwrap().flatten().map(|e| e.file_name().to_str().unwrap().to_owned()).collect();
    assert_eq!(files.len(), 3, "{files:?}");
    f.assert_rows_match_disk();
}

fn journal_path(f: &Fx) -> std::path::PathBuf {
    f.dir.join("journal/web-fsops.json")
}

#[test]
fn an_interrupted_recovery_of_a_delete_is_finished_by_the_next_one() {
    for k in [0usize, 1] {
        let mut f = standard();
        f.ops.delete_file(1).unwrap();
        POINT.with(|c| *c.borrow_mut() = Some(("after_unlink", 0, PointAction::Crash)));
        assert_eq!(f.ops.delete_file_permanently(1).map(|_| ()), Err(Error::RestartRequired));
        let saved = fs::read(journal_path(&f)).unwrap();
        // The first recovery dies after step k, as a process crash would; the journal on
        // disk is what it was before that recovery started.
        POINT.with(|c| *c.borrow_mut() = Some(("recovery_unlink", k, PointAction::Crash)));
        assert_eq!(f.reopen_and_recover().manual.len(), 1, "k={k}");
        fs::write(journal_path(&f), saved).unwrap();
        assert!(f.reopen_and_recover().manual.is_empty(), "k={k}");
        assert!(raw_trash_names(&f).is_empty());
        assert_eq!((count(&f, "files"), count(&f, "companion_files")), (0, 0));
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn an_interrupted_rollback_during_recovery_is_finished_by_the_next_one() {
    for op in [Op::Trash, Op::Restore] {
        let mut twin = start(op);
        let before = view(&twin);
        let _ = &mut twin;
        let mut f = start(op);
        POINT.with(|c| *c.borrow_mut() = Some(("after_companion", 1, PointAction::Crash)));
        assert_eq!(run(&mut f, op), Err(Error::RestartRequired));
        let saved = fs::read(journal_path(&f)).unwrap();
        POINT.with(|c| *c.borrow_mut() = Some(("recovery_rollback", 1, PointAction::Crash)));
        assert_eq!(f.reopen_and_recover().manual.len(), 1, "{op:?}");
        fs::write(journal_path(&f), saved).unwrap();
        assert!(f.reopen_and_recover().manual.is_empty(), "{op:?}");
        assert_eq!(view(&f), before, "{op:?}");
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn a_journal_whose_places_do_not_match_its_rows_is_left_for_review() {
    // (field to forge, new value) on the first companion step of an interrupted trash.
    let forgeries: &[(&str, &str)] = &[
        ("source", "Foo_extras/zzz.png"),
        ("target", ".katalog-papierkorb/companion-2-aaaaaaaa-zzz.png"),
    ];
    for (field, value) in forgeries {
        let mut f = standard();
        POINT.with(|c| *c.borrow_mut() = Some(("after_companion", 0, PointAction::Crash)));
        assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
        let mut doc: serde_json::Value = serde_json::from_slice(&fs::read(journal_path(&f)).unwrap()).unwrap();
        doc["intents"][0]["companions"][0][*field] = serde_json::json!(value);
        fs::write(journal_path(&f), serde_json::to_vec(&doc).unwrap()).unwrap();
        let trash_before = raw_trash_names(&f);
        assert_eq!(f.reopen_and_recover().manual.len(), 1, "{field}");
        assert_eq!(raw_trash_names(&f), trash_before, "{field}: nothing is moved on a forged journal");
        assert!(f.p("Foo_extras/zzz.png").is_file());
    }
    // A forged trash path in the row's new state is refused as well.
    let mut f = standard();
    POINT.with(|c| *c.borrow_mut() = Some(("after_companion", 0, PointAction::Crash)));
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
    let mut doc: serde_json::Value = serde_json::from_slice(&fs::read(journal_path(&f)).unwrap()).unwrap();
    doc["intents"][0]["companions"][0]["after"]["trash_path"] = serde_json::json!("/elsewhere/x");
    fs::write(journal_path(&f), serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(f.reopen_and_recover().manual.len(), 1);
}
