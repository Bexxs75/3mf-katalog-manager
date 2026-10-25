use super::*;
use crate::unique_test_dir;
use std::io::Write;

const TRIANGLE_OBJ: &[u8] = b"v 0 0 0\nv 10 0 0\nv 0 20 0\nf 1 2 3\n";
const TRIANGLE_STL: &[u8] = b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 20 0\nendloop\nendfacet\nendsolid x\n";

fn three_mf(size: u32) -> Vec<u8> {
    let xml = format!(
        r#"<model unit="millimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="{size}" y="0" z="0"/><vertex x="0" y="{size}" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn catalog(path: &Path) -> (Connection, i64) {
    let c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, path.to_str().unwrap(), None).unwrap();
    (c, id)
}

fn fails(result: Result<ModelFileDto>) -> RescanError {
    result.expect_err("rescan must fail")
}

fn limits() -> WebImportLimits {
    WebImportLimits::default()
}

#[test]
fn web_rescan_refreshes_3mf_stl_and_obj_below_the_root() {
    let root = unique_test_dir("rescan_formats");
    let cases: [(&str, Vec<u8>); 3] = [
        ("a.3mf", three_mf(30)),
        ("b.stl", TRIANGLE_STL.to_vec()),
        ("c.obj", TRIANGLE_OBJ.to_vec()),
    ];
    for (name, bytes) in cases {
        let path = root.join(name);
        std::fs::write(&path, &bytes).unwrap();
        let (mut c, id) = catalog(&path);
        let dto = rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits()).unwrap();
        assert_eq!(dto.id, id.to_string());
        assert_eq!(dto.file_size_bytes, bytes.len() as i64, "{name}");
        assert!(dto.dimensions_mm.is_some(), "{name}");
        assert!(dto.content_hash.is_some(), "{name}");
        // The stored record, not only the answer, carries the new values.
        let stored = db::get_file(&c, id).unwrap().unwrap();
        assert_eq!(stored.file_size_bytes, bytes.len() as i64);
    }
}

#[test]
fn web_rescan_picks_up_a_changed_file() {
    let root = unique_test_dir("rescan_changed");
    let path = root.join("a.3mf");
    std::fs::write(&path, three_mf(10)).unwrap();
    let (mut c, id) = catalog(&path);
    let first = rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits()).unwrap();
    std::fs::write(&path, three_mf(40)).unwrap();
    let second = rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits()).unwrap();
    assert_ne!(first.content_hash, second.content_hash);
    assert_ne!(first.dimensions_mm, second.dimensions_mm);
    // Running it again on an unchanged file yields the same record.
    let third = rescan_file_in_roots(&mut c, id, &[root], limits()).unwrap();
    assert_eq!(second.content_hash, third.content_hash);
    assert_eq!(second.dimensions_mm, third.dimensions_mm);
}

#[test]
fn web_rescan_reports_missing_unknown_and_trashed_without_paths() {
    let root = unique_test_dir("rescan_missing");
    let (mut c, id) = catalog(&root.join("gone.stl"));
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Missing(String::new()));
    assert_eq!(fails(rescan_file_in_roots(&mut c, 999, std::slice::from_ref(&root), limits())), RescanError::NotInCatalog);
    let path = root.join("t.stl");
    std::fs::write(&path, TRIANGLE_STL).unwrap();
    let (mut c, id) = catalog(&path);
    c.execute("UPDATE files SET deleted_at='2026-01-01T00:00:00Z' WHERE id=?1", [id]).unwrap();
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, &[root], limits())), RescanError::NotInCatalog);
}

#[test]
fn web_rescan_refuses_files_outside_the_roots_and_links() {
    let root = unique_test_dir("rescan_root");
    let other = unique_test_dir("rescan_other");
    let outside = other.join("o.stl");
    std::fs::write(&outside, TRIANGLE_STL).unwrap();
    let (mut c, id) = catalog(&outside);
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Outside);
    // `..` segments are resolved before the root check.
    let sneaky = root.join("..").join(other.file_name().unwrap()).join("o.stl");
    let (mut c, id) = catalog(&sneaky);
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Outside);
    #[cfg(unix)]
    {
        let inside = root.join("real.stl");
        std::fs::write(&inside, TRIANGLE_STL).unwrap();
        let link = root.join("link.stl");
        std::os::unix::fs::symlink(&inside, &link).unwrap();
        let (mut c, id) = catalog(&link);
        assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Outside);
        let out_link = root.join("out.stl");
        std::os::unix::fs::symlink(&outside, &out_link).unwrap();
        let (mut c, id) = catalog(&out_link);
        assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Outside);
        // A directory with a model extension is not a regular file.
        let dir = root.join("dir.stl");
        std::fs::create_dir(&dir).unwrap();
        let (mut c, id) = catalog(&dir);
        assert_eq!(fails(rescan_file_in_roots(&mut c, id, &[root], limits())), RescanError::Outside);
    }
}

#[test]
fn web_rescan_keeps_the_record_when_the_file_is_rejected() {
    let root = unique_test_dir("rescan_reject");
    let step = root.join("a.step");
    std::fs::write(&step, b"ISO-10303-21;").unwrap();
    let (mut c, id) = catalog(&step);
    c.execute("UPDATE files SET dimension_x_mm=1, dimension_y_mm=2, dimension_z_mm=3 WHERE id=?1", [id]).unwrap();
    for name in ["a.step", "a.stp"] {
        c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![root.join(name).to_str().unwrap(), id]).unwrap();
        assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::StepUnavailable);
    }
    let broken = root.join("broken.3mf");
    std::fs::write(&broken, b"not a zip").unwrap();
    c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![broken.to_str().unwrap(), id]).unwrap();
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Invalid);
    let small = WebImportLimits { max_mesh_file_bytes: 8, ..limits() };
    let obj = root.join("big.obj");
    std::fs::write(&obj, TRIANGLE_OBJ).unwrap();
    c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![obj.to_str().unwrap(), id]).unwrap();
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), small)), RescanError::TooLarge);
    let text = root.join("a.txt");
    std::fs::write(&text, b"x").unwrap();
    c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![text.to_str().unwrap(), id]).unwrap();
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, &[root], limits())), RescanError::Unsupported);
    let stored = db::get_file(&c, id).unwrap().unwrap();
    assert_eq!(stored.dimensions_mm, Some([1.0, 2.0, 3.0]));
}

#[test]
fn desktop_rescan_reads_the_stored_path_as_before() {
    let dir = unique_test_dir("rescan_desktop");
    let path = dir.join("a.obj");
    std::fs::write(&path, TRIANGLE_OBJ).unwrap();
    let (mut c, id) = catalog(&path);
    let dto = rescan_file(&mut c, id).unwrap();
    assert_eq!(dto.file_size_bytes, TRIANGLE_OBJ.len() as i64);
    assert!(dto.dimensions_mm.is_some());
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(fails(rescan_file(&mut c, id)), RescanError::Missing(p) if p == path.to_str().unwrap()));
    assert_eq!(fails(rescan_file(&mut c, 999)), RescanError::NotInCatalog);
}

#[cfg(unix)]
#[test]
fn web_rescan_refuses_a_link_to_an_outside_directory_and_special_files() {
    let root = unique_test_dir("rescan_dirlink_root");
    let other = unique_test_dir("rescan_dirlink_other");
    std::fs::write(other.join("o.stl"), TRIANGLE_STL).unwrap();
    // The final component is a regular file, but an intermediate directory leads out of the root.
    std::os::unix::fs::symlink(&other, root.join("dir")).unwrap();
    let (mut c, id) = catalog(&root.join("dir").join("o.stl"));
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Outside);
    // A FIFO must be rejected without being opened for reading (which would block).
    let fifo = root.join("pipe.stl");
    let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    let (mut c, id) = catalog(&fifo);
    let (tx, rx) = std::sync::mpsc::channel();
    let roots = vec![root.clone()];
    std::thread::spawn(move || tx.send(fails(rescan_file_in_roots(&mut c, id, &roots, limits()))).ok());
    assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(10)).expect("must not hang"), RescanError::Outside);
    // A device node cannot be below the roots at all.
    let (mut c, id) = catalog(Path::new("/dev/zero.stl"));
    assert!(matches!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Missing(_)));
    let (mut c, id) = catalog(Path::new("/dev/null"));
    assert_eq!(fails(rescan_file_in_roots(&mut c, id, std::slice::from_ref(&root), limits())), RescanError::Unsupported);
}

#[test]
fn write_step_rejects_a_file_or_row_that_changed_after_parsing() {
    let root = unique_test_dir("rescan_two_step");
    let path = root.join("a.obj");
    std::fs::write(&path, TRIANGLE_OBJ).unwrap();
    let (mut c, id) = catalog(&path);
    let roots = [root.clone()];
    let prepared = prepare_web_rescan(&c, id, &roots, limits()).unwrap();
    // Reading alone changes nothing.
    assert_eq!(db::get_file(&c, id).unwrap().unwrap().content_hash, None);
    std::fs::write(&path, [TRIANGLE_OBJ, b"v 1 1 1\n"].concat()).unwrap();
    assert_eq!(fails(apply_web_rescan(&mut c, prepared)), RescanError::Changed);
    assert_eq!(db::get_file(&c, id).unwrap().unwrap().content_hash, None);
    let prepared = prepare_web_rescan(&c, id, &roots, limits()).unwrap();
    c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![root.join("b.obj").to_str().unwrap(), id]).unwrap();
    assert_eq!(fails(apply_web_rescan(&mut c, prepared)), RescanError::Changed);
    c.execute("UPDATE files SET path=?1 WHERE id=?2", rusqlite::params![path.to_str().unwrap(), id]).unwrap();
    let prepared = prepare_web_rescan(&c, id, &roots, limits()).unwrap();
    c.execute("UPDATE files SET deleted_at='2026-01-01T00:00:00Z' WHERE id=?1", [id]).unwrap();
    assert_eq!(fails(apply_web_rescan(&mut c, prepared)), RescanError::Changed);
    c.execute("UPDATE files SET deleted_at=NULL WHERE id=?1", [id]).unwrap();
    let prepared = prepare_web_rescan(&c, id, &roots, limits()).unwrap();
    assert!(apply_web_rescan(&mut c, prepared).is_ok());
}
