//! Golden scenarios of renaming and moving a catalog folder (issue #20, package P6): every
//! refusal, the success with nested folders, models and companion files (also the trashed ones,
//! the R5 question), the case-only rename through an intermediate name, and a fault in each
//! step with the disk calls of `FaultFs` logged. Recorded with the code as it was before the
//! folder flow went into `katalog_core::folder_move`; the golden files must not change with
//! the move. See `docs/plans/issue20-p6-zuschnitt.md`.
use super::folders::{move_folder_with_conn_fs, rename_folder_with_conn_fs};
use super::fs_boundary::{FaultFs, Op};
use super::golden_harness::{run_golden, World};
use super::*;
use std::io::ErrorKind::{AlreadyExists, PermissionDenied, Unsupported};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const STAMP: &str = "2026-01-01T00:00:00Z";
const TMP_MARK: &str = ".3mf-rename-tmp-";

/// The intermediate name of a case-only rename carries the process id and a counter; both
/// become placeholders so the snapshot does not depend on the run.
fn scrub(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(TMP_MARK) {
        out.push_str(&rest[..at + TMP_MARK.len()]);
        let after = &rest[at + TMP_MARK.len()..];
        let digits = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
        let (pid, tail) = after.split_at(digits);
        let tail = tail.strip_prefix('-').filter(|_| !pid.is_empty());
        match tail {
            Some(tail) => {
                let counter = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
                out.push_str("<PID>-<N>");
                rest = &tail[counter..];
            }
            None => rest = after,
        }
    }
    out.push_str(rest);
    out
}

fn scrub_err(mut e: CmdError) -> CmdError {
    e.message = scrub(&e.message);
    e.params = e.params.iter().map(|p| scrub(p)).collect();
    e
}

fn put(w: &World, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = w.root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn ok<T>(value: T) -> CmdResult<T> {
    Ok(value)
}

fn calls(fs: &FaultFs) -> Vec<String> {
    fs.calls()
        .iter()
        .map(|(op, a, b)| scrub(&format!("{op:?} {}{}", a.display(), b.as_ref().map(|b| format!(" -> {}", b.display())).unwrap_or_default())))
        .collect()
}

fn listing(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        let rel = scrub(&path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
        if std::fs::symlink_metadata(&path).unwrap().is_dir() {
            out.push(format!("{rel}/"));
            listing(&path, base, out);
        } else {
            out.push(rel);
        }
    }
}

fn rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

/// The state after a step: folder rows, file rows and companion rows (with their trash paths)
/// and the file tree. The tables are dumped again at the end of the scenario.
fn state(w: &mut World, label: &str) {
    let mut tree = Vec::new();
    listing(&w.root, &w.root, &mut tree);
    let folders = rows(&w.conn, "SELECT id || '|' || name || '|' || ifnull(parent_id, '-') || '|' || path FROM folders ORDER BY id");
    let files = rows(
        &w.conn,
        "SELECT id || '|' || path || '|' || ifnull(folder_id, '-') || '|' || ifnull(trash_path, '-') FROM files ORDER BY id",
    );
    let companions = rows(
        &w.conn,
        "SELECT id || '|' || file_id || '|' || path || '|' || (deleted_at IS NOT NULL) || '|' || ifnull(trash_path, '-') FROM companion_files ORDER BY id",
    );
    let value = serde_json::json!({ "tree": tree, "folders": folders, "files": files, "companions": companions });
    w.step(&format!("{label}: state"), ok(value));
}

fn companion(w: &World, model: i64, name: &str) -> i64 {
    let source = put(w, &format!("outside/{name}"), PNG);
    companions::add_companion_with_conn(&w.conn, model, &source, &[]).unwrap().id
}

/// The tree every scenario starts from (all below `cat`, the catalog base directory):
/// `Deko` (model a + companion; `sub` with model b + one companion in the trash and one live;
/// a model in the trash), `Deko2` (similar name, model c + companion) and `Target`.
struct Fx {
    cat: i64,
    deko: i64,
    sub: i64,
    deko2: i64,
    target: i64,
}

fn folder(w: &World, name: &str, parent: Option<i64>, rel: &str) -> i64 {
    db::insert_folder_with_parent(&w.conn, name, parent, &w.dir(rel).to_string_lossy()).unwrap()
}

fn model(w: &World, rel: &str, folder: i64) -> i64 {
    let path = put(w, rel, b"model");
    db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), Some(folder)).unwrap()
}

fn fixture(w: &World) -> Fx {
    db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.join("cat").to_string_lossy()).unwrap();
    let cat = folder(w, "cat", None, "cat");
    let deko = folder(w, "Deko", Some(cat), "cat/Deko");
    let sub = folder(w, "sub", Some(deko), "cat/Deko/sub");
    let deko2 = folder(w, "Deko2", Some(cat), "cat/Deko2");
    let target = folder(w, "Target", Some(cat), "cat/Target");
    let a = model(w, "cat/Deko/a.3mf", deko);
    companion(w, a, "a.png");
    let b = model(w, "cat/Deko/sub/b.3mf", sub);
    let live = companion(w, b, "live.png");
    let _ = live;
    let gone = companion(w, b, "gone.png");
    let trash = w.dir("trash");
    companions::remove_companion_with_conn(&w.conn, gone, &trash).unwrap();
    let t = model(w, "cat/Deko/t.3mf", deko);
    let trashed = put(w, "trash/t.3mf", b"model");
    db::soft_delete_file(&w.conn, t, Some(&trashed.to_string_lossy()), STAMP).unwrap();
    let c = model(w, "cat/Deko2/c.3mf", deko2);
    companion(w, c, "c.png");
    Fx { cat, deko, sub, deko2, target }
}

fn rename(w: &mut World, label: &str, fs: &FaultFs, id: i64, name: &str, sensitive: &[PathBuf]) {
    let result = rename_folder_with_conn_fs(fs, &w.conn, id, name.into(), sensitive).map_err(scrub_err);
    w.step(label, result);
    w.step(&format!("{label}: disk calls"), ok(calls(fs)));
    state(w, label);
}

fn mv(w: &mut World, label: &str, fs: &FaultFs, id: i64, target: Option<i64>, sensitive: &[PathBuf]) {
    let result = move_folder_with_conn_fs(fs, &w.conn, id, target, sensitive).map_err(scrub_err);
    w.step(label, result);
    w.step(&format!("{label}: disk calls"), ok(calls(fs)));
    state(w, label);
}

/// Fails every update of the companion rows (the last statement of the path update).
fn block_companion_update(w: &World) {
    w.conn
        .execute_batch("CREATE TRIGGER block_companion_paths BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
        .unwrap();
}

#[test]
fn golden_folder_rename_rules() {
    run_golden("folder_rename_rules", |w| {
        let fx = fixture(w);
        state(w, "start");
        for (label, name) in [
            ("empty name", ""),
            ("slash in the name", "a/b"),
            ("dot dot", ".."),
            ("trailing dot", "Neu."),
            ("trailing space", "Neu "),
            ("reserved Windows name", "CON"),
            ("forbidden character", "a:b"),
        ] {
            rename(w, label, &FaultFs::new(), fx.deko, name, &[]);
        }
        rename(w, "unknown folder", &FaultFs::new(), 9999, "Neu", &[]);
        rename(w, "same name is a no-op", &FaultFs::new(), fx.deko, "Deko", &[]);
        rename(w, "the catalog base folder cannot be renamed", &FaultFs::new(), fx.cat, "cat2", &[]);
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.join("cat/Deko/sub").to_string_lossy()).unwrap();
        rename(w, "an ancestor of the catalog base folder cannot be renamed", &FaultFs::new(), fx.deko, "Neu", &[]);
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.join("cat").to_string_lossy()).unwrap();
        rename(w, "new path is protected", &FaultFs::new(), fx.deko, "Neu", &[w.root.join("cat/Neu")]);
        rename(w, "folder contains a protected directory", &FaultFs::new(), fx.deko, "Neu", &[w.root.join("cat/Deko/sub")]);
        rename(w, "name taken by a folder on disk", &FaultFs::new(), fx.deko, "Target", &[]);
        put(w, "cat/Datei", b"x");
        rename(w, "name taken by a file on disk", &FaultFs::new(), fx.deko, "Datei", &[]);
        // The catalog knows a folder with the new path, the disk does not: the DB refuses (UNIQUE
        // path) after the rename, which is undone.
        folder(w, "Phantom", Some(fx.cat), "elsewhere");
        w.conn.execute("UPDATE folders SET path = ?1 WHERE name = 'Phantom'", [w.root.join("cat/Phantom").to_string_lossy().to_string()]).unwrap();
        rename(w, "name taken only in the catalog (disk rename undone)", &FaultFs::new(), fx.deko2, "Phantom", &[]);
        // The folder is gone from the disk.
        std::fs::remove_dir_all(w.root.join("cat/Target")).unwrap();
        rename(w, "source missing on disk", &FaultFs::new(), fx.target, "Ziel", &[]);
        rename(w, "success: nested folders, models and companion files", &FaultFs::new(), fx.deko, "Decor", &[]);
        rename(w, "success: name with non-ASCII characters", &FaultFs::new(), fx.sub, "Übersicht", &[]);
        rename(w, "the renamed folder again under its old name", &FaultFs::new(), fx.deko, "Deko", &[]);
    });
}

#[test]
fn golden_folder_rename_faults() {
    run_golden("folder_rename_faults", |w| {
        let fx = fixture(w);
        rename(w, "rename refused by the disk", &FaultFs::new().fail(Op::Rename, 1, PermissionDenied), fx.deko, "Decor", &[]);
        rename(w, "rename refused (other drive)", &FaultFs::new().fail(Op::Rename, 1, Unsupported), fx.deko, "Decor", &[]);
        rename(w, "target check fails", &FaultFs::new().fail(Op::SymlinkMetadata, 1, PermissionDenied), fx.deko, "Decor", &[]);
        rename(w, "target appears after the check", &FaultFs::new().fail(Op::Rename, 1, AlreadyExists), fx.deko, "Decor", &[]);
        block_companion_update(w);
        rename(w, "catalog update fails (folder renamed back)", &FaultFs::new(), fx.deko, "Decor", &[]);
        rename(
            w,
            "catalog update fails and the roll-back is refused",
            &FaultFs::new().fail(Op::Rename, 2, PermissionDenied),
            fx.deko,
            "Decor",
            &[],
        );
        // The folder now lies under the new name on disk, the catalog still has the old one.
        rename(w, "next try finds the folder missing", &FaultFs::new(), fx.deko, "Decor2", &[]);
    });
}

/// Gives the folder left under its intermediate name its real name back.
fn put_back_intermediate(w: &World, name: &str) {
    let leftover: Vec<_> = std::fs::read_dir(w.root.join("cat"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(TMP_MARK))
        .collect();
    for p in leftover {
        std::fs::rename(p, w.root.join("cat").join(name)).unwrap();
    }
}

#[test]
fn golden_folder_rename_case_only() {
    run_golden("folder_rename_case_only", |w| {
        let fx = fixture(w);
        let models = folder(w, "Models", Some(fx.cat), "cat/Models");
        let part = put(w, "cat/Models/part.stl", b"original");
        db::test_insert_minimal_file(&w.conn, &part.to_string_lossy(), Some(models)).unwrap();
        let insensitive = || FaultFs::new().case_insensitive();
        rename(w, "case-only rename through the intermediate name", &insensitive(), models, "models", &[]);
        rename(w, "and back", &insensitive(), models, "Models", &[]);
        rename(w, "first step refused", &insensitive().fail(Op::Rename, 1, PermissionDenied), models, "models", &[]);
        rename(w, "second step refused (put back)", &insensitive().fail(Op::Rename, 2, PermissionDenied), models, "models", &[]);
        rename(
            w,
            "second step and put-back refused (data stays under the intermediate name)",
            &insensitive().fail(Op::Rename, 2, PermissionDenied).fail(Op::Rename, 3, PermissionDenied),
            models,
            "models",
            &[],
        );
        // The intermediate folder is the catalog's folder now only on disk: put it back by hand.
        put_back_intermediate(w, "Models");
        block_companion_update(w);
        // No companion below Models: the update goes through, so use the other folder for the DB fault.
        rename(w, "catalog update fails after a case-only rename (back through the intermediate name)", &insensitive(), fx.deko2, "deko2", &[]);
        rename(
            w,
            "catalog update fails, the way back fails in its second step",
            &insensitive().fail(Op::Rename, 4, PermissionDenied).fail(Op::Rename, 5, PermissionDenied),
            fx.deko2,
            "deko2",
            &[],
        );
        // The snapshot's tree must not carry the process id of the intermediate name.
        put_back_intermediate(w, "Deko2");
    });
}

#[test]
fn golden_folder_move_rules() {
    run_golden("folder_move_rules", |w| {
        let fx = fixture(w);
        state(w, "start");
        mv(w, "no target folder", &FaultFs::new(), fx.deko, None, &[]);
        mv(w, "unknown folder", &FaultFs::new(), 9999, Some(fx.target), &[]);
        mv(w, "unknown target folder", &FaultFs::new(), fx.deko, Some(9999), &[]);
        mv(w, "into itself", &FaultFs::new(), fx.deko, Some(fx.deko), &[]);
        mv(w, "into a descendant", &FaultFs::new(), fx.deko, Some(fx.sub), &[]);
        mv(w, "the catalog base folder cannot be moved", &FaultFs::new(), fx.cat, Some(fx.target), &[]);
        // The base folder lies below Deko: moving Deko would move the base folder with it.
        let base = |w: &World, rel: &str| db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.join(rel).to_string_lossy()).unwrap();
        base(w, "cat/Deko/sub");
        mv(w, "an ancestor of the catalog base folder cannot be moved", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        w.conn.execute("DELETE FROM app_settings WHERE key = ?1", [folders::SETTING_CATALOG_BASE_DIR]).unwrap();
        let nameless = db::insert_folder_with_parent(&w.conn, "nameless", Some(fx.cat), "/").unwrap();
        mv(w, "stored path has no folder name (no base folder configured)", &FaultFs::new(), nameless, Some(fx.target), &[]);
        base(w, "cat");
        mv(w, "new path is protected", &FaultFs::new(), fx.deko, Some(fx.target), &[w.root.join("cat/Target/Deko")]);
        mv(w, "folder contains a protected directory", &FaultFs::new(), fx.deko, Some(fx.target), &[w.root.join("cat/Deko/sub")]);
        mv(w, "already below that parent", &FaultFs::new(), fx.deko, Some(fx.cat), &[]);
        w.dir("cat/Target/Deko");
        mv(w, "name taken by a folder in the target", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        std::fs::remove_dir(w.root.join("cat/Target/Deko")).unwrap();
        put(w, "cat/Target/Deko", b"file");
        mv(w, "name taken by a file in the target", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        std::fs::remove_file(w.root.join("cat/Target/Deko")).unwrap();
        // The catalog lists `Inner` as a sibling of `Deko2`, but it really lies inside it.
        let inner = folder(w, "Inner", Some(fx.cat), "cat/Deko2/Inner");
        mv(w, "stale catalog nesting (real locations compared)", &FaultFs::new(), fx.deko2, Some(inner), &[]);
        mv(w, "real parent equals the target", &FaultFs::new(), fx.sub, Some(fx.deko), &[]);
        let ghost = db::insert_folder_with_parent(&w.conn, "ghost", Some(fx.cat), &w.root.join("cat/ghost").to_string_lossy()).unwrap();
        mv(w, "target missing on disk", &FaultFs::new(), fx.deko, Some(ghost), &[]);
        let vanished = db::insert_folder_with_parent(&w.conn, "vanished", Some(fx.cat), &w.root.join("cat/vanished").to_string_lossy()).unwrap();
        mv(w, "source missing on disk", &FaultFs::new(), vanished, Some(fx.target), &[]);
        mv(w, "success: nested folders, models and companion files", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        mv(w, "success: back to the parent", &FaultFs::new(), fx.deko, Some(fx.cat), &[]);
        mv(w, "success: a child folder to another parent", &FaultFs::new(), fx.sub, Some(fx.deko2), &[]);
    });
}

#[test]
fn golden_folder_move_faults() {
    run_golden("folder_move_faults", |w| {
        let fx = fixture(w);
        mv(w, "move refused by the disk", &FaultFs::new().fail(Op::Rename, 1, PermissionDenied), fx.deko, Some(fx.target), &[]);
        mv(w, "move refused (other drive)", &FaultFs::new().fail(Op::Rename, 1, Unsupported), fx.deko, Some(fx.target), &[]);
        mv(w, "reading the source folder is refused", &FaultFs::new().fail(Op::Metadata, 1, PermissionDenied), fx.deko, Some(fx.target), &[]);
        mv(w, "reading the target folder is refused", &FaultFs::new().fail(Op::Metadata, 2, PermissionDenied), fx.deko, Some(fx.target), &[]);
        block_companion_update(w);
        mv(w, "catalog update fails (folder moved back)", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        mv(
            w,
            "catalog update fails and the roll-back is refused",
            &FaultFs::new().fail(Op::Rename, 2, PermissionDenied),
            fx.deko,
            Some(fx.target),
            &[],
        );
        mv(w, "next try finds the folder missing", &FaultFs::new(), fx.deko, Some(fx.deko2), &[]);
    });
}

#[test]
fn golden_folder_move_faults_resolving() {
    run_golden("folder_move_faults_resolving", |w| {
        let fx = fixture(w);
        // A folder that cannot be resolved skips the comparison of the real locations: the move
        // goes ahead (and, for a stale nesting, would not be caught).
        mv(w, "resolving the source fails (real locations not compared)", &FaultFs::new().fail(Op::Canonicalize, 1, PermissionDenied), fx.sub, Some(fx.deko2), &[]);
        mv(w, "resolving the target fails (real locations not compared)", &FaultFs::new().fail(Op::Canonicalize, 2, PermissionDenied), fx.sub, Some(fx.target), &[]);
        let inner = folder(w, "Inner", Some(fx.cat), "cat/Deko2/Inner");
        mv(w, "stale nesting not caught when resolving the target fails", &FaultFs::new().fail(Op::Canonicalize, 2, PermissionDenied), fx.deko2, Some(inner), &[]);
        // The current parent cannot be resolved: the "already there" check is skipped.
        mv(w, "resolving the current parent fails", &FaultFs::new().fail(Op::Canonicalize, 3, PermissionDenied), fx.target, Some(fx.cat), &[]);
    });
}

/// R5 (plan, "Unklar"): does the path update of a folder rename/move reach the companion files
/// that are in the trash (`companion-<id>-<name>` below the app's trash folder)? The row's
/// `path` (where a restore puts it) follows; `trash_path` (where the file lies now) is not
/// touched. Both are in the snapshot, before and after, for the rename and for the move.
#[test]
fn golden_folder_paths_trashed_companions_r5() {
    run_golden("folder_paths_trashed_companions_r5", |w| {
        let fx = fixture(w);
        state(w, "start (companion gone.png of b is in the trash)");
        rename(w, "rename Deko to Decor", &FaultFs::new(), fx.deko, "Decor", &[]);
        mv(w, "move Decor into Target", &FaultFs::new(), fx.deko, Some(fx.target), &[]);
        // Restoring the trashed companion puts it where the folder is now.
        let gone = rows(&w.conn, "SELECT CAST(id AS TEXT) FROM companion_files WHERE deleted_at IS NOT NULL")[0].parse::<i64>().unwrap();
        let r = companions::restore_companion_with_conn(&w.conn, gone, &[]);
        w.step("restore the trashed companion after the move", r.map(|c| c.path));
        state(w, "restore");
    });
}

#[cfg(unix)]
#[test]
fn golden_folder_move_symlinks() {
    run_golden("folder_move_symlinks", |w| {
        let fx = fixture(w);
        // A folder that is a symlink to a directory outside the catalog.
        let real = w.dir("outside/real");
        let link = w.root.join("cat/Linked");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let linked = db::insert_folder_with_parent(&w.conn, "Linked", Some(fx.cat), &link.to_string_lossy()).unwrap();
        mv(w, "move the symlinked folder", &FaultFs::new(), linked, Some(fx.target), &[]);
        rename(w, "rename the symlinked folder", &FaultFs::new(), linked, "Verlinkt", &[]);
        // A catalog folder whose path is a symlink to its own parent.
        let loop_link = w.root.join("cat/Target/Up");
        std::os::unix::fs::symlink(w.root.join("cat"), &loop_link).unwrap();
        let up = db::insert_folder_with_parent(&w.conn, "Up", Some(fx.target), &loop_link.to_string_lossy()).unwrap();
        mv(w, "move a folder into a symlink that leads back to its parent", &FaultFs::new(), fx.deko, Some(up), &[]);
        let dangling = w.root.join("cat/Dangling");
        std::os::unix::fs::symlink(w.root.join("outside/nothing"), &dangling).unwrap();
        let dang = db::insert_folder_with_parent(&w.conn, "Dangling", Some(fx.cat), &dangling.to_string_lossy()).unwrap();
        mv(w, "move a dangling symlink (source is no folder)", &FaultFs::new(), dang, Some(fx.target), &[]);
        rename(w, "rename a dangling symlink", &FaultFs::new(), dang, "Kaputt", &[]);
    });
}
