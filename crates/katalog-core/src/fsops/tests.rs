use super::*;
use std::{fs, os::unix::fs::symlink};
#[test]
fn primitives_never_replace_and_rmdir_only_empty() {
    let p = crate::unique_test_dir("fsops_primitives");
    let d = Directory::open(&p).unwrap();
    d.mkdir("sub", 0o700).unwrap();
    let sub = d.open_dir("sub").unwrap();
    fs::write(p.join("a"), b"a").unwrap();
    fs::write(p.join("sub/b"), b"b").unwrap();
    assert_eq!(
        d.rename_noreplace("a", &sub, "b")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EEXIST)
    );
    assert!(d.rmdir("sub").is_err());
    d.rename_noreplace("a", &sub, "c").unwrap();
    assert_eq!(fs::read(p.join("sub/c")).unwrap(), b"a");
    sub.unlink("b").unwrap();
    sub.unlink("c").unwrap();
    d.rmdir("sub").unwrap();
    symlink("/tmp", p.join("link")).unwrap();
    assert!(d.open_dir("link").is_err());
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn resolver_rejects_lexical_escapes_links_and_protected_descendants() {
    let p = crate::unique_test_dir("fsops_resolve");
    fs::create_dir(p.join("models")).unwrap();
    fs::create_dir(p.join("models/upload")).unwrap();
    fs::create_dir(p.join("models/secret")).unwrap();
    let root = p.join("models");
    let ctx = FsContext::new(
        std::slice::from_ref(&root),
        &[(root.join("upload"), Protection::Upload)],
    )
    .unwrap();
    assert!(FsContext::new(
        std::slice::from_ref(&root),
        &[(root.join("secret"), Protection::Forbidden)]
    )
    .is_err());
    for suffix in ["/../x", "//x", "/./x", "/x\\y"] {
        assert!(
            ctx.resolve(&format!("{}{suffix}", root.display()), Access::Source)
                .is_err(),
            "{suffix}"
        );
    }
    assert!(ctx.resolve(root.to_str().unwrap(), Access::Source).is_err());
    symlink("/tmp", root.join("link")).unwrap();
    assert!(ctx
        .resolve(root.join("link/x").to_str().unwrap(), Access::Source)
        .is_err());
    assert!(ctx
        .resolve(root.join("upload/x").to_str().unwrap(), Access::Source)
        .is_ok());
    assert!(ctx
        .resolve(root.join("upload/x").to_str().unwrap(), Access::MoveTarget)
        .is_err());
    assert!(ctx
        .resolve(
            root.join("upload/x").to_str().unwrap(),
            Access::RestoreTarget
        )
        .is_ok());
    assert!(ctx
        .resolve(root.join("x").to_str().unwrap(), Access::MoveTarget)
        .is_ok());
    fs::remove_dir_all(p).unwrap();
}
#[test]
fn trash_is_private_and_names_preserve_extensions() {
    use std::os::unix::fs::PermissionsExt;
    let p = crate::unique_test_dir("fsops_trash");
    let d = Directory::open(&p).unwrap();
    let trash = ensure_trash_dir(&d).unwrap();
    assert_eq!(
        trash.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        rename_name("Old.gcode.3mf", "New").unwrap(),
        "New.gcode.3mf"
    );
    assert_eq!(rename_name("a.STL", "e\u{301}").unwrap(), "é.STL");
    for n in ["", ".hidden", "a/b", "a\\b", "NUL", "x.", "x ", "a\u{202e}"] {
        assert!(validate_name(n).is_err(), "{n}");
    }
    assert_eq!(validate_name("%2f").unwrap(), "%2f");
    let n = trash_name(
        123,
        "abcdef0123456789",
        &format!("{}.gcode.3mf", "ü".repeat(200)),
    )
    .unwrap();
    assert!(n.len() <= 255 && n.starts_with("123-abcdef01-") && n.ends_with(".gcode.3mf"));
    fs::set_permissions(p.join(TRASH), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ensure_trash_dir(&d).is_err());
    fs::remove_dir_all(p).unwrap();
}

struct Fixture {
    dir: std::path::PathBuf,
    ops: FileOps<rusqlite::Connection, MemoryLedger>,
}
#[derive(Default)]
struct MemoryLedger(Vec<i64>);
impl UploadLedger for MemoryLedger {
    fn detach(&mut self, ids: &[i64]) -> Result<()> {
        self.0.extend_from_slice(ids);
        Ok(())
    }
}
impl Fixture {
    fn new() -> Self {
        let dir = crate::unique_test_dir("fsops_ops");
        fs::create_dir(dir.join("root")).unwrap();
        fs::create_dir(dir.join("journal")).unwrap();
        let context = FsContext::new(&[dir.join("root")], &[]).unwrap();
        let journal = Journal::open(&dir.join("journal")).unwrap();
        let db = crate::db::connect_in_memory().unwrap();
        Self {
            dir,
            ops: FileOps::new(context, journal, db, MemoryLedger::default()),
        }
    }
    fn add(&mut self, id: i64, name: &str) -> String {
        let path = self
            .dir
            .join("root")
            .join(name)
            .to_str()
            .unwrap()
            .to_owned();
        fs::write(&path, b"model").unwrap();
        self.ops.db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (?1,?2,?3,'stl',5,'2026-01-01T00:00:00Z')", rusqlite::params![id,name,path]).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn file_operations_roundtrip_and_detach() {
    let mut f = Fixture::new();
    let old = f.add(1, "a.stl");
    f.ops.rename_file(1, "B").unwrap();
    assert!(!std::path::Path::new(&old).exists());
    let renamed = f.ops.state(1).unwrap();
    assert!(renamed.path.ends_with("/B.stl"));
    f.ops.delete_file(1).unwrap();
    let trashed = f.ops.state(1).unwrap();
    assert!(trashed.deleted_at.is_some());
    assert!(std::path::Path::new(trashed.trash_path.as_ref().unwrap()).exists());
    f.ops.restore_file(1).unwrap();
    assert_eq!(f.ops.state(1).unwrap(), renamed);
    f.ops.delete_file(1).unwrap();
    assert!(!f.ops.delete_file_permanently(1).unwrap().kept_on_disk);
    assert_eq!(f.ops.state(1), Err(Error::NotFound));
    assert!(f.ops.journal.is_empty());
    assert_eq!(f.ops.ledger.0, vec![1; 5]);
}
#[test]
fn foreign_trash_is_never_unlinked_and_restore_conflicts() {
    let mut f = Fixture::new();
    let old = f.add(1, "a.stl");
    f.ops.delete_file(1).unwrap();
    fs::write(&old, b"control").unwrap();
    assert_eq!(f.ops.restore_file(1), Err(Error::Conflict));
    assert_eq!(fs::read(&old).unwrap(), b"control");
    f.ops
        .db
        .execute("UPDATE files SET trash_path=?1 WHERE id=1", [&old])
        .unwrap();
    assert_eq!(f.ops.restore_file(1), Err(Error::TrashForeign));
    assert!(f.ops.delete_file_permanently(1).unwrap().kept_on_disk);
    assert_eq!(fs::read(old).unwrap(), b"control");
}
#[test]
fn recovery_finishes_rename_after_crash() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::RestartRequired));
    let journal = Journal::open(&f.dir.join("journal")).unwrap();
    f.ops.journal = journal;
    let report = f.ops.recover().unwrap();
    assert!(report.manual.is_empty());
    assert_eq!(report.detach_ids, vec![1]);
    f.ops.finish_recovery().unwrap();
    assert!(f.ops.state(1).unwrap().path.ends_with("/b.stl"));
    assert!(f.ops.journal.is_empty());
}

#[test]
fn recovery_every_durable_phase_and_unlink() {
    for phase in [
        Phase::Prepared,
        Phase::TmpRenamed,
        Phase::Renamed,
        Phase::Committed,
    ] {
        let mut f = Fixture::new();
        let old = f.add(1, "a.stl");
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(f.ops.rename_file(1, "A"), Err(Error::RestartRequired));
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        let report = f.ops.recover().unwrap();
        assert!(report.manual.is_empty(), "{phase:?}");
        f.ops.finish_recovery().unwrap();
        let expected = if phase == Phase::Prepared {
            old
        } else {
            f.dir.join("root/A.stl").to_str().unwrap().to_owned()
        };
        assert_eq!(f.ops.state(1).unwrap().path, expected);
        assert!(std::path::Path::new(&expected).exists());
    }
    for phase in [Phase::Prepared, Phase::Unlinked, Phase::Committed] {
        let mut f = Fixture::new();
        f.add(1, "a.stl");
        f.ops.delete_file(1).unwrap();
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(
            f.ops.delete_file_permanently(1),
            Err(Error::RestartRequired)
        );
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        assert!(f.ops.recover().unwrap().manual.is_empty());
        f.ops.finish_recovery().unwrap();
        assert_eq!(f.ops.state(1).is_ok(), phase == Phase::Prepared);
    }
}
#[test]
fn recovery_source_intact_discards_even_foreign_directory_target() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::RestartRequired));
    fs::create_dir(f.dir.join("root/b.stl")).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn recovery_identity_change_blocks_persistently_until_ack() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::RestartRequired));
    fs::write(f.dir.join("root/b.stl"), b"changed size").unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    let report = f.ops.recover().unwrap();
    assert_eq!(report.manual.len(), 1);
    assert!(f.ops.journal.is_blocked());
    assert_eq!(f.ops.rename_file(1, "c"), Err(Error::RestartRequired));
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert_eq!(f.ops.recover().unwrap().manual, report.manual);
    f.ops.journal.acknowledge(&report.manual).unwrap();
    assert!(!f.ops.journal.is_blocked());
}
#[test]
fn missing_sources_db_only_and_restore_missing_parent() {
    let mut f = Fixture::new();
    let p = f.add(1, "a.stl");
    fs::remove_file(&p).unwrap();
    f.ops.delete_file(1).unwrap();
    assert!(f.ops.state(1).unwrap().trash_path.is_none());
    f.ops.restore_file(1).unwrap();
    assert!(!std::path::Path::new(&p).exists());
    fs::create_dir(f.dir.join("root/sub")).unwrap();
    f.add(2, "sub/b.stl");
    f.ops.delete_file(2).unwrap();
    fs::remove_dir(f.dir.join("root/sub")).unwrap();
    assert_eq!(f.ops.restore_file(2), Err(Error::PathUnresolvable));
    let t = f.ops.state(2).unwrap().trash_path.unwrap();
    fs::remove_file(t).unwrap();
    assert_eq!(f.ops.restore_file(2), Err(Error::TrashMissing));
    assert!(!f.ops.delete_file_permanently(2).unwrap().kept_on_disk);
}
#[test]
fn batches_snapshot_timestamp_comparison_and_control_file() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.add(2, "b.stl");
    let results = f.ops.delete_files(&[1, 1, 999, 2], || true).unwrap();
    assert_eq!(results.len(), 3);
    assert!(results[0].ok && !results[1].ok && results[2].ok);
    f.ops.db.execute("UPDATE files SET deleted_at=CASE id WHEN 1 THEN '2026-01-01T00:00:00+00:00' ELSE '2026-01-01T01:00:00+01:00' END",[]).unwrap();
    assert!(matches!(
        f.ops.empty_trash_snapshot(1, Some("2026-01-01T00:00:00Z")),
        Err(Error::Conflict)
    ));
    let mut snapshot = f
        .ops
        .empty_trash_snapshot(2, Some("2026-01-01T00:00:00Z"))
        .unwrap();
    let control = f.dir.join("root").join(TRASH).join("orphan.stl");
    fs::write(&control, b"control").unwrap();
    f.add(3, "c.stl");
    f.ops.delete_file(3).unwrap();
    assert!(f
        .ops
        .empty_trash(&mut snapshot, || true)
        .unwrap()
        .iter()
        .all(|r| r.ok));
    assert_eq!(snapshot.remaining(), 0);
    assert!(f.ops.state(3).unwrap().deleted_at.is_some());
    assert_eq!(fs::read(control).unwrap(), b"control");
    assert!(f.ops.delete_files(&vec![3; 201], || true).is_err());
}
#[test]
fn move_preserves_identity_timestamps_and_null_is_db_only() {
    use std::os::unix::fs::MetadataExt;
    let mut f = Fixture::new();
    let old = f.add(1, "a.stl");
    let identity = fs::metadata(&old).unwrap().ino();
    let target = f.dir.join("root/sub");
    fs::create_dir(&target).unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path) VALUES (1,'sub',?1)",
            [target.to_str().unwrap()],
        )
        .unwrap();
    assert!(f.ops.move_files_to_folder(&[1], Some(1), || true).unwrap()[0].ok);
    assert_eq!(fs::metadata(target.join("a.stl")).unwrap().ino(), identity);
    f.ops.move_file_to_folder(1, None).unwrap();
    assert!(target.join("a.stl").exists());
    assert_eq!(f.ops.state(1).unwrap().folder_id, None);
    let imported: String = f
        .ops
        .db
        .query_row("SELECT imported_at FROM files WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(imported, "2026-01-01T00:00:00Z");
}
#[test]
fn hardlinks_directories_and_symlinks_are_rejected() {
    let mut f = Fixture::new();
    let p = f.add(1, "a.stl");
    fs::hard_link(&p, f.dir.join("control")).unwrap();
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::PathUnresolvable));
    fs::remove_file(&p).unwrap();
    fs::create_dir(&p).unwrap();
    assert_eq!(f.ops.delete_file(1), Err(Error::PathUnresolvable));
    fs::remove_dir(&p).unwrap();
    symlink(f.dir.join("control"), &p).unwrap();
    assert_eq!(f.ops.delete_file(1), Err(Error::PathUnresolvable));
    assert_eq!(fs::read(f.dir.join("control")).unwrap(), b"model");
}

#[test]
fn trash_name_uses_the_intent_operation_id() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(f.ops.delete_file(1), Err(Error::RestartRequired));
    let i = &f.ops.journal.intents[0];
    assert!(i
        .target
        .as_ref()
        .unwrap()
        .contains(&format!("1-{}-", &i.op_id[..8])));
}
#[test]
fn upload_rename_case_conflict_move_in_and_restore_matrix() {
    let mut f = Fixture::new();
    let upload = f.dir.join("root/upload");
    fs::create_dir(&upload).unwrap();
    f.ops.context = FsContext::new(
        &[f.dir.join("root")],
        &[(upload.clone(), Protection::Upload)],
    )
    .unwrap();
    f.add(1, "upload/a.stl");
    f.add(2, "upload/b.stl");
    f.add(3, "c.stl");
    f.ops.rename_file(1, "A").unwrap();
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::Conflict));
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path) VALUES (1,'upload',?1)",
            [upload.to_str().unwrap()],
        )
        .unwrap();
    assert_eq!(f.ops.move_file_to_folder(3, Some(1)), Err(Error::Protected));
    f.ops.delete_file(1).unwrap();
    f.ops.restore_file(1).unwrap();
    assert!(upload.join("A.stl").exists());
    assert!(f.ops.ledger.0.contains(&1));
}
#[test]
fn recovery_table_has_a_test_for_each_decision() {
    let base = RecoveryFacts {
        trusted: true,
        db_before: true,
        db_after: false,
        source_matches: false,
        source_absent: true,
        target_matches: false,
        temporary_matches: false,
        unlink: false,
        db_only: false,
    };
    let rows = [
        (RecoveryDecision::Manual, base),
        (
            RecoveryDecision::Committed,
            RecoveryFacts {
                db_before: false,
                db_after: true,
                target_matches: true,
                ..base
            },
        ),
        (
            RecoveryDecision::Discard,
            RecoveryFacts {
                source_matches: true,
                source_absent: false,
                ..base
            },
        ),
        (
            RecoveryDecision::Finish,
            RecoveryFacts {
                target_matches: true,
                ..base
            },
        ),
        (
            RecoveryDecision::FinishTemporary,
            RecoveryFacts {
                temporary_matches: true,
                ..base
            },
        ),
        (
            RecoveryDecision::RollbackTemporary,
            RecoveryFacts {
                db_before: false,
                temporary_matches: true,
                ..base
            },
        ),
        (
            RecoveryDecision::FinishUnlink,
            RecoveryFacts {
                unlink: true,
                ..base
            },
        ),
    ];
    for ((decision, _), (expected, facts)) in RECOVERY_TABLE.iter().zip(rows) {
        assert_eq!(*decision, expected);
        assert_eq!(recovery_decision(facts), expected);
    }
    assert_eq!(
        recovery_decision(RecoveryFacts {
            trusted: false,
            target_matches: true,
            ..base
        }),
        RecoveryDecision::Manual
    );
}
#[test]
fn journal_corruption_read_only_startup_and_sync_failure() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.fail_sync = true;
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::RestartRequired));
    assert!(f.dir.join("root/a.stl").exists());
    assert_eq!(
        f.ops.journal.check_startup(false),
        Err(Error::RestartRequired)
    );
    assert_eq!(f.ops.journal.check_startup(true), Ok(()));
    fs::write(f.dir.join("journal/web-fsops.json"), b"{broken").unwrap();
    assert!(matches!(
        Journal::open(&f.dir.join("journal")),
        Err(Error::RestartRequired)
    ));
}
#[test]
fn root_identity_change_recovery_blocks_without_touching_files() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Renamed);
    let _ = f.ops.rename_file(1, "b");
    fs::rename(f.dir.join("root"), f.dir.join("old-root")).unwrap();
    fs::create_dir(f.dir.join("root")).unwrap();
    fs::write(f.dir.join("root/b.stl"), b"control").unwrap();
    f.ops.context = FsContext::new(&[f.dir.join("root")], &[]).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert_eq!(f.ops.recover().unwrap().manual.len(), 1);
    assert_eq!(fs::read(f.dir.join("root/b.stl")).unwrap(), b"control");
    assert!(f.dir.join("old-root/b.stl").exists());
}
#[test]
fn tmp_recovery_rolls_back_when_db_was_changed() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::TmpRenamed);
    let _ = f.ops.rename_file(1, "A");
    f.ops
        .db
        .execute("UPDATE files SET name='external' WHERE id=1", [])
        .unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    assert!(f.ops.journal.is_empty());
    assert!(f.dir.join("root/a.stl").exists());
    assert_eq!(f.ops.state(1).unwrap().name, "external");
}
#[test]
fn db_path_collision_including_trashed_row_precedes_filesystem_work() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    let b = f.add(2, "b.stl");
    f.ops.delete_file(2).unwrap();
    assert!(!std::path::Path::new(&b).exists());
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::Conflict));
    assert!(f.dir.join("root/a.stl").exists());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn session_recheck_stops_remaining_ids_without_changes() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.add(2, "b.stl");
    let mut n = 0;
    let result = f
        .ops
        .delete_files(&[1, 2], || {
            n += 1;
            n == 1
        })
        .unwrap();
    assert!(result[0].ok);
    assert_eq!(result[1].kind, Some(Error::NotStarted));
    assert!(f.dir.join("root/b.stl").exists());
}

#[test]
fn rename_syscall_errors_leave_original_untouched() {
    for (errno, expected) in [
        (libc::EXDEV, Error::CrossDevice),
        (libc::EBUSY, Error::Busy),
        (libc::EINVAL, Error::Unsupported),
        (libc::ENOSYS, Error::Unsupported),
    ] {
        let mut f = Fixture::new();
        let p = f.add(1, "a.stl");
        crate::upload_fs::FSOPS_RENAME_ERROR.with(|h| h.set(Some(errno)));
        assert_eq!(f.ops.rename_file(1, "b"), Err(expected));
        assert_eq!(fs::read(p).unwrap(), b"model");
        assert!(f.ops.journal.is_empty());
    }
}
#[test]
fn same_device_file_bind_mount_is_rejected() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    crate::upload_fs::FSOPS_ENTRY_MOUNT.with(|h| h.set(Some(u64::MAX)));
    let result = f.ops.rename_file(1, "b");
    crate::upload_fs::FSOPS_ENTRY_MOUNT.with(|h| h.set(None));
    assert_eq!(result, Err(Error::CrossDevice));
    assert!(f.dir.join("root/a.stl").exists());
}

#[test]
fn names_reject_reserved_stems_with_spaces_and_leading_space() {
    for n in [" CON", "CON .a", "LPT1 .a"] {
        assert_eq!(validate_name(n), Err(Error::InvalidName));
    }
    assert_eq!(validate_name(&"a".repeat(256)), Err(Error::InvalidName));
}

#[test]
fn post_rename_stat_error_keeps_intent_and_blocks() {
    for errno in [libc::EIO, libc::ENOENT] {
        let mut f = Fixture::new();
        f.add(1, "a.stl");
        crate::upload_fs::FSOPS_AFTER_RENAME_ERROR.with(|h| h.set(Some(errno)));
        assert_eq!(f.ops.rename_file(1, "b"), Err(Error::RestartRequired));
        assert!(!f.ops.journal.is_empty());
        assert!(f.ops.journal.is_blocked());
    }
}
struct ChangingDb {
    c: rusqlite::Connection,
    calls: usize,
}
impl FsDatabase for ChangingDb {
    fn with_conn<T>(
        &mut self,
        f: impl FnOnce(&mut rusqlite::Connection) -> Result<T>,
    ) -> Result<T> {
        self.calls += 1;
        if self.calls == 3 {
            self.c
                .execute("UPDATE files SET name='external' WHERE id=1", [])
                .unwrap();
        }
        f(&mut self.c)
    }
}
#[test]
fn case_only_db_conflict_rolls_back_via_temporary() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    let c = std::mem::replace(&mut f.ops.db, crate::db::connect_in_memory().unwrap());
    let mut ops = FileOps::new(
        FsContext::new(&[f.dir.join("root")], &[]).unwrap(),
        Journal::open(&f.dir.join("journal")).unwrap(),
        ChangingDb { c, calls: 0 },
        MemoryLedger::default(),
    );
    crate::upload_fs::FSOPS_CASE_INSENSITIVE.with(|h| h.set(true));
    let result = ops.rename_file(1, "A");
    crate::upload_fs::FSOPS_CASE_INSENSITIVE.with(|h| h.set(false));
    assert_eq!(result, Err(Error::Changed));
    assert!(f.dir.join("root/a.stl").exists());
    assert!(!f.dir.join("root/A.stl").exists());
    assert!(ops.journal.is_empty());
    assert_eq!(ops.state(1).unwrap().name, "external");
}

#[test]
fn recovery_handles_filesystem_step_before_phase_was_persisted() {
    for (phase, rename) in [
        (Phase::TmpRenamed, true),
        (Phase::Renamed, true),
        (Phase::Unlinked, false),
        (Phase::Committed, true),
    ] {
        let mut f = Fixture::new();
        f.add(1, "a.stl");
        if !rename {
            f.ops.delete_file(1).unwrap();
        }
        f.ops.journal.crash_at = Some(phase);
        if rename {
            let _ = f.ops.rename_file(1, "A");
        } else {
            let _ = f.ops.delete_file_permanently(1);
        }
        f.ops.journal.intents[0].phase = Phase::Prepared;
        f.ops.journal.persist().unwrap();
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        assert!(f.ops.recover().unwrap().manual.is_empty(), "{phase:?}");
        f.ops.finish_recovery().unwrap();
        if rename {
            assert!(f.ops.state(1).unwrap().path.ends_with("/A.stl"));
        } else {
            assert_eq!(f.ops.state(1), Err(Error::NotFound));
        }
    }
}
#[test]
fn cross_root_moves_and_foreign_other_root_trash() {
    let mut f = Fixture::new();
    let other = f.dir.join("other");
    fs::create_dir(&other).unwrap();
    f.ops.context = FsContext::new(&[f.dir.join("root"), other.clone()], &[]).unwrap();
    f.add(1, "a.stl");
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path) VALUES (1,'other',?1)",
            [other.to_str().unwrap()],
        )
        .unwrap();
    assert_eq!(
        f.ops.move_file_to_folder(1, Some(1)),
        Err(Error::CrossDevice)
    );
    f.ops.delete_file(1).unwrap();
    let control = other.join(TRASH).join("1-foreign.stl");
    fs::write(&control, b"control").unwrap();
    f.ops
        .db
        .execute(
            "UPDATE files SET trash_path=?1 WHERE id=1",
            [control.to_str().unwrap()],
        )
        .unwrap();
    assert_eq!(f.ops.restore_file(1), Err(Error::TrashForeign));
    assert!(f.ops.delete_file_permanently(1).unwrap().kept_on_disk);
    assert_eq!(fs::read(control).unwrap(), b"control");
}
#[test]
fn empty_trash_has_bounded_blocks_and_frozen_ids() {
    let mut f = Fixture::new();
    for id in 1..=205 {
        f.ops.db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at,deleted_at) VALUES (?1,'missing',?2,'stl',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",rusqlite::params![id,format!("/missing/{id}")]).unwrap();
    }
    let summary = f.ops.empty_trash_summary().unwrap();
    assert_eq!(summary.count, 205);
    assert_eq!(summary.bytes, 205);
    let mut snapshot = f
        .ops
        .empty_trash_snapshot(205, summary.max_deleted_at.as_deref())
        .unwrap();
    let first = f.ops.empty_trash(&mut snapshot, || true).unwrap();
    assert_eq!(first.len(), 200);
    assert!(first.iter().all(|r| r.ok));
    assert_eq!(snapshot.remaining(), 5);
    assert_eq!(f.ops.empty_trash(&mut snapshot, || true).unwrap().len(), 5);
    assert_eq!(f.ops.empty_trash_summary().unwrap().count, 0);
}
#[test]
fn symlink_at_each_parent_component_never_reaches_control_file() {
    for depth in 0..3 {
        let mut f = Fixture::new();
        fs::create_dir_all(f.dir.join("root/a/b/c")).unwrap();
        f.add(1, "a/b/c/model.stl");
        let outside = f.dir.join("control-dir");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("model.stl"), b"control").unwrap();
        let relative = ["a", "a/b", "a/b/c"][depth];
        fs::remove_dir_all(f.dir.join("root").join(relative)).unwrap();
        symlink(&outside, f.dir.join("root").join(relative)).unwrap();
        assert_eq!(f.ops.delete_file(1), Err(Error::PathUnresolvable));
        assert_eq!(fs::read(outside.join("model.stl")).unwrap(), b"control");
    }
}
#[test]
fn db_mutex_is_released_before_upload_ledger_calls() {
    struct Ledger<'a>(&'a std::sync::Mutex<rusqlite::Connection>);
    impl UploadLedger for Ledger<'_> {
        fn identity(&mut self, _: i64) -> Result<Option<Identity>> {
            assert!(self.0.try_lock().is_ok());
            Ok(None)
        }
        fn detach(&mut self, _: &[i64]) -> Result<()> {
            assert!(self.0.try_lock().is_ok());
            Ok(())
        }
    }
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    let c = std::mem::replace(&mut f.ops.db, crate::db::connect_in_memory().unwrap());
    let mutex = std::sync::Mutex::new(c);
    let mut ops = FileOps::new(
        FsContext::new(&[f.dir.join("root")], &[]).unwrap(),
        Journal::open(&f.dir.join("journal")).unwrap(),
        &mutex,
        Ledger(&mutex),
    );
    ops.rename_file(1, "b").unwrap();
}

#[test]
fn upload_as_root_still_rejects_incoming_targets() {
    let root = crate::unique_test_dir("upload_root");
    let ctx = FsContext::new(
        std::slice::from_ref(&root),
        &[(root.clone(), Protection::Upload)],
    )
    .unwrap();
    assert!(matches!(
        ctx.resolve(root.join("x.stl").to_str().unwrap(), Access::MoveTarget),
        Err(Error::Protected)
    ));
    assert!(ctx
        .resolve(root.join("x.stl").to_str().unwrap(), Access::RenameTarget)
        .is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unreadable_owned_trash_is_not_misclassified_as_foreign() {
    for errno in [libc::EACCES, libc::EIO] {
        let mut f = Fixture::new();
        f.add(1, "a.stl");
        f.ops.delete_file(1).unwrap();
        let trash = f.ops.state(1).unwrap().trash_path.unwrap();
        crate::upload_fs::FSOPS_READ_ERROR.with(|h| h.set(Some(errno)));
        assert_eq!(f.ops.delete_file_permanently(1), Err(Error::Internal));
        assert!(f.ops.state(1).is_ok());
        assert!(std::path::Path::new(&trash).exists());
    }
}

#[test]
fn recovery_directory_read_failure_cannot_prove_unlink() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.delete_file(1).unwrap();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    let _ = f.ops.delete_file_permanently(1);
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    crate::upload_fs::FSOPS_READDIR_ERROR.with(|h| h.set(Some(libc::EIO)));
    assert_eq!(f.ops.recover().unwrap().manual.len(), 1);
    assert!(f.ops.state(1).is_ok());
}

#[test]
fn empty_trash_retains_not_started_ids_for_the_next_block() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.add(2, "b.stl");
    f.ops.delete_files(&[1, 2], || true).unwrap();
    let summary = f.ops.empty_trash_summary().unwrap();
    let mut snapshot = f
        .ops
        .empty_trash_snapshot(2, summary.max_deleted_at.as_deref())
        .unwrap();
    let mut n = 0;
    let first = f
        .ops
        .empty_trash(&mut snapshot, || {
            n += 1;
            n == 1
        })
        .unwrap();
    assert!(first[0].ok);
    assert_eq!(first[1].kind, Some(Error::NotStarted));
    assert_eq!(snapshot.remaining(), 1);
    let next = f.ops.empty_trash(&mut snapshot, || true).unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].id, "2");
    assert!(next[0].ok);
    assert_eq!(snapshot.remaining(), 0);
}

#[test]
fn non_utf8_database_path_is_unresolvable() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops
        .db
        .execute("UPDATE files SET path=CAST(x'2fff' AS TEXT) WHERE id=1", [])
        .unwrap();
    assert_eq!(f.ops.rename_file(1, "b"), Err(Error::PathUnresolvable));
    assert!(f.dir.join("root/a.stl").exists());
}
