//! Umbenennen und Verschieben von Modellen mit Begleitdateien: Soll-Verhalten,
//! Namenskonflikte, Randfaelle und ein Absturz an jedem Schritt.
use super::tests::{PointAction, BEFORE_MUTATION, POINT};
use super::*;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

const PNG: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52];

#[derive(Default)]
struct Ledger;
impl UploadLedger for Ledger {
    fn reset_imported(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
    fn detach(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
}

struct Fx {
    dir: PathBuf,
    db_path: PathBuf,
    ops: FileOps<rusqlite::Connection, Ledger>,
}
impl Drop for Fx {
    fn drop(&mut self) {
        POINT.with(|p| *p.borrow_mut() = None);
        BEFORE_MUTATION.with(|h| *h.borrow_mut() = None);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Fx {
    fn new() -> Self {
        Self::with_protected(&[])
    }
    fn with_protected(protected: &[(&str, Protection)]) -> Self {
        let dir = crate::unique_test_dir("fsops_companion_carry");
        fs::create_dir_all(dir.join("root/sub")).unwrap();
        fs::create_dir(dir.join("journal")).unwrap();
        let protected: Vec<(PathBuf, Protection)> =
            protected.iter().map(|(p, k)| (dir.join("root").join(p), *k)).collect();
        for (p, _) in &protected {
            fs::create_dir_all(p).unwrap();
        }
        let db_path = dir.join("catalog.db");
        let db = crate::db::connect(&db_path).unwrap();
        db.execute(
            "INSERT INTO folders(id,name,path) VALUES (1,'sub',?1)",
            [dir.join("root/sub").to_str().unwrap()],
        )
        .unwrap();
        let context = FsContext::new(&[dir.join("root")], &protected).unwrap();
        let journal = Journal::open(&dir.join("journal")).unwrap();
        Fx { dir: dir.clone(), db_path, ops: FileOps::new(context, journal, db, Ledger) }
    }
    fn root(&self) -> PathBuf {
        self.dir.join("root")
    }
    fn p(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
    }
    /// Model file `rel` (relative to the root) with a catalog row.
    fn model(&mut self, id: i64, rel: &str) {
        let path = self.p(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("model {id}")).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        self.ops
            .db
            .execute(
                "INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (?1,?2,?3,'3mf',5,'2026-01-01T00:00:00Z')",
                rusqlite::params![id, name, path.to_str().unwrap()],
            )
            .unwrap();
    }
    /// A companion row with a real file at `rel`.
    fn companion(&mut self, model: i64, rel: &str) -> i64 {
        let path = self.p(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, PNG).unwrap();
        self.row_for(model, &path)
    }
    /// A companion row without any file on disk.
    fn row_for(&mut self, model: i64, path: &Path) -> i64 {
        let name = path.file_name().unwrap().to_str().unwrap();
        crate::db::companions::insert_companion(
            &self.ops.db,
            model,
            name,
            path.to_str().unwrap(),
            "image",
            Some(PNG.len() as i64),
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
    }
    fn cpath(&self, id: i64) -> String {
        self.ops
            .db
            .query_row("SELECT path FROM companion_files WHERE id=?1", [id], |r| r.get(0))
            .unwrap()
    }
    fn mpath(&self, id: i64) -> String {
        self.ops.db.query_row("SELECT path FROM files WHERE id=?1", [id], |r| r.get(0)).unwrap()
    }
    /// Disk tree and database rows, paths relative to the root: two equal snapshots
    /// mean the same state.
    fn snapshot(&self) -> String {
        let mut lines = Vec::new();
        walk(&self.root(), &self.root(), &mut lines);
        let strip = |s: String| s.replace(self.root().to_str().unwrap(), "");
        let mut stmt = self.ops.db.prepare("SELECT id,name,path,folder_id,deleted_at,trash_path FROM files ORDER BY id").unwrap();
        for r in stmt
            .query_map([], |r| {
                Ok(format!(
                    "F {} {} {} {:?} {:?} {:?}",
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?
                ))
            })
            .unwrap()
        {
            lines.push(strip(r.unwrap()));
        }
        let mut stmt = self
            .ops
            .db
            .prepare("SELECT id,file_id,name,path,deleted_at,trash_path FROM companion_files ORDER BY id")
            .unwrap();
        for r in stmt
            .query_map([], |r| {
                Ok(format!(
                    "C {} {} {} {} {:?} {:?}",
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?
                ))
            })
            .unwrap()
        {
            lines.push(strip(r.unwrap()));
        }
        lines.join("\n")
    }
    /// Every active companion row points at a regular file that exists.
    fn assert_rows_match_disk(&self) {
        let mut stmt = self.ops.db.prepare("SELECT path FROM companion_files WHERE deleted_at IS NULL").unwrap();
        for p in stmt.query_map([], |r| r.get::<_, String>(0)).unwrap() {
            let p = p.unwrap();
            assert!(Path::new(&p).is_file(), "row without file: {p}");
        }
    }
    fn reopen_and_recover(&mut self) -> RecoveryReport {
        self.ops.journal = Journal::open(&self.dir.join("journal")).unwrap();
        let report = self.ops.recover().unwrap();
        if report.manual.is_empty() {
            self.ops.finish_recovery().unwrap();
        }
        report
    }
    fn journal_text(&self) -> String {
        fs::read_to_string(self.dir.join("journal/web-fsops.json")).unwrap()
    }
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let rel = p.strip_prefix(base).unwrap().display().to_string();
        if rel.starts_with(TRASH) {
            continue;
        }
        let meta = fs::symlink_metadata(&p).unwrap();
        if meta.file_type().is_symlink() {
            out.push(format!("l /{rel}"));
        } else if meta.is_dir() {
            out.push(format!("d /{rel}"));
            walk(base, &p, out);
        } else {
            out.push(format!("f /{rel} {:?}", fs::read(&p).unwrap()));
        }
    }
}

/// Foo.3mf (id 1) with two companions in Foo_extras, plus an unrelated model.
fn standard() -> Fx {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.companion(1, "Foo_extras/pic.png");
    f.companion(1, "Foo_extras/zzz.png");
    f
}

// ---- Soll-Verhalten ----

#[test]
fn rename_carries_every_companion_and_removes_the_empty_old_folder() {
    let mut f = standard();
    f.ops.rename_file(1, "Bar").unwrap();
    assert!(f.p("Bar.3mf").exists() && !f.p("Foo.3mf").exists());
    assert!(f.p("Bar_extras/pic.png").is_file() && f.p("Bar_extras/zzz.png").is_file());
    assert!(!f.p("Foo_extras").exists(), "the empty old folder goes away");
    assert_eq!(f.cpath(1), f.p("Bar_extras/pic.png").to_str().unwrap());
    assert_eq!(f.cpath(2), f.p("Bar_extras/zzz.png").to_str().unwrap());
    assert!(f.ops.journal.is_empty());
    f.assert_rows_match_disk();
}

#[test]
fn case_only_rename_carries_the_companions_too() {
    let mut f = standard();
    f.ops.rename_file(1, "foo").unwrap();
    assert!(f.p("foo.3mf").exists());
    assert!(f.p("foo_extras/pic.png").is_file());
    assert!(!f.p("Foo_extras").exists());
    f.assert_rows_match_disk();
}

#[test]
fn move_creates_the_target_folder_and_carries_the_files() {
    let mut f = standard();
    f.ops.move_file_to_folder(1, Some(1)).unwrap();
    assert!(f.p("sub/Foo.3mf").exists());
    assert!(f.p("sub/Foo_extras/pic.png").is_file() && f.p("sub/Foo_extras/zzz.png").is_file());
    assert!(!f.p("Foo_extras").exists());
    f.assert_rows_match_disk();
}

#[test]
fn models_with_the_same_stem_only_take_their_own_files() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.model(2, "Foo.stl");
    f.companion(1, "Foo_extras/mine.png");
    f.companion(2, "Foo_extras/theirs.png");
    // An unregistered file stays where it is as well.
    fs::write(f.p("Foo_extras/loose.txt"), b"loose").unwrap();
    let theirs = f.cpath(2);
    f.ops.rename_file(1, "Bar").unwrap();
    assert!(f.p("Bar_extras/mine.png").is_file());
    assert!(f.p("Foo_extras/theirs.png").is_file(), "the other model keeps its file");
    assert!(f.p("Foo_extras/loose.txt").is_file());
    assert_eq!(f.cpath(2), theirs, "its row is untouched");
    f.assert_rows_match_disk();
}

#[test]
fn move_merges_into_an_existing_extras_folder_of_a_model_with_the_same_stem() {
    let mut f = standard();
    f.model(2, "sub/Foo.stl");
    f.companion(2, "sub/Foo_extras/other.png");
    f.ops.move_file_to_folder(1, Some(1)).unwrap();
    assert!(f.p("sub/Foo_extras/other.png").is_file());
    assert!(f.p("sub/Foo_extras/pic.png").is_file());
    f.assert_rows_match_disk();
}

#[test]
fn a_missing_file_only_rewrites_its_row_and_creates_no_folder() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    let gone = f.row_for(1, &f.p("Foo_extras/gone.png"));
    f.ops.move_file_to_folder(1, Some(1)).unwrap();
    assert_eq!(f.cpath(gone), f.p("sub/Foo_extras/gone.png").to_str().unwrap());
    assert!(!f.p("sub/Foo_extras").exists(), "nothing to carry, so no folder");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn present_and_missing_files_mix() {
    let mut f = standard();
    let gone = f.row_for(1, &f.p("Foo_extras/gone.png"));
    f.ops.rename_file(1, "Bar").unwrap();
    assert_eq!(f.cpath(gone), f.p("Bar_extras/gone.png").to_str().unwrap());
    assert!(f.p("Bar_extras/pic.png").is_file());
}

#[test]
fn rows_outside_the_extras_folder_and_trashed_rows_stay_as_they_are() {
    let mut f = standard();
    fs::create_dir(f.p("elsewhere")).unwrap();
    let outside = f.companion(1, "elsewhere/odd.png");
    let trashed = f.companion(1, "Foo_extras/old.png");
    f.ops
        .db
        .execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z' WHERE id=?1", [trashed])
        .unwrap();
    let (outside_path, trashed_path) = (f.cpath(outside), f.cpath(trashed));
    f.ops.rename_file(1, "Bar").unwrap();
    assert_eq!(f.cpath(outside), outside_path);
    assert_eq!(f.cpath(trashed), trashed_path);
    assert!(f.p("elsewhere/odd.png").is_file());
    assert!(f.p("Foo_extras/old.png").is_file(), "the trashed row's file is not ours to move");
    assert!(f.p("Foo_extras").exists(), "so the old folder stays");
}

#[test]
fn batch_move_with_mixed_models() {
    let mut f = standard();
    f.model(2, "Free.3mf");
    f.model(3, "Clash.3mf");
    f.companion(3, "Clash_extras/pic.png");
    fs::create_dir_all(f.p("sub/Clash_extras")).unwrap();
    fs::write(f.p("sub/Clash_extras/pic.png"), b"someone else").unwrap();
    let r = f.ops.move_files_to_folder(&[1, 2, 3], Some(1), || true).unwrap();
    assert!(r[0].ok && r[1].ok);
    assert_eq!(r[2].kind, Some(Error::CompanionsConflict));
    assert!(f.p("sub/Foo_extras/pic.png").is_file());
    assert!(f.p("Clash.3mf").exists() && f.p("Clash_extras/pic.png").is_file());
    assert_eq!(fs::read(f.p("sub/Clash_extras/pic.png")).unwrap(), b"someone else");
}

// ---- Konflikte und Sperren ----

#[test]
fn a_taken_name_at_the_destination_changes_nothing() {
    let mut f = standard();
    fs::create_dir(f.p("sub/Foo_extras")).unwrap();
    fs::write(f.p("sub/Foo_extras/zzz.png"), b"someone else").unwrap();
    let before = f.snapshot();
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::CompanionsConflict));
    assert_eq!(f.snapshot(), before);
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_row_holding_the_destination_path_is_a_conflict() {
    let mut f = standard();
    f.model(2, "Other.3mf");
    f.row_for(2, &f.p("Bar_extras/pic.png"));
    let before = f.snapshot();
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::CompanionsConflict));
    assert_eq!(f.snapshot(), before);
}

#[test]
fn a_name_taken_after_the_pre_check_rolls_the_first_file_back() {
    let mut f = standard();
    let p = f.p("sub/Foo_extras/zzz.png");
    // Lands between the pre-check and the second rename, after the folder exists.
    POINT.with(|c| {
        *c.borrow_mut() = Some((
            "after_companion",
            0,
            PointAction::Run(Box::new(move || fs::write(&p, b"late").unwrap())),
        ))
    });
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::CompanionsConflict));
    assert_eq!(fs::read(f.p("sub/Foo_extras/zzz.png")).unwrap(), b"late", "the late file is not ours and stays");
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(f.p("Foo.3mf").exists() && !f.p("sub/Foo.3mf").exists());
    assert!(!f.p("sub/Foo_extras/pic.png").exists(), "the first file is back");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_destination_extras_symlink_or_file_is_a_conflict() {
    for kind in ["symlink", "file"] {
        let mut f = standard();
        if kind == "symlink" {
            fs::create_dir(f.dir.join("elsewhere")).unwrap();
            symlink(f.dir.join("elsewhere"), f.p("sub/Foo_extras")).unwrap();
        } else {
            fs::write(f.p("sub/Foo_extras"), b"x").unwrap();
        }
        let before = f.snapshot();
        assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::CompanionsConflict), "{kind}");
        assert_eq!(f.snapshot(), before, "{kind}");
        assert!(f.dir.join("elsewhere").read_dir().map(|mut d| d.next().is_none()).unwrap_or(true));
    }
}

#[test]
fn an_extras_symlink_at_the_source_stays_blocked() {
    for kind in ["symlink", "file"] {
        let mut f = Fx::new();
        f.model(1, "Foo.3mf");
        if kind == "symlink" {
            fs::create_dir(f.dir.join("elsewhere")).unwrap();
            fs::write(f.dir.join("elsewhere/pic.png"), PNG).unwrap();
            symlink(f.dir.join("elsewhere"), f.p("Foo_extras")).unwrap();
        } else {
            fs::write(f.p("Foo_extras"), b"x").unwrap();
        }
        f.row_for(1, &f.p("Foo_extras/pic.png"));
        let before = f.snapshot();
        assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::HasCompanions), "{kind}");
        assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::HasCompanions), "{kind}");
        assert_eq!(f.snapshot(), before, "{kind}");
        assert!(f.dir.join("elsewhere").read_dir().map(|d| d.count() <= 1).unwrap_or(true));
    }
}

#[test]
fn a_symlinked_or_hardlinked_companion_stays_blocked() {
    for kind in ["symlink", "hardlink"] {
        let mut f = Fx::new();
        f.model(1, "Foo.3mf");
        fs::create_dir(f.p("Foo_extras")).unwrap();
        fs::write(f.dir.join("outside.png"), PNG).unwrap();
        if kind == "symlink" {
            symlink(f.dir.join("outside.png"), f.p("Foo_extras/pic.png")).unwrap();
        } else {
            fs::hard_link(f.dir.join("outside.png"), f.p("Foo_extras/pic.png")).unwrap();
        }
        f.row_for(1, &f.p("Foo_extras/pic.png"));
        let before = f.snapshot();
        assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::HasCompanions), "{kind}");
        assert_eq!(f.snapshot(), before, "{kind}");
    }
}

#[test]
fn the_upload_area_keeps_companions_blocked() {
    let mut f = Fx::with_protected(&[("up", Protection::Upload)]);
    f.model(1, "up/Foo.3mf");
    f.companion(1, "up/Foo_extras/pic.png");
    let before = f.snapshot();
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::HasCompanions));
    assert_eq!(f.snapshot(), before);
}

#[test]
fn trash_and_permanent_delete_stay_blocked() {
    let mut f = standard();
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
    f.ops
        .db
        .execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path=NULL WHERE id=1", [])
        .unwrap();
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
}

// ---- Rennen zwischen Pruefung und Commit ----

#[test]
fn a_row_added_after_the_pre_check_undoes_the_whole_move() {
    let mut f = standard();
    let db_path = f.db_path.clone();
    let extra = f.p("Foo_extras/late.png");
    fs::write(&extra, PNG).unwrap();
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = crate::db::connect(&db_path).unwrap();
            crate::db::companions::insert_companion(&other, 1, "late.png", extra.to_str().unwrap(), "image", None, "t").unwrap();
        }))
    });
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::CompanionsChanged));
    assert!(f.p("Foo.3mf").exists() && !f.p("sub/Foo.3mf").exists());
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(!f.p("sub/Foo_extras").exists(), "the folder this move made is gone again");
    assert_eq!(f.cpath(1), f.p("Foo_extras/pic.png").to_str().unwrap());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn a_row_changed_after_the_pre_check_undoes_the_whole_rename() {
    let mut f = standard();
    let db_path = f.db_path.clone();
    BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = crate::db::connect(&db_path).unwrap();
            other.execute("DELETE FROM companion_files WHERE id=2", []).unwrap();
        }))
    });
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::CompanionsChanged));
    assert!(f.p("Foo.3mf").exists() && f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(!f.p("Bar_extras").exists());
}

#[test]
fn a_companion_changed_after_the_pre_check_is_not_moved() {
    let mut f = standard();
    let file = f.p("Foo_extras/pic.png");
    BEFORE_MUTATION.with(|h| *h.borrow_mut() = Some(Box::new(move || fs::write(&file, b"changed size").unwrap())));
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::Changed));
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"changed size");
    assert!(f.p("Foo.3mf").exists() && !f.p("Bar_extras").exists());
    assert!(f.ops.journal.is_empty());
}

// ---- Absturz an jedem Schritt ----

#[derive(Clone, Copy, Debug)]
enum Op {
    Rename,
    Move,
    CaseRename,
}
fn run(f: &mut Fx, op: Op) -> Result<()> {
    match op {
        Op::Rename => f.ops.rename_file(1, "Bar"),
        Op::Move => f.ops.move_file_to_folder(1, Some(1)),
        Op::CaseRename => f.ops.rename_file(1, "foo"),
    }
}
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

#[test]
fn a_crash_at_any_step_ends_completely_before_or_completely_after() {
    let points: &[(Crash, Outcome, bool)] = &[
        (Crash::Phase(Phase::Prepared), Outcome::Before, true),
        (Crash::Point("after_mkdir", 0), Outcome::Before, true),
        (Crash::Point("after_companion", 0), Outcome::Before, true),
        (Crash::Point("after_companion", 1), Outcome::Before, true),
        (Crash::Phase(Phase::CompanionsMoved), Outcome::Before, true),
        (Crash::Phase(Phase::TmpRenamed), Outcome::After, false),
        (Crash::Phase(Phase::Renamed), Outcome::After, true),
        (Crash::Point("before_commit", 0), Outcome::After, true),
        (Crash::Phase(Phase::Committed), Outcome::After, true),
    ];
    for op in [Op::Rename, Op::Move, Op::CaseRename] {
        let mut twin = standard();
        run(&mut twin, op).unwrap();
        let after = twin.snapshot();
        for (crash, expected, any_op) in points {
            // Only a case-only rename passes through the temporary name.
            if !any_op && !matches!(op, Op::CaseRename) {
                continue;
            }
            let mut f = standard();
            let before = f.snapshot();
            match crash {
                Crash::Phase(p) => f.ops.journal.crash_at = Some(*p),
                Crash::Point(name, k) => POINT.with(|c| *c.borrow_mut() = Some((name, *k, PointAction::Crash))),
            }
            assert_eq!(run(&mut f, op), Err(Error::RestartRequired), "{op:?} {crash:?}");
            f.ops.journal.crash_at = None;
            let report = f.reopen_and_recover();
            assert!(report.manual.is_empty(), "{op:?} {crash:?}");
            assert!(f.ops.journal.is_empty(), "{op:?} {crash:?}");
            let now = f.snapshot();
            match expected {
                Outcome::Before => assert_eq!(now, before, "{op:?} {crash:?}"),
                Outcome::After => assert_eq!(now, after, "{op:?} {crash:?}"),
            }
            f.assert_rows_match_disk();
            // And the system works again.
            assert!(f.ops.state(1).is_ok());
        }
    }
}

#[test]
fn a_journal_with_companions_is_version_2_and_one_without_stays_version_1() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::CompanionsMoved);
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    let text = f.journal_text();
    assert!(text.contains("\"version\":2") && text.contains("\"companions\"") && text.contains("\"companion_rows\""));

    let mut plain = Fx::new();
    plain.model(1, "Foo.3mf");
    plain.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(plain.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    let text = plain.journal_text();
    assert!(text.contains("\"version\":1"));
    for key in ["companions", "companion_rows", "extras_dir"] {
        assert!(!text.contains(key), "{key}");
    }
    assert!(plain.reopen_and_recover().manual.is_empty());
}

#[test]
fn a_journal_in_the_old_format_still_loads_and_recovers() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    // A file exactly as the previous release wrote it: version 1 and none of the new keys.
    let path = f.dir.join("journal/web-fsops.json");
    let value: serde_json::Value = serde_json::from_str(&f.journal_text()).unwrap();
    assert_eq!(value["version"], 1);
    let intent = value["intents"][0].as_object().unwrap();
    assert!(!intent.contains_key("companions") && !intent.contains_key("extras_dir"));
    let text = serde_json::to_string(&value).unwrap();
    fs::write(&path, text).unwrap();
    assert!(f.reopen_and_recover().manual.is_empty());
    assert!(f.p("Bar.3mf").exists());
    assert_eq!(f.mpath(1), f.p("Bar.3mf").to_str().unwrap());
}

#[test]
fn a_version_1_file_with_companion_data_and_unknown_versions_are_refused() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::CompanionsMoved);
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    let path = f.dir.join("journal/web-fsops.json");
    let good = f.journal_text();
    for bad in [good.replace("\"version\":2", "\"version\":1"), good.replace("\"version\":2", "\"version\":3")] {
        fs::write(&path, bad).unwrap();
        assert!(matches!(Journal::open(&f.dir.join("journal")), Err(Error::RestartRequired)));
    }
    fs::write(&path, good).unwrap();
    let reopened = Journal::open(&f.dir.join("journal")).unwrap();
    assert_eq!(reopened.intents.len(), 1);
    assert_eq!(reopened.intents[0].companions.len(), 2);
    assert_eq!(reopened.intents[0].extras_dir.as_ref().map(|d| d.identity.is_some()), Some(true));
}

#[test]
fn a_foreign_file_at_a_target_after_a_crash_needs_manual_review_and_loses_nothing() {
    let mut f = standard();
    POINT.with(|c| *c.borrow_mut() = Some(("after_companion", 0, PointAction::Crash)));
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    // The moved file is replaced by something else before recovery looks.
    fs::remove_file(f.p("Bar_extras/pic.png")).unwrap();
    fs::write(f.p("Bar_extras/pic.png"), b"foreign").unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1);
    assert!(f.ops.journal.is_blocked());
    assert_eq!(fs::read(f.p("Bar_extras/pic.png")).unwrap(), b"foreign");
    assert!(f.p("Foo_extras/zzz.png").is_file());
    assert_eq!(f.mpath(1), f.p("Foo.3mf").to_str().unwrap());
}

#[test]
fn a_failing_rollback_blocks_the_journal_and_deletes_nothing() {
    let mut f = standard();
    let (foreign, late) = (f.p("sub/Foo_extras/pic.png"), f.p("sub/Foo_extras/zzz.png"));
    // After the first file moved: swap it for another file and occupy the second target.
    POINT.with(|c| {
        *c.borrow_mut() = Some((
            "after_companion",
            0,
            PointAction::Run(Box::new(move || {
                fs::remove_file(&foreign).unwrap();
                fs::write(&foreign, b"foreign").unwrap();
                fs::write(&late, b"late").unwrap();
            })),
        ))
    });
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::RestartRequired));
    assert!(f.ops.journal.is_blocked());
    assert_eq!(fs::read(f.p("sub/Foo_extras/pic.png")).unwrap(), b"foreign");
    assert_eq!(fs::read(f.p("sub/Foo_extras/zzz.png")).unwrap(), b"late");
    assert!(f.p("Foo_extras/zzz.png").is_file());
    assert!(f.p("Foo.3mf").exists());
    assert_eq!(f.ops.rename_file(1, "Other"), Err(Error::RestartRequired));
}

#[test]
fn a_folder_created_but_not_recorded_is_left_alone_by_recovery() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::RestartRequired));
    // As if the process died between mkdir and recording its inode.
    fs::create_dir(f.p("sub/Foo_extras")).unwrap();
    assert!(f.reopen_and_recover().manual.is_empty());
    assert!(f.p("sub/Foo_extras").is_dir(), "an unknown inode is never removed");
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo.3mf").exists());
}

#[test]
fn an_old_entry_without_companion_data_is_not_finished_over_a_companion_added_since() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    // The desktop added a companion while the web side was down.
    f.companion(1, "Foo_extras/new.png");
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1, "finishing would strand the new file");
    assert_eq!(f.mpath(1), f.p("Foo.3mf").to_str().unwrap());

    // Giving the operation up is still safe: nothing was committed.
    let mut g = Fx::new();
    g.model(1, "Foo.3mf");
    g.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(g.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    g.companion(1, "Foo_extras/new.png");
    assert!(g.reopen_and_recover().manual.is_empty());
    assert!(g.p("Foo.3mf").exists());
}

#[test]
fn recovery_decision_cases_for_companions() {
    let base = RecoveryFacts {
        trusted: true,
        db_before: true,
        db_after: false,
        source_matches: true,
        source_absent: false,
        target_matches: false,
        temporary_matches: false,
        unlink: false,
        db_only: false,
        companions_db_before: true,
        companions_db_after: true,
        companions_all_source: true,
        companions_all_target: true,
        companions_mixed: false,
        companions_any: false,
    };
    use RecoveryDecision::*;
    let partly = RecoveryFacts { companions_any: true, companions_all_source: false, companions_all_target: false, companions_mixed: true, ..base };
    let all_there = RecoveryFacts { companions_any: true, companions_all_source: false, ..base };
    let invalid = RecoveryFacts { companions_any: true, companions_all_source: false, companions_all_target: false, ..base };
    assert_eq!(recovery_decision(partly), RollbackCompanions);
    assert_eq!(recovery_decision(all_there), RollbackCompanions);
    assert_eq!(recovery_decision(invalid), Manual, "a lost or foreign companion needs a human");
    assert_eq!(recovery_decision(RecoveryFacts { companions_any: true, ..base }), Discard);
    let moved = RecoveryFacts { source_matches: false, source_absent: true, target_matches: true, ..all_there };
    assert_eq!(recovery_decision(moved), Finish);
    assert_eq!(recovery_decision(RecoveryFacts { companions_all_target: false, companions_mixed: true, ..moved }), Manual);
    assert_eq!(recovery_decision(RecoveryFacts { companions_db_before: false, ..moved }), Manual);
    let committed = RecoveryFacts { db_before: false, db_after: true, ..moved };
    assert_eq!(recovery_decision(committed), Committed);
    assert_eq!(recovery_decision(RecoveryFacts { companions_db_after: false, ..committed }), Manual);
    assert_eq!(recovery_decision(RecoveryFacts { companions_all_target: false, ..committed }), Manual);
    let temp = RecoveryFacts { source_matches: false, source_absent: true, temporary_matches: true, ..all_there };
    assert_eq!(recovery_decision(temp), FinishTemporary);
    assert_eq!(recovery_decision(RecoveryFacts { companions_all_target: false, ..temp }), Manual);
    assert_eq!(recovery_decision(RecoveryFacts { db_before: false, ..temp }), Manual, "never move only the model back");
}

fn run_at(name: &'static str, k: usize, f: impl FnOnce() + 'static) {
    POINT.with(|c| *c.borrow_mut() = Some((name, k, PointAction::Run(Box::new(f)))));
}

#[test]
fn a_failed_model_rename_puts_the_companions_back() {
    for op in [Op::Rename, Op::Move] {
        let mut f = standard();
        let before = f.snapshot();
        // The next rename after the last companion is the model's own.
        run_at("after_companion", 1, || {
            crate::upload_fs::FSOPS_RENAME_ERROR.with(|h| h.set(Some(libc::EEXIST)))
        });
        assert_eq!(run(&mut f, op), Err(Error::Conflict), "{op:?}");
        assert_eq!(f.snapshot(), before, "{op:?}");
        assert!(f.ops.journal.is_empty() && !f.ops.journal.is_blocked(), "{op:?}");
    }
}

#[test]
fn a_foreign_file_at_a_source_blocks_the_rollback_and_nothing_is_deleted() {
    let mut f = standard();
    let (source, late) = (f.p("Foo_extras/pic.png"), f.p("sub/Foo_extras/zzz.png"));
    run_at("after_companion", 0, move || {
        fs::write(&source, b"foreign at source").unwrap();
        fs::write(&late, b"late").unwrap();
    });
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::RestartRequired));
    assert!(f.ops.journal.is_blocked());
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"foreign at source");
    assert_eq!(fs::read(f.p("sub/Foo_extras/pic.png")).unwrap(), PNG, "our moved file is not touched");
    assert_eq!(fs::read(f.p("sub/Foo_extras/zzz.png")).unwrap(), b"late");
    assert!(f.p("Foo.3mf").exists());
}

#[test]
fn a_row_with_the_destination_path_inserted_before_the_commit_is_a_conflict() {
    let mut f = standard();
    f.model(2, "Other.3mf");
    let db_path = f.db_path.clone();
    let target = f.p("Bar_extras/pic.png");
    run_at("before_commit", 0, move || {
        let other = crate::db::connect(&db_path).unwrap();
        crate::db::companions::insert_companion(&other, 2, "pic.png", target.to_str().unwrap(), "image", None, "t").unwrap();
    });
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::CompanionsConflict));
    assert!(f.p("Foo.3mf").exists() && !f.p("Bar.3mf").exists());
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    assert!(!f.p("Bar_extras").exists());
    assert_eq!(f.cpath(1), f.p("Foo_extras/pic.png").to_str().unwrap());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn the_folder_is_checked_by_identity_before_each_file() {
    let mut f = standard();
    let (old, aside) = (f.p("Foo_extras"), f.p("aside"));
    // Between the folder check and the file checks the folder is replaced by another one.
    run_at("plan_files", 0, move || {
        fs::rename(&old, &aside).unwrap();
        fs::create_dir(&old).unwrap();
        fs::copy(aside.join("pic.png"), old.join("pic.png")).unwrap();
        fs::copy(aside.join("zzz.png"), old.join("zzz.png")).unwrap();
    });
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::HasCompanions));
    assert!(f.p("Foo.3mf").exists() && !f.p("Bar_extras").exists());
}

#[test]
fn a_folder_swapped_for_another_one_is_not_removed_by_the_rollback() {
    let mut f = standard();
    let (made, source) = (f.p("sub/Foo_extras"), f.p("Foo_extras/pic.png"));
    run_at("after_mkdir", 0, move || {
        // A different empty folder takes the place of the one this move created, and the
        // first file changes so that the move fails.
        fs::remove_dir(&made).unwrap();
        fs::create_dir(&made).unwrap();
        fs::write(&source, b"changed size").unwrap();
    });
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::Changed));
    assert!(f.p("sub/Foo_extras").is_dir(), "an inode that is not ours stays");
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"changed size");
    assert!(f.ops.journal.is_empty());
}

#[test]
fn an_unchanged_source_with_nothing_moved_is_discarded_by_recovery() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::RestartRequired));
    fs::write(f.p("Foo_extras/pic.png"), b"edited in place").unwrap();
    assert!(f.reopen_and_recover().manual.is_empty());
    assert!(f.p("Foo.3mf").exists());
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), b"edited in place");
}

#[test]
fn a_loose_file_at_the_name_of_a_missing_companion_is_a_conflict() {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    let gone = f.row_for(1, &f.p("Foo_extras/gone.png"));
    fs::create_dir(f.p("sub/Foo_extras")).unwrap();
    fs::write(f.p("sub/Foo_extras/gone.png"), b"not registered").unwrap();
    let before = f.snapshot();
    assert_eq!(f.ops.move_file_to_folder(1, Some(1)), Err(Error::CompanionsConflict));
    assert_eq!(f.snapshot(), before);
    assert_eq!(f.cpath(gone), f.p("Foo_extras/gone.png").to_str().unwrap());
}
