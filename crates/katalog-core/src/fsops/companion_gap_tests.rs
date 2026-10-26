//! Die Web-Dateiverwaltung zieht Begleitdateien (`companion_files`, Ordner
//! `<Modell>_extras`) beim Umbenennen und Verschieben einzelner Modelle mit.
//! Papierkorb, endgueltiges Loeschen und Ordneroperationen lehnt sie weiter ab,
//! solange ein betroffenes Modell Begleitdateien hat. Die `#[ignore]`-Tests
//! beschreiben den Sollzustand fuer das spaetere Mitziehen dieser Operationen.
use super::*;
use std::{fs, path::PathBuf};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52,
];

#[derive(Default)]
struct Ledger;
impl UploadLedger for Ledger {
    fn reset_imported(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
    fn detach(&mut self, _: &[i64]) -> Result<()> {
        Ok(())
    }
}

struct Fx {
    dir: PathBuf,
    ops: FileOps<rusqlite::Connection, Ledger>,
    companion_id: i64,
}
impl Drop for Fx {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl Fx {
    fn root(&self) -> PathBuf {
        self.dir.join("root")
    }
    fn extras(&self) -> PathBuf {
        self.root().join("Foo_extras")
    }
    fn companion_path(&mut self) -> String {
        let id = self.companion_id;
        self.ops
            .db
            .query_row("SELECT path FROM companion_files WHERE id=?1", [id], |r| r.get(0))
            .unwrap()
    }
    fn companion_rows(&mut self) -> i64 {
        self.ops
            .db
            .query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0))
            .unwrap()
    }
}

/// Modell `Foo.3mf` mit `Foo_extras/pic.png` und passendem companion_files-Eintrag.
fn fixture() -> Fx {
    let dir = crate::unique_test_dir("fsops_companion_gap");
    let db = crate::db::connect_in_memory().unwrap();
    build(dir, db, true)
}

/// Datei-Datenbank ohne Begleitdatei, damit ein Test zwischen Vorpruefung und
/// Commit ueber eine zweite Verbindung eine Begleitdatei einfuegen kann.
fn fixture_on_file() -> (Fx, PathBuf) {
    let dir = crate::unique_test_dir("fsops_companion_gap");
    let path = dir.join("catalog.db");
    let db = crate::db::connect(&path).unwrap();
    (build(dir, db, false), path)
}

fn insert_companion_via_second_connection(db_path: &std::path::Path, file_id: i64, pic: &std::path::Path) {
    let other = crate::db::connect(db_path).unwrap();
    crate::db::companions::insert_companion(
        &other,
        file_id,
        "pic.png",
        pic.to_str().unwrap(),
        "image",
        Some(PNG.len() as i64),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
}

fn build(dir: PathBuf, db: rusqlite::Connection, with_companion: bool) -> Fx {
    fs::create_dir(dir.join("root")).unwrap();
    fs::create_dir(dir.join("root/sub")).unwrap();
    fs::create_dir(dir.join("journal")).unwrap();
    let context = FsContext::new(&[dir.join("root")], &[]).unwrap();
    let journal = Journal::open(&dir.join("journal")).unwrap();
    let model = dir.join("root/Foo.3mf");
    fs::write(&model, b"model").unwrap();
    fs::create_dir(dir.join("root/Foo_extras")).unwrap();
    let pic = dir.join("root/Foo_extras/pic.png");
    fs::write(&pic, PNG).unwrap();
    db.execute(
        "INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (1,'Foo.3mf',?1,'3mf',5,'2026-01-01T00:00:00Z')",
        [model.to_str().unwrap()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO folders(id,name,path) VALUES (1,'sub',?1)",
        [dir.join("root/sub").to_str().unwrap()],
    )
    .unwrap();
    let companion_id = if with_companion {
        crate::db::companions::insert_companion(
            &db,
            1,
            "pic.png",
            pic.to_str().unwrap(),
            "image",
            Some(PNG.len() as i64),
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
    } else {
        0
    };
    Fx {
        dir,
        ops: FileOps::new(context, journal, db, Ledger),
        companion_id,
    }
}

fn exists(p: &std::path::Path) -> bool {
    p.exists()
}

// ---- (a) Umbenennen ----

#[test]
fn soll_rename_zieht_extras_mit() {
    let mut f = fixture();
    f.ops.rename_file(1, "Bar").unwrap();
    let new = f.root().join("Bar_extras/pic.png");
    assert!(exists(&new));
    assert!(!exists(&f.extras()));
    assert_eq!(f.companion_path(), new.to_str().unwrap());
    let name: String = f
        .ops
        .db
        .query_row("SELECT name FROM companion_files WHERE id=?1", [f.companion_id], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "pic.png", "der Name der Begleitdatei bleibt");
    assert!(f.ops.journal.is_empty());
}

// ---- (b) Verschieben ----

#[test]
fn soll_move_zieht_extras_mit() {
    let mut f = fixture();
    f.ops.move_file_to_folder(1, Some(1)).unwrap();
    let new = f.root().join("sub/Foo_extras/pic.png");
    assert!(exists(&new));
    assert!(!exists(&f.extras()));
    assert_eq!(f.companion_path(), new.to_str().unwrap());
    assert!(f.ops.journal.is_empty());
}

// ---- (c) Papierkorb ----

#[test]
fn sperre_trash_mit_begleitdatei() {
    let mut f = fixture();
    let old = f.companion_path();
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
    assert!(exists(&f.root().join("Foo.3mf")));
    assert!(exists(&f.extras().join("pic.png")));
    assert_eq!(f.companion_path(), old);
    let deleted: Option<String> = f
        .ops
        .db
        .query_row("SELECT deleted_at FROM companion_files WHERE id=?1", [f.companion_id], |r| r.get(0))
        .unwrap();
    assert!(deleted.is_none(), "Begleitdatei nicht als gelöscht markiert");
}

#[test]
#[ignore = "Sperre aktiv; Mitziehen noch nicht umgesetzt"]
fn soll_trash_nimmt_extras_aus_dem_katalogordner() {
    let mut f = fixture();
    f.ops.delete_file(1).unwrap();
    assert!(!exists(&f.extras()), "Foo_extras darf nicht verwaist liegen bleiben");
    let deleted: Option<String> = f
        .ops
        .db
        .query_row("SELECT deleted_at FROM companion_files WHERE id=?1", [f.companion_id], |r| r.get(0))
        .unwrap();
    assert!(deleted.is_some(), "Begleitdatei als gelöscht markiert");
}

// ---- (d) Endgueltig loeschen ----

#[test]
fn sperre_delete_permanently_mit_begleitdatei() {
    let mut f = fixture();
    let old = f.companion_path();
    // Papierkorb-Zustand direkt herstellen, weil delete_file selbst gesperrt ist.
    f.ops
        .db
        .execute(
            "UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path=NULL WHERE id=1",
            [],
        )
        .unwrap();
    assert_eq!(f.ops.delete_file_permanently(1), Err(Error::HasCompanions));
    assert!(exists(&f.extras().join("pic.png")));
    assert_eq!(f.companion_rows(), 1);
    assert_eq!(f.companion_path(), old);
    let still: i64 = f.ops.db.query_row("SELECT COUNT(*) FROM files WHERE id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(still, 1);
}

#[test]
#[ignore = "Sperre aktiv; Mitziehen noch nicht umgesetzt"]
fn soll_delete_permanently_raeumt_extras_auf() {
    let mut f = fixture();
    f.ops.delete_file(1).unwrap();
    f.ops.delete_file_permanently(1).unwrap();
    assert!(!exists(&f.extras()), "Foo_extras darf nicht verwaist bleiben");
    assert_eq!(f.companion_rows(), 0);
    assert!(!contains_file_named(&f.root(), "pic.png"), "auch im Papierkorb darf pic.png nicht liegen");
}

// ---- Freie Modelle, Stapel, Datenbankpfad ----

/// Zweites Modell `Free.3mf` ohne Begleitdateien.
fn add_free(f: &mut Fx) {
    let p = f.root().join("Free.3mf");
    fs::write(&p, b"model").unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (2,'Free.3mf',?1,'3mf',5,'2026-01-01T00:00:00Z')",
            [p.to_str().unwrap()],
        )
        .unwrap();
}

#[test]
fn modell_ohne_begleitdatei_bleibt_bedienbar() {
    let mut f = fixture();
    add_free(&mut f);
    f.ops.rename_file(2, "Neu").unwrap();
    assert!(exists(&f.root().join("Neu.3mf")));
    f.ops.move_file_to_folder(2, Some(1)).unwrap();
    assert!(exists(&f.root().join("sub/Neu.3mf")));
    f.ops.delete_file(2).unwrap();
    f.ops.delete_file_permanently(2).unwrap();
    let n: i64 = f.ops.db.query_row("SELECT COUNT(*) FROM files WHERE id=2", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn stapel_loeschen_sperrt_nur_das_betroffene_modell() {
    let mut f = fixture();
    add_free(&mut f);
    let r = f.ops.delete_files(&[1, 2], || true).unwrap();
    assert!(!r[0].ok);
    assert_eq!(r[0].kind, Some(Error::HasCompanions));
    assert!(r[1].ok);
    assert!(exists(&f.root().join("Foo.3mf")));
    assert!(!exists(&f.root().join("Free.3mf")));
}

#[test]
fn stapel_verschieben_zieht_nur_das_betroffene_modell_samt_begleitdateien() {
    let mut f = fixture();
    add_free(&mut f);
    let r = f.ops.move_files_to_folder(&[1, 2], Some(1), || true).unwrap();
    assert!(r[0].ok && r[1].ok);
    assert!(exists(&f.root().join("sub/Foo.3mf")));
    assert!(exists(&f.root().join("sub/Foo_extras/pic.png")));
    assert!(exists(&f.root().join("sub/Free.3mf")));
    assert!(!exists(&f.extras()));
}

#[test]
fn papierkorb_leeren_laesst_gesperrtes_modell_unberuehrt() {
    let mut f = fixture();
    add_free(&mut f);
    f.ops.delete_file(2).unwrap();
    f.ops
        .db
        .execute(
            "UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path=NULL WHERE id=1",
            [],
        )
        .unwrap();
    let mut snap = f.ops.empty_trash_snapshot(1, Some("2030-01-01T00:00:00Z")).unwrap();
    let r = f.ops.empty_trash(&mut snap, || true).unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].ok);
    assert_eq!(f.companion_rows(), 1);
}

#[test]
fn verschieben_nur_in_der_datenbank_bleibt_erlaubt() {
    let mut f = fixture();
    f.ops.db.execute("UPDATE files SET folder_id=1 WHERE id=1", []).unwrap();
    f.ops.move_file_to_folder(1, None).unwrap();
    let fid: Option<i64> = f.ops.db.query_row("SELECT folder_id FROM files WHERE id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(fid, None);
    assert!(exists(&f.root().join("Foo.3mf")));
}

// ---- Ordner ----

/// Ordner `a` (id 2) mit Unterordner `a/b` (id 3), darin `Deep.3mf` (id 3).
fn add_nested(f: &mut Fx, with_companion: bool) {
    let a = f.root().join("a");
    let b = a.join("b");
    fs::create_dir_all(&b).unwrap();
    let m = b.join("Deep.3mf");
    fs::write(&m, b"model").unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path) VALUES (2,'a',?1)",
            [a.to_str().unwrap()],
        )
        .unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path,parent_id) VALUES (3,'b',?1,2)",
            [b.to_str().unwrap()],
        )
        .unwrap();
    f.ops
        .db
        .execute(
            "INSERT INTO files(id,name,path,folder_id,file_type,file_size_bytes,imported_at) VALUES (3,'Deep.3mf',?1,3,'3mf',5,'2026-01-01T00:00:00Z')",
            [m.to_str().unwrap()],
        )
        .unwrap();
    if with_companion {
        let pic = b.join("Deep_extras/pic.png");
        fs::create_dir(b.join("Deep_extras")).unwrap();
        fs::write(&pic, PNG).unwrap();
        crate::db::companions::insert_companion(
            &f.ops.db,
            3,
            "pic.png",
            pic.to_str().unwrap(),
            "image",
            Some(PNG.len() as i64),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
    }
}

#[test]
fn ordner_umbenennen_und_verschieben_gesperrt_bei_begleitdatei_im_unterordner() {
    let mut f = fixture();
    add_nested(&mut f, true);
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert_eq!(f.ops.move_folder(2, Some(1)), Err(Error::HasCompanions));
    assert!(exists(&f.root().join("a/b/Deep_extras/pic.png")));
    assert!(!exists(&f.root().join("neu")));
    assert!(!exists(&f.root().join("sub/a")));
    let p: String = f.ops.db.query_row("SELECT path FROM folders WHERE id=2", [], |r| r.get(0)).unwrap();
    assert_eq!(p, f.root().join("a").to_str().unwrap());
}

#[test]
fn ordner_umbenennen_und_verschieben_ohne_begleitdatei_gelingt() {
    let mut f = fixture();
    add_nested(&mut f, false);
    f.ops.rename_folder(2, "neu").unwrap();
    assert!(exists(&f.root().join("neu/b/Deep.3mf")));
    f.ops.move_folder(2, Some(1)).unwrap();
    assert!(exists(&f.root().join("sub/neu/b/Deep.3mf")));
}

#[test]
fn ordner_mit_begleitdatei_nur_in_fremdem_zweig_bleibt_bedienbar() {
    // Foo (mit Begleitdatei) liegt im Wurzelordner, nicht im Teilbaum von `a`.
    let mut f = fixture();
    add_nested(&mut f, false);
    f.ops.rename_folder(2, "neu").unwrap();
    assert!(exists(&f.extras().join("pic.png")));
}

fn contains_file_named(dir: &std::path::Path, name: &str) -> bool {
    fs::read_dir(dir).unwrap().flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            contains_file_named(&p, name)
        } else {
            p.file_name().is_some_and(|n| n == name)
        }
    })
}

// ---- Nachbesserungen ----

#[test]
fn getrennt_entfernte_begleitdatei_sperrt_umbenennen_nicht_aber_den_papierkorb() {
    let mut f = fixture();
    let id = f.companion_id;
    f.ops
        .db
        .execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z' WHERE id=?1", [id])
        .unwrap();
    let old = f.companion_path();
    f.ops.rename_file(1, "Bar").unwrap();
    assert!(exists(&f.root().join("Bar.3mf")));
    assert_eq!(f.companion_path(), old, "die entfernte Zeile bleibt unveraendert");
    assert!(exists(&f.extras().join("pic.png")), "ihre Datei wird nicht angefasst");
    assert_eq!(f.ops.delete_file(1), Err(Error::HasCompanions));
}

#[test]
fn gleicher_name_bleibt_auch_mit_begleitdatei_ein_noop() {
    let mut f = fixture();
    assert_eq!(f.ops.rename_file(1, "Foo"), Ok(()));
    assert!(exists(&f.root().join("Foo.3mf")));
}

#[test]
fn verschieben_ohne_pfadaenderung_bleibt_mit_begleitdatei_erlaubt() {
    let mut f = fixture();
    // Ordner-Eintrag, dessen Pfad der Wurzelordner ist: Modell liegt dort schon.
    f.ops
        .db
        .execute(
            "INSERT INTO folders(id,name,path) VALUES (9,'rootf',?1)",
            [f.root().to_str().unwrap()],
        )
        .unwrap();
    assert_eq!(f.ops.move_file_to_folder(1, Some(9)), Ok(()));
    assert!(exists(&f.root().join("Foo.3mf")));
}

#[test]
fn ordner_gesperrt_wenn_papierkorb_modell_mit_begleitdatei_im_teilbaum_liegt() {
    let mut f = fixture();
    add_nested(&mut f, true);
    f.ops
        .db
        .execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z' WHERE id=3", [])
        .unwrap();
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert_eq!(f.ops.move_folder(2, Some(1)), Err(Error::HasCompanions));
    assert!(exists(&f.root().join("a/b/Deep_extras/pic.png")));
}

#[test]
fn begleitdatei_nach_vorpruefung_rollt_umbenennen_zurueck() {
    let (mut f, db_path) = fixture_on_file();
    let pic = f.extras().join("pic.png");
    super::tests::BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            insert_companion_via_second_connection(&db_path, 1, &pic)
        }));
    });
    assert_eq!(f.ops.rename_file(1, "Bar"), Err(Error::CompanionsChanged));
    assert!(exists(&f.root().join("Foo.3mf")), "Modell zurueck am alten Ort");
    assert!(!exists(&f.root().join("Bar.3mf")));
    assert!(exists(&f.extras().join("pic.png")));
    let (name, path): (String, String) = f
        .ops
        .db
        .query_row("SELECT name,path FROM files WHERE id=1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(name, "Foo.3mf");
    assert_eq!(path, f.root().join("Foo.3mf").to_str().unwrap());
    assert!(f.ops.recover().is_ok());
}

#[test]
fn begleitdatei_nach_vorpruefung_rollt_ordner_umbenennen_zurueck() {
    let (mut f, db_path) = fixture_on_file();
    add_nested(&mut f, false);
    let pic = f.root().join("a/b/pic.png");
    fs::write(&pic, PNG).unwrap();
    super::tests::BEFORE_MUTATION.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            insert_companion_via_second_connection(&db_path, 3, &pic)
        }));
    });
    assert_eq!(f.ops.rename_folder(2, "neu"), Err(Error::HasCompanions));
    assert!(exists(&f.root().join("a/b/Deep.3mf")), "Ordner zurueck am alten Ort");
    assert!(!exists(&f.root().join("neu")));
    let p: String = f.ops.db.query_row("SELECT path FROM folders WHERE id=2", [], |r| r.get(0)).unwrap();
    assert_eq!(p, f.root().join("a").to_str().unwrap());
}

#[test]
fn papierkorb_zaehlung_und_snapshot_lassen_gesperrte_eintraege_aus() {
    let mut f = fixture();
    add_free(&mut f);
    f.ops
        .db
        .execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path=NULL", [])
        .unwrap();
    let s = f.ops.empty_trash_summary().unwrap();
    assert_eq!((s.count, s.blocked_by_companions), (1, 1));
    let snap = f
        .ops
        .empty_trash_snapshot(1, Some("2030-01-01T00:00:00Z"))
        .unwrap();
    assert_eq!(snap.ids(), &[2]);
    assert_eq!(f.ops.has_companions(1), Ok(true));
    assert_eq!(f.ops.has_companions(2), Ok(false));
}
