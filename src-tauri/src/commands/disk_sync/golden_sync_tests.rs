//! Golden scenarios of the disk sync (issue #20, package P8): the plan of a scan (new, missing,
//! renamed, copies, trash block, companion folder, missing folders), the apply step with every
//! refusal, the remembered copies (`disk_sync_known_copies`), folders that follow their files, a
//! fault in step n through `FaultFs`, the case-insensitive comparison and (Unix) symlinks. Each
//! step writes the scan or apply JSON (or the error JSON), the log lines without their source
//! location and, with `FaultFs`, the number of disk calls per operation; a scan after the apply
//! shows what is left. Recorded with the code as it was before the sync went into
//! `katalog_core::disk_sync`; the golden files must not change with the move. See
//! `docs/plans/issue20-p8-zuschnitt.md`.
use super::super::fs_boundary::{FaultFs, Op};
use super::super::golden_harness::{run_golden, World};
use super::super::test_log::capture;
use super::*;
use std::collections::BTreeMap;
use std::io::ErrorKind::PermissionDenied;

const STAMP: &str = "2026-01-01T00:00:00Z";

/// A small valid OBJ whose content (and hash) differs with `n`.
fn obj(n: u32) -> String {
    format!("v 0 0 0\nv {n} 0 0\nv 0 {n} 0\nf 1 2 3\n")
}

fn put(w: &World, rel: &str, content: &str) -> PathBuf {
    w.file(rel, content)
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn id_of(w: &World, rel: &str) -> i64 {
    w.conn.query_row("SELECT id FROM files WHERE path = ?1", [s(&w.root.join(crate::commands::golden_harness::nat(rel)))], |r| r.get(0)).unwrap()
}

/// The real import reader without the preview images: their PNG size depends on the platform.
fn golden_read(path: &Path) -> CmdResult<db::models::NewFile> {
    read_model_with_hash(path).map(|mut file| {
        file.thumbnail_png = None;
        file.render_snapshot_png = None;
        file
    })
}

/// Log texts end in `(file.rs:line)`; the location is not part of the contract.
fn strip_location(text: &str) -> String {
    match text.rfind(" (") {
        Some(at) if text.ends_with(')') && text[at..].contains(".rs:") => text[..at].to_string(),
        _ => text.to_string(),
    }
}

fn logged<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let mut out = None;
    let records = capture(|| out = Some(f()));
    let lines = records.into_iter().map(|(level, target, text)| format!("{level} {target}: {}", strip_location(&text))).collect();
    (out.unwrap(), lines)
}

/// Disk calls per operation; the order of `read_dir` depends on the file system, the counts do not.
fn op_counts(fs: &FaultFs) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for (op, _, _) in fs.calls() {
        *counts.entry(format!("{op:?}")).or_insert(0) += 1;
    }
    counts
}

fn record<T: Serialize>(w: &mut World, label: &str, result: &SyncResult<T>, logs: Vec<String>, fs: &FaultFs) {
    let mut value = match result {
        Ok(v) => serde_json::json!({ "value": v }),
        Err(e) => serde_json::json!({ "syncError": e }),
    };
    value["logs"] = serde_json::json!(logs);
    value["fsCalls"] = serde_json::json!(op_counts(fs));
    w.step(label, Ok::<_, CmdError>(value));
}

fn scan_step(w: &mut World, label: &str, fs: &FaultFs, case_insensitive: bool) -> DiskScan {
    let (result, logs) = logged(|| scan_with_fs(fs, &w.conn, &mut |dir| read_directory_fs(fs, dir), case_insensitive, &|p| hash_for_match_fs(fs, p)));
    record(w, label, &result, logs, fs);
    result.unwrap_or_default()
}

fn apply_step(w: &mut World, label: &str, fs: &FaultFs, selection: DiskSelection, case_insensitive: bool) {
    let (result, logs) = logged(|| apply_with_fs(fs, &mut w.conn, selection, golden_read, case_insensitive));
    record(w, label, &result, logs, fs);
}

/// A user who takes over everything the scan offers (copies included).
fn everything(scan: &DiskScan, remove_missing: bool) -> DiskSelection {
    DiskSelection {
        new_paths: scan
            .new_folders
            .iter()
            .chain(&scan.new_files)
            .chain(scan.duplicate_files.iter().map(|d| &d.entry))
            .map(|e| e.path.clone())
            .collect(),
        missing_ids: scan.missing_files.iter().filter_map(|e| e.id.clone()).collect(),
        remove_missing,
        renames: scan.renamed_files.iter().map(|r| RenameChoice { id: r.id.clone(), new_path: r.new_path.clone() }).collect(),
        ..Default::default()
    }
}

fn rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

/// Folder, file and remembered-copy rows after a step (all tables are dumped at the end too).
fn state(w: &mut World, label: &str) {
    let folders = rows(&w.conn, "SELECT id || '|' || name || '|' || ifnull(parent_id, '-') || '|' || path FROM folders ORDER BY id");
    let files = rows(
        &w.conn,
        "SELECT id || '|' || name || '|' || path || '|' || ifnull(folder_id, '-') || '|' || (deleted_at IS NOT NULL) || '|' || ifnull(trash_path, '-') FROM files ORDER BY id",
    );
    let copies = rows(&w.conn, "SELECT path || '|' || size_bytes FROM disk_sync_known_copies ORDER BY path");
    let value = serde_json::json!({ "folders": folders, "files": files, "knownCopies": copies });
    w.step(&format!("{label}: state"), Ok::<_, CmdError>(value));
}

/// A catalog root `models` with the given files on disk, all taken over by a first sync.
fn catalogue(w: &mut World, files: &[(&str, String)], case_insensitive: bool) -> PathBuf {
    let root = w.dir("models");
    db::ensure_folder_path(&w.conn, &root, &root).unwrap();
    for (rel, content) in files {
        put(w, &format!("models/{rel}"), content);
    }
    let first = scan_step(w, "first scan", &FaultFs::new(), case_insensitive);
    apply_step(w, "take over everything", &FaultFs::new(), everything(&first, false), case_insensitive);
    root
}

#[test]
fn golden_disk_sync_plan() {
    run_golden("disk_sync_plan", |w| {
        let files: Vec<(&str, String)> = vec![
            ("keep.obj", obj(1)),
            ("gone.obj", obj(2)),
            ("old.obj", obj(3)),
            ("orig.obj", obj(4)),
            ("Deko/d.obj", obj(5)),
            ("Weg/w.obj", obj(6)),
            ("Weg/tief/x.obj", obj(7)),
            ("t.obj", obj(8)),
            ("m.obj", obj(9)),
            ("broken.obj", "kein modell".to_string()),
        ];
        let root = catalogue(w, &files, false);
        state(w, "after the first sync");
        // A trash row whose file is still on disk blocks the path.
        let t = id_of(w, "models/t.obj");
        db::soft_delete_file(&w.conn, t, Some(&s(&w.root.join(crate::commands::golden_harness::nat("trash/t.obj")))), STAMP).unwrap();
        // The companion folder of a catalog model is not scanned.
        let extras = katalog_core::companions::extras_dir_for(&root.join("m.obj")).unwrap();
        let extras_rel = extras.strip_prefix(&w.root).unwrap().join("inside.obj");
        put(w, &extras_rel.to_string_lossy(), &obj(10));
        std::fs::remove_file(root.join("gone.obj")).unwrap();
        std::fs::rename(root.join("old.obj"), root.join("renamed.obj")).unwrap();
        put(w, "models/copy.obj", &obj(4));
        std::fs::remove_dir_all(root.join("Weg")).unwrap();
        put(w, "models/fresh/new.obj", &obj(11));
        put(w, "models/fresh/deeper/n2.obj", &obj(12));
        put(w, "models/notes.txt", "kein Modellformat");
        let plan = scan_step(w, "scan after the changes", &FaultFs::new(), false);
        apply_step(w, "apply everything, missing entries removed", &FaultFs::new(), everything(&plan, true), false);
        state(w, "after apply");
        scan_step(w, "scan after apply", &FaultFs::new(), false);
    });
}

#[test]
fn golden_disk_sync_known_copies() {
    run_golden("disk_sync_known_copies", |w| {
        let root = catalogue(w, &[("orig.obj", obj(1))], false);
        let copy = s(&put(w, "models/copy.obj", &obj(1)));
        let copy2 = s(&put(w, "models/copy2.obj", &obj(1)));
        scan_step(w, "copies offered", &FaultFs::new(), false);
        let decline = DiskSelection { declined_copies: vec![copy.clone(), copy2.clone()], ..Default::default() };
        apply_step(w, "decline both copies", &FaultFs::new(), decline, false);
        state(w, "after declining");
        scan_step(w, "remembered copies are not offered", &FaultFs::new(), false);
        let decline_again = DiskSelection { declined_copies: vec![copy2.clone(), s(&root.join("unknown.obj"))], ..Default::default() };
        apply_step(w, "decline a remembered copy and an unknown path", &FaultFs::new(), decline_again, false);
        std::fs::write(&copy, format!("{}# geaendert\n", obj(1))).unwrap();
        scan_step(w, "changed copy is a new file again", &FaultFs::new(), false);
        state(w, "after the change");
        std::fs::remove_file(root.join("orig.obj")).unwrap();
        scan_step(w, "original gone: the remembered copy is offered as its rename", &FaultFs::new(), false);
        let chosen_and_declined = DiskSelection { new_paths: vec![copy.clone()], declined_copies: vec![copy.clone()], ..Default::default() };
        apply_step(w, "a chosen path is never remembered", &FaultFs::new(), chosen_and_declined, false);
        let plan = scan_step(w, "rename still offered", &FaultFs::new(), false);
        apply_step(w, "take over the rename", &FaultFs::new(), everything(&plan, false), false);
        state(w, "after the rename");
        put(w, "models/copy3.obj", &obj(1));
        let plan = scan_step(w, "a new copy of the renamed model", &FaultFs::new(), false);
        let declined: Vec<String> = plan.duplicate_files.iter().map(|d| d.entry.path.clone()).collect();
        apply_step(w, "decline it", &FaultFs::new(), DiskSelection { declined_copies: declined, ..Default::default() }, false);
        std::fs::remove_file(root.join("copy3.obj")).unwrap();
        scan_step(w, "a remembered copy that is gone is forgotten", &FaultFs::new(), false);
        state(w, "end");
    });
}

#[test]
fn golden_disk_sync_emptied_folders() {
    run_golden("disk_sync_emptied_folders", |w| {
        let files: Vec<(&str, String)> = vec![
            ("Alt/a.obj", obj(1)),
            ("Alt/sub/b.obj", obj(2)),
            ("P/x.obj", obj(3)),
            ("P/y.obj", obj(4)),
            ("M/m.obj", obj(5)),
            ("Ziel/z.obj", obj(6)),
            ("Basis/q.obj", obj(7)),
            ("Bleibt/r.obj", obj(8)),
        ];
        let root = catalogue(w, &files, false);
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &s(&root.join("Basis"))).unwrap();
        state(w, "before");
        std::fs::rename(root.join("Alt"), root.join("Neu")).unwrap();
        std::fs::create_dir(root.join("Q")).unwrap();
        std::fs::rename(root.join(crate::commands::golden_harness::nat("P/x.obj")), root.join(crate::commands::golden_harness::nat("Q/x.obj"))).unwrap();
        std::fs::rename(root.join("M"), root.join(crate::commands::golden_harness::nat("Ziel/M"))).unwrap();
        std::fs::rename(root.join("Basis"), root.join("Basis2")).unwrap();
        // All files moved, but the folder itself is still there: its row stays.
        std::fs::create_dir(root.join("Bleibt2")).unwrap();
        std::fs::rename(root.join(crate::commands::golden_harness::nat("Bleibt/r.obj")), root.join(crate::commands::golden_harness::nat("Bleibt2/r.obj"))).unwrap();
        let plan = scan_step(w, "renames found", &FaultFs::new(), false);
        apply_step(w, "take over the renames", &FaultFs::new(), everything(&plan, false), false);
        state(w, "after apply");
        scan_step(w, "scan after apply", &FaultFs::new(), false);
    });
}

#[test]
fn golden_disk_sync_refusals() {
    run_golden("disk_sync_refusals", |w| {
        let files: Vec<(&str, String)> = vec![
            ("a.obj", obj(1)),
            ("c.obj", obj(3)),
            ("d.obj", obj(4)),
            ("Weg/w.obj", obj(5)),
            ("Weg/unter/u.obj", obj(6)),
            ("Basis/b.obj", obj(7)),
        ];
        let root = catalogue(w, &files, false);
        let loose = put(w, "outside/loose.obj", &obj(8));
        db::test_insert_minimal_file(&w.conn, &s(&loose), None).unwrap();
        let fresh = FaultFs::new;

        let stale = DiskSelection { new_paths: vec![s(&root.join("none.obj"))], ..Default::default() };
        apply_step(w, "new path the scan does not offer", &fresh(), stale, false);
        let stale = DiskSelection { renames: vec![RenameChoice { id: "1".into(), new_path: s(&root.join("x.obj")) }], ..Default::default() };
        apply_step(w, "rename the scan does not offer", &fresh(), stale, false);
        let stale = DiskSelection { missing_ids: vec!["999".into()], remove_missing: true, ..Default::default() };
        apply_step(w, "remove a row that is not missing", &fresh(), stale, false);

        std::fs::create_dir(root.join("Prot")).unwrap();
        std::fs::rename(root.join("a.obj"), root.join(crate::commands::golden_harness::nat("Prot/a.obj"))).unwrap();
        let plan = scan_step(w, "rename into a protected folder", &fresh(), false);
        let mut protected = everything(&plan, false);
        protected.sensitive_dirs = vec![root.join("Prot")];
        apply_step(w, "protected target fails that pairing", &fresh(), protected, false);
        apply_step(w, "same rename without protection", &fresh(), everything(&plan, false), false);

        w.conn
            .execute_batch("CREATE TRIGGER block_insert BEFORE INSERT ON files WHEN NEW.path LIKE '%boom%' BEGIN SELECT RAISE(ABORT, 'blocked insert'); END;")
            .unwrap();
        put(w, "models/boom.obj", &obj(9));
        put(w, "models/neu/n.obj", &obj(10));
        std::fs::rename(root.join(crate::commands::golden_harness::nat("Prot/a.obj")), root.join("a2.obj")).unwrap();
        let plan = scan_step(w, "new files and a rename", &fresh(), false);
        apply_step(w, "insert fails: nothing is kept", &fresh(), everything(&plan, false), false);
        state(w, "after the failed insert");
        w.conn.execute_batch("DROP TRIGGER block_insert").unwrap();
        std::fs::remove_file(root.join("boom.obj")).unwrap();

        w.conn
            .execute_batch("CREATE TRIGGER block_rename BEFORE UPDATE OF path ON files WHEN NEW.path LIKE '%blockren%' BEGIN SELECT RAISE(ABORT, 'blocked rename'); END;")
            .unwrap();
        std::fs::rename(root.join("c.obj"), root.join("blockren.obj")).unwrap();
        std::fs::rename(root.join("d.obj"), root.join("d2.obj")).unwrap();
        let plan = scan_step(w, "two renames, one row blocked", &fresh(), false);
        apply_step(w, "a failed pairing leaves the others", &fresh(), everything(&plan, false), false);
        state(w, "after the blocked rename");
        w.conn.execute_batch("DROP TRIGGER block_rename").unwrap();

        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &s(&root.join("Basis"))).unwrap();
        std::fs::remove_dir_all(root.join("Weg")).unwrap();
        std::fs::remove_dir_all(root.join("Basis")).unwrap();
        std::fs::remove_file(&loose).unwrap();
        let plan = scan_step(w, "missing files and folders", &fresh(), false);
        apply_step(w, "missing files kept without remove_missing", &fresh(), everything(&plan, false), false);
        let plan = scan_step(w, "missing entries still offered", &fresh(), false);
        apply_step(w, "remove the missing entries", &fresh(), everything(&plan, true), false);
        state(w, "after removing");
        scan_step(w, "scan after removing", &fresh(), false);

        std::fs::remove_dir_all(w.root.join("outside")).unwrap();
        let loose2 = w.root.join(crate::commands::golden_harness::nat("outside/again.obj"));
        db::test_insert_minimal_file(&w.conn, &s(&loose2), None).unwrap();
        scan_step(w, "loose file without its folder is unreachable", &fresh(), false);
        w.conn.execute("DELETE FROM files WHERE path = ?1", [s(&loose2)]).unwrap();

        let entries: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).collect();
        for path in entries {
            if path.is_dir() {
                std::fs::remove_dir_all(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
        let plan = scan_step(w, "empty root with catalogued files is unreachable", &fresh(), false);
        apply_step(w, "apply on an unreachable root", &fresh(), everything(&plan, true), false);
        std::fs::remove_dir(&root).unwrap();
        scan_step(w, "root gone", &fresh(), false);
        state(w, "end");
    });
}

#[test]
fn golden_disk_sync_faults() {
    run_golden("disk_sync_faults", |w| {
        let files: Vec<(&str, String)> = vec![("Sub/s.obj", obj(1)), ("a.obj", obj(2)), ("Other/o.obj", obj(3))];
        let root = catalogue(w, &files, false);
        std::fs::remove_file(root.join("a.obj")).unwrap();
        put(w, "models/Sub/new.obj", &obj(4));
        put(w, "models/Other/new2.obj", &obj(5));
        scan_step(w, "no fault", &FaultFs::new(), false);
        let fs = FaultFs::new().fail_on(Op::ReadDir, Some("Sub"), 1, PermissionDenied);
        scan_step(w, "listing a subfolder fails", &fs, false);
        let fs = FaultFs::new().fail(Op::ReadDir, 1, PermissionDenied);
        scan_step(w, "listing the root fails", &fs, false);
        let fs = FaultFs::new().fail(Op::ReadDir, 2, PermissionDenied);
        scan_step(w, "second listing fails", &fs, false);
        let fs = FaultFs::new().fail_on(Op::Metadata, Some("a.obj"), 1, PermissionDenied);
        scan_step(w, "probing a missing file fails", &fs, false);
        let fs = FaultFs::new().fail(Op::Canonicalize, 1, PermissionDenied);
        scan_step(w, "resolving the root fails", &fs, false);
        let fs = FaultFs::new().fail_on(Op::SymlinkMetadata, Some("Other"), 1, PermissionDenied);
        scan_step(w, "link check of a subfolder fails", &fs, false);
        let fs = FaultFs::new().fail_on(Op::Metadata, Some("new2.obj"), 1, PermissionDenied);
        scan_step(w, "size of a new file fails", &fs, false);
        let fs = FaultFs::new().fail_on(Op::Metadata, Some("new2.obj"), 2, PermissionDenied);
        scan_step(w, "hash probe of a new file fails", &fs, false);

        // A fault in step n of the apply: each run has its own root, a new file, a missing
        // file and a rename, and fails the n-th call of one operation below that root.
        for (k, (op, name)) in [(Op::Canonicalize, "canon"), (Op::Metadata, "meta"), (Op::SymlinkMetadata, "link"), (Op::ReadDir, "list")].into_iter().enumerate() {
            // Content unique per run, so no run sees a copy of another one.
            let base = 1000 * (k as u32 + 1);
            for n in 1..=8u32 {
                let dir = format!("sw-{name}-{n}");
                let run_root = w.dir(&dir);
                db::ensure_folder_path(&w.conn, &run_root, &run_root).unwrap();
                put(w, &format!("{dir}/old.obj"), &obj(base + 100 + n));
                put(w, &format!("{dir}/gone.obj"), &obj(base + 200 + n));
                let first = scan_step(w, &format!("{dir}: prepare"), &FaultFs::new(), false);
                apply_step(w, &format!("{dir}: take over"), &FaultFs::new(), everything(&first, false), false);
                std::fs::rename(run_root.join("old.obj"), run_root.join("new-name.obj")).unwrap();
                std::fs::remove_file(run_root.join("gone.obj")).unwrap();
                put(w, &format!("{dir}/Neu/fresh.obj"), &obj(base + 300 + n));
                let plan = scan_step(w, &format!("{dir}: plan"), &FaultFs::new(), false);
                let fs = FaultFs::new().fail_on(op, Some(&dir), n as usize, PermissionDenied);
                apply_step(w, &format!("{dir}: apply with {op:?} #{n} failing"), &fs, everything(&plan, true), false);
                // Settle the run so it does not change the next ones.
                let rest = scan_step(w, &format!("{dir}: rest"), &FaultFs::new(), false);
                apply_step(w, &format!("{dir}: settle"), &FaultFs::new(), everything(&rest, true), false);
            }
        }
    });
}

#[test]
fn golden_disk_sync_case_insensitive() {
    run_golden("disk_sync_case_insensitive", |w| {
        let insensitive = || FaultFs::new().case_insensitive();
        let root = w.dir("models");
        db::ensure_folder_path(&w.conn, &root, &root).unwrap();
        put(w, "models/Part/A.obj", &obj(1));
        put(w, "models/orig.obj", &obj(2));
        let first = scan_step(w, "first scan", &insensitive(), true);
        apply_step(w, "take over everything", &insensitive(), everything(&first, false), true);
        std::fs::rename(root.join(crate::commands::golden_harness::nat("Part/A.obj")), root.join(crate::commands::golden_harness::nat("Part/a.obj"))).unwrap();
        scan_step(w, "case-only rename is neither new nor missing", &insensitive(), true);
        let kopie = put(w, "models/Kopie.obj", &obj(2));
        let plan = scan_step(w, "copy offered", &insensitive(), true);
        let declined: Vec<String> = plan.duplicate_files.iter().map(|d| d.entry.path.clone()).collect();
        apply_step(w, "decline the copy", &insensitive(), DiskSelection { declined_copies: declined, ..Default::default() }, true);
        std::fs::rename(&kopie, root.join("kopie.obj")).unwrap();
        scan_step(w, "remembered copy under another spelling", &insensitive(), true);
        put(w, "models/Part/B.obj", &obj(3));
        db::test_insert_minimal_file(&w.conn, &s(&root.join(crate::commands::golden_harness::nat("Part/b.obj"))), None).unwrap();
        scan_step(w, "row with another spelling of a new file", &insensitive(), true);
        put(w, "models/Neu/N.obj", &obj(4));
        let plan = scan_step(w, "new folder", &insensitive(), true);
        apply_step(w, "take over the new folder", &insensitive(), everything(&plan, false), true);
        state(w, "end");
        scan_step(w, "scan after apply", &insensitive(), true);
    });
}

#[cfg(unix)]
#[test]
fn golden_disk_sync_symlinks() {
    run_golden("disk_sync_symlinks", |w| {
        use std::os::unix::fs::symlink;
        let real = w.dir("real");
        put(w, "real/a.obj", &obj(1));
        put(w, "elsewhere/e.obj", &obj(2));
        let alias = w.root.join("alias");
        symlink(&real, &alias).unwrap();
        db::ensure_folder_path(&w.conn, &alias, &alias).unwrap();
        symlink(w.root.join(crate::commands::golden_harness::nat("elsewhere/e.obj")), real.join("link.obj")).unwrap();
        symlink(w.root.join("elsewhere"), real.join("linkdir")).unwrap();
        let first = scan_step(w, "root through a link, links inside are not followed", &FaultFs::new(), false);
        apply_step(w, "take over", &FaultFs::new(), everything(&first, false), false);
        state(w, "after apply");
        std::fs::rename(real.join("a.obj"), real.join("b.obj")).unwrap();
        let plan = scan_step(w, "rename below the aliased root", &FaultFs::new(), false);
        apply_step(w, "take over the rename", &FaultFs::new(), everything(&plan, false), false);
        state(w, "after the rename");
        // Link sizes depend on the temp path; they never reach the tree.
        for link in [real.join("link.obj"), real.join("linkdir"), alias] {
            std::fs::remove_file(link).unwrap();
        }
    });
}
