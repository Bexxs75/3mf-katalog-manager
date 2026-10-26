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
    fn reset_imported(&mut self, ids: &[i64]) -> Result<()> {
        self.0.extend_from_slice(ids);
        Ok(())
    }
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
        companions_db_before: true,
        companions_db_after: true,
        companions_all_source: true,
        companions_all_target: true,
        companions_mixed: false,
        companions_any: false,
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
        (
            RecoveryDecision::RollbackCompanions,
            RecoveryFacts {
                source_matches: true,
                source_absent: false,
                companions_any: true,
                companions_all_source: false,
                companions_all_target: false,
                companions_mixed: true,
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
        fn reset_imported(&mut self, _: &[i64]) -> Result<()> {
            assert!(self.0.try_lock().is_ok());
            Ok(())
        }
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

#[test]
fn ack_rejects_nonmanual_and_unknown_ids() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert!(f.ops.rename_file(1, "b").is_err());
    let ids = f.ops.journal.pending_op_ids();
    assert_eq!(
        f.ops.journal.acknowledge(&ids),
        Err(Error::InvalidArguments)
    );
    assert_eq!(
        f.ops.journal.acknowledge(&["unknown".into()]),
        Err(Error::InvalidArguments)
    );
    assert_eq!(f.ops.journal.pending_op_ids(), ids);
}
#[test]
fn snapshot_rechecks_cutoff_after_restore_and_delete() {
    let mut f = Fixture::new();
    for id in 1..=200 {
        f.ops.db.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at,deleted_at) VALUES (?1,'missing',?2,'stl',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",rusqlite::params![id,format!("/missing/{id}")]).unwrap();
    }
    f.add(201, "a.stl");
    f.ops.delete_file(201).unwrap();
    let summary = f.ops.empty_trash_summary().unwrap();
    let mut snapshot = f
        .ops
        .empty_trash_snapshot(201, summary.max_deleted_at.as_deref())
        .unwrap();
    assert_eq!(
        f.ops.empty_trash(&mut snapshot, || true).unwrap().len(),
        200
    );
    assert_eq!(snapshot.remaining(), 1);
    f.ops.restore_file(201).unwrap();
    f.ops.delete_file(201).unwrap();
    f.ops
        .db
        .execute(
            "UPDATE files SET deleted_at='2099-01-01T00:00:00Z' WHERE id=201",
            [],
        )
        .unwrap();
    let result = f.ops.empty_trash(&mut snapshot, || true).unwrap();
    assert_eq!(result[0].kind, Some(Error::Changed));
    assert!(f.ops.state(201).is_ok());
}
#[test]
fn recovery_intact_source_ignores_unresolvable_destination() {
    let mut f = Fixture::new();
    f.add(1, "a.stl");
    fs::create_dir(f.dir.join("root/dest")).unwrap();
    let id = crate::db::insert_folder_with_parent(
        &f.ops.db,
        "dest",
        None,
        f.dir.join("root/dest").to_str().unwrap(),
    )
    .unwrap();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert!(f.ops.move_file_to_folder(1, Some(id)).is_err());
    fs::remove_dir(f.dir.join("root/dest")).unwrap();
    symlink("/tmp", f.dir.join("root/dest")).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn final_component_errors_are_not_absence() {
    let f = Fixture::new();
    let path = f.dir.join("root").join("x".repeat(256));
    assert!(matches!(
        f.ops
            .context
            .resolve(path.to_str().unwrap(), Access::Directory),
        Err(Error::Internal)
    ));
}
#[test]
fn excessive_trash_extension_is_conflict() {
    assert_eq!(
        trash_name(1, "abcdef0123456789", &format!("x.{}", "a".repeat(250))),
        Err(Error::TrashConflict)
    );
}

thread_local! {
    pub(super) static BEFORE_MUTATION: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
/// What a named point inside a companion-carrying operation does when it is reached.
pub(super) enum PointAction {
    /// Stop as a crash would: nothing is rolled back.
    Crash,
    /// Run something at that moment, e.g. to make the next step fail.
    Run(Box<dyn FnOnce()>),
}
thread_local! {
    pub(super) static POINT: std::cell::RefCell<Option<(&'static str, usize, PointAction)>> = const { std::cell::RefCell::new(None) };
}
/// Returns true when the operation must stop here as if the process had died.
pub(super) fn at_point(name: &str, k: usize) -> bool {
    POINT.with(|p| {
        let hit = p.borrow().as_ref().is_some_and(|(n, i, _)| *n == name && *i == k);
        if !hit {
            return false;
        }
        match p.borrow_mut().take().map(|(_, _, a)| a) {
            Some(PointAction::Run(f)) => {
                f();
                false
            }
            Some(PointAction::Crash) => true,
            None => false,
        }
    })
}
pub(super) fn before_mutation() {
    BEFORE_MUTATION.with(|h| {
        if let Some(f) = h.borrow_mut().take() {
            f();
        }
    });
}
#[test]
fn pre_mutation_size_change_does_not_block() {
    for mode in ["rename", "case", "unlink"] {
        let mut f = Fixture::new();
        f.add(1, "a.stl");
        if mode == "unlink" {
            f.ops.delete_file(1).unwrap();
        }
        let state = f.ops.state(1).unwrap();
        let path = state.trash_path.unwrap_or(state.path);
        BEFORE_MUTATION.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || fs::write(path, b"changed size").unwrap()))
        });
        let result = match mode {
            "unlink" => f.ops.delete_file_permanently(1).map(|_| ()),
            "case" => f.ops.rename_file(1, "A"),
            _ => f.ops.rename_file(1, "b"),
        };
        assert_eq!(result, Err(Error::Changed), "{mode}");
        assert!(f.ops.journal.is_empty());
        assert!(!f.ops.journal.is_blocked());
    }
}

impl Fixture {
    fn folder(&mut self, name: &str, parent: Option<i64>) -> i64 {
        let path = self.dir.join("root").join(name);
        fs::create_dir_all(&path).unwrap();
        crate::db::insert_folder_with_parent(
            &self.ops.db,
            path.file_name().unwrap().to_str().unwrap(),
            parent,
            path.to_str().unwrap(),
        )
        .unwrap()
    }
    fn assign(&mut self, file: i64, folder: i64) {
        self.ops
            .db
            .execute(
                "UPDATE files SET folder_id=?1 WHERE id=?2",
                rusqlite::params![folder, file],
            )
            .unwrap();
    }
}
#[test]
fn folders_create_rename_move_unicode_prefix_and_restore_paths() {
    let mut f = Fixture::new();
    let parent = f.folder("parent", None);
    let a = f.ops.create_folder(Some(parent), "Ä").unwrap();
    let child = f.ops.create_folder(Some(a), "child").unwrap();
    let sibling = f.folder("parent/ÄB", Some(parent));
    f.add(1, "parent/Ä/child/a.stl");
    f.assign(1, child);
    f.add(2, "parent/ÄB/b.stl");
    f.assign(2, sibling);
    f.add(3, "parent/Ä/t.stl");
    f.assign(3, a);
    f.ops.delete_file(3).unwrap();
    let trash = f.ops.state(3).unwrap().trash_path;
    fs::write(f.dir.join("root/parent/Ä/untracked"), b"control").unwrap();
    f.ops.rename_folder(a, "Ö").unwrap();
    assert!(f.ops.state(1).unwrap().path.ends_with("/Ö/child/a.stl"));
    assert!(f.ops.state(2).unwrap().path.ends_with("/ÄB/b.stl"));
    assert_eq!(f.ops.state(3).unwrap().trash_path, trash);
    let dest = f.folder("dest", None);
    f.ops.move_folder(a, Some(dest)).unwrap();
    f.ops.restore_file(3).unwrap();
    assert!(f.dir.join("root/dest/Ö/t.stl").exists());
    assert_eq!(
        fs::read(f.dir.join("root/dest/Ö/untracked")).unwrap(),
        b"control"
    );
    assert!(f.ops.ledger.0.contains(&1));
}
#[test]
fn folder_failures_leave_source_and_database_untouched() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let b = f.folder("A/B", Some(a));
    assert_eq!(f.ops.move_folder(a, Some(b)), Err(Error::Cycle));
    assert_eq!(f.ops.move_folder(a, Some(a)), Err(Error::Cycle));
    assert_eq!(f.ops.move_folder(a, None), Err(Error::Conflict));
    assert_eq!(f.ops.move_folder(b, Some(a)), Err(Error::Conflict));
    assert_eq!(f.ops.create_folder(None, "x"), Err(Error::InvalidArguments));
    assert_eq!(f.ops.create_folder(Some(a), "B"), Err(Error::Conflict));
    assert_eq!(f.ops.rename_folder(a, ".hidden"), Err(Error::InvalidName));
    assert_eq!(f.ops.rename_folder(999, "x"), Err(Error::NotFound));
    f.add(1, "outside.stl");
    f.assign(1, b);
    assert_eq!(f.ops.rename_folder(a, "C"), Err(Error::Inconsistent));
    assert!(f.dir.join("root/A/B").is_dir());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn folder_recovery_each_phase_ignores_directory_size() {
    for phase in [Phase::Prepared, Phase::Renamed, Phase::Committed] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        f.add(1, "A/a.stl");
        f.assign(1, a);
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::RestartRequired));
        let path = if phase == Phase::Prepared { "A" } else { "B" };
        for n in 0..40 {
            fs::write(
                f.dir.join("root").join(path).join(format!("extra{n}")),
                b"x",
            )
            .unwrap();
        }
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        assert!(f.ops.recover().unwrap().manual.is_empty());
        f.ops.finish_recovery().unwrap();
        assert!(f.ops.state(1).unwrap().path.contains(&format!("/{path}/")));
        assert!(f.ops.journal.is_empty());
    }
}
#[test]
fn folder_create_database_failure_rolls_back_empty_directory() {
    let mut f = Fixture::new();
    let p = f.folder("parent", None);
    f.ops.db.execute_batch("CREATE TRIGGER fail_folder BEFORE INSERT ON folders BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(f.ops.create_folder(Some(p), "new"), Err(Error::Internal));
    assert!(!f.dir.join("root/parent/new").exists());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn folder_removal_preserves_disk_trash_and_unused_tags() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let b = f.folder("A/B", Some(a));
    f.add(1, "A/B/a.stl");
    f.assign(1, b);
    f.add(2, "A/t.stl");
    f.assign(2, a);
    f.ops.delete_file(2).unwrap();
    f.ops
        .db
        .execute_batch("INSERT INTO tags VALUES(1,'unused',0); INSERT INTO file_tags VALUES(1,1);")
        .unwrap();
    let summary = f.ops.folder_removal_summary(a).unwrap();
    assert_eq!((summary.model_count, summary.subfolder_count), (1, 1));
    let result = f.ops.remove_folder_from_catalog(a).unwrap();
    assert_eq!((result.model_count, result.folder_count), (1, 2));
    assert_eq!(f.ops.state(1), Err(Error::NotFound));
    assert_eq!(f.ops.state(2).unwrap().folder_id, None);
    assert!(f.dir.join("root/A/B/a.stl").exists());
    assert_eq!(
        f.ops
            .db
            .query_row("SELECT count(*) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn folder_protection_matrix_and_catalog_setting() {
    let mut f = Fixture::new();
    let parent = f.folder("parent", None);
    let upload = f.folder("parent/upload", Some(parent));
    let sub = f.folder("parent/upload/sub", Some(upload));
    let catalog = f.folder("catalog", None);
    let child = f.folder("catalog/child", Some(catalog));
    let trash = crate::db::insert_folder_with_parent(
        &f.ops.db,
        "trash",
        None,
        f.dir.join("root").join(TRASH).to_str().unwrap(),
    )
    .unwrap();
    f.ops.context = FsContext::new(
        &[f.dir.join("root")],
        &[
            (f.dir.join("root/parent/upload"), Protection::Upload),
            (f.dir.join("root/catalog"), Protection::CatalogBase),
        ],
    )
    .unwrap();
    for id in [parent, upload, sub, catalog, trash] {
        assert_eq!(f.ops.rename_folder(id, "renamed"), Err(Error::Protected));
        assert_eq!(
            f.ops.remove_folder_from_catalog(id).unwrap_err(),
            Error::Protected
        );
    }
    assert_eq!(
        f.ops.create_folder(Some(upload), "new"),
        Err(Error::Protected)
    );
    f.ops.rename_folder(child, "free").unwrap();
    f.ops.create_folder(Some(catalog), "new").unwrap();
    // The persisted setting is checked even if a caller omitted its startup protection.
    f.ops.context = FsContext::new(&[f.dir.join("root")], &[]).unwrap();
    crate::db::printer_link::set_setting(
        &f.ops.db,
        "catalog_base_dir",
        f.dir.join("root/parent/upload").to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        f.ops.rename_folder(parent, "renamed"),
        Err(Error::Protected)
    );
    assert_eq!(
        f.ops.remove_folder_from_catalog(parent).unwrap_err(),
        Error::Protected
    );
}
#[test]
fn folder_syscall_errors_conflicts_and_symlinks_preserve_control() {
    for (errno, expected) in [
        (libc::EXDEV, Error::CrossDevice),
        (libc::EBUSY, Error::Busy),
        (libc::EINVAL, Error::Unsupported),
        (libc::ENOSYS, Error::Unsupported),
        (libc::EACCES, Error::Internal),
    ] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        crate::upload_fs::FSOPS_RENAME_ERROR.with(|h| h.set(Some(errno)));
        assert_eq!(f.ops.rename_folder(a, "B"), Err(expected));
        assert!(f.dir.join("root/A").is_dir());
        assert!(f.ops.journal.is_empty());
    }
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let b = f.folder("B", None);
    fs::write(f.dir.join("root/B/control"), b"control").unwrap();
    assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::Conflict));
    assert_eq!(fs::read(f.dir.join("root/B/control")).unwrap(), b"control");
    fs::remove_dir(f.dir.join("root/A")).unwrap();
    symlink(f.dir.join("root/B"), f.dir.join("root/A")).unwrap();
    assert_eq!(f.ops.rename_folder(a, "C"), Err(Error::PathUnresolvable));
    assert_eq!(
        f.ops.create_folder(Some(a), "new"),
        Err(Error::PathUnresolvable)
    );
    assert!(f.ops.folder_removal_summary(b).is_ok());
}
#[test]
fn folder_physical_cycle_even_when_db_parent_links_differ() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let b = f.folder("A/B", None);
    assert_eq!(f.ops.move_folder(a, Some(b)), Err(Error::Cycle));
    assert!(f.dir.join("root/A/B").exists());
}
#[test]
fn folder_create_recovery_rolls_back_only_known_empty_inode() {
    for phase in [Phase::Prepared, Phase::Renamed, Phase::Committed] {
        let mut f = Fixture::new();
        let p = f.folder("parent", None);
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(
            f.ops.create_folder(Some(p), "new"),
            Err(Error::RestartRequired)
        );
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        assert!(f.ops.recover().unwrap().manual.is_empty());
        f.ops.finish_recovery().unwrap();
        assert_eq!(
            f.dir.join("root/parent/new").exists(),
            phase == Phase::Committed
        );
    }
    for nonempty in [false, true] {
        let mut f = Fixture::new();
        let p = f.folder("parent", None);
        f.ops.journal.crash_at = Some(if nonempty {
            Phase::Renamed
        } else {
            Phase::Prepared
        });
        assert!(f.ops.create_folder(Some(p), "new").is_err());
        if !nonempty {
            fs::create_dir(f.dir.join("root/parent/new")).unwrap();
        }
        fs::write(f.dir.join("root/parent/new/control"), b"control").unwrap();
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        assert_eq!(f.ops.recover().unwrap().manual.len(), 1);
        assert_eq!(
            fs::read(f.dir.join("root/parent/new/control")).unwrap(),
            b"control"
        );
    }
}
#[test]
fn folder_create_failed_insert_never_removes_nonempty_directory() {
    let mut f = Fixture::new();
    let p = f.folder("parent", None);
    f.ops.db.execute_batch("CREATE TRIGGER fail_folder BEFORE INSERT ON folders BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    let path = f.dir.join("root/parent/new/control");
    BEFORE_MUTATION
        .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::write(path, b"control").unwrap())));
    assert_eq!(
        f.ops.create_folder(Some(p), "new"),
        Err(Error::RestartRequired)
    );
    assert_eq!(
        fs::read(f.dir.join("root/parent/new/control")).unwrap(),
        b"control"
    );
}
#[test]
fn folder_database_failure_rolls_back_rename_and_removal() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    f.ops.db.execute_batch("CREATE TRIGGER fail_folder BEFORE UPDATE ON folders BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::Internal));
    assert!(f.dir.join("root/A/a.stl").exists());
    assert!(f.ops.journal.is_empty());
    f.ops.db.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON folders BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(
        f.ops.remove_folder_from_catalog(a).unwrap_err(),
        Error::Internal
    );
    assert!(f.ops.state(1).is_ok());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn folder_recovery_foreign_target_changed_db_and_root() {
    for mode in ["foreign", "db", "root"] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        f.ops.journal.crash_at = Some(if mode == "foreign" {
            Phase::Prepared
        } else {
            Phase::Renamed
        });
        assert!(f.ops.rename_folder(a, "B").is_err());
        if mode == "foreign" {
            fs::create_dir(f.dir.join("root/B")).unwrap();
        }
        if mode == "db" {
            f.ops
                .db
                .execute("UPDATE folders SET name='external' WHERE id=?1", [a])
                .unwrap();
        }
        if mode == "root" {
            f.ops.context.roots[0].identity.ino += 1;
        }
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        let report = f.ops.recover().unwrap();
        assert_eq!(report.manual.is_empty(), mode == "foreign");
        assert!(f.dir.join("root/B").exists());
    }
}
#[test]
fn folder_remove_recovery_resets_upload_ids_after_commit() {
    for phase in [Phase::Prepared, Phase::Committed] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        f.add(1, "A/a.stl");
        f.assign(1, a);
        f.ops.journal.crash_at = Some(phase);
        assert_eq!(
            f.ops.remove_folder_from_catalog(a).unwrap_err(),
            Error::RestartRequired
        );
        f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
        let report = f.ops.recover().unwrap();
        assert!(report.manual.is_empty());
        assert_eq!(
            report.reset_ids,
            if phase == Phase::Committed {
                vec![1]
            } else {
                vec![]
            }
        );
        assert!(report.detach_ids.is_empty());
        f.ops.finish_recovery().unwrap();
        assert!(f.dir.join("root/A/a.stl").exists());
    }
}

#[test]
fn folder_removal_fk_matrix_and_unassigned_physical_file() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    f.add(2, "A/t.stl");
    f.assign(2, a);
    f.ops.delete_file(2).unwrap();
    f.add(3, "A/unassigned.stl");
    f.ops.db.execute_batch("INSERT INTO tags VALUES(1,'tag',0); INSERT INTO tags VALUES(2,'unused',0);
    INSERT INTO collections VALUES(1,'collection','today'); INSERT INTO printers(id,name) VALUES(1,'printer');
    INSERT INTO filament_spools(id,material,diameter_mm,original_weight_g,remaining_weight_g,created_at) VALUES(1,'PLA',1.75,1000,900,'today');
    INSERT INTO printer_jobs(printer_id,remote_id,file_name,outcome,raw_status,ended_at,print_duration_s,used_mm,booked_file_id,booked_spool_id) VALUES(1,'remote','a.stl','completed','complete',1,2,3,1,1);").unwrap();
    for id in [1, 2] {
        for sql in [
            "INSERT INTO file_tags VALUES(?1,1)",
            "INSERT INTO collection_files VALUES(1,?1,0)",
            "INSERT INTO file_metadata VALUES(?1,'key','value')",
            "INSERT INTO file_materials(file_id,name) VALUES(?1,'PLA')",
            "INSERT INTO print_log(file_id,printed_at,created_at) VALUES(?1,'today','today')",
        ] {
            f.ops.db.execute(sql, [id]).unwrap();
        }
    }
    f.ops.remove_folder_from_catalog(a).unwrap();
    for table in [
        "file_tags",
        "collection_files",
        "file_metadata",
        "file_materials",
        "print_log",
        "printer_jobs",
        "filament_spools",
        "printers",
        "collections",
    ] {
        assert_eq!(
            f.ops
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1,
            "{table}"
        );
    }
    assert_eq!(
        f.ops
            .db
            .query_row("SELECT count(*) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        f.ops
            .db
            .query_row("SELECT booked_file_id FROM printer_jobs", [], |r| r
                .get::<_, Option<i64>>(0))
            .unwrap(),
        None
    );
    assert!(f.ops.state(3).is_ok());
    assert!(f.dir.join("root/A/unassigned.stl").exists());
    assert_eq!(
        f.ops
            .db
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
}
#[test]
fn folder_recovery_validates_intent_paths_before_database_update() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert!(f.ops.rename_folder(a, "B").is_err());
    let path = f.dir.join("journal/web-fsops.json");
    let mut doc: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    doc["folders"][0]["after"]["folders"][0]["path"] =
        serde_json::json!(f.dir.join("outside").to_str().unwrap());
    fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert_eq!(f.ops.recover().unwrap().manual.len(), 1);
    assert_eq!(
        f.ops
            .db
            .query_row("SELECT path FROM folders WHERE id=?1", [a], |r| r
                .get::<_, String>(0))
            .unwrap(),
        f.dir.join("root/A").to_str().unwrap()
    );
}
#[test]
fn folder_parent_chain_changed_before_rename_is_rejected() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let dest = f.folder("dest", None);
    let upload = f.folder("upload", None);
    let _ = upload;
    f.ops.context = FsContext::new(
        &[f.dir.join("root")],
        &[(f.dir.join("root/upload"), Protection::Upload)],
    )
    .unwrap();
    let from = f.dir.join("root/dest");
    let to = f.dir.join("root/upload/dest");
    BEFORE_MUTATION
        .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::rename(from, to).unwrap())));
    assert_eq!(f.ops.move_folder(a, Some(dest)), Err(Error::Protected));
    assert!(f.dir.join("root/A").is_dir());
    assert!(f.ops.journal.is_empty());
}
#[test]
fn folder_post_rename_identity_error_blocks_and_preserves_source() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    crate::upload_fs::FSOPS_AFTER_RENAME_ERROR.with(|h| h.set(Some(libc::EIO)));
    assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::RestartRequired));
    assert!(f.ops.journal.is_blocked());
    assert!(f.dir.join("root/A/a.stl").exists());
}
#[test]
fn folder_recovery_phase_marker_may_lag_rename() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert!(f.ops.rename_folder(a, "B").is_err());
    fs::rename(f.dir.join("root/A"), f.dir.join("root/B")).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert!(f.ops.recover().unwrap().manual.is_empty());
    f.ops.finish_recovery().unwrap();
    assert!(f
        .ops
        .db
        .query_row("SELECT path FROM folders WHERE id=?1", [a], |r| r
            .get::<_, String>(0))
        .unwrap()
        .ends_with("/B"));
}

#[test]
fn folder_removal_handles_missing_disk_and_inconsistent_links_like_desktop() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let b = f.folder("elsewhere", Some(a));
    f.add(1, "other.stl");
    f.assign(1, b);
    fs::remove_dir(f.dir.join("root/A")).unwrap();
    assert_eq!(f.ops.folder_removal_summary(a).unwrap().model_count, 1);
    let result = f.ops.remove_folder_from_catalog(a).unwrap();
    assert_eq!((result.model_count, result.folder_count), (1, 2));
    assert!(f.dir.join("root/other.stl").exists());
    assert!(f.dir.join("root/elsewhere").exists());
}
#[test]
fn oversized_folder_intent_is_rejected_before_filesystem_mutation() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    f.ops
        .db
        .execute(
            "UPDATE files SET name=?1 WHERE id=1",
            ["x".repeat(8 * 1024 * 1024)],
        )
        .unwrap();
    assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::Unsupported));
    assert!(f.dir.join("root/A/a.stl").exists());
    assert!(f.ops.journal.is_empty());
    assert!(Journal::open(&f.dir.join("journal")).unwrap().is_empty());
}

thread_local! {
    pub(super) static AFTER_DIRECTORY_RENAME: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn after_directory_rename() {
    AFTER_DIRECTORY_RENAME.with(|h| {
        if let Some(f) = h.borrow_mut().take() {
            f();
        }
    });
}
#[test]
fn folder_parent_chain_changed_after_rename_rolls_back_and_blocks() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    let dest = f.folder("dest", None);
    f.folder("upload", None);
    f.ops.context = FsContext::new(
        &[f.dir.join("root")],
        &[(f.dir.join("root/upload"), Protection::Upload)],
    )
    .unwrap();
    let from = f.dir.join("root/dest");
    let to = f.dir.join("root/upload/dest");
    AFTER_DIRECTORY_RENAME
        .with(|h| *h.borrow_mut() = Some(Box::new(move || fs::rename(from, to).unwrap())));
    assert_eq!(
        f.ops.move_folder(a, Some(dest)),
        Err(Error::RestartRequired)
    );
    assert!(f.dir.join("root/A").is_dir());
    assert!(f.ops.journal.is_blocked());
    assert!(!f.dir.join("root/upload/dest/A").exists());
}
#[test]
fn folder_removal_inconsistent_db_cannot_cascade_into_catalog_base() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.folder("catalog", Some(a));
    f.ops.context = FsContext::new(
        &[f.dir.join("root")],
        &[(f.dir.join("root/catalog"), Protection::CatalogBase)],
    )
    .unwrap();
    assert_eq!(
        f.ops.remove_folder_from_catalog(a).unwrap_err(),
        Error::Protected
    );
    assert_eq!(f.ops.folder_removal_summary(a).unwrap().subfolder_count, 1);
}
#[test]
fn folder_commit_checks_changed_db_and_rolls_back_without_losing_external_update() {
    struct Db {
        conn: rusqlite::Connection,
        target: std::path::PathBuf,
        changed: bool,
    }
    impl FsDatabase for Db {
        fn with_conn<T>(
            &mut self,
            f: impl FnOnce(&mut rusqlite::Connection) -> Result<T>,
        ) -> Result<T> {
            if !self.changed && self.target.exists() {
                self.conn
                    .execute("UPDATE files SET name='external' WHERE id=1", [])
                    .unwrap();
                self.changed = true;
            }
            f(&mut self.conn)
        }
    }
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    let conn = std::mem::replace(&mut f.ops.db, crate::db::connect_in_memory().unwrap());
    let mut ops = FileOps::new(
        FsContext::new(&[f.dir.join("root")], &[]).unwrap(),
        Journal::open(&f.dir.join("journal")).unwrap(),
        Db {
            conn,
            target: f.dir.join("root/B"),
            changed: false,
        },
        MemoryLedger::default(),
    );
    assert_eq!(ops.rename_folder(a, "B"), Err(Error::Changed));
    assert!(f.dir.join("root/A/a.stl").exists());
    assert!(!f.dir.join("root/B").exists());
    assert_eq!(ops.state(1).unwrap().name, "external");
    assert!(ops.journal.is_empty());
}
#[test]
fn folder_recovery_rejects_replacement_inode_and_ack_only_manual() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.ops.journal.crash_at = Some(Phase::Renamed);
    assert!(f.ops.rename_folder(a, "B").is_err());
    let ids = f.ops.journal.pending_op_ids();
    assert_eq!(
        f.ops.journal.acknowledge(&ids),
        Err(Error::InvalidArguments)
    );
    fs::rename(f.dir.join("root/B"), f.dir.join("root/original")).unwrap();
    fs::create_dir(f.dir.join("root/B")).unwrap();
    f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
    assert_eq!(f.ops.recover().unwrap().manual, ids);
    f.ops.journal.acknowledge(&ids).unwrap();
    assert!(!f.ops.journal.is_blocked());
    assert!(f.dir.join("root/original").exists());
    assert!(f.dir.join("root/B").exists());
}

#[test]
fn a2fix_catalog_base_outside_roots_and_missing() {
    for base in [
        "outside",
        "outside\\name",
        "missing/deep",
        "root/missing/deep",
    ] {
        let mut f = Fixture::new();
        if base.starts_with("outside") {
            fs::create_dir(f.dir.join(base)).unwrap();
        }
        crate::db::printer_link::set_setting(
            &f.ops.db,
            "catalog_base_dir",
            f.dir.join(base).to_str().unwrap(),
        )
        .unwrap();
        let a = f.folder("A", None);
        f.ops.rename_folder(a, "B").unwrap();
        f.ops.remove_folder_from_catalog(a).unwrap();
    }
}

#[test]
fn a2fix_missing_catalog_base_still_protects_existing_ancestors() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    crate::db::printer_link::set_setting(
        &f.ops.db,
        "catalog_base_dir",
        f.dir.join("root/A/missing/deep").to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::Protected));
}

#[test]
fn a2fix_db_removal_skips_unresolvable_descendants() {
    for suffix in ["root/link/child", "outside", "root"] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        fs::create_dir(f.dir.join("outside")).unwrap();
        symlink(f.dir.join("outside"), f.dir.join("root/link")).unwrap();
        crate::db::insert_folder_with_parent(
            &f.ops.db,
            "stale",
            Some(a),
            f.dir.join(suffix).to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(f.ops.remove_folder_from_catalog(a).unwrap().folder_count, 2);
        assert!(f.dir.join("outside").is_dir());
        assert!(f.dir.join("root/A").is_dir());
    }
}

#[test]
fn a2fix_move_existing_linux_names_recovers_each_phase() {
    for name in ["Star Wars: X", "Vol.", "AUX"] {
        for phase in [
            None,
            Some(Phase::Prepared),
            Some(Phase::Renamed),
            Some(Phase::Committed),
        ] {
            let mut f = Fixture::new();
            let a = f.folder(name, None);
            let dest = f.folder("dest", None);
            f.ops.journal.crash_at = phase;
            let result = f.ops.move_folder(a, Some(dest));
            if phase.is_some() {
                assert_eq!(result, Err(Error::RestartRequired));
            } else {
                result.unwrap();
            }
            f.ops.journal = Journal::open(&f.dir.join("journal")).unwrap();
            assert!(f.ops.recover().unwrap().manual.is_empty());
            f.ops.finish_recovery().unwrap();
            let path = if phase == Some(Phase::Prepared) {
                f.dir.join("root").join(name)
            } else {
                f.dir.join("root/dest").join(name)
            };
            assert!(path.is_dir());
            let db_path: String = f
                .ops
                .db
                .query_row("SELECT path FROM folders WHERE id=?1", [a], |r| r.get(0))
                .unwrap();
            assert_eq!(db_path, path.to_str().unwrap());
            assert!(f.ops.journal.is_empty());
        }
    }
}

#[test]
fn a2fix_stale_target_descendants_reject_before_mutation() {
    for operation in ["rename", "move", "create"] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        let dest = f.folder("dest", None);
        let target = if operation == "rename" { "B" } else { "dest/A" };
        let stale = f.folder(&format!("{target}/x"), None);
        fs::remove_dir_all(f.dir.join("root").join(target)).unwrap();
        let result = match operation {
            "rename" => f.ops.rename_folder(a, "B"),
            "move" => f.ops.move_folder(a, Some(dest)),
            _ => f.ops.create_folder(Some(dest), "A").map(|_| ()),
        };
        assert_eq!(result, Err(Error::Conflict), "{operation}");
        assert!(f.dir.join("root/A").is_dir());
        assert!(!f.dir.join("root").join(target).exists());
        assert!(f.ops.folder_removal_summary(stale).is_ok());
        assert!(f.ops.journal.is_empty());
    }
}

thread_local! { static FILE_STATE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(super) fn file_state_read() {
    FILE_STATE_READS.with(|n| n.set(n.get() + 1));
}

#[test]
fn a2fix_large_catalog_only_reads_subtree_file_states() {
    let mut f = Fixture::new();
    let a = f.folder("A", None);
    f.add(1, "A/a.stl");
    f.assign(1, a);
    f.add(2, "A/unassigned.stl");
    f.ops.db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(3) UNION ALL SELECT x+1 FROM n WHERE x<10002) INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) SELECT x,'other','/unrelated/'||x,'stl',5,'2026-01-01T00:00:00Z' FROM n;").unwrap();
    FILE_STATE_READS.with(|n| n.set(0));
    f.ops.rename_folder(a, "B").unwrap();
    let reads = FILE_STATE_READS.with(|n| n.get());
    assert!(reads <= 10, "rename read {reads} file states");
    assert!(f.ops.state(2).unwrap().path.ends_with("/B/unassigned.stl"));
    FILE_STATE_READS.with(|n| n.set(0));
    assert_eq!(f.ops.folder_removal_summary(a).unwrap().model_count, 1);
    assert_eq!(FILE_STATE_READS.with(|n| n.get()), 1);
    FILE_STATE_READS.with(|n| n.set(0));
    f.ops.remove_folder_from_catalog(a).unwrap();
    assert!(FILE_STATE_READS.with(|n| n.get()) <= 4);
    assert!(f.ops.state(2).is_ok());
}

#[test]
fn a2fix_catalog_resolution_inside_root_fails_closed() {
    for base in ["root/link/catalog", "root/bad\\name"] {
        let mut f = Fixture::new();
        let a = f.folder("A", None);
        fs::create_dir_all(f.dir.join("outside/catalog")).unwrap();
        symlink(f.dir.join("outside"), f.dir.join("root/link")).unwrap();
        crate::db::printer_link::set_setting(
            &f.ops.db,
            "catalog_base_dir",
            f.dir.join(base).to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(f.ops.rename_folder(a, "B"), Err(Error::PathUnresolvable));
        assert!(f.dir.join("root/A").is_dir());
        assert!(f.ops.journal.is_empty());
    }
}

#[test]
fn a2fix_db_removal_unlocated_source_is_explicitly_rejected() {
    let mut f = Fixture::new();
    let id = crate::db::insert_folder_with_parent(
        &f.ops.db,
        "outside",
        None,
        f.dir.join("outside").to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        f.ops.remove_folder_from_catalog(id).unwrap_err(),
        Error::PathUnresolvable
    );
    assert!(f.ops.folder_removal_summary(id).is_ok());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn stale_journal_temp_files_are_removed_but_fresh_and_foreign_names_stay() {
    use std::time::{Duration, SystemTime};
    let dir = crate::unique_test_dir("fsops_stale_part");
    let old_part = dir.join(format!(".fsops-{}.part", "a".repeat(32)));
    let fresh_part = dir.join(format!(".fsops-{}.part", "b".repeat(32)));
    let foreign = dir.join(".fsops-notanid.part");
    let journal_like = dir.join("unrelated.txt");
    for path in [&old_part, &fresh_part, &foreign, &journal_like] {
        fs::write(path, b"x").unwrap();
    }
    let old = SystemTime::now() - Duration::from_secs(2 * 3600);
    for path in [&old_part, &foreign] {
        fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
    }
    // A symlink with a matching name must not be followed or removed.
    let link = dir.join(format!(".fsops-{}.part", "c".repeat(32)));
    symlink(&journal_like, &link).unwrap();
    let journal = Journal::open(&dir).unwrap();
    assert_eq!(journal.remove_stale_temp_files(Duration::from_secs(3600)), 1);
    assert!(!old_part.exists());
    assert!(fresh_part.exists() && foreign.exists() && journal_like.exists());
    assert!(link.symlink_metadata().is_ok());
}
