//! Golden scenarios of the model trash family (issue #20, package P7): delete (single and
//! batch), restore, delete for good, empty the trash, the startup clean-up after 7 days and the
//! folder trash that uses the model trash. Recorded with the code as it was before the actions
//! moved into `katalog_core::model_trash`; the golden files must not change with the move.
//! See `docs/plans/issue20-p7-zuschnitt.md`.
//!
//! The startup clean-up compares with the real clock, so the scenarios set `deleted_at` to
//! 8 and 6 days before now (the snapshot shows time columns as `<TS>`).
use super::fs_boundary::{FaultFs, Op};
use super::golden_harness::{run_golden, World};
use super::*;
use std::io::ErrorKind::{PermissionDenied, Unsupported};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

fn put(w: &World, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = w.root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn cat(w: &World) -> i64 {
    db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap()
}

fn model(w: &World, rel: &str, folder: Option<i64>) -> (i64, PathBuf) {
    let path = put(w, rel, rel.as_bytes());
    (db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), folder).unwrap(), path)
}

fn companion(w: &World, model: i64, name: &str) {
    let source = put(w, &format!("outside/{name}"), PNG);
    companions::add_companion_with_conn(&w.conn, model, &source, &[]).unwrap();
}

fn load(w: &World, id: i64) -> db::models::FileRecord {
    db::get_file(&w.conn, id).unwrap().unwrap()
}

fn del(w: &World, id: i64) -> CmdResult<()> {
    trash::delete_file_with_conn(&w.conn, &load(w, id), id, &w.dir("trash"))
}

fn restore(w: &World, id: i64, sensitive: &[PathBuf]) -> CmdResult<()> {
    trash::restore_file_with_conn(&w.conn, &load(w, id), id, sensitive)
}

fn calls(fs: &FaultFs) -> Vec<String> {
    fs.calls()
        .iter()
        .map(|(op, a, b)| format!("{op:?} {}{}", a.display(), b.as_ref().map(|b| format!(" -> {}", b.display())).unwrap_or_default()))
        .collect()
}

fn ok<T>(value: T) -> CmdResult<T> {
    Ok(value)
}

fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339()
}

/// Sets the time a trashed model (and the companions that went with it) was deleted.
fn age(w: &World, id: i64, when: &str) {
    w.conn.execute("UPDATE files SET deleted_at = ?1 WHERE id = ?2", rusqlite::params![when, id]).unwrap();
    w.conn.execute("UPDATE companion_files SET deleted_at = ?1 WHERE file_id = ?2 AND deleted_at IS NOT NULL", rusqlite::params![when, id]).unwrap();
}

fn block_soft_delete(w: &World, id: i64) {
    w.conn
        .execute_batch(&format!(
            "CREATE TRIGGER block_soft_delete_{id} BEFORE UPDATE ON files
             WHEN NEW.id = {id} AND NEW.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulated failure of soft_delete_file'); END;"
        ))
        .unwrap();
}

fn unblock_soft_delete(w: &World, id: i64) {
    w.conn.execute_batch(&format!("DROP TRIGGER block_soft_delete_{id}")).unwrap();
}

fn block_restore(w: &World) {
    w.conn
        .execute_batch(
            "CREATE TRIGGER block_restore BEFORE UPDATE ON files
             WHEN NEW.deleted_at IS NULL AND OLD.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulated failure of restore_file'); END;",
        )
        .unwrap();
}

#[test]
fn golden_trash_delete() {
    run_golden("trash_delete", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", Some(folder));
        companion(w, a, "pic.png");
        companion(w, a, "second.png");
        let r = del(w, a);
        w.step("delete a model with two companions", r);
        let r = del(w, a);
        w.step("delete again (already in the trash)", r);
        let (gone, gone_path) = model(w, "cat/gone.3mf", Some(folder));
        std::fs::remove_file(gone_path).unwrap();
        let r = del(w, gone);
        w.step("delete a model whose file is gone (only soft deleted)", r);
        let (taken, _) = model(w, "cat/taken.3mf", Some(folder));
        put(w, &format!("trash/{taken}-taken.3mf"), b"older");
        let r = del(w, taken);
        w.step("delete with the trash name taken", r);
        let (d, _) = model(w, "cat/d.3mf", Some(folder));
        companion(w, d, "d.png");
        block_soft_delete(w, d);
        let r = del(w, d);
        w.step("delete with the DB update failing (file moved back, companion stays)", r);
        unblock_soft_delete(w, d);
        let r = del(w, d);
        w.step("delete d again", r);
        let (stale, _) = model(w, "cat/stale.3mf", Some(folder));
        let record = load(w, stale);
        db::delete_file(&w.conn, stale).unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, stale, &w.dir("trash"));
        w.step("delete with a stale record (row already gone, file moved back)", r);
        let (loose, _) = model(w, "loose/x.3mf", None);
        let r = del(w, loose);
        w.step("delete a model outside every catalog folder", r);
        let (no_ext, _) = model(w, "cat/README", Some(folder));
        let r = del(w, no_ext);
        w.step("delete a model without extension", r);
        let r = trash::delete_missing_file_with_conn(&w.conn, 9999);
        w.step("soft delete an unknown id", r);
        let (soft, _) = model(w, "cat/soft.3mf", Some(folder));
        let r = trash::delete_missing_file_with_conn(&w.conn, soft);
        w.step("soft delete without moving anything", r);
    });
}

#[test]
fn golden_trash_delete_batch() {
    run_golden("trash_delete_batch", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", Some(folder));
        companion(w, a, "pic.png");
        let (b, _) = model(w, "cat/b.3mf", Some(folder));
        let (failing, _) = model(w, "cat/failing.3mf", Some(folder));
        let (c, _) = model(w, "cat/c.3mf", Some(folder));
        block_soft_delete(w, failing);
        let ids = vec![
            "abc".to_string(),
            "-1".to_string(),
            a.to_string(),
            a.to_string(),
            "9999".to_string(),
            failing.to_string(),
            " 7".to_string(),
            b.to_string(),
        ];
        let r = trash::delete_files_with_conn(&w.conn, &w.dir("trash"), ids);
        w.step("batch (invalid, unknown, duplicate and failing ids are skipped)", r);
        let r = trash::delete_files_with_conn(&w.conn, &w.dir("trash"), vec![]);
        w.step("empty batch", r);
        unblock_soft_delete(w, failing);
        let r = trash::delete_files_with_conn(&w.conn, &w.dir("trash"), vec![failing.to_string(), c.to_string()]);
        w.step("batch with the former failure and c", r);
    });
}

#[test]
fn golden_trash_delete_faults() {
    run_golden("trash_delete_faults", |w| {
        let folder = cat(w);
        let trash_dir = w.dir("trash");
        let run = |w: &mut World, label: &str, rel: &str, with_companion: bool, fs: FaultFs| {
            let (id, _) = model(w, rel, Some(folder));
            if with_companion {
                companion(w, id, &format!("{id}.png"));
            }
            let r = trash::delete_file_with_conn_fs(&fs, &w.conn, &load(w, id), id, &trash_dir);
            w.step(label, r);
            w.step("calls", ok(calls(&fs)));
            id
        };
        run(w, "metadata fails with PermissionDenied", "cat/m1.3mf", false, FaultFs::new().fail(Op::Metadata, 1, PermissionDenied));
        run(w, "trash on another drive (hard link unsupported, copy path)", "cat/m2.3mf", true, FaultFs::new().fail(Op::HardLink, 1, Unsupported));
        run(
            w,
            "another drive and removing the original fails",
            "cat/m3.3mf",
            false,
            FaultFs::new().fail(Op::HardLink, 1, Unsupported).fail(Op::RemoveFile, 1, PermissionDenied),
        );
        run(w, "hard link ok but removing the original fails", "cat/m4.3mf", false, FaultFs::new().fail(Op::RemoveFile, 1, PermissionDenied));
        run(
            w,
            "copy fails too",
            "cat/m5.3mf",
            false,
            FaultFs::new().fail(Op::HardLink, 1, Unsupported).fail(Op::CopyNew, 1, PermissionDenied),
        );
        run(
            w,
            "companion move fails, the model still goes to the trash",
            "cat/m6.3mf",
            true,
            FaultFs::new().fail_on(Op::HardLink, Some("m6_extras"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("m6_extras"), 1, PermissionDenied),
        );
        run(w, "companion original removal fails", "cat/m7.3mf", true, FaultFs::new().fail(Op::RemoveFile, 2, PermissionDenied));
        // The DB update fails after the move, and the move back fails as well.
        let (id, _) = model(w, "cat/m8.3mf", Some(folder));
        block_soft_delete(w, id);
        let fs = FaultFs::new().fail(Op::HardLink, 2, PermissionDenied).fail(Op::CopyNew, 1, PermissionDenied);
        let r = trash::delete_file_with_conn_fs(&fs, &w.conn, &load(w, id), id, &trash_dir);
        w.step("DB update fails and the move back fails (file stays in the trash)", r);
        w.step("calls", ok(calls(&fs)));
        // A case-insensitive volume: the trash name differs from an existing one only in case.
        let (id, _) = model(w, "cat/Case.3mf", Some(folder));
        put(w, &format!("trash/{id}-case.3mf"), b"older");
        let fs = FaultFs::new().case_insensitive();
        let r = trash::delete_file_with_conn_fs(&fs, &w.conn, &load(w, id), id, &trash_dir);
        w.step("trash name collides in case only (case-insensitive volume)", r);
        w.step("calls", ok(calls(&fs)));
    });
}

#[test]
fn golden_trash_restore() {
    run_golden("trash_restore", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", Some(folder));
        companion(w, a, "pic.png");
        let (b, _) = model(w, "cat/b.3mf", Some(folder));
        let (n, _) = model(w, "cat/notes", Some(folder));
        let (x, _) = model(w, "cat/x.3mf", Some(folder));
        let (soft, _) = model(w, "cat/soft.3mf", Some(folder));
        let (miss, _) = model(w, "cat/miss.3mf", Some(folder));
        for id in [a, b, n, miss] {
            del(w, id).unwrap();
        }
        trash::delete_missing_file_with_conn(&w.conn, soft).unwrap();
        let r = restore(w, a, &[]);
        w.step("restore a model with a companion", r);
        let r = restore(w, x, &[]);
        w.step("restore a model that is not in the trash", r);
        put(w, "cat/b.3mf", b"occupant");
        let r = restore(w, b, &[]);
        w.step("restore with the name taken (wiederhergestellt)", r);
        put(w, "cat/notes", b"occupant");
        let r = restore(w, n, &[]);
        w.step("restore without extension with the name taken", r);
        let r = restore(w, soft, &[]);
        w.step("restore an entry without trash path", r);
        std::fs::remove_file(w.root.join("trash").join(format!("{miss}-miss.3mf"))).unwrap();
        let r = restore(w, miss, &[]);
        w.step("restore with the trash file gone", r);
        let r = restore(w, miss, &[]);
        w.step("restore again (still marked as deleted)", r);
        // Second round on the names now taken by the restored ones.
        del(w, b).unwrap();
        let r = restore(w, b, &[]);
        w.step("delete and restore the renamed one", r);
    });
}

#[test]
fn golden_trash_restore_rollback() {
    run_golden("trash_restore_rollback", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", Some(folder));
        companion(w, a, "pic.png");
        del(w, a).unwrap();
        block_restore(w);
        let r = restore(w, a, &[]);
        w.step("DB update fails after the move (file goes back into the trash)", r);
        w.conn.execute_batch("DROP TRIGGER block_restore").unwrap();
        let r = restore(w, a, &[]);
        w.step("restore works afterwards", r);
    });
}

/// Catalog base `catalog` with folder rows catalog -> A -> B, two models in B, trashed with their folder.
fn folder_world(w: &mut World) -> (PathBuf, i64, [i64; 2]) {
    let base = w.dir("catalog");
    db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &base.to_string_lossy()).unwrap();
    let base_id = db::insert_folder_with_parent(&w.conn, "catalog", None, &base.to_string_lossy()).unwrap();
    let a = db::insert_folder_with_parent(&w.conn, "A", Some(base_id), &w.dir("catalog/A").to_string_lossy()).unwrap();
    let b_folder = db::insert_folder_with_parent(&w.conn, "B", Some(a), &w.dir("catalog/A/B").to_string_lossy()).unwrap();
    let (one, _) = model(w, "catalog/A/B/one.3mf", Some(b_folder));
    let (two, _) = model(w, "catalog/A/B/two.3mf", Some(b_folder));
    companion(w, one, "pic.png");
    (base, a, [one, two])
}

#[test]
fn golden_trash_restore_missing_folder() {
    run_golden("trash_restore_missing_folder", |w| {
        let (_base, a, [one, two]) = folder_world(w);
        let trash_dir = w.dir("trash");
        let r = folder_trash::trash_folder_result_for_test(&w.conn, a, &trash_dir, &[]);
        w.step("trash folder A", r);
        let r = restore(w, one, &[]);
        w.step("restore one (folders recreated on disk and in the catalog)", r);
        let r = restore(w, two, &[]);
        w.step("restore two (joins the recreated folder)", r);
    });
}

#[test]
fn golden_trash_restore_missing_folder_refusals() {
    run_golden("trash_restore_missing_folder_refusals", |w| {
        let (base, a, [one, two]) = folder_world(w);
        let trash_dir = w.dir("trash");
        let r = folder_trash::trash_folder_result_for_test(&w.conn, a, &trash_dir, &[]);
        w.step("trash folder A", r);
        let r = restore(w, one, std::slice::from_ref(&base));
        w.step("restore into a protected location (refused, nothing created)", r);
        block_restore(w);
        let r = restore(w, one, &[]);
        w.step("DB update fails (created directories are removed again)", r);
        w.conn.execute_batch("DROP TRIGGER block_restore").unwrap();
        let r = restore(w, two, &[]);
        w.step("restore two", r);
    });
}

#[test]
fn golden_trash_restore_foreign_file_and_removed_row() {
    run_golden("trash_restore_foreign_file", |w| {
        let (_base, a, [one, two]) = folder_world(w);
        put(w, "catalog/A/B/foreign.txt", b"not ours");
        let trash_dir = w.dir("trash");
        let r = folder_trash::trash_folder_result_for_test(&w.conn, a, &trash_dir, &[]);
        w.step("trash folder A (a foreign file keeps the directory)", r);
        let r = restore(w, one, &[]);
        w.step("restore one (directory exists, folder row comes back)", r);
        // A folder row the user removed on purpose is not brought back by a plain restore.
        let (loose, _) = model(w, "elsewhere/x.3mf", None);
        let elsewhere = db::insert_folder_with_parent(&w.conn, "elsewhere", None, &w.dir("elsewhere").to_string_lossy()).unwrap();
        w.conn.execute("UPDATE files SET folder_id = ?1 WHERE id = ?2", rusqlite::params![elsewhere, loose]).unwrap();
        del(w, loose).unwrap();
        catalog_removal::remove_folder_with_conn(&w.conn, &elsewhere.to_string()).unwrap();
        let r = restore(w, loose, &[]);
        w.step("restore after the catalog folder was removed", r);
        let r = restore(w, two, &[]);
        w.step("restore two", r);
    });
}

#[test]
fn golden_trash_restore_top_level_folder() {
    run_golden("trash_restore_top_level_folder", |w| {
        let top = db::insert_folder_with_parent(&w.conn, "top", None, &w.dir("top").to_string_lossy()).unwrap();
        let sub = db::insert_folder_with_parent(&w.conn, "sub", Some(top), &w.dir("top/sub").to_string_lossy()).unwrap();
        let (m, _) = model(w, "top/sub/m.3mf", Some(sub));
        let trash_dir = w.dir("trash");
        let r = folder_trash::trash_folder_result_for_test(&w.conn, top, &trash_dir, &[]);
        w.step("trash the top-level folder (no catalog base set)", r);
        let r = restore(w, m, &[]);
        w.step("restore (outermost directory becomes a top-level folder row)", r);
    });
}

#[test]
fn golden_trash_folder_models() {
    run_golden("trash_folder_models", |w| {
        let (_base, a, [one, two]) = folder_world(w);
        let (failing, _) = model(w, "catalog/A/failing.3mf", None);
        w.conn.execute("UPDATE files SET folder_id = ?1 WHERE id = ?2", rusqlite::params![a, failing]).unwrap();
        block_soft_delete(w, failing);
        let trash_dir = w.dir("trash");
        let r = folder_trash::trash_folder_result_for_test(&w.conn, a, &trash_dir, &[]);
        w.step("trash folder A (one model cannot be moved)", r);
        unblock_soft_delete(w, failing);
        let r = folder_trash::trash_folder_result_for_test(&w.conn, a, &trash_dir, &[]);
        w.step("trash folder A again", r);
        let r = restore(w, two, &[]);
        w.step("restore two", r);
        let r = restore(w, one, &[]);
        w.step("restore one", r);
    });
}

#[test]
fn golden_trash_purge() {
    run_golden("trash_purge", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", Some(folder));
        companion(w, a, "pic.png");
        let (b, _) = model(w, "cat/b.3mf", Some(folder));
        let (c, _) = model(w, "cat/c.3mf", Some(folder));
        let (d, _) = model(w, "cat/d.3mf", Some(folder));
        companion(w, d, "d.png");
        let (soft, _) = model(w, "cat/soft.3mf", Some(folder));
        for id in [a, b, d] {
            del(w, id).unwrap();
        }
        trash::delete_missing_file_with_conn(&w.conn, soft).unwrap();
        let trash_dir = w.dir("trash");
        let r = trash::delete_file_permanently_with_conn(&w.conn, a, &trash_dir);
        w.step("delete for good with a companion in the trash", r);
        std::fs::remove_file(trash_dir.join(format!("{b}-b.3mf"))).unwrap();
        let r = trash::delete_file_permanently_with_conn(&w.conn, b, &trash_dir);
        w.step("delete for good with the trash file already gone", r);
        let r = trash::delete_file_permanently_with_conn(&w.conn, c, &trash_dir);
        w.step("delete for good a model that is not in the trash", r);
        let r = trash::delete_file_permanently_with_conn(&w.conn, 9999, &trash_dir);
        w.step("delete for good an unknown id", r);
        let r = trash::delete_file_permanently_with_conn(&w.conn, soft, &trash_dir);
        w.step("delete for good an entry without trash path", r);
        // A companion removed on its own from a model that stays in the catalog.
        let (e, _) = model(w, "cat/e.3mf", Some(folder));
        companion(w, e, "e.png");
        let row = db::companions::list_companions(&w.conn, e).unwrap()[0].id;
        let r = companions::remove_companion_with_conn(&w.conn, row, &trash_dir);
        w.step("remove a companion of an active model", r);
        let (f, _) = model(w, "cat/f.3mf", Some(folder));
        companion(w, f, "f.png");
        del(w, f).unwrap();
        let r = trash::empty_trash_with_conn(&w.conn, &trash_dir);
        w.step("empty the trash (models, their companions and the loose companion)", r);
        let r = trash::empty_trash_with_conn(&w.conn, &trash_dir);
        w.step("empty the empty trash", r);
    });
}

#[test]
fn golden_trash_startup_cleanup() {
    run_golden("trash_startup_cleanup", |w| {
        let folder = cat(w);
        let trash_dir = w.dir("trash");
        let (old, _) = model(w, "cat/old.3mf", Some(folder));
        companion(w, old, "old.png");
        let (fresh, _) = model(w, "cat/fresh.3mf", Some(folder));
        companion(w, fresh, "fresh.png");
        let (old_gone, _) = model(w, "cat/oldgone.3mf", Some(folder));
        let (old_soft, _) = model(w, "cat/oldsoft.3mf", Some(folder));
        for id in [old, fresh, old_gone] {
            del(w, id).unwrap();
        }
        std::fs::remove_file(trash_dir.join(format!("{old_gone}-oldgone.3mf"))).unwrap();
        trash::delete_missing_file_with_conn(&w.conn, old_soft).unwrap();
        age(w, old, &days_ago(8));
        age(w, old_gone, &days_ago(8));
        age(w, old_soft, &days_ago(8));
        age(w, fresh, &days_ago(6));
        // Companions removed on their own from models that stay in the catalog.
        let (keep, _) = model(w, "cat/keep.3mf", Some(folder));
        companion(w, keep, "old_alone.png");
        companion(w, keep, "fresh_alone.png");
        let rows = db::companions::list_companions(&w.conn, keep).unwrap();
        for (row, when) in rows.iter().zip([days_ago(8), days_ago(6)]) {
            companions::remove_companion_with_conn(&w.conn, row.id, &trash_dir).unwrap();
            w.conn.execute("UPDATE companion_files SET deleted_at = ?1 WHERE id = ?2", rusqlite::params![when, row.id]).unwrap();
        }
        trash::purge_expired_trash_on_startup(&w.conn, &trash_dir);
        w.step("startup clean-up (8 days go, 6 days stay)", ok(()));
        trash::purge_expired_trash_on_startup(&w.conn, &trash_dir);
        w.step("startup clean-up again (nothing left to do)", ok(()));
    });
}

/// Things whose error text or behaviour depends on the system's file rules: Linux only.
#[cfg(target_os = "linux")]
#[test]
fn golden_trash_linux_special() {
    run_golden("trash_linux_special", |w| {
        let folder = cat(w);
        let trash_dir = w.dir("trash");
        // The stored path is a directory.
        let dir_path = w.dir("cat/adir");
        let (id, _) = model(w, "cat/other.3mf", Some(folder));
        w.conn.execute("UPDATE files SET path = ?1, name = 'adir' WHERE id = ?2", rusqlite::params![dir_path.to_string_lossy(), id]).unwrap();
        let r = del(w, id);
        w.step("delete a model whose path is a directory", r);
        // The recorded spelling differs from the file on disk only in case (case-sensitive volume).
        let (cased, cased_path) = model(w, "cat/Real.3mf", Some(folder));
        std::fs::rename(&cased_path, w.root.join("cat/real.3mf")).unwrap();
        let r = del(w, cased);
        w.step("delete a model recorded as Real.3mf while real.3mf is on disk", r);
        let (twin, twin_path) = model(w, "cat/Twin.3mf", Some(folder));
        std::fs::rename(&twin_path, w.root.join("cat/twin.3mf")).unwrap();
        let fs = FaultFs::new().case_insensitive();
        let r = trash::delete_file_with_conn_fs(&fs, &w.conn, &load(w, twin), twin, &trash_dir);
        w.step("same on a simulated case-insensitive volume (the twin is found, the move fails)", r);
        w.step("calls", ok(calls(&fs)));
        // Restore: a file that differs in case is a different file here.
        let (m, _) = model(w, "cat/Model.3mf", Some(folder));
        del(w, m).unwrap();
        put(w, "cat/model.3mf", b"other file");
        let r = restore(w, m, &[]);
        w.step("restore Model.3mf while model.3mf exists (separate file on this system)", r);
        // Restore with a file where the directory should be.
        let (blocked, _) = model(w, "cat/blocked/b.3mf", Some(folder));
        del(w, blocked).unwrap();
        std::fs::remove_dir(w.root.join("cat/blocked")).unwrap();
        put(w, "cat/blocked", b"a file where the folder was");
        let r = restore(w, blocked, &[]);
        w.step("restore where the folder cannot be created", r);
        // Delete for good, empty and the startup clean-up with a trash path that is a non-empty directory.
        let mut stuck = Vec::new();
        for name in ["s1", "s2", "s3"] {
            let (id, _) = model(w, &format!("cat/{name}.3mf"), Some(folder));
            companion(w, id, &format!("{name}.png"));
            del(w, id).unwrap();
            let tp = trash_dir.join(format!("{id}-{name}.3mf"));
            std::fs::remove_file(&tp).unwrap();
            std::fs::create_dir(&tp).unwrap();
            std::fs::write(tp.join("inner"), b"x").unwrap();
            stuck.push(id);
        }
        let r = trash::delete_file_permanently_with_conn(&w.conn, stuck[0], &trash_dir);
        w.step("delete for good: trash path is a non-empty directory", r);
        let r = trash::empty_trash_with_conn(&w.conn, &trash_dir);
        w.step("empty the trash: entries that cannot be removed stay", r);
        age(w, stuck[1], &days_ago(8));
        age(w, stuck[2], &days_ago(8));
        trash::purge_expired_trash_on_startup(&w.conn, &trash_dir);
        w.step("startup clean-up: entries that cannot be removed stay", ok(()));
    });
}
