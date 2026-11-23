use super::tests::{PointAction, POINT};
use super::*;
use std::{fs, os::unix::fs::symlink, path::PathBuf};

struct Ledger;
impl UploadLedger for Ledger {
    fn reset_imported(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
    fn detach(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
}
struct Fixture {
    dir: PathBuf,
    ops: FileOps<rusqlite::Connection, Ledger>,
    companion: i64,
}
impl Fixture {
    fn new() -> Self {
        let dir = crate::unique_test_dir("desktop_purge");
        for name in ["models", "db", "db/trash", "auth"] {
            fs::create_dir(dir.join(name)).unwrap();
        }
        fs::write(dir.join("auth/web-auth.json"), b"control").unwrap();
        let db = crate::db::connect(&dir.join("db/catalog.db")).unwrap();
        let model = dir.join("models/a.stl").to_string_lossy().into_owned();
        let trash = dir.join("db/trash/1-a.stl").to_string_lossy().into_owned();
        fs::write(&trash, b"model").unwrap();
        db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at,deleted_at,trash_path) VALUES(1,'a.stl',?1,'stl',5,'2026-01-01T00:00:00Z','2026-01-02T00:00:00Z',?2)", rusqlite::params![model,trash]).unwrap();
        let companion = crate::db::companions::insert_companion(
            &db,
            1,
            "pic.png",
            dir.join("models/a_extras/pic.png").to_str().unwrap(),
            "image",
            Some(3),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let ct = dir.join(format!("db/trash/companion-{companion}-pic.png"));
        fs::write(&ct, b"pic").unwrap();
        db.execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z',trash_path=?1 WHERE id=?2", rusqlite::params![ct.to_str().unwrap(),companion]).unwrap();
        let mut context = FsContext::new(
            &[dir.join("models")],
            &[
                (dir.join("db"), Protection::Forbidden),
                (dir.join("auth"), Protection::Forbidden),
            ],
        )
        .unwrap();
        context.desktop_trash = DesktopTrashPurge::open(&dir.join("db")).unwrap();
        let journal = Journal::open(&dir.join("auth")).unwrap();
        Self {
            dir,
            ops: FileOps::new(context, journal, db, Ledger),
            companion,
        }
    }
    fn model(&self) -> PathBuf {
        self.dir.join("db/trash/1-a.stl")
    }
    fn companion(&self) -> PathBuf {
        self.dir
            .join(format!("db/trash/companion-{}-pic.png", self.companion))
    }
    fn assert_gone(&mut self) {
        assert!(!self.model().exists());
        assert!(!self.companion().exists());
        assert_eq!(self.ops.state(1), Err(Error::NotFound));
        assert_eq!(
            self.ops
                .db
                .query_row("SELECT count(*) FROM companion_files", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(self.dir.join("db/catalog.db").is_file());
        assert_eq!(
            fs::read(self.dir.join("auth/web-auth.json")).unwrap(),
            b"control"
        );
    }
    fn recover(&mut self) {
        self.ops.journal = Journal::open(&self.dir.join("auth")).unwrap();
        assert!(self.ops.recover().unwrap().manual.is_empty());
        self.ops.finish_recovery().unwrap();
        assert!(self.ops.journal.is_empty());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn desktop_files_are_counted_and_actually_unlinked() {
    let mut f = Fixture::new();
    let summary = f.ops.empty_trash_summary().unwrap();
    assert_eq!(summary.count, 1);
    assert_eq!(summary.companion_count, 1);
    assert_eq!(summary.blocked_by_companions, 0);
    assert!(is_desktop_trash(
        &f.ops.db,
        f.model().to_str().unwrap(),
        1,
        false
    ));
    assert_eq!(f.ops.restore_file(1), Err(Error::TrashForeign));
    assert!(!f.ops.delete_file_permanently(1).unwrap().kept_on_disk);
    f.assert_gone();
}

#[test]
fn desktop_purge_recovers_before_and_after_every_unlink() {
    for point in ["before_unlink", "after_unlink"] {
        for index in 0..2 {
            let mut f = Fixture::new();
            POINT.with(|p| *p.borrow_mut() = Some((point, index, PointAction::Crash)));
            assert_eq!(
                f.ops.delete_file_permanently(1),
                Err(Error::RestartRequired)
            );
            assert!(f.ops.state(1).is_ok());
            let doc: serde_json::Value =
                serde_json::from_slice(&fs::read(f.dir.join("auth/web-fsops.json")).unwrap())
                    .unwrap();
            assert_eq!(doc["version"], 3);
            assert_eq!(doc["intents"][0]["operation"], "purge_desktop_trash");
            f.recover();
            f.assert_gone();
            f.recover();
        }
    }
    for phase in [Phase::Prepared, Phase::Unlinked, Phase::Committed] {
        let mut f = Fixture::new();
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(
            f.ops.delete_file_permanently(1),
            Err(Error::RestartRequired)
        );
        f.recover();
        f.assert_gone();
    }
}

#[test]
fn links_and_directories_are_refused_before_any_unlink() {
    for companion in [false, true] {
        for kind in ["symlink", "hardlink", "directory"] {
            let mut f = Fixture::new();
            let victim = if companion { f.companion() } else { f.model() };
            fs::remove_file(&victim).unwrap();
            let control = f.dir.join("db/control");
            fs::write(&control, b"control").unwrap();
            match kind {
                "symlink" => symlink(&control, &victim).unwrap(),
                "hardlink" => fs::hard_link(&control, &victim).unwrap(),
                _ => fs::create_dir(&victim).unwrap(),
            }
            assert!(f.ops.delete_file_permanently(1).is_err());
            assert!(f.ops.state(1).is_ok());
            assert!(f.model().exists() && f.companion().exists());
            assert_eq!(fs::read(control).unwrap(), b"control");
            assert!(f.ops.journal.is_empty());
        }
    }
}

#[test]
fn capability_never_grants_move_restore_or_parent_access() {
    let f = Fixture::new();
    for path in [
        f.model(),
        f.dir.join("db/catalog.db"),
        f.dir.join("auth/web-auth.json"),
    ] {
        for access in [
            Access::Source,
            Access::MoveTarget,
            Access::RestoreTarget,
            Access::Trash,
        ] {
            assert!(f
                .ops
                .context
                .resolve(path.to_str().unwrap(), access)
                .is_err());
        }
    }
    for name in ["1-", "2-a.stl", "companion-1-a.stl", "a.stl", "1-.."] {
        assert!(!is_desktop_trash(
            &f.ops.db,
            f.dir.join("db/trash").join(name).to_str().unwrap(),
            1,
            false
        ));
    }
    for path in [f.dir.join("db/1-a.stl"), f.dir.join("db/trash/sub/1-a.stl")] {
        assert!(!is_desktop_trash(
            &f.ops.db,
            path.to_str().unwrap(),
            1,
            false
        ));
    }
}

#[test]
fn active_companion_blocks_and_missing_files_recover() {
    let mut f = Fixture::new();
    f.ops
        .db
        .execute("UPDATE companion_files SET deleted_at=NULL", [])
        .unwrap();
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
    assert!(f.model().exists() && f.companion().exists());
    f.ops
        .db
        .execute(
            "UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z'",
            [],
        )
        .unwrap();
    fs::remove_file(f.model()).unwrap();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(
        f.ops.delete_file_permanently(1),
        Err(Error::RestartRequired)
    );
    f.recover();
    f.assert_gone();
}

#[test]
fn loose_desktop_companion_is_counted_and_purged_with_recovery() {
    for phase in [Phase::Prepared, Phase::Unlinked, Phase::Committed] {
        let mut f = Fixture::new();
        f.ops
            .db
            .execute("UPDATE files SET deleted_at=NULL,trash_path=NULL", [])
            .unwrap();
        assert_eq!(
            f.ops.empty_trash_summary().unwrap().loose_companion_count,
            1
        );
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(
            f.ops.purge_loose_companion(f.companion, None),
            Err(Error::RestartRequired)
        );
        f.recover();
        assert!(!f.companion().exists());
        assert!(f.ops.state(1).is_ok());
        assert!(f.model().exists());
    }
}

#[test]
fn recovery_refuses_replacement_inode_and_changed_row() {
    for change_row in [false, true] {
        let mut f = Fixture::new();
        f.ops.journal.crash_at = Some(Phase::Prepared);
        assert_eq!(
            f.ops.delete_file_permanently(1),
            Err(Error::RestartRequired)
        );
        if change_row {
            f.ops
                .db
                .execute("UPDATE files SET deleted_at=NULL", [])
                .unwrap();
        } else {
            fs::rename(f.model(), f.dir.join("db/old")).unwrap();
            fs::write(f.model(), b"replacement").unwrap();
        }
        f.ops.journal = Journal::open(&f.dir.join("auth")).unwrap();
        assert_eq!(f.ops.recover().unwrap().manual.len(), 1);
        assert!(f.model().exists() && f.companion().exists());
        assert!(f.ops.state(1).is_ok());
    }
}

#[test]
fn empty_trash_removes_desktop_files_and_rows() {
    let mut f = Fixture::new();
    let summary = f.ops.empty_trash_summary().unwrap();
    let mut snapshot = f
        .ops
        .empty_trash_snapshot(summary.count, summary.max_deleted_at.as_deref())
        .unwrap();
    let results = f.ops.empty_trash(&mut snapshot, || true).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].ok);
    assert_eq!(results[0].kept_on_disk, Some(false));
    f.assert_gone();
}

#[test]
fn desktop_root_symlink_is_not_a_capability() {
    let dir = crate::unique_test_dir("desktop_symlink");
    fs::create_dir(dir.join("db")).unwrap();
    fs::create_dir(dir.join("outside")).unwrap();
    symlink(dir.join("outside"), dir.join("db/trash")).unwrap();
    assert!(DesktopTrashPurge::open(&dir.join("db")).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn wrong_companion_names_and_parent_paths_leave_every_file_untouched() {
    for relative in [
        "db/trash/companion-999-pic.png",
        "db/trash/pic.png",
        "db/companion-1-pic.png",
        "db/trash/sub/companion-1-pic.png",
    ] {
        let mut f = Fixture::new();
        let wrong = f.dir.join(relative);
        fs::create_dir_all(wrong.parent().unwrap()).unwrap();
        fs::rename(f.companion(), &wrong).unwrap();
        f.ops
            .db
            .execute(
                "UPDATE companion_files SET trash_path=?1",
                [wrong.to_str().unwrap()],
            )
            .unwrap();
        assert!(f.ops.delete_file_permanently(1).is_err());
        assert!(f.model().exists() && wrong.exists());
        assert!(f.ops.state(1).is_ok());
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn desktop_purge_does_not_require_the_original_model_root() {
    for loose in [false, true] {
        let mut f = Fixture::new();
        f.ops
            .db
            .execute("UPDATE files SET path='/unavailable/a.stl'", [])
            .unwrap();
        if loose {
            f.ops
                .db
                .execute("UPDATE files SET deleted_at=NULL,trash_path=NULL", [])
                .unwrap();
            assert_eq!(
                f.ops.empty_trash_summary().unwrap().loose_companion_count,
                1
            );
            f.ops.purge_loose_companion(f.companion, None).unwrap();
            assert!(!f.companion().exists());
        } else {
            f.ops.delete_file_permanently(1).unwrap();
            f.assert_gone();
        }
    }
}
