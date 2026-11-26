//! Golden scenarios of moving model files (issue #20, package P5): the single move, the batch
//! (limit 5000), every refusal and every roll-back branch, with the disk calls of `FaultFs`
//! logged per step. Recorded with the code as it was before the move flow went into
//! `katalog_core::file_move`; the golden files must not change with the move.
//! See `docs/plans/issue20-p5-zuschnitt.md`.
use super::files::{move_file_outcome_fs, move_files_to_folder_with, MoveFileResult, MAX_BATCH_MOVE_FILES};
use super::fs_boundary::{FaultFs, Op};
use super::golden_harness::{run_golden, World};
use super::*;
use std::io::ErrorKind::{AlreadyExists, PermissionDenied, Unsupported};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const STAMP: &str = "2026-01-01T00:00:00Z";

fn put(w: &World, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = w.root.join(crate::commands::golden_harness::nat(rel));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn folder(w: &World, name: &str) -> i64 {
    db::insert_folder_with_parent(&w.conn, name, None, &w.dir(name).to_string_lossy()).unwrap()
}

/// A model file in `<dir>/<name>` that the catalog knows (in `folder`).
fn model(w: &World, rel: &str, folder: Option<i64>) -> (i64, PathBuf) {
    let path = put(w, rel, b"model");
    (db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), folder).unwrap(), path)
}

fn companion(w: &World, model: i64, name: &str) {
    let source = put(w, &format!("outside/{name}"), PNG);
    companions::add_companion_with_conn(&w.conn, model, &source, &[]).unwrap();
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

fn listing(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        if std::fs::symlink_metadata(&path).unwrap().is_dir() {
            out.push(format!("{rel}/"));
            listing(&path, base, out);
        } else {
            out.push(rel);
        }
    }
}

/// One move step: outcome with refusal code, the disk calls, and the state afterwards
/// (file tree below `cat` plus path/folder of the model and its companions).
fn mv(w: &mut World, label: &str, fs: &FaultFs, id: i64, target: Option<i64>, sensitive: &[PathBuf]) {
    let outcome = move_file_outcome_fs(fs, &w.conn, id, target, sensitive);
    w.step(label, ok(outcome));
    w.step(&format!("{label}: disk calls"), ok(calls(fs)));
    after(w, label, id);
}

fn after(w: &mut World, label: &str, id: i64) {
    let mut tree = Vec::new();
    listing(&w.root, &w.root, &mut tree);
    let row = db::get_file(&w.conn, id).unwrap().map(|f| serde_json::json!({ "path": f.path, "folderId": f.folder_id }));
    let comps: Vec<_> = db::companions::list_companions(&w.conn, id)
        .unwrap()
        .iter()
        .map(|c| serde_json::json!({ "id": c.id, "path": c.path, "trashed": c.deleted_at.is_some() }))
        .collect();
    w.step(&format!("{label}: state"), ok(serde_json::json!({ "tree": tree, "file": row, "companions": comps })));
}

#[test]
fn golden_file_move_single() {
    run_golden("file_move_single", |w| {
        let src = folder(w, "src");
        let dst = folder(w, "dst");
        let (a, _) = model(w, "src/a.3mf", Some(src));
        let (b, _) = model(w, "src/b.3mf", Some(src));
        model(w, "dst/b.3mf", Some(dst));
        let fs = FaultFs::new();
        mv(w, "move a to dst", &fs, a, Some(dst), &[]);
        let fs = FaultFs::new();
        mv(w, "move a to dst again (already there)", &fs, a, Some(dst), &[]);
        let fs = FaultFs::new();
        mv(w, "move a with no folder (stays, catalog folder cleared)", &fs, a, None, &[]);
        let fs = FaultFs::new();
        mv(w, "move a to dst once more (lies in the directory, assigned only)", &fs, a, Some(dst), &[]);
        let fs = FaultFs::new();
        mv(w, "move b to dst (name taken on disk)", &fs, b, Some(dst), &[]);
        let fs = FaultFs::new();
        mv(w, "unknown file", &fs, 999, Some(dst), &[]);
        let fs = FaultFs::new();
        mv(w, "unknown folder", &fs, b, Some(12345), &[]);
        let (t, _) = model(w, "src/t.3mf", Some(src));
        db::soft_delete_file(&w.conn, t, Some("x"), STAMP).unwrap();
        let fs = FaultFs::new();
        mv(w, "file in the trash", &fs, t, Some(dst), &[]);
        // A file lying in the target's directory but filed under another folder.
        let (d, _) = model(w, "dst/d.3mf", Some(src));
        let fs = FaultFs::new();
        mv(w, "file in the target directory under another folder", &fs, d, Some(dst), &[]);

        // Sensitive paths: target folder, source, and the new path.
        let secret = w.dir("secret");
        let hidden = folder(w, "secret");
        let (e, _) = model(w, "src/e.3mf", Some(src));
        let fs = FaultFs::new();
        mv(w, "target folder is protected", &fs, e, Some(hidden), std::slice::from_ref(&secret));
        let (s, _) = model(w, "secret/s.3mf", Some(dst));
        let fs = FaultFs::new();
        mv(w, "source is protected", &fs, s, Some(dst), std::slice::from_ref(&secret));
        let fs = FaultFs::new();
        mv(w, "new path is protected", &fs, e, Some(dst), &[w.root.join(crate::commands::golden_harness::nat("dst/e.3mf"))]);

        // Invalid stored paths.
        let root_file = db::test_insert_minimal_file(&w.conn, "/", None).unwrap();
        let fs = FaultFs::new();
        mv(w, "stored path has no parent (no folder)", &fs, root_file, None, &[]);
        let fs = FaultFs::new();
        mv(w, "stored path has no file name", &fs, root_file, Some(dst), &[]);

        // Disk and catalog out of step.
        let ghost = db::insert_folder_with_parent(&w.conn, "ghost", None, &w.root.join("ghost").to_string_lossy()).unwrap();
        let fs = FaultFs::new();
        mv(w, "target folder missing on disk", &fs, e, Some(ghost), &[]);
        let gone = db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("src/gone.3mf")).to_string_lossy(), Some(src)).unwrap();
        let fs = FaultFs::new();
        mv(w, "source missing on disk", &fs, gone, Some(dst), &[]);
        w.dir("src/dirmodel.3mf");
        let dir_row = db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("src/dirmodel.3mf")).to_string_lossy(), Some(src)).unwrap();
        let fs = FaultFs::new();
        mv(w, "source is a directory", &fs, dir_row, Some(dst), &[]);
        let (g, _) = model(w, "src/g.3mf", Some(src));
        db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("dst/g.3mf")).to_string_lossy(), Some(dst)).unwrap();
        let fs = FaultFs::new();
        mv(w, "name taken only in the catalog (file moved back)", &fs, g, Some(dst), &[]);

        // Companions travel along; the emptied extras folder goes away.
        let (foo, _) = model(w, "src/Foo.3mf", Some(src));
        companion(w, foo, "pic.png");
        companion(w, foo, "pic2.png");
        let fs = FaultFs::new();
        mv(w, "model with companions", &fs, foo, Some(dst), &[]);
        let (bar, _) = model(w, "src/Bar.3mf", Some(src));
        companion(w, bar, "bar.png");
        put(w, "dst/Bar_extras/bar.png", PNG);
        let fs = FaultFs::new();
        mv(w, "companion name taken in the new extras folder (move undone)", &fs, bar, Some(dst), &[]);

        // The command wrappers.
        let (c, _) = model(w, "src/c.3mf", Some(src));
        let r = super::files::move_file_to_folder_with_conn(&w.conn, c, Some(dst), &[]);
        w.step("wrapper: move c", r);
        let r = super::files::move_file_to_folder_with_conn(&w.conn, c, Some(src), &[]);
        w.step("wrapper: move c back", r);
        let r = super::files::move_file_to_folder_with_conn(&w.conn, 999, Some(src), &[]);
        w.step("wrapper: unknown file", r);
    });
}

fn mixed_batch_world(w: &mut World) -> (i64, i64, Vec<String>) {
    let src = folder(w, "src");
    let dst = folder(w, "dst");
    let (ok1, _) = model(w, "src/ok1.3mf", Some(src));
    let (ok2, _) = model(w, "src/ok2.3mf", Some(src));
    let (dup, _) = model(w, "src/dup.3mf", Some(src));
    model(w, "dst/dup.3mf", Some(dst));
    let (here, _) = model(w, "dst/here.3mf", Some(dst));
    let (loose, _) = model(w, "dst/loose.3mf", None);
    let gone = db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("src/gone.3mf")).to_string_lossy(), Some(src)).unwrap();
    let (trashed, _) = model(w, "src/trashed.3mf", Some(src));
    db::soft_delete_file(&w.conn, trashed, Some("x"), STAMP).unwrap();
    let (taken, _) = model(w, "src/taken.3mf", Some(src));
    db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("dst/taken.3mf")).to_string_lossy(), Some(dst)).unwrap();
    let (foo, _) = model(w, "src/Foo.3mf", Some(src));
    companion(w, foo, "pic.png");
    let ids = [ok1, ok2, dup, here, loose, gone, trashed, taken, foo, ok1, 9999]
        .iter()
        .map(|i| i.to_string())
        .chain(["abc".to_string(), "".to_string()])
        .collect();
    (src, dst, ids)
}

fn batch_summary(r: &CmdResult<Vec<MoveFileResult>>) -> serde_json::Value {
    match r {
        Ok(rows) => {
            let mut by_status = std::collections::BTreeMap::<String, usize>::new();
            for row in rows {
                *by_status.entry(format!("{}/{}", row.status, row.reason.as_deref().unwrap_or("-"))).or_default() += 1;
            }
            serde_json::json!({ "len": rows.len(), "byStatusAndReason": by_status })
        }
        Err(e) => serde_json::json!({ "err": e }),
    }
}

#[test]
fn golden_file_move_batch() {
    run_golden("file_move_batch", |w| {
        let (src, dst, ids) = mixed_batch_world(w);
        let r = move_files_to_folder_with(|| Ok(&w.conn), &ids, dst, &[]);
        w.step("mixed batch", r);
        after(w, "mixed batch", 1);
        // An undo call with the reported old places: moving back by folder.
        let r = move_files_to_folder_with(|| Ok(&w.conn), &["1".to_string(), "2".to_string()], src, &[]);
        w.step("batch back to src", r);
        after(w, "batch back to src", 1);

        // Whole-call failures, before any file is touched.
        let r = move_files_to_folder_with(|| Ok(&w.conn), &["1".to_string()], 12345, &[]);
        w.step("unknown target folder", r);
        let r = move_files_to_folder_with(|| Ok(&w.conn), &["1".to_string()], dst, &[w.root.join("dst")]);
        w.step("protected target folder", r);
        let ghost = db::insert_folder_with_parent(&w.conn, "ghost", None, &w.root.join("ghost").to_string_lossy()).unwrap();
        let r = move_files_to_folder_with(|| Ok(&w.conn), &["1".to_string()], ghost, &[]);
        w.step("target folder missing on disk", r);
        let r = move_files_to_folder_with(|| -> CmdResult<&Connection> { Err("locked".into()) }, &["1".to_string()], dst, &[]);
        w.step("the lock fails at once", r);
        let mut calls_left = 2;
        let r = move_files_to_folder_with(
            || {
                calls_left -= 1;
                if calls_left >= 0 { Ok(&w.conn) } else { Err("locked later".into()) }
            },
            &["1".to_string(), "2".to_string()],
            dst,
            &[],
        );
        w.step("the lock fails for the second file (first stays moved)", r);
        after(w, "lock fails later", 1);
        let r = move_files_to_folder_with(|| Ok(&w.conn), &[], dst, &[]);
        w.step("empty batch", r);

        // The limit: exactly the limit passes, one more is refused before any lock.
        let at_limit: Vec<String> = (0..MAX_BATCH_MOVE_FILES).map(|_| "x".to_string()).collect();
        let r = move_files_to_folder_with(|| Ok(&w.conn), &at_limit, dst, &[]);
        w.step("exactly the limit (all ids invalid)", ok(batch_summary(&r)));
        let over: Vec<String> = (0..=MAX_BATCH_MOVE_FILES).map(|i| i.to_string()).collect();
        let r = move_files_to_folder_with(|| -> CmdResult<&Connection> { unreachable!("refused before the first lock") }, &over, 1, &[]);
        w.step("one more than the limit", r);
        w.step("limit value", ok(MAX_BATCH_MOVE_FILES));
    });
}

#[test]
fn golden_file_move_faults() {
    run_golden("file_move_faults", |w| {
        let src = folder(w, "src");
        let dst = folder(w, "dst");
        let new = |w: &World, name: &str, comp: bool| {
            let (id, _) = model(w, &format!("src/{name}"), Some(src));
            if comp {
                companion(w, id, &format!("{name}.png"));
            }
            id
        };
        let unsupported = |mut fs: FaultFs, n: usize| {
            for nth in 1..=n {
                fs = fs.fail(Op::HardLink, nth, Unsupported);
            }
            fs
        };

        let id = new(w, "f01.3mf", false);
        mv(w, "link and remove", &FaultFs::new(), id, Some(dst), &[]);
        let id = new(w, "f02.3mf", false);
        mv(w, "other drive: link refused, copy and remove", &unsupported(FaultFs::new(), 1), id, Some(dst), &[]);
        let id = new(w, "f03.3mf", false);
        let fs = unsupported(FaultFs::new(), 1).fail(Op::RemoveFile, 1, PermissionDenied);
        mv(w, "copy done, removing the source fails (copy removed)", &fs, id, Some(dst), &[]);
        let id = new(w, "f04.3mf", false);
        mv(w, "link done, removing the source fails (link removed)", &FaultFs::new().fail(Op::RemoveFile, 1, PermissionDenied), id, Some(dst), &[]);
        let id = new(w, "f05.3mf", false);
        mv(w, "link says the name exists", &FaultFs::new().fail(Op::HardLink, 1, AlreadyExists), id, Some(dst), &[]);
        let id = new(w, "f06.3mf", false);
        let fs = unsupported(FaultFs::new(), 1).fail(Op::CopyNew, 1, PermissionDenied);
        mv(w, "other drive: the copy itself fails", &fs, id, Some(dst), &[]);
        let id = new(w, "f07.3mf", true);
        let fs = FaultFs::new().fail_on(Op::HardLink, Some("f07_extras"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("f07_extras"), 1, PermissionDenied);
        mv(w, "a companion cannot move (model moved back)", &fs, id, Some(dst), &[]);
        let id = new(w, "f08.3mf", true);
        let fs = FaultFs::new()
            .fail(Op::HardLink, 2, PermissionDenied)
            .fail(Op::CopyNew, 1, PermissionDenied)
            .fail(Op::HardLink, 3, PermissionDenied)
            .fail(Op::CopyNew, 2, PermissionDenied);
        mv(w, "a companion cannot move and the move back fails too", &fs, id, Some(dst), &[]);

        // The catalog update fails after the move.
        let id = new(w, "f09.3mf", true);
        w.conn.execute_batch(&format!("CREATE TRIGGER block_move BEFORE UPDATE OF path ON files WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'blocked'); END;")).unwrap();
        mv(w, "catalog update fails, companions and model are put back", &FaultFs::new(), id, Some(dst), &[]);
        w.conn.execute_batch("DROP TRIGGER block_move").unwrap();
        let id = new(w, "f10.3mf", true);
        w.conn.execute_batch(&format!("CREATE TRIGGER block_move BEFORE UPDATE OF path ON files WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'blocked'); END;")).unwrap();
        let fs = FaultFs::new().fail_on(Op::HardLink, Some("f10.3mf"), 2, PermissionDenied).fail_on(Op::CopyNew, Some("f10.3mf"), 1, PermissionDenied);
        mv(w, "catalog update fails and the move back fails too", &fs, id, Some(dst), &[]);
        w.conn.execute_batch("DROP TRIGGER block_move").unwrap();
        let id = new(w, "f11.3mf", false);
        w.conn.execute_batch(&format!("CREATE TRIGGER block_move BEFORE UPDATE OF path ON files WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'blocked'); END;")).unwrap();
        mv(w, "other drive: catalog update fails, the move back is a copy", &unsupported(FaultFs::new(), 4), id, Some(dst), &[]);
        w.conn.execute_batch("DROP TRIGGER block_move").unwrap();
        let id = new(w, "f12.3mf", false);
        db::test_insert_minimal_file(&w.conn, &w.root.join(crate::commands::golden_harness::nat("dst/f12.3mf")).to_string_lossy(), Some(dst)).unwrap();
        mv(w, "path owned by another catalog row (name exists)", &FaultFs::new(), id, Some(dst), &[]);

        // Simulated case-insensitive volume.
        let id = new(w, "f13.3mf", false);
        put(w, "dst/F13.3MF", b"other");
        mv(w, "name differs only in case on a case-insensitive volume", &FaultFs::new().case_insensitive(), id, Some(dst), &[]);
        let id = new(w, "f14.3mf", true);
        let fs = unsupported(FaultFs::new(), 6).fail_on(Op::CopyNew, Some("f14_extras"), 1, PermissionDenied);
        mv(w, "other drive: the companion copy fails, the model comes back by copy", &fs, id, Some(dst), &[]);
        let id = new(w, "f15.3mf", true);
        let fs = unsupported(FaultFs::new(), 6);
        mv(w, "other drive: model and companions travel by copy", &fs, id, Some(dst), &[]);

        // Lookups that fail count as "missing".
        let id = new(w, "f16.3mf", false);
        mv(w, "reading the target folder is refused", &FaultFs::new().fail(Op::Metadata, 1, PermissionDenied), id, Some(dst), &[]);
        mv(w, "reading the source file is refused", &FaultFs::new().fail(Op::Metadata, 2, PermissionDenied), id, Some(dst), &[]);
        let id = new(w, "f17.3mf", true);
        mv(w, "emptied extras folder cannot be removed (ignored)", &FaultFs::new().fail(Op::RemoveDir, 1, PermissionDenied), id, Some(dst), &[]);
    });
}

#[cfg(unix)]
#[test]
fn golden_file_move_symlink() {
    run_golden("file_move_symlink", |w| {
        let src = folder(w, "src");
        let dst = folder(w, "dst");
        let real = put(w, "outside/real.3mf", b"real model");
        let link = w.root.join(crate::commands::golden_harness::nat("src/linked.3mf"));
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let id = db::test_insert_minimal_file(&w.conn, &link.to_string_lossy(), Some(src)).unwrap();
        mv(w, "symlinked model (no hard link, copy)", &FaultFs::new(), id, Some(dst), &[]);
        let dangling = w.root.join(crate::commands::golden_harness::nat("src/dangling.3mf"));
        std::os::unix::fs::symlink(w.root.join(crate::commands::golden_harness::nat("outside/nothing.3mf")), &dangling).unwrap();
        let id = db::test_insert_minimal_file(&w.conn, &dangling.to_string_lossy(), Some(src)).unwrap();
        mv(w, "dangling symlink (source missing)", &FaultFs::new(), id, Some(dst), &[]);
    });
}
