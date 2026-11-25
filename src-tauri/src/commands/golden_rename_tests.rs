//! Golden scenarios of renaming a file (issue #20, package P4): the checks, the plain and the
//! case-only rename (through an intermediate name), the companion files that follow, both
//! rollback branches, and the date rename that shares the flow. Recorded with the code as it
//! was before the rename moved into `katalog_core::file_rename`; the golden files must not
//! change with the move. See `docs/plans/issue20-p4-zuschnitt.md`.
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

fn rec(row: &db::models::CompanionRecord) -> serde_json::Value {
    serde_json::json!({
        "id": row.id, "fileId": row.file_id, "name": row.name, "path": row.path, "kind": row.kind,
        "sizeBytes": row.size_bytes, "trashed": row.deleted_at.is_some(), "trashPath": row.trash_path,
    })
}

fn cat(w: &World) -> i64 {
    db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap()
}

fn model(w: &World, rel: &str, folder: i64) -> (i64, PathBuf) {
    let path = put(w, rel, b"model");
    (db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), Some(folder)).unwrap(), path)
}

fn add(w: &World, model: i64, name: &str) {
    let source = put(w, &format!("outside/{name}"), PNG);
    companions::add_companion_with_conn(&w.conn, model, &source, &[]).unwrap();
}

fn rows_of(w: &mut World, label: &str, id: i64) {
    let r = companions::list_companions_with_conn(&w.conn, id);
    w.step(label, r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
}

fn calls(fs: &FaultFs) -> Vec<String> {
    fs.calls()
        .iter()
        .map(|(op, a, b)| scrub(&format!("{op:?} {}{}", a.display(), b.as_ref().map(|b| format!(" -> {}", b.display())).unwrap_or_default())))
        .collect()
}

/// Names in a directory of the scenario (sorted, intermediate names scrubbed).
fn ls(w: &mut World, label: &str, rel: &str) {
    let mut names: Vec<String> = std::fs::read_dir(w.root.join(rel)).unwrap().map(|e| scrub(&e.unwrap().file_name().to_string_lossy())).collect();
    names.sort();
    w.step(label, Ok::<_, CmdError>(names));
}

fn ok<T>(value: T) -> CmdResult<T> {
    Ok(value)
}

fn file_row(w: &mut World, label: &str, id: i64) {
    let row = db::get_file(&w.conn, id).unwrap().map(|f| (f.name, f.path));
    w.step(label, ok(row));
}

fn rename_step(w: &mut World, label: &str, fs: &FaultFs, id: i64, name: &str, case_insensitive: bool) {
    let r = files::rename_file_with_conn_fs_case(fs, &w.conn, id, name.into(), &[], case_insensitive).map_err(scrub_err);
    w.step(label, r);
}

#[test]
fn golden_file_rename_rules() {
    run_golden("file_rename_rules", |w| {
        let folder = cat(w);
        let (a, _) = model(w, "cat/a.3mf", folder);
        let (_taken, _) = model(w, "cat/taken.3mf", folder);
        w.dir("cat/adir.3mf");
        let r = rename_file_with_conn(&w.conn, a, "b.3mf".into(), &[]);
        w.step("rename a -> b", r);
        let r = rename_file_with_conn(&w.conn, a, "b.3mf".into(), &[]);
        w.step("same name again", r);
        for (label, name) in [
            ("empty name", String::new()),
            ("blank name", "   ".into()),
            ("slash", "x/y.3mf".into()),
            ("colon", "x:y.3mf".into()),
            ("control character", "x\u{1}y.3mf".into()),
            ("too long", format!("{}.3mf", "a".repeat(300))),
            ("dot", ".".into()),
            ("dot dot", "..".into()),
            ("ends with a dot", "name.".into()),
            ("ends with a space", "name ".into()),
            ("windows reserved", "CON.3mf".into()),
        ] {
            let r = rename_file_with_conn(&w.conn, a, name, &[]);
            w.step(label, r);
        }
        let r = rename_file_with_conn(&w.conn, 999, "x.3mf".into(), &[]);
        w.step("unknown id", r);
        let (trashed, _) = model(w, "cat/t.3mf", folder);
        db::soft_delete_file(&w.conn, trashed, Some("x"), STAMP).unwrap();
        let r = rename_file_with_conn(&w.conn, trashed, "t2.3mf".into(), &[]);
        w.step("model in the trash", r);
        let r = rename_file_with_conn(&w.conn, a, "c.3mf".into(), &[w.root.join("cat")]);
        w.step("protected folder", r);
        let r = rename_file_with_conn(&w.conn, a, "taken.3mf".into(), &[]);
        w.step("name of another file", r);
        let r = rename_file_with_conn(&w.conn, a, "adir.3mf".into(), &[]);
        w.step("name of a directory", r);
        let (gone, gone_path) = model(w, "cat/gone.3mf", folder);
        std::fs::remove_file(&gone_path).unwrap();
        let r = rename_file_with_conn(&w.conn, gone, "gone2.3mf".into(), &[]);
        w.step("file missing on disk", r);
        let r = rename_file_with_conn(&w.conn, a, "B.3mf".into(), &[]);
        w.step("b -> B (case only, free name on this volume)", r);
        file_row(w, "row of a", a);
    });
}

#[test]
fn golden_file_rename_companions() {
    run_golden("file_rename_companions", |w| {
        let folder = cat(w);
        // 1: the companions follow into the folder named after the new name.
        let (m1, _) = model(w, "cat/Foo.3mf", folder);
        add(w, m1, "a.png");
        add(w, m1, "b.png");
        let fs = FaultFs::new();
        rename_step(w, "rename Foo -> Bar", &fs, m1, "Bar.3mf", false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Bar", m1);
        // 2: a name is taken in the new folder: the file goes back.
        let (m2, _) = model(w, "cat/Baz.3mf", folder);
        add(w, m2, "a.png");
        add(w, m2, "b.png");
        put(w, "cat/Qux_extras/b.png", b"in the way");
        let fs = FaultFs::new();
        rename_step(w, "rename Baz -> Qux, second name taken", &fs, m2, "Qux.3mf", false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Baz", m2);
        file_row(w, "row of Baz", m2);
        // 3: the same, but putting the file back fails too.
        let (m3, _) = model(w, "cat/Moo.3mf", folder);
        add(w, m3, "a.png");
        add(w, m3, "b.png");
        put(w, "cat/Boo_extras/b.png", b"in the way");
        let fs = FaultFs::new().fail_on(Op::HardLink, Some("Boo.3mf"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("Boo.3mf"), 1, PermissionDenied);
        rename_step(w, "rename Moo -> Boo, second name taken and the way back locked", &fs, m3, "Boo.3mf", false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Moo", m3);
        file_row(w, "row of Moo", m3);
        // 4: the catalog row cannot be updated: files and companions go back.
        let (m4, _) = model(w, "cat/Db.3mf", folder);
        add(w, m4, "a.png");
        w.conn
            .execute_batch(&format!("CREATE TRIGGER block_rename BEFORE UPDATE ON files WHEN NEW.id = {m4} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"))
            .unwrap();
        let fs = FaultFs::new();
        rename_step(w, "rename Db -> Dc, row cannot be updated", &fs, m4, "Dc.3mf", false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Db", m4);
        file_row(w, "row of Db", m4);
        // 5: the same, and the way back for the file is locked.
        let (m5, _) = model(w, "cat/Lk.3mf", folder);
        w.conn
            .execute_batch(&format!("CREATE TRIGGER block_rename_lk BEFORE UPDATE ON files WHEN NEW.id = {m5} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"))
            .unwrap();
        let fs = FaultFs::new().fail_on(Op::HardLink, Some("Ll.3mf"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("Ll.3mf"), 1, PermissionDenied);
        rename_step(w, "rename Lk -> Ll, row cannot be updated and the way back locked", &fs, m5, "Ll.3mf", false);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of Lk", m5);
        // 6: a model without companions and a model whose extras folder is a file.
        let (m6, _) = model(w, "cat/Plain.3mf", folder);
        let fs = FaultFs::new();
        rename_step(w, "rename Plain -> Plain2", &fs, m6, "Plain2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        let (m7, _) = model(w, "cat/Fl.3mf", folder);
        add(w, m7, "a.png");
        put(w, "cat/Fm_extras", b"a file");
        let fs = FaultFs::new();
        rename_step(w, "rename Fl -> Fm, extras name is a file", &fs, m7, "Fm.3mf", false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Fl", m7);
    });
}

#[test]
fn golden_file_rename_faults() {
    run_golden("file_rename_faults", |w| {
        let folder = cat(w);
        let (f1, _) = model(w, "cat/lock.3mf", folder);
        let fs = FaultFs::new().fail(Op::HardLink, 1, PermissionDenied).fail(Op::CopyNew, 1, PermissionDenied);
        rename_step(w, "locked (link and copy refused)", &fs, f1, "lock2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of lock", f1);
        let (f2, _) = model(w, "cat/vol.3mf", folder);
        let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported);
        rename_step(w, "hard link not possible, copy path", &fs, f2, "vol2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of vol", f2);
        let (f3, _) = model(w, "cat/rm.3mf", folder);
        let fs = FaultFs::new().fail(Op::RemoveFile, 1, PermissionDenied);
        rename_step(w, "source cannot be removed after the link", &fs, f3, "rm2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of rm", f3);
        let (f4, _) = model(w, "cat/race.3mf", folder);
        let fs = FaultFs::new().fail(Op::HardLink, 1, AlreadyExists);
        rename_step(w, "target appears after the check", &fs, f4, "race2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        let (f5, _) = model(w, "cat/meta.3mf", folder);
        let fs = FaultFs::new().fail_on(Op::SymlinkMetadata, Some("meta2.3mf"), 1, PermissionDenied);
        rename_step(w, "metadata of the target unreadable", &fs, f5, "meta2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        let (f6, _) = model(w, "cat/cp.3mf", folder);
        let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported).fail(Op::CopyNew, 1, PermissionDenied);
        rename_step(w, "hard link not possible and copy refused", &fs, f6, "cp2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of cp", f6);
        let (f7, _) = model(w, "cat/ok.3mf", folder);
        let fs = FaultFs::new();
        rename_step(w, "plain success", &fs, f7, "ok2.3mf", false);
        w.step("calls", ok(calls(&fs)));
        ls(w, "cat", "cat");
    });
}

// The recorded call trace is the Unix one: on Windows the collision check also canonicalizes both
// paths, so the trace differs by design. The case rules themselves run in the katalog-core tests.
#[cfg(not(windows))]
#[test]
fn golden_file_rename_case_only() {
    run_golden("file_rename_case_only", |w| {
        let folder = cat(w);
        // 1: the rename goes through an intermediate name; the companion folder follows.
        let (m1, _) = model(w, "cat/foo.3mf", folder);
        add(w, m1, "a.png");
        add(w, m1, "b.png");
        let fs = FaultFs::new().case_insensitive();
        rename_step(w, "foo -> Foo", &fs, m1, "Foo.3mf", true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Foo", m1);
        file_row(w, "row of Foo", m1);
        ls(w, "cat", "cat");
        // 2: the first step is refused.
        let (m2, _) = model(w, "cat/one.3mf", folder);
        let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 1, PermissionDenied);
        rename_step(w, "one -> One, first step refused", &fs, m2, "One.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of one", m2);
        ls(w, "cat", "cat");
        // 3: the second step is locked; the intermediate goes back to the old name.
        let (m3, _) = model(w, "cat/two.3mf", folder);
        let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 2, PermissionDenied);
        rename_step(w, "two -> Two, second step locked", &fs, m3, "Two.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of two", m3);
        ls(w, "cat", "cat");
        // 4: the second step and the way back are locked; the data stays under the intermediate name.
        let (m4, p4) = model(w, "cat/three.3mf", folder);
        let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 2, PermissionDenied).fail(Op::Rename, 3, PermissionDenied);
        rename_step(w, "three -> Three, second step and way back locked", &fs, m4, "Three.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of three", m4);
        ls(w, "cat", "cat");
        let left = std::fs::read_dir(w.root.join("cat"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.to_string_lossy().contains(TMP_MARK))
            .expect("the intermediate file stays");
        std::fs::rename(left, p4).unwrap();
        // 5: the companion folder cannot be renamed: the model goes back.
        let (m5, _) = model(w, "cat/bar.3mf", folder);
        add(w, m5, "a.png");
        let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 3, PermissionDenied);
        rename_step(w, "bar -> Bar, extras folder refused", &fs, m5, "Bar.3mf", true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of bar", m5);
        file_row(w, "row of bar", m5);
        ls(w, "cat", "cat");
        // 6: the row cannot be updated: the folder and the file go back through the intermediate name.
        let (m6, _) = model(w, "cat/db.3mf", folder);
        add(w, m6, "a.png");
        w.conn
            .execute_batch(&format!("CREATE TRIGGER block_case BEFORE UPDATE ON files WHEN NEW.id = {m6} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"))
            .unwrap();
        let fs = FaultFs::new().case_insensitive();
        rename_step(w, "db -> DB, row cannot be updated", &fs, m6, "DB.3mf", true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of db", m6);
        file_row(w, "row of db", m6);
        ls(w, "cat", "cat");
        // 7: the same without companions, and the way back is locked.
        let (m7, _) = model(w, "cat/dbf.3mf", folder);
        w.conn
            .execute_batch(&format!("CREATE TRIGGER block_case_f BEFORE UPDATE ON files WHEN NEW.id = {m7} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"))
            .unwrap();
        let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 3, PermissionDenied);
        rename_step(w, "dbf -> Dbf, row cannot be updated and the way back locked", &fs, m7, "Dbf.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of dbf", m7);
        ls(w, "cat", "cat");
        // 8: a different file already has the name in another case.
        let (m8, _) = model(w, "cat/Other.3mf", folder);
        let (_m9, _) = model(w, "cat/other-name.3mf", folder);
        let fs = FaultFs::new().case_insensitive();
        rename_step(w, "Other -> other-name (distinct entry)", &fs, m8, "other-name.3mf", true);
        w.step("calls", ok(calls(&fs)));
        // 9: the directory listing for the stored-spelling check fails.
        let (m10, _) = model(w, "cat/list.3mf", folder);
        let fs = FaultFs::new().case_insensitive().fail(Op::ReadDir, 1, PermissionDenied);
        rename_step(w, "list -> List, directory unreadable", &fs, m10, "List.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of list", m10);
    });
}

/// Needs symlinks (not available to the Windows runner), so it has its own golden file.
#[cfg(unix)]
#[test]
fn golden_file_rename_symlinks() {
    run_golden("file_rename_symlinks", |w| {
        let folder = cat(w);
        let (real, real_path) = model(w, "cat/real.3mf", folder);
        let link = w.root.join("cat/link.3mf");
        std::os::unix::fs::symlink(&real_path, &link).unwrap();
        let link_id = db::test_insert_minimal_file(&w.conn, &link.to_string_lossy(), Some(folder)).unwrap();
        // The target is a symlink onto the file itself: a distinct entry, so a collision.
        std::os::unix::fs::symlink(&real_path, w.root.join("cat/alias.3mf")).unwrap();
        let r = rename_file_with_conn(&w.conn, real, "alias.3mf".into(), &[]);
        w.step("rename onto a symlink to itself", r);
        // A dangling symlink is a collision too.
        std::os::unix::fs::symlink(w.root.join("cat/nowhere"), w.root.join("cat/dangling.3mf")).unwrap();
        let r = rename_file_with_conn(&w.conn, real, "dangling.3mf".into(), &[]);
        w.step("rename onto a dangling symlink", r);
        // A symlink model is renamed as a link, not by moving its content.
        let r = rename_file_with_conn(&w.conn, link_id, "link2.3mf".into(), &[]);
        w.step("rename a symlink model", r);
        // Two names of the same file are distinct entries.
        std::fs::hard_link(&real_path, w.root.join("cat/twin.3mf")).unwrap();
        let r = rename_file_with_conn(&w.conn, real, "twin.3mf".into(), &[]);
        w.step("rename onto a hard link twin", r);
        // A symlink model with a case-only change goes through the intermediate name.
        let fs = FaultFs::new().case_insensitive();
        rename_step(w, "symlink model link2 -> Link2 (case only)", &fs, link_id, "Link2.3mf", true);
        w.step("calls", ok(calls(&fs)));
        file_row(w, "row of the link", link_id);
        ls(w, "cat", "cat");
    });
}

// ---- date rename: the same flow behind the confirmed list -----------------------------------

/// Fixed modification time (noon UTC); the prefix of the new names still depends on the time zone
/// of the machine, so the snapshot only ever sees `DATEPFX_` (see `anonymise_dates`).
fn stamp() -> std::time::SystemTime {
    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_760_011_200)
}

fn prefix() -> String {
    import_rename::date_prefix(chrono::DateTime::<chrono::Local>::from(stamp()))
}

fn plain_dates(text: &str) -> String {
    text.replace(&prefix(), "DATEPFX_")
}

fn date_setup(w: &World) -> i64 {
    let folder = cat(w);
    db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.to_string_lossy()).unwrap();
    folder
}

fn dated(w: &World, rel: &str, folder: i64) -> (i64, String) {
    let path = put(w, rel, b"model");
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(stamp()).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    (db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), Some(folder)).unwrap(), format!("{}{name}", prefix()))
}

fn date_apply(w: &mut World, label: &str, id: i64, new_name: &str) {
    let item = DateRenameItem { id: id.to_string(), new_name: new_name.into() };
    let r = date_rename::apply_with_conn(&w.conn, vec![item], &[]).map_err(scrub_err);
    w.step(label, r);
}

fn date_file_row(w: &mut World, label: &str, id: i64) {
    let row = db::get_file(&w.conn, id).unwrap().map(|f| (plain_dates(&f.name), plain_dates(&f.path)));
    w.step(label, ok(row));
}

fn date_rows(w: &mut World, label: &str, id: i64) {
    let rows: Vec<serde_json::Value> = db::companions::list_active_companions(&w.conn, id)
        .unwrap()
        .iter()
        .map(|r| serde_json::from_str(&plain_dates(&rec(r).to_string())).unwrap())
        .collect();
    w.step(label, ok(rows));
}

/// The names of the scenario carry the date of the machine's time zone: swaps it for a fixed
/// text in the catalog and on disk, so the tables and the tree are the same everywhere.
fn anonymise_dates(w: &World) {
    w.conn.execute_batch("DROP TRIGGER IF EXISTS block_date;").unwrap();
    let p = prefix();
    w.conn
        .execute("UPDATE files SET name = REPLACE(name, ?1, 'DATEPFX_'), path = REPLACE(path, ?1, 'DATEPFX_')", [&p])
        .unwrap();
    w.conn
        .execute("UPDATE companion_files SET path = REPLACE(path, ?1, 'DATEPFX_')", [&p])
        .unwrap();
    for entry in std::fs::read_dir(w.root.join("cat")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.contains(&p) {
            std::fs::rename(&path, path.with_file_name(name.replace(&p, "DATEPFX_"))).unwrap();
        }
    }
}

#[test]
fn golden_date_rename_apply() {
    run_golden("date_rename_apply", |w| {
        let folder = date_setup(w);
        let (m1, n1) = dated(w, "cat/plain.3mf", folder);
        date_apply(w, "plain", m1, &n1);
        date_file_row(w, "row of plain", m1);
        let (m2, n2) = dated(w, "cat/withcomp.3mf", folder);
        add(w, m2, "a.png");
        add(w, m2, "b.png");
        date_apply(w, "with companions", m2, &n2);
        date_rows(w, "rows of withcomp", m2);
        date_file_row(w, "row of withcomp", m2);
        let (m3, n3) = dated(w, "cat/blocked.3mf", folder);
        add(w, m3, "a.png");
        std::fs::write(w.root.join("cat").join(format!("{}_extras", n3.trim_end_matches(".3mf"))), b"a file").unwrap();
        date_apply(w, "companion folder name is a file", m3, &n3);
        date_rows(w, "rows of blocked", m3);
        date_file_row(w, "row of blocked", m3);
        let (m4, n4) = dated(w, "cat/dbfail.3mf", folder);
        add(w, m4, "a.png");
        w.conn
            .execute_batch(&format!("CREATE TRIGGER block_date BEFORE UPDATE ON files WHEN NEW.id = {m4} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"))
            .unwrap();
        date_apply(w, "row cannot be updated", m4, &n4);
        date_rows(w, "rows of dbfail", m4);
        date_file_row(w, "row of dbfail", m4);
        let (m5, n5) = dated(w, "cat/race.3mf", folder);
        let racing = w.root.join("cat").join(&n5);
        import_rename::set_race_hook(Some(Box::new(move |stage, path| {
            if stage == "before-file-rename" && path == racing {
                std::fs::write(path, b"someone else").unwrap();
            }
        })));
        date_apply(w, "target created after the check", m5, &n5);
        import_rename::set_race_hook(None);
        date_file_row(w, "row of race", m5);
        date_apply(w, "second run of the first", m1, &n1);
        anonymise_dates(w);
    });
}
