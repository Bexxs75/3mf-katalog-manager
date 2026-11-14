//! The desktop rescan in three phases: reading a file (hash, parse) must neither hold the
//! database lock nor the catalog gate; the write step is exclusive against imports and
//! refuses a file that changed in between. Synthetic archives and OBJ files only.
use super::*;
use katalog_core::import::CatalogGate;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const OBJ: &[u8] = b"v 0 0 0\nv 10 0 0\nv 0 20 0\nf 1 2 3\n";

/// A 3MF with a mesh of `triangles` triangles, so reading it takes measurable time.
fn write_3mf(path: &Path, triangles: usize) {
    let mut vertices = String::new();
    let mut faces = String::new();
    for i in 0..triangles {
        let x = i as f64;
        vertices.push_str(&format!(r#"<vertex x="{x}" y="0" z="0"/><vertex x="{x}" y="1" z="0"/><vertex x="{x}" y="0" z="1"/>"#));
        faces.push_str(&format!(r#"<triangle v1="{}" v2="{}" v3="{}"/>"#, i * 3, i * 3 + 1, i * 3 + 2));
    }
    let model = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices>{vertices}</vertices><triangles>{faces}</triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#
    );
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
        zip.start_file("3D/3dmodel.model", options).unwrap();
        zip.write_all(model.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    std::fs::write(path, buf).unwrap();
}

fn catalog_with(paths: &[&Path]) -> (Mutex<Connection>, Vec<i64>) {
    let conn = crate::db::connect_in_memory().expect("connect");
    let ids = paths
        .iter()
        .map(|p| crate::db::test_insert_minimal_file(&conn, p.to_str().unwrap(), None).unwrap())
        .collect();
    (Mutex::new(conn), ids)
}

fn gate() -> Arc<CatalogGate> {
    Arc::new(CatalogGate::default())
}

fn content_hash(db: &Mutex<Connection>, id: i64) -> Option<String> {
    crate::db::get_file(&db.lock().unwrap(), id).unwrap().unwrap().content_hash
}

fn print_info_rows(db: &Mutex<Connection>) -> i64 {
    db.lock().unwrap().query_row("SELECT COUNT(*) FROM file_print_info", [], |r| r.get(0)).unwrap()
}

fn rescan(db: &Mutex<Connection>, gate: &Arc<CatalogGate>, id: i64, lean: bool) -> CmdResult<ModelFileDto> {
    rescan_in_phases(db, gate, id, lean, &|_| {}).map(|(dto, _)| dto)
}

#[test]
fn h1_reading_holds_neither_the_database_lock_nor_the_catalog_gate() {
    let dir = unique_test_dir("rescan_phase_h1");
    let path = dir.join("a.3mf");
    write_3mf(&path, 50);
    let (db, ids) = catalog_with(&[&path]);
    let gate = gate();
    let reached = std::cell::Cell::new(false);
    let hook = |_: &katalog_core::rescan::PreparedRescan| {
        reached.set(true);
        assert!(db.try_lock().is_ok(), "the database lock must be free while the file is read");
        let free = gate.try_background_exclusive().unwrap();
        assert!(free.is_some(), "the catalog gate must be free while the file is read");
    };
    rescan_in_phases(&db, &gate, ids[0], false, &hook).unwrap();
    assert!(reached.get(), "the hook between the read and the write step must run");
    assert!(content_hash(&db, ids[0]).is_some());
}

#[test]
fn h2_a_file_changed_after_reading_is_refused_and_the_row_stays_as_it_was() {
    let dir = unique_test_dir("rescan_phase_h2");
    let path = dir.join("a.obj");
    std::fs::write(&path, OBJ).unwrap();
    let (db, ids) = catalog_with(&[&path]);
    let gate = gate();
    let rewrite = |_: &katalog_core::rescan::PreparedRescan| std::fs::write(&path, [OBJ, b"v 1 1 1\n"].concat()).unwrap();
    let err = rescan_in_phases(&db, &gate, ids[0], false, &rewrite).unwrap_err();
    assert_eq!((err.message.as_str(), err.expected), ("rescan_file_changed", true));
    assert_eq!(content_hash(&db, ids[0]), None);
    // Removed after reading: the file is no longer the one that was read.
    let remove = |_: &katalog_core::rescan::PreparedRescan| std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, OBJ).unwrap();
    let err = rescan_in_phases(&db, &gate, ids[0], false, &remove).unwrap_err();
    assert_eq!(err.message, "rescan_file_changed");
    assert_eq!(content_hash(&db, ids[0]), None);
    // Gone before reading starts: the old message with the stored path.
    let err = rescan(&db, &gate, ids[0], false).unwrap_err();
    assert_eq!(err.message, format!("Datei nicht gefunden: {}", path.display()));
    assert_eq!(content_hash(&db, ids[0]), None);
    // The gate was released on every failure.
    std::fs::write(&path, OBJ).unwrap();
    assert!(rescan(&db, &gate, ids[0], false).is_ok());
}

#[test]
fn h3_the_write_step_waits_for_no_import_and_refuses_while_one_runs() {
    let dir = unique_test_dir("rescan_phase_h3");
    let path = dir.join("a.3mf");
    write_3mf(&path, 5);
    let (db, ids) = catalog_with(&[&path]);
    let gate = gate();
    let share = gate.import();
    let err = rescan(&db, &gate, ids[0], false).unwrap_err();
    assert_eq!((err.message.as_str(), err.expected), ("importActive", true));
    assert_eq!(content_hash(&db, ids[0]), None);
    assert_eq!(print_info_rows(&db), 0);
    drop(share);
    assert!(rescan(&db, &gate, ids[0], false).is_ok());
    assert_eq!(print_info_rows(&db), 1);
    // An import that starts between the read and the write step: same answer, nothing written.
    let other = dir.join("b.3mf");
    write_3mf(&other, 5);
    let (db, ids) = catalog_with(&[&other]);
    let held = std::cell::RefCell::new(None);
    let start_import = |_: &katalog_core::rescan::PreparedRescan| *held.borrow_mut() = Some(gate.import());
    let err = rescan_in_phases(&db, &gate, ids[0], false, &start_import).unwrap_err();
    assert_eq!(err.message, "importActive");
    assert_eq!(print_info_rows(&db), 0);
}

#[test]
fn h6_lean_leaves_the_images_out_of_the_answer_but_not_out_of_the_database() {
    let dir = unique_test_dir("rescan_phase_h6");
    let path = dir.join("a.3mf");
    write_3mf(&path, 5);
    let (db, ids) = catalog_with(&[&path]);
    let id = ids[0];
    crate::db::set_custom_image_png(&db.lock().unwrap(), id, &[1, 2, 3]).unwrap();
    let gate = gate();
    let full = rescan(&db, &gate, id, false).unwrap();
    assert!(full.custom_image.is_some(), "without lean the answer is complete");
    let lean = rescan(&db, &gate, id, true).unwrap();
    assert!(lean.custom_image.is_none() && lean.thumbnail_image.is_none() && lean.render_snapshot_image.is_none());
    // Everything else is identical to the full answer.
    assert_eq!(lean.content_hash, full.content_hash);
    let state = |d: &ModelFileDto| d.print_info.as_ref().map(|p| (p.slice_state.clone(), p.settings_state.clone()));
    assert_eq!(state(&lean), state(&full));
    assert_eq!(lean.dimensions_mm, full.dimensions_mm);
    // The image is still stored, and the next full answer carries it again.
    assert_eq!(crate::db::get_file(&db.lock().unwrap(), id).unwrap().unwrap().custom_image_png, Some(vec![1, 2, 3]));
    assert!(rescan(&db, &gate, id, false).unwrap().custom_image.is_some());
}

/// 200 files with another command running meanwhile and an import starting mid-run.
#[test]
fn h5_a_run_over_200_files_keeps_the_app_responsive_and_pauses_for_an_import() {
    const FILES: usize = 200;
    let dir = unique_test_dir("rescan_phase_h5");
    let paths: Vec<PathBuf> = (0..FILES)
        .map(|i| {
            let p = dir.join(format!("m{i}.3mf"));
            write_3mf(&p, 3000 + i);
            p
        })
        .collect();
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let (db, ids) = catalog_with(&refs);
    let (db, gate) = (Arc::new(db), gate());
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Another command: list the summaries over and over, timing each answer including the wait for the lock.
    let probe = {
        let (db, done) = (db.clone(), done.clone());
        std::thread::spawn(move || {
            let mut durations = Vec::new();
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                let started = Instant::now();
                let conn = db.lock().unwrap();
                let rows = katalog_core::read::list_file_summaries(&conn).unwrap().len();
                drop(conn);
                durations.push(started.elapsed());
                assert_eq!(rows, FILES);
                std::thread::sleep(Duration::from_millis(2));
            }
            durations
        })
    };

    // The run: one file at a time, `importActive` is a pause.
    let runner = {
        let (db, gate, ids) = (db.clone(), gate.clone(), ids.clone());
        std::thread::spawn(move || {
            let (mut holds, mut paused, mut hash_parse) = (Vec::new(), 0usize, Vec::new());
            let started = Instant::now();
            for id in ids {
                loop {
                    let file_started = Instant::now();
                    match rescan_in_phases(&db, &gate, id, true, &|_| {}) {
                        Ok((_, held)) => {
                            hash_parse.push(file_started.elapsed());
                            holds.push(held);
                            break;
                        }
                        Err(e) if e.message == "importActive" => {
                            paused += 1;
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(e) => panic!("rescan failed: {}", e.message),
                    }
                }
            }
            (holds, paused, hash_parse, started.elapsed())
        })
    };

    // An import starts in the middle of the run, holds its share for a while, and writes nothing meanwhile.
    while print_info_rows(&db) < (FILES / 2) as i64 {
        std::thread::sleep(Duration::from_millis(2));
    }
    let share = gate.import();
    let rows_at_import = print_info_rows(&db);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(print_info_rows(&db), rows_at_import, "no print data may be written while an import holds the gate");
    drop(share);

    let (holds, paused, hash_parse, total) = runner.join().unwrap();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut latencies = probe.join().unwrap();

    latencies.sort();
    let p95 = latencies[(latencies.len() * 95 / 100).min(latencies.len() - 1)];
    let max_hold = holds.iter().max().copied().unwrap();
    let mean_hold = holds.iter().sum::<Duration>() / holds.len() as u32;
    let mean_file = hash_parse.iter().sum::<Duration>() / hash_parse.len() as u32;
    eprintln!(
        "rescan phases: {FILES} files, total {total:?}, per file (all phases) {mean_file:?}, write step lock hold mean {mean_hold:?} max {max_hold:?}, \
         other command {} answers p95 {p95:?} max {:?}, paused {paused} times",
        latencies.len(),
        latencies.last().unwrap()
    );
    assert!(p95 < Duration::from_millis(200), "p95 of the other command was {p95:?}");
    assert!(max_hold < Duration::from_millis(50), "the write step held the lock for {max_hold:?}");
    assert!(paused > 0, "the import must have paused the run");

    // Criterion 6: the database is intact and every file was read.
    let conn = db.lock().unwrap();
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(integrity, "ok");
    assert_eq!(print_info_rows_locked(&conn), FILES as i64);
}

fn print_info_rows_locked(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM file_print_info", [], |r| r.get(0)).unwrap()
}
