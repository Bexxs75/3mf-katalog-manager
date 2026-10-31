//! Eine einzelne Begleitdatei entfernen, wiederherstellen und beim Leeren des
//! Papierkorbs endgueltig loeschen: Sollverhalten, Sperren, Konflikte mit dem Desktop
//! und ein Absturz an jedem Schritt des Journals.
use super::companion_carry_tests::{standard, Fx, PNG};
use super::companion_trash_tests::{count, crow, model_deleted_at, raw_trash_names, view};
use super::tests::{PointAction, BEFORE_MUTATION, POINT};
use super::*;
use std::{fs, os::unix::fs::symlink, path::Path};

/// Foo.3mf with pic.png (row 1) and zzz.png (row 2); row 1 is already removed.
fn removed_first() -> Fx {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    f
}

fn run_at(name: &'static str, k: usize, action: PointAction) {
    POINT.with(|c| *c.borrow_mut() = Some((name, k, action)));
}

fn before_mutation(f: impl FnOnce() + 'static) {
    BEFORE_MUTATION.with(|h| *h.borrow_mut() = Some(Box::new(f)));
}

fn other_connection(f: &Fx) -> rusqlite::Connection {
    crate::db::connect(&f.db_path).unwrap()
}

// ---- Entfernen ----

#[test]
fn removing_one_companion_moves_only_that_file_into_the_flat_web_trash() {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    assert!(f.p("Foo.3mf").is_file(), "the model is untouched");
    assert!(!f.p("Foo_extras/pic.png").exists());
    assert!(f.p("Foo_extras/zzz.png").is_file(), "the other companion stays");
    let raw = raw_trash_names(&f);
    assert_eq!(raw.len(), 1);
    assert!(raw[0].starts_with("companion-1-") && raw[0].ends_with("-pic.png"), "{raw:?}");
    let (name, path, deleted, trash) = crow(&f, 1);
    assert_eq!(name, "pic.png");
    assert_eq!(path, f.p("Foo_extras/pic.png").to_str().unwrap(), "the old place stays for the restore");
    assert!(deleted.is_some());
    assert_eq!(trash.as_deref(), f.p(TRASH).join(&raw[0]).to_str());
    assert!(model_deleted_at(&f, 1).is_none());
    assert_eq!(crow(&f, 2).2, None);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn removing_the_last_file_takes_the_empty_folder_with_it() {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    f.ops.trash_companion(2).unwrap();
    assert!(!f.p("Foo_extras").exists());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn remove_and_restore_round_trip_gives_the_same_disk_and_tables() {
    let mut f = standard();
    let before = view(&f);
    f.ops.trash_companion(2).unwrap();
    assert_ne!(view(&f), before);
    f.ops.restore_companion(2).unwrap();
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
    f.assert_rows_match_disk();
}

#[test]
fn removing_twice_a_trashed_model_or_an_unknown_row_is_refused() {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    assert_eq!(f.ops.trash_companion(1), Err(Error::Conflict));
    assert_eq!(f.ops.trash_companion(404), Err(Error::NotFound));
    assert_eq!(f.ops.trash_companion(0), Err(Error::InvalidArguments));
    f.ops.delete_file(1).unwrap();
    let with_model = view(&f);
    assert_eq!(f.ops.trash_companion(2), Err(Error::Conflict), "the model is in the trash");
    assert_eq!(view(&f), with_model);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_companion_row_without_a_file_is_marked_removed_and_can_come_back() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    let ghost = f.row_for(1, &f.p("Foo_extras/ghost.png"));
    f.ops.trash_companion(ghost).unwrap();
    let (_, _, deleted, trash) = crow(&f, ghost);
    assert!(deleted.is_some() && trash.is_none());
    assert!(raw_trash_names(&f).is_empty());
    f.ops.restore_companion(ghost).unwrap();
    let (_, path, deleted, trash) = crow(&f, ghost);
    assert_eq!((deleted, trash), (None, None));
    assert_eq!(path, f.p("Foo_extras/ghost.png").to_str().unwrap());
    assert!(!f.p("Foo_extras").exists(), "no folder is made for a file that is not there");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_removed_companion_keeps_its_own_timestamp_when_the_model_goes_and_comes_back() {
    let mut f = removed_first();
    let stamp = crow(&f, 1).2;
    f.ops.delete_file(1).unwrap();
    assert_eq!(crow(&f, 1).2, stamp, "the earlier removal keeps its own timestamp");
    f.ops.restore_file(1).unwrap();
    assert_eq!(crow(&f, 1).2, stamp, "restoring the model does not bring it back");
    assert!(crow(&f, 2).2.is_none());
}

// ---- Sperren (Wahrheitstabelle) ----

#[test]
fn what_cannot_be_trusted_is_refused_and_leaves_everything_as_it_was() {
    type Setup = fn(&Fx);
    let cases: &[(&str, Setup)] = &[
        ("source is a symlink", |f| {
            fs::remove_file(f.p("Foo_extras/pic.png")).unwrap();
            fs::write(f.p("real.png"), PNG).unwrap();
            symlink(f.p("real.png"), f.p("Foo_extras/pic.png")).unwrap();
        }),
        ("source has a second name", |f| fs::hard_link(f.p("Foo_extras/pic.png"), f.p("alias.png")).unwrap()),
        ("source is a folder", |f| {
            fs::remove_file(f.p("Foo_extras/pic.png")).unwrap();
            fs::create_dir(f.p("Foo_extras/pic.png")).unwrap();
        }),
        ("the companion folder is a link", |f| {
            fs::rename(f.p("Foo_extras"), f.p("moved")).unwrap();
            symlink(f.p("moved"), f.p("Foo_extras")).unwrap();
        }),
        ("the companion folder is a file", |f| {
            fs::remove_dir_all(f.p("Foo_extras")).unwrap();
            fs::write(f.p("Foo_extras"), b"x").unwrap();
        }),
    ];
    for (what, setup) in cases {
        let mut f = standard();
        setup(&f);
        let before = view(&f);
        assert_eq!(f.ops.trash_companion(1), Err(Error::HasCompanions), "{what}");
        assert_eq!(view(&f), before, "{what}");
        assert!(raw_trash_names(&f).is_empty() && f.ops.journal.is_empty(), "{what}");
    }
}

#[test]
fn a_model_in_a_protected_place_and_a_row_in_a_foreign_folder_are_refused() {
    let mut f = Fx::with_protected(&[("up", Protection::Upload)]);
    f.model(1, "up/Up.3mf");
    let row = f.companion(1, "up/Up_extras/a.png");
    assert_eq!(f.ops.trash_companion(row), Err(Error::Protected));
    assert!(f.p("up/Up_extras/a.png").is_file() && f.ops.journal.is_empty());
    // A row that claims another model's folder.
    let mut g = standard();
    g.model(2, "Bar.3mf");
    let stray = g.companion(2, "Foo_extras/stray.png");
    assert_eq!(g.ops.trash_companion(stray), Err(Error::Inconsistent));
    assert!(g.p("Foo_extras/stray.png").is_file());
}

#[test]
fn malformed_stored_rows_are_never_touched() {
    let mut f = standard();
    f.ops.db.execute("UPDATE companion_files SET kind='pdf' WHERE id=1", []).unwrap();
    let before = view(&f);
    assert_eq!(f.ops.trash_companion(1), Err(Error::Inconsistent));
    assert_eq!(view(&f), before);
}

// ---- Wiederherstellen ----

#[test]
fn a_taken_name_gets_a_number_on_disk_and_in_the_table() {
    let mut f = removed_first();
    fs::write(f.p("Foo_extras/pic.png"), b"new one").unwrap();
    f.ops.restore_companion(1).unwrap();
    assert_eq!(crow(&f, 1).0, "pic (2).png");
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"new one", "nothing is replaced");
    assert!(f.p("Foo_extras/pic (2).png").is_file());
    // Taken on disk (pic.png) and in the table only (pic (2).png, a row without a file).
    let mut g = removed_first();
    fs::write(g.p("Foo_extras/pic.png"), b"new one").unwrap();
    g.row_for(1, &g.p("Foo_extras/pic (2).png"));
    g.ops.restore_companion(1).unwrap();
    assert_eq!(crow(&g, 1).0, "pic (3).png");
    // Nothing else holds the old path: the row gets it back.
    let mut h = removed_first();
    h.ops.restore_companion(1).unwrap();
    assert_eq!(crow(&h, 1).0, "pic.png");
}

#[test]
fn restoring_recreates_the_folder_and_removes_it_again_on_failure() {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    f.ops.trash_companion(2).unwrap();
    assert!(!f.p("Foo_extras").exists());
    f.ops.restore_companion(2).unwrap();
    assert!(f.p("Foo_extras/zzz.png").is_file());
    // Same again, but the commit fails: the folder made for it goes away.
    let mut g = standard();
    g.ops.trash_companion(1).unwrap();
    g.ops.trash_companion(2).unwrap();
    let late = g.p("Foo_extras/late.png");
    let other = other_connection(&g);
    run_at(
        "before_commit",
        0,
        PointAction::Run(Box::new(move || {
            crate::db::companions::insert_companion(&other, 1, "late.png", late.to_str().unwrap(), "image", None, "t").unwrap();
        })),
    );
    assert_eq!(g.ops.restore_companion(1), Err(Error::CompanionsChanged));
    assert!(!g.p("Foo_extras").exists(), "the folder this restore made is gone");
    assert_eq!(raw_trash_names(&g).len(), 2, "both files are back in the trash");
    assert!(g.ops.journal.is_empty());
}

#[test]
fn a_file_the_desktop_put_in_the_trash_is_not_restored_here() {
    let mut f = standard();
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/desktop/trash/companion-1-x.png' WHERE id=1",
            [],
        )
        .unwrap();
    let before = view(&f);
    assert_eq!(f.ops.restore_companion(1), Err(Error::TrashForeign));
    assert_eq!(view(&f), before);
    // Inside our trash, but under another row's name.
    let mut g = removed_first();
    let trash = crow(&g, 1).3.unwrap();
    let renamed = g.p(TRASH).join("companion-9-abcd1234-pic.png");
    fs::rename(&trash, &renamed).unwrap();
    g.ops
        .db
        .execute("UPDATE companion_files SET trash_path=?1 WHERE id=1", [renamed.to_str().unwrap()])
        .unwrap();
    assert_eq!(g.ops.restore_companion(1), Err(Error::TrashForeign));
    assert!(renamed.is_file());
}

#[test]
fn a_file_missing_from_the_trash_restores_the_row_only() {
    let mut f = removed_first();
    fs::remove_file(crow(&f, 1).3.unwrap()).unwrap();
    f.ops.restore_companion(1).unwrap();
    let (_, path, deleted, trash) = crow(&f, 1);
    assert_eq!((deleted, trash), (None, None));
    assert!(!Path::new(&path).exists(), "the row shows as missing, nothing is invented");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn restoring_needs_a_live_model_a_removed_row_and_room() {
    let mut f = removed_first();
    assert_eq!(f.ops.restore_companion(2), Err(Error::Conflict), "row 2 was never removed");
    f.ops.delete_file(1).unwrap();
    assert_eq!(f.ops.restore_companion(1), Err(Error::Conflict), "the model is in the trash");
    f.ops.restore_file(1).unwrap();
    // 100 active rows: no room.
    for n in 0..99 {
        f.row_for(1, &f.p(&format!("Foo_extras/n{n}.png")));
    }
    let before = view(&f);
    assert_eq!(f.ops.restore_companion(1), Err(Error::CompanionLimit));
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_full_model_refuses_a_restore_before_anything_is_journalled_or_moved() {
    let mut f = removed_first();
    for n in 0..99 {
        f.row_for(1, &f.p(&format!("Foo_extras/n{n}.png")));
    }
    let reached = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = reached.clone();
    before_mutation(move || flag.set(true));
    assert_eq!(f.ops.restore_companion(1), Err(Error::CompanionLimit));
    assert!(!reached.get(), "the refusal comes from the pre-check, not from an undone move");
}

#[test]
fn a_removed_file_of_a_trashed_model_is_not_purged_on_its_own() {
    let mut f = removed_first();
    f.ops.delete_file(1).unwrap();
    let before = view(&f);
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::Changed));
    assert_eq!(view(&f), before);
    assert!(f.ops.journal.is_empty());
}

// ---- Konflikte mit dem Desktop ----

#[test]
fn the_desktop_changing_the_rows_during_a_remove_undoes_it() {
    for mutate in [
        "INSERT INTO companion_files(file_id,name,path,kind,size_bytes,added_at) VALUES (1,'late.png','/x/Foo_extras/late.png','image',1,'t')",
        "UPDATE companion_files SET name='other.png' WHERE id=2",
        "DELETE FROM companion_files WHERE id=2",
        "UPDATE companion_files SET deleted_at='2026-02-02T00:00:00Z' WHERE id=1",
    ] {
        let mut f = standard();
        let other = other_connection(&f);
        before_mutation(move || {
            other.execute(mutate, []).unwrap();
        });
        assert_eq!(f.ops.trash_companion(1), Err(Error::CompanionsChanged), "{mutate}");
        assert!(f.p("Foo_extras/pic.png").is_file(), "{mutate}");
        assert!(raw_trash_names(&f).is_empty() && f.ops.journal.is_empty(), "{mutate}");
    }
}

#[test]
fn the_desktop_changing_the_rows_during_a_restore_undoes_it() {
    for mutate in [
        "UPDATE companion_files SET name='other.png' WHERE id=2",
        "UPDATE companion_files SET deleted_at=NULL, trash_path=NULL WHERE id=1",
    ] {
        let mut f = removed_first();
        let before = view(&f);
        let other = other_connection(&f);
        before_mutation(move || {
            other.execute(mutate, []).unwrap();
        });
        assert_eq!(f.ops.restore_companion(1), Err(Error::CompanionsChanged), "{mutate}");
        assert_eq!(raw_trash_names(&f).len(), 1, "{mutate}: the file is back in the trash");
        assert!(!f.p("Foo_extras/pic.png").exists() && f.ops.journal.is_empty(), "{mutate}");
        let _ = before;
    }
}

#[test]
fn the_desktop_moving_or_trashing_the_model_during_a_remove_undoes_it() {
    for mutate in [
        "UPDATE files SET deleted_at='2026-02-02T00:00:00Z' WHERE id=1",
        "UPDATE files SET name='Zed.3mf' WHERE id=1",
    ] {
        let mut f = standard();
        let other = other_connection(&f);
        before_mutation(move || {
            other.execute(mutate, []).unwrap();
        });
        assert_eq!(f.ops.trash_companion(1), Err(Error::Changed), "{mutate}");
        assert!(f.p("Foo_extras/pic.png").is_file() && raw_trash_names(&f).is_empty() && f.ops.journal.is_empty());
    }
}

#[test]
fn a_name_taken_during_a_restore_is_never_overwritten() {
    let mut f = removed_first();
    let taken = f.p("Foo_extras/pic.png");
    before_mutation(move || fs::write(&taken, b"desktop").unwrap());
    assert_eq!(f.ops.restore_companion(1), Err(Error::CompanionsConflict));
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"desktop");
    assert_eq!(raw_trash_names(&f).len(), 1, "the removed file is still in the trash");
    assert!(crow(&f, 1).2.is_some());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_file_swapped_after_planning_is_not_moved() {
    let mut f = standard();
    let pic = f.p("Foo_extras/pic.png");
    before_mutation(move || {
        fs::remove_file(&pic).unwrap();
        fs::write(&pic, b"someone else").unwrap();
    });
    assert_eq!(f.ops.trash_companion(1), Err(Error::Changed));
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"someone else");
    assert!(raw_trash_names(&f).is_empty() && f.ops.journal.is_empty());
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
    Purge,
}

fn start(op: Op) -> Fx {
    match op {
        Op::Trash => standard(),
        Op::Restore | Op::Purge => removed_first(),
    }
}
fn run(f: &mut Fx, op: Op) -> Result<()> {
    match op {
        Op::Trash => f.ops.trash_companion(1),
        Op::Restore => f.ops.restore_companion(1),
        Op::Purge => f.ops.purge_loose_companion(1, None),
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
        Crash::Point(name, k) => run_at(name, k, PointAction::Crash),
    }
    assert_eq!(run(&mut f, op), Err(Error::RestartRequired), "{op:?} {crash:?}");
    f.ops.journal.crash_at = None;
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty(), "{op:?} {crash:?}");
    assert!(report.detach_ids.is_empty(), "the model is never detached: {op:?} {crash:?}");
    assert!(f.ops.journal.is_empty(), "{op:?} {crash:?}");
    (f, before, after)
}
fn check(op: Op, points: &[(Crash, Outcome)]) {
    for (crash, expected) in points {
        let (f, before, after) = crash_and_recover(op, *crash);
        assert_eq!(view(&f), if *expected == Outcome::Before { before } else { after }, "{op:?} {crash:?}");
        f.assert_rows_match_disk();
        let leftovers: Vec<String> = fs::read_dir(f.p("Foo_extras"))
            .map(|d| d.flatten().map(|e| e.file_name().to_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        assert!(leftovers.iter().all(|n| !n.starts_with(".fsops-")), "{op:?} {crash:?} {leftovers:?}");
    }
}

#[test]
fn a_crash_at_any_step_of_removing_one_companion_ends_completely_before_or_after() {
    check(
        Op::Trash,
        &[
            (Crash::Phase(Phase::Prepared), Outcome::Before),
            (Crash::Point("after_companion", 0), Outcome::Before),
            (Crash::Phase(Phase::CompanionsMoved), Outcome::Before),
            (Crash::Point("before_commit", 0), Outcome::Before),
            (Crash::Phase(Phase::Committed), Outcome::After),
        ],
    );
}

#[test]
fn a_crash_at_any_step_of_restoring_one_companion_ends_completely_before_or_after() {
    check(
        Op::Restore,
        &[
            (Crash::Phase(Phase::Prepared), Outcome::Before),
            (Crash::Point("after_companion", 0), Outcome::Before),
            (Crash::Phase(Phase::CompanionsMoved), Outcome::Before),
            (Crash::Point("before_commit", 0), Outcome::Before),
            (Crash::Phase(Phase::Committed), Outcome::After),
        ],
    );
}

#[test]
fn a_crash_while_restoring_into_a_folder_that_is_made_for_it_leaves_no_folder_behind() {
    let start_empty = || {
        let mut f = standard();
        f.ops.trash_companion(1).unwrap();
        f.ops.trash_companion(2).unwrap();
        f
    };
    for (name, k) in [("after_mkdir", 0), ("after_companion", 0), ("before_commit", 0)] {
        let mut f = start_empty();
        let before = view(&f);
        run_at(name, k, PointAction::Crash);
        assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired), "{name}");
        let report = f.reopen_and_recover();
        assert!(report.manual.is_empty() && f.ops.journal.is_empty(), "{name}");
        assert_eq!(view(&f), before, "{name}");
        assert!(!f.p("Foo_extras").exists(), "{name}");
    }
    for p in [Phase::Prepared, Phase::CompanionsMoved] {
        let mut f = start_empty();
        let before = view(&f);
        f.ops.journal.crash_at = Some(p);
        assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired), "{p:?}");
        f.ops.journal.crash_at = None;
        assert!(f.reopen_and_recover().manual.is_empty());
        assert_eq!(view(&f), before, "{p:?}");
        assert!(!f.p("Foo_extras").exists(), "{p:?}");
    }
}

#[test]
fn a_crash_at_any_step_of_a_permanent_delete_goes_forward_once_the_file_is_gone() {
    check(
        Op::Purge,
        &[
            (Crash::Phase(Phase::Prepared), Outcome::Before),
            (Crash::Point("after_unlink", 0), Outcome::After),
            (Crash::Phase(Phase::CompanionsMoved), Outcome::After),
            (Crash::Phase(Phase::Unlinked), Outcome::After),
            (Crash::Phase(Phase::Committed), Outcome::After),
        ],
    );
}

#[test]
fn the_database_only_remove_restore_and_purge_survive_a_crash_at_either_end() {
    for (op, crash) in [
        (Op::Trash, Phase::Prepared),
        (Op::Trash, Phase::Committed),
        (Op::Restore, Phase::Prepared),
        (Op::Restore, Phase::Committed),
        (Op::Purge, Phase::Prepared),
        (Op::Purge, Phase::Committed),
    ] {
        let setup = || {
            let mut f = Fx::new();
            f.model(1, "Foo.3mf");
            f.row_for(1, &f.p("Foo_extras/ghost.png"));
            if matches!(op, Op::Restore | Op::Purge) {
                f.ops.trash_companion(1).unwrap();
            }
            f
        };
        let mut twin = setup();
        run(&mut twin, op).unwrap();
        let (before, after) = (view(&setup()), view(&twin));
        let mut f = setup();
        f.ops.journal.crash_at = Some(crash);
        assert_eq!(run(&mut f, op), Err(Error::RestartRequired), "{op:?} {crash:?}");
        f.ops.journal.crash_at = None;
        assert!(f.reopen_and_recover().manual.is_empty(), "{op:?} {crash:?}");
        assert_eq!(view(&f), if crash == Phase::Prepared { before } else { after }, "{op:?} {crash:?}");
    }
}

#[test]
fn a_foreign_file_where_the_removed_one_should_be_needs_manual_review_and_loses_nothing() {
    let mut f = standard();
    run_at("after_companion", 0, PointAction::Crash);
    assert_eq!(f.ops.trash_companion(1), Err(Error::RestartRequired));
    let moved = fs::read_dir(f.p(TRASH)).unwrap().flatten().next().unwrap().path();
    fs::remove_file(&moved).unwrap();
    fs::write(&moved, b"foreign").unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1);
    assert_eq!(fs::read(&moved).unwrap(), b"foreign");
    assert!(f.ops.journal.is_blocked());
    // The same for a file that vanished from both places.
    let mut g = standard();
    run_at("after_companion", 0, PointAction::Crash);
    assert_eq!(g.ops.trash_companion(1), Err(Error::RestartRequired));
    fs::remove_file(fs::read_dir(g.p(TRASH)).unwrap().flatten().next().unwrap().path()).unwrap();
    assert_eq!(g.reopen_and_recover().manual.len(), 1);
}

#[test]
fn a_permanent_delete_never_unlinks_a_foreign_file() {
    // Another file takes the place of the removed one after the plan was made.
    let mut f = removed_first();
    let trash = crow(&f, 1).3.unwrap();
    let swapped = std::path::PathBuf::from(&trash);
    before_mutation(move || {
        fs::remove_file(&swapped).unwrap();
        fs::write(&swapped, b"foreign").unwrap();
    });
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::Changed));
    assert_eq!(fs::read(&trash).unwrap(), b"foreign");
    assert!(f.ops.journal.is_empty());
    // A crash before the unlink, then the swap: recovery stops for review.
    let mut g = removed_first();
    g.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(g.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
    g.ops.journal.crash_at = None;
    let trash = crow(&g, 1).3.unwrap();
    fs::remove_file(&trash).unwrap();
    fs::write(&trash, b"foreign").unwrap();
    let before = view(&g);
    let report = g.reopen_and_recover();
    // Row before, file at neither place as ours: nothing was unlinked yet, so the entry
    // is simply dropped, and the foreign file is untouched either way.
    assert!(report.manual.len() <= 1);
    assert_eq!(fs::read(&trash).unwrap(), b"foreign");
    assert_eq!(count(&g, "companion_files"), 2);
    let _ = before;
}

#[test]
fn a_tampered_journal_entry_is_never_followed() {
    for tamper in ["restore_target_outside", "two_steps", "model_shape"] {
        let mut f = removed_first();
        f.ops.journal.crash_at = Some(Phase::Prepared);
        assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired));
        f.ops.journal.crash_at = None;
        {
            let intent = &mut f.ops.journal.intents[0];
            match tamper {
                "restore_target_outside" => {
                    let step = &mut intent.companions[0];
                    let after = step.after.as_mut().unwrap();
                    after.path = format!("{}/elsewhere_extras/{}", after.path.rsplit_once("/Foo_extras/").unwrap().0, after.name);
                    step.target = Some(format!("elsewhere_extras/{}", after.name));
                }
                "two_steps" => {
                    let again = intent.companions[0].clone();
                    intent.companions.push(again);
                }
                _ => intent.source = Some("Foo.3mf".to_owned()),
            }
        }
        f.ops.journal.persist().unwrap();
        let report = f.reopen_and_recover();
        assert_eq!(report.manual.len(), 1, "{tamper}");
        assert_eq!(raw_trash_names(&f).len(), 1, "{tamper}: the trash file stays");
    }
}

// ---- Journal-Version 3 ----

#[test]
fn the_journal_is_version_3_only_while_a_single_companion_entry_exists() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.trash_companion(1), Err(Error::RestartRequired));
    let text = f.journal_text();
    assert!(text.contains("\"version\":3") && text.contains("\"companion_trash\""), "{text}");
    f.ops.journal.crash_at = None;
    assert!(f.reopen_and_recover().manual.is_empty());
    assert!(f.journal_text().contains("\"version\":1"), "back to the oldest format once it is empty");
    // A model operation with companions stays at version 2.
    let mut g = standard();
    g.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(g.ops.delete_file(1), Err(Error::RestartRequired));
    assert!(g.journal_text().contains("\"version\":2"));
}

#[test]
fn a_journal_whose_version_does_not_match_its_content_is_refused() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.trash_companion(1), Err(Error::RestartRequired));
    let path = f.dir.join("journal/web-fsops.json");
    let good = fs::read_to_string(&path).unwrap();
    for to in ["\"version\":2", "\"version\":1", "\"version\":4"] {
        fs::write(&path, good.replacen("\"version\":3", to, 1)).unwrap();
        assert!(matches!(Journal::open(&f.dir.join("journal")), Err(Error::RestartRequired)), "{to}");
    }
    // Version 3 with only model entries was not written by us.
    let mut g = standard();
    g.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(g.ops.delete_file(1), Err(Error::RestartRequired));
    let model_path = g.dir.join("journal/web-fsops.json");
    let text = fs::read_to_string(&model_path).unwrap().replacen("\"version\":2", "\"version\":3", 1);
    fs::write(&model_path, text).unwrap();
    assert!(matches!(Journal::open(&g.dir.join("journal")), Err(Error::RestartRequired)));
    fs::write(&path, good).unwrap();
    assert!(Journal::open(&f.dir.join("journal")).is_ok());
}

#[test]
fn the_recovery_decision_follows_the_table() {
    use CompanionEditDecision::*;
    let bools = [false, true];
    let mut seen = std::collections::HashSet::new();
    for purge in bools {
        for db_before in bools {
            for db_after in bools {
                for file_any in bools {
                    for file_at_source in bools {
                        for file_at_target in bools {
                            let f = CompanionEditFacts { purge, db_before, db_after, file_any, file_at_source, file_at_target };
                            let d = companion_edit_decision(f);
                            seen.insert(d);
                            if d == Committed {
                                assert!(db_after, "{f:?}");
                                assert!(!(purge && file_any && file_at_source), "{f:?}");
                            }
                            if d == UnlinkThenFinishPurge {
                                assert!(purge && db_after && file_any && file_at_source, "{f:?}");
                            }
                            if d == Rollback {
                                assert!(!purge && db_before && !db_after && file_any && file_at_target, "{f:?}");
                            }
                            if d == FinishPurge {
                                assert!(purge && db_before && !db_after && file_at_target, "{f:?}");
                            }
                            if d == Discard {
                                assert!(db_before && !db_after && (!file_any || file_at_source), "{f:?}");
                            }
                            if !db_before && !db_after {
                                assert_eq!(d, Manual, "{f:?}");
                            }
                        }
                    }
                }
            }
        }
    }
    for (decision, _) in COMPANION_EDIT_TABLE {
        assert!(seen.contains(decision), "{decision:?} is never reached");
    }
    let base = CompanionEditFacts { purge: false, db_before: true, db_after: false, file_any: true, file_at_source: true, file_at_target: false };
    assert_eq!(companion_edit_decision(base), Discard);
    assert_eq!(companion_edit_decision(CompanionEditFacts { file_at_source: false, file_at_target: true, ..base }), Rollback);
    assert_eq!(companion_edit_decision(CompanionEditFacts { file_at_source: false, ..base }), Manual);
    assert_eq!(
        companion_edit_decision(CompanionEditFacts { db_before: false, db_after: true, file_at_source: false, file_at_target: true, ..base }),
        Committed
    );
    assert_eq!(companion_edit_decision(CompanionEditFacts { db_before: false, db_after: true, ..base }), Manual);
    assert_eq!(
        companion_edit_decision(CompanionEditFacts { purge: true, db_before: false, db_after: true, ..base }),
        UnlinkThenFinishPurge
    );
    assert_eq!(
        companion_edit_decision(CompanionEditFacts { purge: true, file_at_source: false, file_at_target: true, ..base }),
        FinishPurge
    );
}

// ---- Papierkorb leeren ----

fn loose_summary(f: &mut Fx) -> TrashSummary {
    f.ops.empty_trash_summary().unwrap()
}

#[test]
fn emptying_the_trash_takes_the_individually_removed_companions_along() {
    let mut f = standard();
    f.model(2, "Free.3mf");
    f.ops.trash_companion(1).unwrap();
    let size = fs::metadata(crow(&f, 1).3.unwrap()).unwrap().len();
    f.ops.db.execute("UPDATE companion_files SET size_bytes=?1 WHERE id=1", [size as i64]).unwrap();
    f.ops.delete_file(2).unwrap();
    let summary = loose_summary(&mut f);
    assert_eq!((summary.count, summary.loose_companion_count, summary.loose_companion_bytes), (1, 1, size));
    let mut snap = f.ops.empty_trash_snapshot_with(1, 1, summary.max_deleted_at.as_deref()).unwrap();
    assert_eq!((snap.ids(), snap.loose_count(), snap.remaining()), (&[2][..], 1, 2));
    assert!(snap.next_loose_block_ids().is_empty(), "models come first");
    let first = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(snap.next_loose_block_ids(), &[1]);
    let second = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert!(second[0].ok && second[0].id == "c1", "{second:?}");
    assert_eq!(snap.remaining(), 0);
    assert!(raw_trash_names(&f).is_empty());
    assert_eq!(count(&f, "companion_files"), 1, "only the active companion is left");
    assert!(f.p("Foo_extras/zzz.png").is_file() && f.p("Foo.3mf").is_file());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_changed_count_cutoff_or_foreign_row_keeps_the_trash_as_it_is() {
    let mut f = removed_first();
    let stamp = loose_summary(&mut f).max_deleted_at.unwrap();
    assert!(matches!(f.ops.empty_trash_snapshot_with(0, 0, Some(&stamp)), Err(Error::Conflict)), "one is there");
    assert!(matches!(f.ops.empty_trash_snapshot_with(0, 2, Some(&stamp)), Err(Error::Conflict)));
    assert!(matches!(f.ops.empty_trash_snapshot_with(0, 1, Some("2000-01-01T00:00:00Z")), Err(Error::Conflict)), "removed after the cutoff");
    assert!(matches!(f.ops.empty_trash_snapshot(0, Some(&stamp)), Err(Error::Conflict)), "the old entry point does not know them");
    // A desktop row, a row of a trashed model and a foreign-name row are not ours to empty.
    let mut g = standard();
    g.ops
        .db
        .execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/desktop/trash/companion-1-x.png' WHERE id=1", [])
        .unwrap();
    assert_eq!(loose_summary(&mut g).loose_companion_count, 0);
    let mut h = removed_first();
    h.ops.delete_file(1).unwrap();
    assert_eq!(loose_summary(&mut h).loose_companion_count, 0, "it belongs to the trashed model now");
    let mut i = removed_first();
    let trash = crow(&i, 1).3.unwrap();
    let odd = i.p(TRASH).join("companion-7-abcd1234-pic.png");
    fs::rename(&trash, &odd).unwrap();
    i.ops.db.execute("UPDATE companion_files SET trash_path=?1 WHERE id=1", [odd.to_str().unwrap()]).unwrap();
    assert_eq!(loose_summary(&mut i).loose_companion_count, 0);
}

#[test]
fn a_row_without_a_trash_file_is_emptied_too_and_without_a_cutoff_nothing_is() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    let ghost = f.row_for(1, &f.p("Foo_extras/ghost.png"));
    f.ops.trash_companion(ghost).unwrap();
    let summary = loose_summary(&mut f);
    assert_eq!((summary.loose_companion_count, summary.loose_companion_bytes), (1, 0));
    assert!(matches!(f.ops.empty_trash_snapshot_with(0, 1, None), Err(Error::Conflict)));
    let mut snap = f.ops.empty_trash_snapshot_with(0, 1, summary.max_deleted_at.as_deref()).unwrap();
    let r = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert!(r[0].ok);
    assert_eq!(count(&f, "companion_files"), 0);
}

#[test]
fn the_purge_refuses_what_changed_after_the_snapshot() {
    // Restored in the meantime: the row is active again and stays.
    let mut f = removed_first();
    let stamp = loose_summary(&mut f).max_deleted_at.unwrap();
    let mut snap = f.ops.empty_trash_snapshot_with(0, 1, Some(&stamp)).unwrap();
    f.ops.restore_companion(1).unwrap();
    let r = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert_eq!(r[0].kind, Some(Error::Changed));
    assert!(f.p("Foo_extras/pic.png").is_file() && count(&f, "companion_files") == 2);
    // A row the desktop changed between check and unlink.
    let mut g = removed_first();
    let other = other_connection(&g);
    before_mutation(move || {
        other.execute("UPDATE companion_files SET name='other.png' WHERE id=2", []).unwrap();
    });
    assert_eq!(g.ops.purge_loose_companion(1, None), Err(Error::CompanionsChanged));
    assert_eq!(raw_trash_names(&g).len(), 1, "nothing was unlinked");
    assert!(g.ops.journal.is_empty());
    // Removed after the cutoff.
    let mut h = removed_first();
    let cutoff = chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap();
    assert_eq!(h.ops.purge_loose_companion(1, Some(cutoff)), Err(Error::Changed));
    // A live row is not a removed one.
    assert_eq!(h.ops.purge_loose_companion(2, None), Err(Error::Changed));
}

#[test]
fn a_blocked_journal_stops_every_new_remove_restore_and_purge() {
    let mut f = removed_first();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired));
    f.ops.journal.crash_at = None;
    assert_eq!(f.ops.trash_companion(2), Err(Error::RestartRequired));
    assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired));
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
}

// ---- Entfernte Zeile nach Umbenennen/Verschieben des Modells ----

#[test]
fn a_removed_file_can_be_restored_after_the_model_was_renamed_or_moved() {
    for how in ["rename", "move"] {
        let mut f = removed_first();
        let old_path = crow(&f, 1).1;
        match how {
            "rename" => f.ops.rename_file(1, "Bar").unwrap(),
            _ => f.ops.move_file_to_folder(1, Some(1)).unwrap(),
        }
        assert_eq!(crow(&f, 1).1, old_path, "{how}: the removed row keeps its old place");
        f.ops.restore_companion(1).unwrap();
        let (name, path, deleted, trash) = crow(&f, 1);
        let model_dir = Path::new(&f.mpath(1)).parent().unwrap().to_owned();
        let stem = if how == "rename" { "Bar" } else { "Foo" };
        assert_eq!(Path::new(&path), model_dir.join(format!("{stem}_extras")).join("pic.png"), "{how}");
        assert!(Path::new(&path).is_file() && deleted.is_none() && trash.is_none() && name == "pic.png", "{how}");
        assert!(raw_trash_names(&f).is_empty() && f.ops.journal.is_empty());
        f.assert_rows_match_disk();
    }
}

#[test]
fn a_removed_file_is_counted_and_emptied_after_the_model_was_renamed() {
    let mut f = removed_first();
    f.ops.rename_file(1, "Bar").unwrap();
    let summary = loose_summary(&mut f);
    assert_eq!(summary.loose_companion_count, 1);
    let mut snap = f.ops.empty_trash_snapshot_with(0, 1, summary.max_deleted_at.as_deref()).unwrap();
    let r = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert!(r[0].ok, "{r:?}");
    assert!(raw_trash_names(&f).is_empty());
    assert_eq!(count(&f, "companion_files"), 1);
    assert_eq!(loose_summary(&mut f).loose_companion_count, 0);
}

#[test]
fn a_restore_interrupted_after_a_rename_of_the_model_is_still_settled() {
    for phase in [Phase::Prepared, Phase::CompanionsMoved, Phase::Committed] {
        let mut f = removed_first();
        f.ops.rename_file(1, "Bar").unwrap();
        let before = view(&f);
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(f.ops.restore_companion(1), Err(Error::RestartRequired), "{phase:?}");
        f.ops.journal.crash_at = None;
        let report = f.reopen_and_recover();
        assert!(report.manual.is_empty() && f.ops.journal.is_empty(), "{phase:?}");
        if phase != Phase::Committed {
            assert_eq!(view(&f), before, "{phase:?}");
        } else {
            assert!(f.p("Bar_extras/pic.png").is_file());
        }
    }
}

#[test]
fn a_purge_crash_after_a_rename_of_the_model_goes_forward() {
    let mut f = removed_first();
    f.ops.rename_file(1, "Bar").unwrap();
    run_at("after_unlink", 0, PointAction::Crash);
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
    assert!(f.reopen_and_recover().manual.is_empty());
    assert_eq!(count(&f, "companion_files"), 1);
    assert!(raw_trash_names(&f).is_empty());
}

#[test]
fn a_purge_whose_row_vanished_before_the_unlink_still_removes_its_own_file_after_a_crash() {
    let mut f = removed_first();
    run_at("before_unlink", 0, PointAction::Crash);
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
    assert_eq!(raw_trash_names(&f).len(), 1, "the file is still in the trash");
    other_connection(&f).execute("DELETE FROM companion_files WHERE id=1", []).unwrap();
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty() && f.ops.journal.is_empty());
    assert!(raw_trash_names(&f).is_empty(), "no orphan is left in the trash");
}

#[test]
fn a_foreign_file_under_a_purged_rows_trash_name_is_not_removed() {
    let mut f = removed_first();
    run_at("before_unlink", 0, PointAction::Crash);
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
    other_connection(&f).execute("DELETE FROM companion_files WHERE id=1", []).unwrap();
    let name = raw_trash_names(&f).remove(0);
    let path = f.p(&format!(".katalog-papierkorb/{name}"));
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"somebody else").unwrap();
    assert!(f.reopen_and_recover().manual.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"somebody else");
}

#[test]
fn a_row_that_changed_after_the_unlink_is_not_deleted_by_the_purge() {
    let mut f = removed_first();
    let other = other_connection(&f);
    run_at(
        "after_unlink",
        0,
        PointAction::Run(Box::new(move || {
            other.execute("UPDATE companion_files SET deleted_at=NULL, trash_path=NULL WHERE id=1", []).unwrap();
        })),
    );
    assert_eq!(f.ops.purge_loose_companion(1, None), Err(Error::RestartRequired));
    assert_eq!(count(&f, "companion_files"), 2, "the row the desktop restored in between survives");
    assert!(f.ops.journal.is_blocked());
}

#[test]
fn an_unreadable_timestamp_or_a_malformed_row_neither_breaks_nor_inflates_the_summary() {
    let mut f = removed_first();
    f.ops.trash_companion(2).unwrap();
    f.ops.db.execute("UPDATE companion_files SET deleted_at='garbage' WHERE id=2", []).unwrap();
    let summary = loose_summary(&mut f);
    assert_eq!(summary.loose_companion_count, 1, "only the readable one counts");
    let mut snap = f.ops.empty_trash_snapshot_with(0, 1, summary.max_deleted_at.as_deref()).unwrap();
    assert!(f.ops.empty_trash(&mut snap, || true).unwrap()[0].ok);
    // A row its removal would refuse (kind does not fit the name) is not offered either.
    let mut g = removed_first();
    g.ops.db.execute("UPDATE companion_files SET kind='pdf' WHERE id=1", []).unwrap();
    assert_eq!(loose_summary(&mut g).loose_companion_count, 0);
}
