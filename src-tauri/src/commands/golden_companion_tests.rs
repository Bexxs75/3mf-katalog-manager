//! Golden scenarios of the companion actions (issue #20, package P3): add, trash, restore,
//! purge, assign from an archive and the carry that follows a model. Recorded with the code
//! as it was before the actions moved into `katalog_core::companion_actions`; the golden files
//! must not change with the move. See `docs/plans/issue20-p3-zuschnitt.md`.
use super::fs_boundary::{FaultFs, Op};
use super::golden_harness::{run_golden, World};
use super::*;
use std::io::ErrorKind::{PermissionDenied, Unsupported};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";
const EXE: &[u8] = b"MZ\x90\0\x03\0\0\0";
const ZIP: &[u8] = b"PK\x03\x04\x14\0\0\0";
const BAD_TEXT: &[u8] = b"abc\xff\xfe\0def";
const STAMP: &str = "2026-01-01T00:00:00Z";

fn put(w: &World, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = w.root.join(crate::commands::golden_harness::nat(rel));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The record as JSON without the time stamp (the tables carry that as `<TS>`).
fn rec(row: &db::models::CompanionRecord) -> serde_json::Value {
    serde_json::json!({
        "id": row.id, "fileId": row.file_id, "name": row.name, "path": row.path, "kind": row.kind,
        "sizeBytes": row.size_bytes, "trashed": row.deleted_at.is_some(), "trashPath": row.trash_path,
    })
}

fn cat(w: &World) -> i64 {
    db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap()
}

fn model(w: &World, rel: &str, folder: Option<i64>) -> (i64, PathBuf) {
    let path = put(w, rel, b"model");
    (db::test_insert_minimal_file(&w.conn, &path.to_string_lossy(), folder).unwrap(), path)
}

/// A source file outside the catalog, like one the user picked.
fn source(w: &World, name: &str, bytes: &[u8]) -> PathBuf {
    put(w, &format!("outside/{name}"), bytes)
}

fn add(w: &World, model: i64, name: &str, bytes: &[u8]) -> db::models::CompanionRecord {
    companions::add_companion_with_conn(&w.conn, model, &source(w, name, bytes), &[]).unwrap()
}

fn calls(fs: &FaultFs) -> Vec<String> {
    fs.calls().iter().map(|(op, a, b)| format!("{op:?} {}{}", a.display(), b.as_ref().map(|b| format!(" -> {}", b.display())).unwrap_or_default())).collect()
}

fn ok<T>(value: T) -> CmdResult<T> {
    Ok(value)
}

#[test]
fn golden_companion_add() {
    run_golden("companion_add", |w| {
        let folder = cat(w);
        let (m, _) = model(w, "cat/m.3mf", Some(folder));
        let (loose, _) = model(w, "loose/x.3mf", None);
        let (trashed, _) = model(w, "cat/t.3mf", Some(folder));
        db::soft_delete_file(&w.conn, trashed, Some("x"), STAMP).unwrap();
        let (full, _) = model(w, "cat/full.3mf", Some(folder));
        for i in 0..100 {
            db::companions::insert_companion(&w.conn, full, &format!("n{i}.png"), &format!("/nowhere/n{i}.png"), "image", None, STAMP).unwrap();
        }
        let photo = source(w, "photo.png", PNG);
        let r = companions::add_companion_with_conn(&w.conn, m, &photo, &[]);
        w.step("add photo.png", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, m, &photo, &[]);
        w.step("add photo.png again (free name)", r.map(|c| rec(&c)));
        let notes = source(w, "notes.txt", "Lizenz: CC-BY\n".as_bytes());
        let r = companions::add_companion_with_conn(&w.conn, m, &notes, &[]);
        w.step("add notes.txt", r.map(|c| rec(&c)));
        let pdf = source(w, "doc.pdf", PDF);
        let r = companions::add_companion_with_conn(&w.conn, m, &pdf, &[]);
        w.step("add doc.pdf", r.map(|c| rec(&c)));
        let fake = source(w, "fake.png", EXE);
        let r = companions::add_companion_with_conn(&w.conn, m, &fake, &[]);
        w.step("exe named png (content refused)", r.map(|c| rec(&c)));
        let zip = source(w, "data.zip", ZIP);
        let r = companions::add_companion_with_conn(&w.conn, m, &zip, &[]);
        w.step("zip (type refused)", r.map(|c| rec(&c)));
        let bad = source(w, "bad.txt", BAD_TEXT);
        let r = companions::add_companion_with_conn(&w.conn, m, &bad, &[]);
        w.step("text with invalid bytes", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, m, Path::new("photo.png"), &[]);
        w.step("relative source path", r.map(|c| rec(&c)));
        let dotdot = w.root.join("outside").join("..").join("outside").join("photo.png");
        let r = companions::add_companion_with_conn(&w.conn, m, &dotdot, &[]);
        w.step("source path with ..", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, m, &w.dir("outside/adir"), &[]);
        w.step("source is a directory", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, m, &w.root.join(crate::commands::golden_harness::nat("outside/missing.png")), &[]);
        w.step("source missing", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, 999, &photo, &[]);
        w.step("unknown model", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, loose, &photo, &[]);
        w.step("model outside every catalog folder", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, trashed, &photo, &[]);
        w.step("model in the trash", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, full, &photo, &[]);
        w.step("model already holds 100", r.map(|c| rec(&c)));
        let secret = w.dir("outside/secret");
        let hidden = put(w, "outside/secret/a.png", PNG);
        let r = companions::add_companion_with_conn(&w.conn, m, &hidden, &[secret]);
        w.step("source in a protected folder", r.map(|c| rec(&c)));
        let r = companions::add_companion_with_conn(&w.conn, m, &source(w, "a b (1).png", PNG), &[]);
        w.step("awkward file name", r.map(|c| rec(&c)));
        let r = companions::list_companions_with_conn(&w.conn, m);
        w.step("list", r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
        let r = companions::list_companions_with_conn(&w.conn, 999);
        w.step("list unknown model", r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
    });
}

#[test]
fn golden_companion_add_db_failure_cleans_up() {
    run_golden("companion_add_db_failure", |w| {
        let folder = cat(w);
        let (m, _) = model(w, "cat/m.3mf", Some(folder));
        w.conn
            .execute_batch("CREATE TRIGGER no_insert BEFORE INSERT ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let r = companions::add_companion_with_conn(&w.conn, m, &source(w, "photo.png", PNG), &[]);
        w.step("add with the row insert failing (file and new folder are removed again)", r.map(|c| rec(&c)));
        w.conn.execute_batch("DROP TRIGGER no_insert").unwrap();
        w.dir("cat/m_extras");
        let r = companions::add_companion_with_conn(&w.conn, m, &source(w, "photo.png", PNG), &[]);
        w.step("add into an existing extras folder", r.map(|c| rec(&c)));
    });
}

#[cfg(unix)]
#[test]
fn golden_companion_add_symlinks() {
    run_golden("companion_add_symlinks", |w| {
        let folder = cat(w);
        let (m, _) = model(w, "cat/m.3mf", Some(folder));
        let (n, _) = model(w, "cat/n.3mf", Some(folder));
        let real = source(w, "photo.png", PNG);
        let link = w.root.join(crate::commands::golden_harness::nat("outside/link.png"));
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let r = companions::add_companion_with_conn(&w.conn, m, &link, &[]);
        w.step("symlink as source", r.map(|c| rec(&c)));
        let elsewhere = w.dir("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, w.root.join(crate::commands::golden_harness::nat("cat/n_extras"))).unwrap();
        let r = companions::add_companion_with_conn(&w.conn, n, &real, &[]);
        w.step("extras folder is a symlink", r.map(|c| rec(&c)));
    });
}

#[test]
fn golden_companion_trash_restore() {
    run_golden("companion_trash_restore", |w| {
        let folder = cat(w);
        let trash = w.dir("trash");
        let (m, _) = model(w, "cat/m.3mf", Some(folder));
        let a = add(w, m, "a.png", PNG);
        let b = add(w, m, "b.png", PNG);
        let c = add(w, m, "c.png", PNG);
        let d = add(w, m, "d.png", PNG);
        let r = companions::remove_companion_with_conn(&w.conn, a.id, &trash);
        w.step("remove a", r);
        let r = companions::remove_companion_with_conn(&w.conn, a.id, &trash);
        w.step("remove a again (already in the trash)", r);
        let r = companions::remove_companion_with_conn(&w.conn, 999, &trash);
        w.step("remove unknown id", r);
        let r = companions::restore_companion_with_conn(&w.conn, a.id, &[]);
        w.step("restore a", r.map(|c| rec(&c)));
        let r = companions::restore_companion_with_conn(&w.conn, a.id, &[]);
        w.step("restore a again (not in the trash)", r.map(|c| rec(&c)));
        let r = companions::restore_companion_with_conn(&w.conn, 999, &[]);
        w.step("restore unknown id", r.map(|c| rec(&c)));
        let r = companions::remove_companion_with_conn(&w.conn, b.id, &trash);
        w.step("remove b", r);
        put(w, "cat/m_extras/b.png", b"newer");
        let r = companions::restore_companion_with_conn(&w.conn, b.id, &[]);
        w.step("restore b (name taken, gets a free one)", r.map(|c| rec(&c)));
        std::fs::remove_file(&d.path).unwrap();
        let r = companions::remove_companion_with_conn(&w.conn, d.id, &trash);
        w.step("remove d whose file is gone (row dropped)", r);
        std::fs::remove_file(&c.path).unwrap();
        std::fs::create_dir(&c.path).unwrap();
        let r = companions::remove_companion_with_conn(&w.conn, c.id, &trash);
        w.step("remove c that is now a directory", r);
        let e = add(w, m, "e.png", PNG);
        let r = companions::remove_companion_with_conn(&w.conn, e.id, &trash);
        w.step("remove e", r);
        db::soft_delete_file(&w.conn, m, Some("x"), STAMP).unwrap();
        let r = companions::restore_companion_with_conn(&w.conn, e.id, &[]);
        w.step("restore e while the model is in the trash", r.map(|c| rec(&c)));
        let r = companions::remove_companion_with_conn(&w.conn, a.id, &trash);
        w.step("remove a while the model is in the trash", r);
    });
}

#[test]
fn golden_companion_trash_restore_db_failures() {
    run_golden("companion_trash_restore_db_failures", |w| {
        let folder = cat(w);
        let trash = w.dir("trash");
        let (m, _) = model(w, "cat/m.3mf", Some(folder));
        let a = add(w, m, "a.png", PNG);
        let b = add(w, m, "b.png", PNG);
        w.conn
            .execute_batch("CREATE TRIGGER no_trash BEFORE UPDATE OF deleted_at ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let r = companions::remove_companion_with_conn(&w.conn, a.id, &trash);
        w.step("remove a with the row update failing (file moved back)", r);
        w.conn.execute_batch("DROP TRIGGER no_trash").unwrap();
        let r = companions::remove_companion_with_conn(&w.conn, b.id, &trash);
        w.step("remove b", r);
        w.conn
            .execute_batch("CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let r = companions::restore_companion_with_conn(&w.conn, b.id, &[]);
        w.step("restore b with the row update failing (file back in the trash)", r.map(|c| rec(&c)));
        w.conn.execute_batch("DROP TRIGGER block_path").unwrap();
        std::fs::remove_dir_all(w.root.join(crate::commands::golden_harness::nat("cat/m_extras"))).unwrap();
        w.conn
            .execute_batch("CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let r = companions::restore_companion_with_conn(&w.conn, b.id, &[]);
        w.step("restore b with the folder gone and the row update failing (new folder removed again)", r.map(|c| rec(&c)));
    });
}

#[test]
fn golden_companion_with_model() {
    run_golden("companion_with_model", |w| {
        let folder = cat(w);
        let trash = w.dir("trash");
        // 1: plain trash and restore with the model.
        let (m1, _) = model(w, "cat/m1.3mf", Some(folder));
        add(w, m1, "a.png", PNG);
        add(w, m1, "b.png", PNG);
        let fs = FaultFs::new();
        companions::trash_with_model_fs(&fs, &w.conn, m1, &trash, STAMP);
        w.step("trash with model m1", ok(calls(&fs)));
        companions::restore_with_model(&w.conn, m1, STAMP, &[]);
        w.step("restore with model m1", ok(()));
        // 2: the trash is on another volume: the hard link fails, the copy path is used.
        let (m2, _) = model(w, "cat/m2.3mf", Some(folder));
        add(w, m2, "a.png", PNG);
        let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported);
        companions::trash_with_model_fs(&fs, &w.conn, m2, &trash, STAMP);
        w.step("trash with model m2, hard link not possible", ok(calls(&fs)));
        // 3: the file is locked: the companion stays active where it is.
        let (m3, _) = model(w, "cat/m3.3mf", Some(folder));
        add(w, m3, "a.png", PNG);
        let fs = FaultFs::new().fail_on(Op::HardLink, Some("m3_extras"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("m3_extras"), 1, PermissionDenied);
        companions::trash_with_model_fs(&fs, &w.conn, m3, &trash, STAMP);
        w.step("trash with model m3, file locked", ok(calls(&fs)));
        // 4: removing the source after the move fails: the target is removed again.
        let (m4, _) = model(w, "cat/m4.3mf", Some(folder));
        add(w, m4, "a.png", PNG);
        let fs = FaultFs::new().fail_on(Op::RemoveFile, Some("m4_extras"), 1, PermissionDenied);
        companions::trash_with_model_fs(&fs, &w.conn, m4, &trash, STAMP);
        w.step("trash with model m4, removing the original refused", ok(calls(&fs)));
        // 5: a companion that is not a file, a file that is gone, a row outside an extras folder.
        let (m5, _) = model(w, "cat/m5.3mf", Some(folder));
        let dir_row = add(w, m5, "a.png", PNG);
        let gone_row = add(w, m5, "b.png", PNG);
        std::fs::remove_file(&dir_row.path).unwrap();
        std::fs::create_dir(&dir_row.path).unwrap();
        std::fs::remove_file(&gone_row.path).unwrap();
        let fs = FaultFs::new();
        companions::trash_with_model_fs(&fs, &w.conn, m5, &trash, STAMP);
        w.step("trash with model m5 (directory, gone file)", ok(calls(&fs)));
        companions::restore_with_model(&w.conn, m5, STAMP, &[]);
        w.step("restore with model m5", ok(()));
        // 6: restore with a model whose companion folder is a file.
        let (m6, _) = model(w, "cat/m6.3mf", Some(folder));
        add(w, m6, "a.png", PNG);
        let fs = FaultFs::new();
        companions::trash_with_model_fs(&fs, &w.conn, m6, &trash, STAMP);
        put(w, "cat/m6_extras", b"in the way");
        companions::restore_with_model(&w.conn, m6, STAMP, &[]);
        w.step("restore with model m6, extras name taken by a file (stays in the trash)", ok(()));
        // 7: a different stamp restores nothing.
        let (m7, _) = model(w, "cat/m7.3mf", Some(folder));
        add(w, m7, "a.png", PNG);
        companions::trash_with_model_fs(&FaultFs::new(), &w.conn, m7, &trash, STAMP);
        companions::restore_with_model(&w.conn, m7, "2027-01-01T00:00:00Z", &[]);
        w.step("restore with model m7, other stamp", ok(()));
        companions::restore_with_model(&w.conn, 999, STAMP, &[]);
        w.step("restore with unknown model", ok(()));
    });
}

#[test]
fn golden_companion_purge() {
    run_golden("companion_purge", |w| {
        let folder = cat(w);
        let trash = w.dir("trash");
        let (old, _) = model(w, "cat/old.3mf", Some(folder));
        let (fresh, _) = model(w, "cat/fresh.3mf", Some(folder));
        let (alone, _) = model(w, "cat/alone.3mf", Some(folder));
        add(w, old, "a.png", PNG);
        add(w, fresh, "b.png", PNG);
        let loner = add(w, alone, "c.png", PNG);
        let keep = add(w, alone, "keep.png", PNG);
        companions::trash_with_model_fs(&FaultFs::new(), &w.conn, old, &trash, STAMP);
        companions::trash_with_model_fs(&FaultFs::new(), &w.conn, fresh, &trash, STAMP);
        companions::remove_companion_with_conn(&w.conn, loner.id, &trash).unwrap();
        let _ = keep;
        // The cutoff must not depend on the clock: age the rows by hand and pass a fixed cutoff.
        w.conn
            .execute("UPDATE companion_files SET deleted_at = '2025-01-01T00:00:00+00:00' WHERE file_id IN (?1, ?2) AND deleted_at IS NOT NULL", rusqlite::params![old, alone])
            .unwrap();
        companions::purge_expired_trashed(&w.conn, &trash, "2025-06-01T00:00:00+00:00");
        w.step("purge expired", ok(()));
        let r = companions::purge_trash_files_of_model(&w.conn, fresh, &trash).map_err(CmdError::from);
        w.step("purge trash files of fresh (rows follow with the model)", r);
        let r = companions::purge_trash_files_of_model(&w.conn, 999, &trash).map_err(CmdError::from);
        w.step("purge trash files of unknown model", r);
        // A row that points outside the trash is refused and stays.
        let precious = source(w, "precious.png", PNG);
        let planted = db::companions::insert_companion(&w.conn, fresh, "x.png", &w.root.join(crate::commands::golden_harness::nat("cat/fresh_extras/x.png")).to_string_lossy(), "image", None, STAMP).unwrap();
        w.conn
            .execute(
                "UPDATE companion_files SET deleted_at = '2000-01-01T00:00:00+00:00', trash_path = ?1 WHERE id = ?2",
                rusqlite::params![precious.to_string_lossy(), planted],
            )
            .unwrap();
        let r = companions::purge_trash_files_of_model(&w.conn, fresh, &trash).map_err(CmdError::from);
        w.step("purge trash files with a row outside the trash (refused)", r);
        companions::purge_all_trashed(&w.conn, &trash);
        w.step("empty trash (the planted row stays)", ok(()));
        // A companion the web version put in its own trash folder.
        let web_dir = w.dir("cat/.katalog-papierkorb");
        let row = db::companions::insert_companion(&w.conn, alone, "web.png", &w.root.join(crate::commands::golden_harness::nat("cat/alone_extras/web.png")).to_string_lossy(), "image", None, STAMP).unwrap();
        let web_file = web_dir.join(format!("companion-{row}-abcd1234-web.png"));
        std::fs::write(&web_file, PNG).unwrap();
        w.conn
            .execute(
                "UPDATE companion_files SET deleted_at = '2026-01-01T00:00:00+00:00', trash_path = ?1 WHERE id = ?2",
                rusqlite::params![web_file.to_string_lossy(), row],
            )
            .unwrap();
        let r = companions::purge_trash_files_of_model(&w.conn, alone, &trash).map_err(CmdError::from);
        w.step("purge a companion in the web trash", r);
        let r = companions::purge_trash_files_of_model(&w.conn, alone, &trash).map_err(CmdError::from);
        w.step("purge again (file already gone)", r);
    });
}

fn dto_of(w: &World, id: i64) -> ModelFileDto {
    to_dto(db::get_file(&w.conn, id).unwrap().unwrap(), &[])
}

#[test]
fn golden_companion_assign_archive_one_model() {
    run_golden("companion_assign_archive_one_model", |w| {
        let folder = cat(w);
        let (m, stl) = model(w, "cat/pack/Model.stl", Some(folder));
        let written = vec![
            stl,
            put(w, "cat/pack/images/front.png", PNG),
            put(w, "cat/pack/docs/manual.pdf", PDF),
            put(w, "cat/pack/license.txt", "CC-BY".as_bytes()),
            put(w, "cat/pack/fake.png", b"this is not a png"),
            put(w, "cat/pack/data.bin", &[1, 2, 3]),
            put(w, "cat/pack/drawing.svg", b"<svg/>"),
            put(w, "cat/pack/.hidden.png", PNG),
            put(w, "cat/pack/more/front.png", PNG),
        ];
        let imported = vec![dto_of(w, m)];
        let r = companions::assign_from_archive(&w.conn, &written, &imported, &[]);
        w.step("assign (assigned, ignored)", ok(r));
        let r = companions::list_companions_with_conn(&w.conn, m);
        w.step("companions", r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
        let r = companions::assign_from_archive(&w.conn, &[], &imported, &[]);
        w.step("assign with nothing written", ok(r));
    });
}

#[test]
fn golden_companion_assign_archive_several_models() {
    run_golden("companion_assign_archive_several_models", |w| {
        let folder = cat(w);
        let (a, a_stl) = model(w, "cat/pack/A/a.stl", Some(folder));
        let (b, b_stl) = model(w, "cat/pack/B/b.stl", Some(folder));
        let (c1, c1_stl) = model(w, "cat/pack/C/one.stl", Some(folder));
        let (c2, c2_stl) = model(w, "cat/pack/C/two.stl", Some(folder));
        let (full, full_stl) = model(w, "cat/pack/F/f.stl", Some(folder));
        let (blocked, blocked_stl) = model(w, "cat/pack/G/g.stl", Some(folder));
        for i in 0..100 {
            db::companions::insert_companion(&w.conn, full, &format!("n{i}.png"), &format!("/nowhere/n{i}.png"), "image", None, STAMP).unwrap();
        }
        put(w, "cat/pack/G/g_extras", b"a file where the folder should be");
        let written = vec![
            a_stl,
            b_stl,
            c1_stl,
            c2_stl,
            full_stl,
            blocked_stl,
            put(w, "cat/pack/A/a.png", PNG),
            put(w, "cat/pack/A/a.pdf", PDF),
            put(w, "cat/pack/B/b.pdf", PDF),
            put(w, "cat/pack/C/c.png", PNG),
            put(w, "cat/pack/D/orphan.png", PNG),
            put(w, "cat/pack/A/bad.png", EXE),
            put(w, "cat/pack/F/f.png", PNG),
            put(w, "cat/pack/G/g.png", PNG),
        ];
        let imported: Vec<ModelFileDto> = [a, b, c1, c2, full, blocked].iter().map(|id| dto_of(w, *id)).collect();
        let r = companions::assign_from_archive(&w.conn, &written, &imported, &[]);
        w.step("assign (assigned, ignored)", ok(r));
        for (label, id) in [("a", a), ("b", b), ("c1", c1), ("c2", c2), ("blocked", blocked)] {
            let r = companions::list_companions_with_conn(&w.conn, id);
            w.step(&format!("companions of {label}"), r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
        }
        // A name that is already taken in the extras folder gets a free one.
        let (d, d_stl) = model(w, "cat/pack2/d.stl", Some(folder));
        put(w, "cat/pack2/d_extras/d.png", b"older");
        let written = vec![d_stl, put(w, "cat/pack2/d.png", PNG)];
        let r = companions::assign_from_archive(&w.conn, &written, &[dto_of(w, d)], &[]);
        w.step("assign onto a taken name", ok(r));
        let r = companions::list_companions_with_conn(&w.conn, d);
        w.step("companions of d", r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
    });
}

/// Moves the model on disk (the caller of `carry` does that before) and returns the new path.
fn move_model(old: &Path, new: &Path) {
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(old, new).unwrap();
}

fn carry_step(w: &mut World, label: &str, fs: &FaultFs, id: i64, old: &Path, new: &Path, case_insensitive: bool) {
    let r = companions::carry_with_fs(fs, &w.conn, id, old, new, case_insensitive).map(|carried| {
        carried.finish_with(fs);
    });
    w.step(label, r);
}

fn rows_of(w: &mut World, label: &str, id: i64) {
    let r = companions::list_companions_with_conn(&w.conn, id);
    w.step(label, r.map(|rows| rows.iter().map(rec).collect::<Vec<_>>()));
}

#[test]
fn golden_companion_carry() {
    run_golden("companion_carry", |w| {
        let folder = cat(w);
        let other = db::insert_folder_with_parent(&w.conn, "dst", None, &w.dir("dst").to_string_lossy()).unwrap();
        let _ = other;
        // 1: move into another folder, finish removes the emptied folder.
        let (m1, old1) = model(w, "cat/Foo.3mf", Some(folder));
        add(w, m1, "a.png", PNG);
        add(w, m1, "b.png", PNG);
        let new1 = w.root.join(crate::commands::golden_harness::nat("dst/Foo.3mf"));
        move_model(&old1, &new1);
        let fs = FaultFs::new();
        carry_step(w, "move Foo to dst", &fs, m1, &old1, &new1, false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Foo", m1);
        // 2: move there and undo.
        let old2 = new1.clone();
        let new2 = w.root.join(crate::commands::golden_harness::nat("cat/Foo.3mf"));
        move_model(&old2, &new2);
        let fs = FaultFs::new();
        let carried = companions::carry_with_fs(&fs, &w.conn, m1, &old2, &new2, false);
        w.step("move Foo back to cat (kept for undo)", carried.as_ref().map(|_| ()).map_err(|e| e.clone()));
        carried.unwrap().undo_with(&fs, &w.conn);
        w.step("undo", ok(calls(&fs)));
        rows_of(w, "rows after undo", m1);
        // 3: name taken for the second file: the first one goes back.
        let (m3, old3) = model(w, "cat/Bar.3mf", Some(folder));
        add(w, m3, "a.png", PNG);
        add(w, m3, "b.png", PNG);
        put(w, "dst/Bar_extras/b.png", b"in the way");
        let new3 = w.root.join(crate::commands::golden_harness::nat("dst/Bar.3mf"));
        move_model(&old3, &new3);
        let fs = FaultFs::new();
        carry_step(w, "move Bar, second name taken", &fs, m3, &old3, &new3, false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Bar", m3);
        // 4: the target folder name is a file.
        let (m4, old4) = model(w, "cat/Baz.3mf", Some(folder));
        add(w, m4, "a.png", PNG);
        put(w, "dst/Baz_extras", b"a file");
        let new4 = w.root.join(crate::commands::golden_harness::nat("dst/Baz.3mf"));
        move_model(&old4, &new4);
        let fs = FaultFs::new();
        carry_step(w, "move Baz, target folder name is a file", &fs, m4, &old4, &new4, false);
        // 5: creating the target folder fails.
        let (m5, old5) = model(w, "cat/Qux.3mf", Some(folder));
        add(w, m5, "a.png", PNG);
        let new5 = w.root.join(crate::commands::golden_harness::nat("dst/Qux.3mf"));
        move_model(&old5, &new5);
        let fs = FaultFs::new().fail(Op::CreateDir, 1, PermissionDenied);
        carry_step(w, "move Qux, folder creation refused", &fs, m5, &old5, &new5, false);
        w.step("calls", ok(calls(&fs)));
        // 6: the move of the second file fails (locked): the first goes back, the new folder goes.
        let (m6, old6) = model(w, "cat/Lock.3mf", Some(folder));
        add(w, m6, "a.png", PNG);
        add(w, m6, "b.png", PNG);
        let new6 = w.root.join(crate::commands::golden_harness::nat("dst/Lock.3mf"));
        move_model(&old6, &new6);
        let fs = FaultFs::new()
            .fail_on(Op::HardLink, Some("b.png"), 1, PermissionDenied)
            .fail_on(Op::CopyNew, Some("b.png"), 1, PermissionDenied);
        carry_step(w, "move Lock, second file locked", &fs, m6, &old6, &new6, false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Lock", m6);
        // 7: the hard link is not possible (other volume): the copy path carries the files.
        let (m7, old7) = model(w, "cat/Vol.3mf", Some(folder));
        add(w, m7, "a.png", PNG);
        let new7 = w.root.join(crate::commands::golden_harness::nat("dst/Vol.3mf"));
        move_model(&old7, &new7);
        let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported);
        carry_step(w, "move Vol, hard link not possible", &fs, m7, &old7, &new7, false);
        w.step("calls", ok(calls(&fs)));
        // 8: a row outside the model's folder and a missing file.
        let (m8, old8) = model(w, "cat/Odd.3mf", Some(folder));
        add(w, m8, "a.png", PNG);
        let stray = source(w, "stray.png", PNG);
        db::companions::insert_companion(&w.conn, m8, "stray.png", &stray.to_string_lossy(), "image", Some(1), STAMP).unwrap();
        let gone = add(w, m8, "gone.png", PNG);
        std::fs::remove_file(&gone.path).unwrap();
        let new8 = w.root.join(crate::commands::golden_harness::nat("dst/Odd.3mf"));
        move_model(&old8, &new8);
        let fs = FaultFs::new();
        carry_step(w, "move Odd (stray row, missing file)", &fs, m8, &old8, &new8, false);
        rows_of(w, "rows of Odd", m8);
        // 9: nothing to carry, same folder, no extras.
        let (m9, old9) = model(w, "cat/None.3mf", Some(folder));
        let new9 = w.root.join(crate::commands::golden_harness::nat("dst/None.3mf"));
        move_model(&old9, &new9);
        carry_step(w, "move a model without companions", &FaultFs::new(), m9, &old9, &new9, false);
        let (m10, old10) = model(w, "cat/Same.3mf", Some(folder));
        add(w, m10, "a.png", PNG);
        let new10 = w.root.join(crate::commands::golden_harness::nat("cat/Same.stl"));
        move_model(&old10, &new10);
        carry_step(w, "change only the extension (same folder)", &FaultFs::new(), m10, &old10, &new10, false);
        // 10: a row fails to update after the first file moved.
        let (m12, old12) = model(w, "cat/Db.3mf", Some(folder));
        add(w, m12, "a.png", PNG);
        add(w, m12, "b.png", PNG);
        w.conn
            .execute_batch("CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files WHEN NEW.path GLOB '*dst*b.png' BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let new12 = w.root.join(crate::commands::golden_harness::nat("dst/Db.3mf"));
        move_model(&old12, &new12);
        let fs = FaultFs::new();
        carry_step(w, "move Db, the row of the second file cannot be updated", &fs, m12, &old12, &new12, false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Db", m12);
    });
}

/// Needs symlinks (not available to the Windows runner), so it has its own golden file.
#[cfg(unix)]
#[test]
fn golden_companion_carry_symlinked_extras() {
    run_golden("companion_carry_symlinked_extras", |w| {
        let folder = cat(w);
        w.dir("dst");
        let (m, old) = model(w, "cat/Link.3mf", Some(folder));
        add(w, m, "a.png", PNG);
        let extras = w.root.join(crate::commands::golden_harness::nat("cat/Link_extras"));
        let elsewhere = w.dir("elsewhere");
        std::fs::rename(&extras, elsewhere.join("held")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("held"), &extras).unwrap();
        let new = w.root.join(crate::commands::golden_harness::nat("dst/Link.3mf"));
        move_model(&old, &new);
        carry_step(w, "move Link, extras folder is a symlink", &FaultFs::new(), m, &old, &new, false);
        rows_of(w, "rows of Link", m);
    });
}

#[test]
fn golden_companion_carry_case_only() {
    run_golden("companion_carry_case_only", |w| {
        let folder = cat(w);
        // 1: only the case changes: the folder itself is renamed.
        let (m1, old1) = model(w, "cat/foo.3mf", Some(folder));
        add(w, m1, "a.png", PNG);
        add(w, m1, "b.png", PNG);
        let new1 = w.root.join(crate::commands::golden_harness::nat("cat/Foo.3mf"));
        let fs = FaultFs::new();
        carry_step(w, "rename foo to Foo (case-insensitive volume)", &fs, m1, &old1, &new1, true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Foo", m1);
        // 2: a folder shared by two models is left alone.
        let (m3, old3) = model(w, "cat/baz.stl", Some(folder));
        let (m4, _) = model(w, "cat/baz.3mf", Some(folder));
        add(w, m3, "one.png", PNG);
        add(w, m4, "two.png", PNG);
        let new3 = w.root.join(crate::commands::golden_harness::nat("cat/Baz.stl"));
        let fs = FaultFs::new();
        carry_step(w, "rename baz.stl to Baz.stl, folder shared", &fs, m3, &old3, &new3, true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of baz.stl", m3);
        rows_of(w, "rows of baz.3mf", m4);
        // 3: renaming the folder is refused.
        let (m5, old5) = model(w, "cat/qux.3mf", Some(folder));
        add(w, m5, "a.png", PNG);
        let new5 = w.root.join(crate::commands::golden_harness::nat("cat/QUX.3mf"));
        let fs = FaultFs::new().fail(Op::Rename, 1, PermissionDenied);
        carry_step(w, "rename qux to QUX, folder rename refused", &fs, m5, &old5, &new5, true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of qux", m5);
        // 4: the row update fails after the folder was renamed: it is renamed back.
        let (m6, old6) = model(w, "cat/db.3mf", Some(folder));
        add(w, m6, "a.png", PNG);
        add(w, m6, "b.png", PNG);
        w.conn
            .execute_batch("CREATE TRIGGER block_path BEFORE UPDATE OF path ON companion_files WHEN NEW.path GLOB '*DB_extras*b.png' BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
        let new6 = w.root.join(crate::commands::golden_harness::nat("cat/DB.3mf"));
        let fs = FaultFs::new();
        carry_step(w, "rename db to DB, second row cannot be updated", &fs, m6, &old6, &new6, true);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of db", m6);
    });
}

/// The same names on a volume that tells `bar` and `Bar` apart: two folders, the files move
/// over. Not run where the real volume ignores case (Windows, macOS).
#[cfg(not(any(windows, target_os = "macos")))]
#[test]
fn golden_companion_carry_case_sensitive() {
    run_golden("companion_carry_case_sensitive", |w| {
        let folder = cat(w);
        let (m, old) = model(w, "cat/bar.3mf", Some(folder));
        add(w, m, "a.png", PNG);
        let new = w.root.join(crate::commands::golden_harness::nat("cat/Bar.3mf"));
        let fs = FaultFs::new();
        carry_step(w, "rename bar to Bar (case-sensitive)", &fs, m, &old, &new, false);
        w.step("calls", ok(calls(&fs)));
        rows_of(w, "rows of Bar", m);
    });
}
