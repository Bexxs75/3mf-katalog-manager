use super::*;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;

fn three_mf(size: u32) -> Vec<u8> {
    let xml = format!(
        r#"<model unit="millimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="{size}" y="0" z="0"/><vertex x="0" y="{size}" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}
fn dir(name: &str) -> PathBuf {
    crate::unique_test_dir(name).canonicalize().unwrap()
}
fn put(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn anchored(roots: &[&Path]) -> Anchored {
    Anchored::new(&roots.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>(), &[]).unwrap()
}
fn known(conn: &Connection) -> CatalogPaths {
    CatalogPaths::load(conn).unwrap()
}
fn catalog() -> Connection {
    db::connect_in_memory().unwrap()
}
fn add_model(conn: &Connection, path: &Path) -> i64 {
    db::test_insert_minimal_file(conn, path.to_str().unwrap(), None).unwrap()
}
fn add_companion(conn: &Connection, file_id: i64, path: &Path) -> i64 {
    db::companions::insert_companion(conn, file_id, "c.pdf", path.to_str().unwrap(), "pdf", None, "2026-01-01T00:00:00Z").unwrap()
}
/// All rows of the tables an import touches, as one comparable text.
fn dump(conn: &Connection) -> String {
    let mut out = String::new();
    for sql in ["SELECT id, path, name FROM files ORDER BY id", "SELECT id, path, parent_id FROM folders ORDER BY id"] {
        let mut stmt = conn.prepare(sql).unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |r| Ok(format!("{}|{}|{:?}", r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, rusqlite::types::Value>(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        out.push_str(&rows.join("\n"));
        out.push('\n');
    }
    out
}
fn plan_new(a: &Anchored, conn: &Connection, rel: &str) -> PlanRow {
    plan_new_file(a, &known(conn), 0, rel).unwrap()
}
fn read(row: &PlanRow, a: &Anchored) -> Res<PreparedNew> {
    read_new_file(row, a, WebImportLimits::default(), None)
}

// ---- validate_relative -------------------------------------------------------------------

#[test]
fn validate_relative_accepts_plain_model_paths_and_rejects_the_rest() {
    assert_eq!(validate_relative("Haus/Tor.3mf").unwrap(), PathBuf::from("Haus/Tor.3mf"));
    assert!(validate_relative("a\\b.3mf").is_ok(), "backslash is an ordinary character");
    assert!(validate_relative("A.STL").is_ok());
    let long_ok = format!("{}.3mf", "a".repeat(4000));
    assert!(validate_relative(&long_ok).is_ok());
    let deep_ok = format!("{}x.3mf", "d/".repeat(63));
    assert!(validate_relative(&deep_ok).is_ok(), "64 components");
    let deep_bad = format!("{}x.3mf", "d/".repeat(64));
    let too_long = format!("{}.3mf", "a".repeat(4093));
    let bad = [
        "", "/abs/x.3mf", "../x.3mf", "a/../x.3mf", "./x.3mf", "a//x.3mf", "a/", "a\0b.3mf", ".hidden/x.3mf", "a/.x.3mf",
        "~$x.3mf", "x.3mf.part", "x.3mf.PART", "m_extras/x.3mf", &deep_bad, &too_long,
    ];
    for path in bad {
        assert_eq!(validate_relative(path), Err(SkipReason::Outside), "{path:?}");
    }
    assert_eq!(validate_relative("a/readme.txt"), Err(SkipReason::Invalid));
    assert_eq!(validate_relative("a/noext"), Err(SkipReason::Invalid));
}

// ---- new files ---------------------------------------------------------------------------

#[test]
fn a_new_file_is_read_and_stored_with_its_folder_chain() {
    let root = dir("da-new");
    put(&root.join("Haus/Tor.3mf"), &three_mf(30));
    let mut conn = catalog();
    let a = anchored(&[&root]);
    let row = plan_new(&a, &conn, "Haus/Tor.3mf");
    assert_eq!(row.key, "0:Haus/Tor.3mf");
    let prepared = read(&row, &a).unwrap();
    let id = store_new_file(&mut conn, &a, prepared).unwrap();
    let stored = db::get_file(&conn, id).unwrap().unwrap();
    assert_eq!(stored.path, root.join("Haus/Tor.3mf").to_str().unwrap());
    assert!(stored.dimensions_mm.is_some());
    let folders: Vec<String> = db::list_folders(&conn).unwrap().into_iter().map(|f| f.path).collect();
    assert!(folders.contains(&root.to_str().unwrap().to_owned()));
    assert!(folders.contains(&root.join("Haus").to_str().unwrap().to_owned()));
    // The disk is untouched (the trash folder FsContext makes is the only addition).
    assert!(root.join("Haus/Tor.3mf").is_file());
    // A second import of the same row finds it in the catalog.
    let again = read(&row, &a).unwrap();
    assert_eq!(store_new_file(&mut conn, &a, again).err(), Some(SkipReason::AlreadyInCatalog));
}

#[test]
fn a_path_already_in_the_catalog_is_not_planned_active_trashed_case_and_normalization() {
    let root = dir("da-known");
    for f in ["a.3mf", "b.3mf", "C.3mf", "\u{e4}.3mf", "fresh.3mf"] {
        put(&root.join(f), &three_mf(10));
    }
    let conn = catalog();
    add_model(&conn, &root.join("a.3mf"));
    let trashed = add_model(&conn, &root.join("b.3mf"));
    conn.execute("UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1", [trashed]).unwrap();
    add_model(&conn, &root.join("c.3MF").with_extension("3mf")); // lower case spelling of C.3mf
    add_model(&conn, &root.join("a\u{308}.3mf")); // NFD spelling of the NFC file name
    let a = anchored(&[&root]);
    let k = known(&conn);
    for name in ["a.3mf", "b.3mf", "C.3mf", "\u{e4}.3mf"] {
        assert_eq!(plan_new_file(&a, &k, 0, name).err(), Some(SkipReason::AlreadyInCatalog), "{name}");
    }
    assert!(plan_new_file(&a, &k, 0, "fresh.3mf").is_ok());
}

#[test]
fn a_file_changed_replaced_or_removed_after_the_preview_is_not_imported() {
    let root = dir("da-changed");
    let conn = catalog();
    let a = anchored(&[&root]);
    let file = root.join("m.3mf");

    put(&file, &three_mf(10));
    let row = plan_new(&a, &conn, "m.3mf");
    put(&file, &three_mf(20)); // different length (same inode)
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Changed), "length");

    put(&file, &three_mf(10));
    let row = plan_new(&a, &conn, "m.3mf");
    let t = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    fs::File::options().write(true).open(&file).unwrap().set_modified(t).unwrap();
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Changed), "mtime");

    put(&file, &three_mf(10));
    let row = plan_new(&a, &conn, "m.3mf");
    let other = root.join("other.tmp");
    fs::write(&other, three_mf(10)).unwrap();
    fs::rename(&other, &file).unwrap(); // same bytes, other inode
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Changed), "replaced by rename");

    let row = plan_new(&a, &conn, "m.3mf");
    fs::remove_file(&file).unwrap();
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Changed), "removed");
}

#[test]
fn links_are_refused_as_last_component_in_the_middle_and_out_of_the_root() {
    let root = dir("da-links");
    let outside = dir("da-links-outside");
    put(&outside.join("secret.3mf"), &three_mf(10));
    put(&root.join("real/x.3mf"), &three_mf(10));
    put(&root.join("a/b/c/deep.3mf"), &three_mf(10));
    symlink(outside.join("secret.3mf"), root.join("file_link.3mf")).unwrap();
    symlink(&outside, root.join("dir_link")).unwrap();
    let conn = catalog();
    let a = anchored(&[&root]);
    let k = known(&conn);
    assert_eq!(plan_new_file(&a, &k, 0, "file_link.3mf").err(), Some(SkipReason::Outside));
    assert_eq!(plan_new_file(&a, &k, 0, "dir_link/secret.3mf").err(), Some(SkipReason::Outside));
    // A link in the middle at every depth: the one-folder-up alias of a real folder.
    for (link, target, rel) in [
        ("a_link", "a", "a_link/b/c/deep.3mf"),
        ("a/b_link", "a/b", "a/b_link/c/deep.3mf"),
        ("a/b/c_link", "a/b/c", "a/b/c_link/deep.3mf"),
    ] {
        symlink(root.join(target), root.join(link)).unwrap();
        assert_eq!(plan_new_file(&a, &k, 0, rel).err(), Some(SkipReason::Outside), "{rel}");
    }
    // Presence never calls a path behind a link "missing".
    assert_eq!(a.presence(0, "dir_link/nothing.3mf"), Presence::Unknown);
    assert_eq!(a.presence(0, "file_link.3mf"), Presence::Other);
    assert_eq!(a.presence(0, "nothing/x.3mf"), Presence::Missing);
}

#[test]
fn a_folder_swapped_for_a_link_to_the_same_folder_after_the_preview_is_refused() {
    let root = dir("da-swap-dir");
    put(&root.join("sub/m.3mf"), &three_mf(10));
    let conn = catalog();
    let a = anchored(&[&root]);
    let row = plan_new(&a, &conn, "sub/m.3mf");
    // Same inode, length and mtime behind the new name: only the component walk can notice.
    fs::rename(root.join("sub"), root.join("sub_real")).unwrap();
    symlink(root.join("sub_real"), root.join("sub")).unwrap();
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Outside));
    // And during the commit step as well (link formed after reading).
    fs::remove_file(root.join("sub")).unwrap();
    fs::rename(root.join("sub_real"), root.join("sub")).unwrap();
    let mut conn = conn;
    let prepared = read(&row, &a).unwrap();
    fs::rename(root.join("sub"), root.join("sub_real")).unwrap();
    symlink(root.join("sub_real"), root.join("sub")).unwrap();
    let before = dump(&conn);
    assert_eq!(store_new_file(&mut conn, &a, prepared).err(), Some(SkipReason::Outside));
    assert_eq!(dump(&conn), before, "rolled back");
}

#[test]
fn excluded_folders_are_not_read_directly_or_below() {
    let root = dir("da-excl");
    put(&root.join("uploads/direct.3mf"), &three_mf(10));
    put(&root.join("uploads/sub/deep.3mf"), &three_mf(10));
    put(&root.join("ok/m.3mf"), &three_mf(10));
    let a = Anchored::new(std::slice::from_ref(&root), &[root.join("uploads")]).unwrap();
    let conn = catalog();
    let k = known(&conn);
    assert_eq!(plan_new_file(&a, &k, 0, "uploads/direct.3mf").err(), Some(SkipReason::Outside));
    assert_eq!(plan_new_file(&a, &k, 0, "uploads/sub/deep.3mf").err(), Some(SkipReason::Outside));
    assert!(plan_new_file(&a, &k, 0, "ok/m.3mf").is_ok());
    // The root trash folder is a name rule (hidden) and a handle rule.
    assert_eq!(plan_new_file(&a, &k, 0, ".katalog-papierkorb/x.3mf").err(), Some(SkipReason::Outside));
    assert_eq!(a.presence(0, "uploads/direct.3mf"), Presence::Unknown);
}

#[test]
fn the_current_path_entry_is_checked_before_the_commit_and_the_catalog_stays_byte_equal() {
    let root = dir("da-commit");
    let file = root.join("d/m.3mf");
    let setup = |name: &str| {
        let _ = name;
        put(&file, &three_mf(10));
    };
    setup("x");
    let mut conn = catalog();
    let a = anchored(&[&root]);

    // Control: nothing changed -> stored.
    let row = plan_new(&a, &conn, "d/m.3mf");
    let prepared = read(&row, &a).unwrap();
    let ok_conn = &mut catalog();
    assert!(store_new_file(ok_conn, &a, prepared).is_ok());

    let before = dump(&conn);
    // Another file moved over the name while the handle still describes the old inode.
    let row = plan_new(&a, &conn, "d/m.3mf");
    let prepared = read(&row, &a).unwrap();
    fs::write(root.join("d/swap.tmp"), three_mf(10)).unwrap();
    fs::rename(root.join("d/swap.tmp"), &file).unwrap();
    assert_eq!(store_new_file(&mut conn, &a, prepared).err(), Some(SkipReason::Changed), "swapped");
    assert_eq!(dump(&conn), before);

    // Removed after reading.
    let row = plan_new(&a, &conn, "d/m.3mf");
    let prepared = read(&row, &a).unwrap();
    fs::remove_file(&file).unwrap();
    assert_eq!(store_new_file(&mut conn, &a, prepared).err(), Some(SkipReason::Changed), "removed");
    assert_eq!(dump(&conn), before);

    // Replaced by a link to the real (still open) file.
    put(&file, &three_mf(10));
    let row = plan_new(&a, &conn, "d/m.3mf");
    let prepared = read(&row, &a).unwrap();
    fs::rename(&file, root.join("d/moved.3mf")).unwrap();
    symlink(root.join("d/moved.3mf"), &file).unwrap();
    assert_eq!(store_new_file(&mut conn, &a, prepared).err(), Some(SkipReason::Outside), "linked");
    assert_eq!(dump(&conn), before);

    // Rewritten in place after reading (the handle sees it).
    fs::remove_file(&file).unwrap();
    put(&file, &three_mf(10));
    let row = plan_new(&a, &conn, "d/m.3mf");
    let prepared = read(&row, &a).unwrap();
    put(&file, &three_mf(99));
    assert_eq!(store_new_file(&mut conn, &a, prepared).err(), Some(SkipReason::Changed), "rewritten");
    assert_eq!(dump(&conn), before);
}

#[test]
fn a_file_rewritten_while_it_is_read_is_changed() {
    let root = dir("da-rewrite");
    put(&root.join("m.3mf"), &three_mf(10));
    let conn = catalog();
    let a = anchored(&[&root]);
    let row = plan_new(&a, &conn, "m.3mf");
    // The stamp check after parsing: change the file between plan and read.
    let t = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    fs::File::options().write(true).open(root.join("m.3mf")).unwrap().set_modified(t).unwrap();
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Changed));
}

#[test]
fn read_limits_bad_archives_and_step_without_a_reader() {
    let root = dir("da-limits");
    put(&root.join("big.3mf"), &three_mf(10));
    put(&root.join("broken.3mf"), b"this is not a zip archive");
    put(&root.join("part.step"), b"ISO-10303-21;");
    let conn = catalog();
    let a = anchored(&[&root]);
    let tiny = WebImportLimits { max_file_bytes: 10, ..WebImportLimits::default() };
    let row = plan_new(&a, &conn, "big.3mf");
    assert_eq!(read_new_file(&row, &a, tiny, None).err(), Some(SkipReason::TooLarge));
    let row = plan_new(&a, &conn, "broken.3mf");
    assert_eq!(read(&row, &a).err(), Some(SkipReason::Invalid));
    let row = plan_new(&a, &conn, "part.step");
    assert_eq!(read(&row, &a).err(), Some(SkipReason::StepUnavailable));
    let reader = |_: &fs::File| Ok((Some([1.0, 2.0, 3.0]), Some(4.0), Some(1)));
    let prepared = read_new_file(&row, &a, WebImportLimits::default(), Some(&reader)).unwrap();
    let mut conn = conn;
    let id = store_new_file(&mut conn, &a, prepared).unwrap();
    assert_eq!(db::get_file(&conn, id).unwrap().unwrap().dimensions_mm, Some([1.0, 2.0, 3.0]));
}

#[test]
fn folder_chain_only_writes_rows_attaches_a_new_root_and_is_idempotent() {
    let base = dir("da-chain");
    let root = base.join("models");
    fs::create_dir_all(root.join("a/b")).unwrap();
    let conn = catalog();
    let base_id = db::ensure_folder_path(&conn, &base, &base).unwrap();
    let leaf = ensure_folder_chain(&conn, &root, &root.join("a/b")).unwrap();
    let rows: Vec<(String, Option<i64>)> = db::list_folders(&conn).unwrap().into_iter().map(|f| (f.path, f.parent_id)).collect();
    assert_eq!(rows.len(), 4);
    let models = rows.iter().find(|(p, _)| *p == root.to_str().unwrap()).unwrap();
    assert_eq!(models.1, Some(base_id), "the new root hangs below its cataloged parent");
    assert_eq!(ensure_folder_chain(&conn, &root, &root.join("a/b")).unwrap(), leaf);
    assert_eq!(db::list_folders(&conn).unwrap().len(), 4);
    assert!(ensure_folder_chain(&conn, &root, &base.join("elsewhere")).is_err());
    assert!(ensure_folder_chain(&conn, &root, &root.join("a/../..")).is_err());
    // No directory was created by the rows.
    assert!(!root.join("a/b/never").exists());
}

// ---- missing models ----------------------------------------------------------------------

#[test]
fn a_missing_model_passes_the_checks_and_each_deviation_skips_it() {
    let root = dir("da-missing");
    put(&root.join("keep.3mf"), &three_mf(10));
    let conn = catalog();
    add_model(&conn, &root.join("keep.3mf"));
    let id = add_model(&conn, &root.join("gone/m.3mf"));
    conn.execute("UPDATE files SET content_hash = 'h1' WHERE id = ?1", [id]).unwrap();
    let a = anchored(&[&root]);
    let row = plan_missing_model(&conn, &a, id).unwrap();
    assert_eq!(row.key, id.to_string());
    assert_eq!(recheck_missing(&row, &conn, &a), Ok(()));

    // Back again: exists_again (File, and Other for a folder or a link).
    put(&root.join("gone/m.3mf"), &three_mf(10));
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::ExistsAgain));
    fs::remove_file(root.join("gone/m.3mf")).unwrap();
    fs::create_dir(root.join("gone/m.3mf")).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::ExistsAgain));
    fs::remove_dir(root.join("gone/m.3mf")).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Ok(()));

    // The row changed.
    conn.execute("UPDATE files SET content_hash = 'h2' WHERE id = ?1", [id]).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::Changed));
    conn.execute("UPDATE files SET content_hash = 'h1', path = ?2 WHERE id = ?1", rusqlite::params![id, root.join("gone/n.3mf").to_str().unwrap()]).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::Changed));
    conn.execute("UPDATE files SET path = ?2, deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1", rusqlite::params![id, root.join("gone/m.3mf").to_str().unwrap()]).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::Changed));
    conn.execute("UPDATE files SET deleted_at = NULL WHERE id = ?1", [id]).unwrap();
    assert_eq!(recheck_missing(&row, &conn, &a), Ok(()));

    // A link in the way: never "missing".
    let outside = dir("da-missing-out");
    fs::remove_dir_all(root.join("gone")).unwrap();
    symlink(&outside, root.join("gone")).unwrap();
    assert_eq!(plan_missing_model(&conn, &a, id).err(), Some(SkipReason::Unclear));
    assert_eq!(recheck_missing(&row, &conn, &a), Err(SkipReason::Unclear));
}

#[test]
fn a_replaced_emptied_or_unreachable_root_never_makes_models_or_companions_missing() {
    let base = dir("da-roots");
    let root = base.join("models");
    put(&root.join("a.3mf"), &three_mf(10));
    put(&root.join("x_extras/doc.pdf"), b"%PDF");
    let conn = catalog();
    let model = add_model(&conn, &root.join("a.3mf"));
    let companion = add_companion(&conn, model, &root.join("x_extras/doc.pdf"));
    // Both gone from disk, root still fine: planned.
    fs::remove_file(root.join("a.3mf")).unwrap();
    fs::remove_file(root.join("x_extras/doc.pdf")).unwrap();
    put(&root.join("other.stl"), b"solid x"); // the root is not empty
    let a = anchored(&[&root]);
    let m_row = plan_missing_model(&conn, &a, model).unwrap();
    let c_row = plan_dangling_companion(&conn, &a, companion).unwrap();
    assert_eq!((recheck_missing(&m_row, &conn, &a), recheck_dangling(&c_row, &conn, &a)), (Ok(()), Ok(())));

    // 1. Root replaced by an empty folder (other inode), checked with the old and with a new context.
    fs::rename(&root, base.join("models_old")).unwrap();
    fs::create_dir(&root).unwrap();
    let fresh = anchored(&[&root]);
    for ctx in [&a, &fresh] {
        assert_eq!(recheck_missing(&m_row, &conn, ctx), Err(SkipReason::RootUnavailable));
        assert_eq!(recheck_dangling(&c_row, &conn, ctx), Err(SkipReason::RootUnavailable));
    }
    let new_file_row = {
        fs::rename(&root, base.join("models_empty")).unwrap();
        fs::rename(base.join("models_old"), &root).unwrap();
        put(&root.join("n.3mf"), &three_mf(10));
        let a2 = anchored(&[&root]);
        let row = plan_new(&a2, &conn, "n.3mf");
        fs::rename(&root, base.join("models_old")).unwrap();
        fs::rename(base.join("models_empty"), &root).unwrap();
        row
    };
    put(&root.join("n.3mf"), &three_mf(10)); // same name, other root inode
    assert_eq!(read(&new_file_row, &anchored(&[&root])).err(), Some(SkipReason::RootUnavailable), "import into a replaced root");

    // 2. Same inode but emptied (unmounted-volume look): the catalog holds models there.
    fs::remove_dir_all(&root).unwrap();
    fs::rename(base.join("models_old"), &root).unwrap();
    let a3 = anchored(&[&root]);
    let m_row = plan_missing_model(&conn, &a3, model).unwrap();
    let c_row = plan_dangling_companion(&conn, &a3, companion).unwrap();
    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { fs::remove_dir_all(path).unwrap() } else { fs::remove_file(path).unwrap() }
    }
    assert_eq!(recheck_missing(&m_row, &conn, &a3), Err(SkipReason::RootUnavailable), "emptied");
    assert_eq!(recheck_dangling(&c_row, &conn, &a3), Err(SkipReason::RootUnavailable), "emptied");
    assert_eq!(plan_missing_model(&conn, &a3, model).err(), Some(SkipReason::RootUnavailable));

    // 3. Root gone altogether.
    fs::remove_dir_all(&root).unwrap();
    let a4 = anchored(&[&root]);
    assert_eq!(plan_missing_model(&conn, &a4, model).err(), Some(SkipReason::RootUnavailable));
    assert_eq!(plan_dangling_companion(&conn, &a4, companion).err(), Some(SkipReason::RootUnavailable));
    assert_eq!(recheck_missing(&m_row, &conn, &a4), Err(SkipReason::RootUnavailable));
}

#[test]
fn an_unreachable_root_does_not_break_the_others() {
    let good = dir("da-multi-good");
    let gone = dir("da-multi-gone").join("not-there");
    put(&good.join("m.3mf"), &three_mf(10));
    let a = Anchored::new(&[gone.clone(), good.clone()], &[]).unwrap();
    let order = disk_report::ordered_roots(&[gone.clone(), good.clone()]);
    let good_index = order.iter().position(|p| *p == good).unwrap();
    let gone_index = order.iter().position(|p| *p == gone).unwrap();
    assert!(a.root_identity(good_index).is_some());
    assert!(a.root_identity(gone_index).is_none());
    assert!(plan_new_file(&a, &known(&catalog()), good_index, "m.3mf").is_ok());
    assert_eq!(plan_new_file(&a, &known(&catalog()), gone_index, "m.3mf").err(), Some(SkipReason::RootUnavailable));
}

// ---- dangling companions -----------------------------------------------------------------

#[test]
fn a_dangling_companion_is_rechecked_with_the_same_rigor() {
    let root = dir("da-comp");
    put(&root.join("m.3mf"), &three_mf(10));
    put(&root.join("keep.stl"), b"solid x");
    let conn = catalog();
    let model = add_model(&conn, &root.join("m.3mf"));
    let lost = add_companion(&conn, model, &root.join("m_extras/lost.pdf"));
    let folder = add_companion(&conn, model, &root.join("m_extras/folder.pdf"));
    fs::create_dir_all(root.join("m_extras/folder.pdf")).unwrap();
    let a = anchored(&[&root]);
    let row = plan_dangling_companion(&conn, &a, lost).unwrap();
    assert_eq!(row.file_id(), Some(model));
    assert_eq!(recheck_dangling(&row, &conn, &a), Ok(()));
    // Something that is not a file is planned but skipped when applying.
    let other = plan_dangling_companion(&conn, &a, folder).unwrap();
    assert_eq!(recheck_dangling(&other, &conn, &a), Err(SkipReason::NotAFile));
    // Back again.
    put(&root.join("m_extras/lost.pdf"), b"%PDF");
    assert_eq!(recheck_dangling(&row, &conn, &a), Err(SkipReason::ExistsAgain));
    assert_eq!(plan_dangling_companion(&conn, &a, lost).err(), Some(SkipReason::ExistsAgain));
    fs::remove_file(root.join("m_extras/lost.pdf")).unwrap();
    // The model went to the trash meanwhile / the row was deleted.
    conn.execute("UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1", [model]).unwrap();
    assert_eq!(recheck_dangling(&row, &conn, &a), Err(SkipReason::Changed));
    conn.execute("UPDATE files SET deleted_at = NULL WHERE id = ?1", [model]).unwrap();
    conn.execute("UPDATE companion_files SET path = ?2 WHERE id = ?1", rusqlite::params![lost, root.join("m_extras/other.pdf").to_str().unwrap()]).unwrap();
    assert_eq!(recheck_dangling(&row, &conn, &a), Err(SkipReason::Changed));
    // A link on the way is unclear, never "missing".
    conn.execute("UPDATE companion_files SET path = ?2 WHERE id = ?1", rusqlite::params![lost, root.join("m_extras/lost.pdf").to_str().unwrap()]).unwrap();
    fs::remove_dir_all(root.join("m_extras")).unwrap();
    symlink(dir("da-comp-out"), root.join("m_extras")).unwrap();
    assert_eq!(recheck_dangling(&row, &conn, &a), Err(SkipReason::Unclear));
}

#[test]
fn rows_outside_every_root_are_outside_and_codes_are_stable() {
    let root = dir("da-out");
    let other = dir("da-out-other");
    let conn = catalog();
    let id = add_model(&conn, &other.join("m.3mf"));
    let a = anchored(&[&root]);
    assert_eq!(plan_missing_model(&conn, &a, id).err(), Some(SkipReason::Outside));
    assert_eq!(plan_missing_model(&conn, &a, 9999).err(), Some(SkipReason::Changed));
    assert_eq!(SkipReason::ExistsAgain.code(), "exists_again");
    assert_eq!(serde_json::to_string(&SkipReason::AlreadyInCatalog).unwrap(), "\"already_in_catalog\"");
}

#[test]
fn a_deleted_subfolder_makes_its_models_missing_but_a_file_or_link_on_the_way_is_unknown() {
    let root = dir("da-parent-rule");
    let outside = dir("da-parent-rule-out");
    put(&root.join("keep.3mf"), &three_mf(10));
    put(&root.join("sub/m.3mf"), &three_mf(10));
    put(&root.join("blocker"), b"not a folder");
    symlink(&outside, root.join("link")).unwrap();
    let conn = catalog();
    let in_deleted = add_model(&conn, &root.join("sub/deeper/m.3mf"));
    let a = anchored(&[&root]);
    // The first absent component is `NotFound` and everything before it is a real folder.
    assert_eq!(a.presence(0, "gone/deeper/m.3mf"), Presence::Missing);
    assert_eq!(a.presence(0, "sub/gone/deeper/m.3mf"), Presence::Missing);
    assert!(plan_missing_model(&conn, &a, in_deleted).is_ok());
    // A file where a folder should be (ENOTDIR) and a link are not "gone".
    assert_eq!(a.presence(0, "blocker/m.3mf"), Presence::Unknown);
    assert_eq!(a.presence(0, "link/m.3mf"), Presence::Unknown);
    assert_eq!(a.presence(0, "sub/m.3mf/deeper.3mf"), Presence::Unknown);
    let blocked = add_model(&conn, &root.join("blocker/m.3mf"));
    assert_eq!(plan_missing_model(&conn, &a, blocked).err(), Some(SkipReason::Unclear));
}

#[test]
fn an_unusable_root_stays_root_unavailable_even_if_its_subfolder_is_gone() {
    let root = dir("da-parent-rule-root");
    put(&root.join("sub/m.3mf"), &three_mf(10));
    let conn = catalog();
    let id = add_model(&conn, &root.join("sub/m.3mf"));
    let a = anchored(&[&root]);
    fs::remove_dir_all(root.join("sub")).unwrap();
    // The root is empty although the catalog holds models below it: not trusted.
    assert_eq!(plan_missing_model(&conn, &a, id).err(), Some(SkipReason::RootUnavailable));
}
