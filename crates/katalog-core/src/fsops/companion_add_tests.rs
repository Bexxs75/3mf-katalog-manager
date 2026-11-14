//! Eine Begleitdatei aus dem Speicher hochladen: Sollverhalten, Sperren, Konflikte mit
//! dem Desktop und ein Absturz an jedem Schritt des Journals.
use super::companion_carry_tests::{standard, Fx, PNG};
use super::companion_trash_tests::{crow, view};
use super::tests::{PointAction, BEFORE_MUTATION, POINT};
use super::*;
use std::{io::Read, fs, os::unix::fs::{symlink, PermissionsExt}, path::Path};

fn run_at(name: &'static str, action: PointAction) {
    POINT.with(|c| *c.borrow_mut() = Some((name, 0, action)));
}

fn before_mutation(f: impl FnOnce() + 'static) {
    BEFORE_MUTATION.with(|h| *h.borrow_mut() = Some(Box::new(f)));
}

fn other_connection(f: &Fx) -> rusqlite::Connection {
    crate::db::connect(&f.db_path).unwrap()
}

/// Uploads `bytes` as if they came in over the wire, with the usual 50 MiB limit.
fn up(f: &mut Fx, id: i64, name: &str, bytes: &[u8]) -> Result<AddedCompanion> {
    up_max(f, id, name, bytes, crate::companions::MAX_COMPANION_BYTES)
}

fn up_max(f: &mut Fx, id: i64, name: &str, bytes: &[u8], max: u64) -> Result<AddedCompanion> {
    f.ops.add_companion(id, name, bytes.len() as u64, max, &mut std::io::Cursor::new(bytes.to_vec()))
}

fn png(extra: usize) -> Vec<u8> {
    let mut v = PNG.to_vec();
    v.extend(std::iter::repeat_n(7u8, extra));
    v
}

/// A model without any companion folder.
fn bare() -> Fx {
    let mut f = Fx::new();
    f.model(1, "Foo.3mf");
    f
}

fn leftovers(f: &Fx) -> Vec<String> {
    let mut out = Vec::new();
    fn go(dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_str().unwrap().to_owned();
            if name.starts_with(".fsops-") {
                out.push(name);
            }
            if e.path().is_dir() {
                go(&e.path(), out);
            }
        }
    }
    go(&f.root(), &mut out);
    out
}

// ---- Pruefung der Bytes und des Namens ----

#[test]
fn the_name_and_size_are_judged_before_any_byte_is_read() {
    let max = crate::companions::MAX_COMPANION_BYTES;
    let ok = |name: &str, size: u64| prepare_companion_name(name, size, max);
    assert_eq!(ok("Bild.PNG", 10).unwrap(), "Bild.png");
    assert_eq!(ok("x.exe", 2), Err(Error::CompanionType));
    assert_eq!(ok("x.svg", 6), Err(Error::CompanionType), "no svg");
    assert_eq!(ok("x.html", 6), Err(Error::CompanionType));
    assert_eq!(ok("noextension", 6), Err(Error::CompanionType));
    assert_eq!(ok("x.png", 0), Err(Error::CompanionContent), "empty");
    assert_eq!(ok("x.txt", 0), Err(Error::CompanionContent), "an empty text is refused too");
    assert!(ok("big.png", max).is_ok(), "exactly the limit is fine");
    assert_eq!(ok("big.png", max + 1), Err(Error::CompanionTooLarge));
    assert_eq!(prepare_companion_name("x.png", 11, 10), Err(Error::CompanionTooLarge), "a smaller configured limit");
    assert_eq!(check_companion_head("x.jpg", &png(10)), Err(Error::CompanionContent), "a PNG named .jpg");
    assert!(check_companion_head("x.png", &png(10)).is_ok());
}

#[test]
fn the_content_is_judged_on_the_bytes_that_are_written() {
    let mut f = bare();
    assert_eq!(up(&mut f, 1, "x.jpg", &png(10)), Err(Error::CompanionContent));
    assert_eq!(up(&mut f, 1, "x.txt", b"a\0b"), Err(Error::CompanionContent));
    assert_eq!(up(&mut f, 1, "x.md", &[0xff, 0xfe, b'a']), Err(Error::CompanionContent));
    assert!(up(&mut f, 1, "x.txt", b"hello \xe2\x82\xac\n").is_ok());
    // Behind the inspected head, in the middle and at the very end of the stream.
    let head = crate::companions::CONTENT_CHECK_BYTES;
    for (bad, what) in [(vec![0u8], "NUL"), (vec![0xffu8], "invalid byte"), (vec![0xe2u8, 0x82], "cut character at the end")] {
        let mut g = bare();
        let mut late = vec![b'a'; head + 1000];
        late.extend(&bad);
        if what != "cut character at the end" {
            late.extend(vec![b'b'; 100]);
        }
        assert_eq!(up(&mut g, 1, "late.txt", &late), Err(Error::CompanionContent), "{what}");
        assert!(!g.p("Foo_extras").exists() && leftovers(&g).is_empty() && g.ops.journal.is_empty(), "{what}");
    }
    // A multi-byte character that straddles the head boundary is fine.
    let mut straddle = vec![b'a'; head - 1];
    straddle.extend("\u{20ac}\u{20ac}".as_bytes());
    assert!(up(&mut bare(), 1, "ok.txt", &straddle).is_ok());
}

#[test]
fn the_limit_is_a_parameter_and_a_refusal_touches_nothing() {
    let mut f = bare();
    let bytes = png(100);
    let size = bytes.len() as u64;
    assert_eq!(up_max(&mut f, 1, "a.png", &bytes, size - 1), Err(Error::CompanionTooLarge));
    assert!(!f.p("Foo_extras").exists() && f.ops.journal.is_empty());
    assert!(up_max(&mut f, 1, "a.png", &bytes, size).is_ok(), "exactly at the limit");
    assert_eq!(f.ops.add_companion(1, "b.png", 1, 0, &mut std::io::empty()), Err(Error::InvalidArguments));
    assert_eq!(
        f.ops.add_companion(1, "b.png", 1, crate::companions::MAX_COMPANION_BYTES_CONFIGURABLE + 1, &mut std::io::empty()),
        Err(Error::InvalidArguments)
    );
}

/// A body that delivers `data` and then fails or keeps going.
struct Body {
    data: std::io::Cursor<Vec<u8>>,
    then: Option<std::io::ErrorKind>,
}
impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.data.read(buf)?;
        match (n, self.then) {
            (0, Some(kind)) => Err(std::io::Error::from(kind)),
            _ => Ok(n),
        }
    }
}

#[test]
fn a_body_that_breaks_off_or_has_the_wrong_length_undoes_everything() {
    let bytes = png(400_000);
    let size = bytes.len() as u64;
    let cases: Vec<(&str, u64, Body, Error)> = vec![
        ("shorter", size, Body { data: std::io::Cursor::new(bytes[..300_000].to_vec()), then: None }, Error::UploadLength),
        ("broken", size, Body { data: std::io::Cursor::new(bytes[..300_000].to_vec()), then: Some(std::io::ErrorKind::ConnectionReset) }, Error::UploadLength),
        ("stalled", size, Body { data: std::io::Cursor::new(bytes[..300_000].to_vec()), then: Some(std::io::ErrorKind::TimedOut) }, Error::UploadTimeout),
        ("longer", size - 5, Body { data: std::io::Cursor::new(bytes.clone()), then: None }, Error::UploadLength),
        ("broken inside the head", size, Body { data: std::io::Cursor::new(bytes[..100].to_vec()), then: Some(std::io::ErrorKind::TimedOut) }, Error::UploadTimeout),
    ];
    for (what, announced, mut body, expected) in cases {
        for start in [bare as fn() -> Fx, standard] {
            let mut f = start();
            let before = view(&f);
            assert_eq!(f.ops.add_companion(1, "big.png", announced, 1 << 30, &mut body), Err(expected), "{what}");
            assert_eq!(view(&f), before, "{what}: disk and tables as before, the folder this upload made is gone");
            assert!(leftovers(&f).is_empty() && f.ops.journal.is_empty(), "{what}");
            body.data.set_position(0);
        }
    }
}

#[test]
fn a_big_upload_is_written_chunk_by_chunk_and_arrives_whole() {
    let mut bytes = png(0);
    bytes.extend((0..3_000_000u32).map(|n| (n % 251) as u8));
    let mut f = bare();
    let added = up(&mut f, 1, "big.png", &bytes).unwrap();
    assert_eq!(added.size, bytes.len() as u64);
    assert_eq!(fs::read(f.p("Foo_extras/big.png")).unwrap(), bytes);
    let mut text = Vec::new();
    for n in 0..200_000 {
        text.extend(format!("line {n} \u{e4}\u{20ac}\n").bytes());
    }
    up(&mut f, 1, "notes.txt", &text).unwrap();
    assert_eq!(fs::read(f.p("Foo_extras/notes.txt")).unwrap(), text);
}

#[test]
fn names_are_cleaned_to_one_harmless_component() {
    let name = |n: &str| prepare_companion_name(n, 4, 100);
    assert_eq!(name("../../etc/pass.png").unwrap(), "pass.png", "no path survives");
    assert_eq!(name("C:\\Users\\me\\a.png").unwrap(), "a.png");
    assert_eq!(name("a\u{202e}gnp.png").unwrap(), "a_gnp.png", "bidi control");
    assert_eq!(name("a\u{200b}b.png").unwrap(), "a_b.png", "zero width");
    assert_eq!(name("a\tb:c.png").unwrap(), "a_b_c.png");
    assert_eq!(name("CON.png").unwrap(), "_CON.png", "reserved device name");
    assert_eq!(name("com1.png").unwrap(), "_com1.png");
    assert_eq!(name(".hidden.png").unwrap(), "hidden.png", "no hidden file");
    assert_eq!(name("e\u{301}.png").unwrap(), "\u{e9}.png", "normalised");
    let long = name(&format!("{}.png", "x".repeat(400))).unwrap();
    assert!(long.len() <= 255 - 7, "room for a counter: {}", long.len());
    assert_eq!(name("a.png.exe"), Err(Error::CompanionType));
}

// ---- Sollverhalten ----

#[test]
fn an_upload_makes_the_folder_and_the_file_and_one_row() {
    let mut f = bare();
    let bytes = png(100);
    let added = up(&mut f, 1, "Bild.png", &bytes).unwrap();
    assert_eq!((added.file_id, added.name.as_str(), added.size), (1, "Bild.png", bytes.len() as u64));
    assert_eq!(fs::read(f.p("Foo_extras/Bild.png")).unwrap(), bytes);
    let mode = fs::metadata(f.p("Foo_extras/Bild.png")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
    assert_eq!(fs::metadata(f.p("Foo_extras")).unwrap().permissions().mode() & 0o777, 0o755);
    let (name, path, deleted, trash) = crow(&f, added.id);
    assert_eq!((name.as_str(), path.as_str(), deleted, trash), ("Bild.png", f.p("Foo_extras/Bild.png").to_str().unwrap(), None, None));
    let (kind, size): (String, i64) = f.ops.db.query_row("SELECT kind,size_bytes FROM companion_files WHERE id=?1", [added.id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((kind.as_str(), size), ("image", bytes.len() as i64));
    assert!(f.ops.journal.is_empty() && leftovers(&f).is_empty());
    f.assert_rows_match_disk();
}

#[test]
fn an_upload_into_an_existing_folder_leaves_the_other_files_alone() {
    let mut f = standard();
    let before = view(&f);
    up(&mut f, 1, "neu.png", &png(1)).unwrap();
    assert!(view(&f).contains("neu.png"));
    assert!(f.p("Foo_extras/pic.png").is_file() && f.p("Foo_extras/zzz.png").is_file());
    let _ = before;
    assert_eq!(crow(&f, 1).0, "pic.png");
}

#[test]
fn a_taken_name_gets_a_number_on_disk_and_in_the_table_and_nothing_is_replaced() {
    let mut f = standard();
    let first = up(&mut f, 1, "pic.png", &png(5)).unwrap();
    assert_eq!(first.name, "pic (2).png");
    assert_eq!(fs::read(f.p("Foo_extras/pic.png")).unwrap(), PNG, "the old file is untouched");
    // Taken only in the table: a removed row keeps its old path.
    let mut g = standard();
    g.ops.trash_companion(1).unwrap();
    let again = up(&mut g, 1, "pic.png", &png(5)).unwrap();
    assert_eq!(again.name, "pic (2).png", "the removed row still holds the path pic.png");
    // A loose file that no row knows is just as taken.
    let mut h = bare();
    fs::create_dir(h.p("Foo_extras")).unwrap();
    fs::write(h.p("Foo_extras/a.png"), b"loose").unwrap();
    fs::write(h.p("Foo_extras/a (2).png"), b"loose").unwrap();
    assert_eq!(up(&mut h, 1, "a.png", &png(1)).unwrap().name, "a (3).png");
}

#[test]
fn models_with_the_same_stem_share_the_folder_and_keep_their_own_rows() {
    let mut f = bare();
    f.model(2, "Foo.stl");
    let a = up(&mut f, 1, "x.png", &png(1)).unwrap();
    let b = up(&mut f, 2, "x.png", &png(2)).unwrap();
    assert_eq!((a.name.as_str(), b.name.as_str()), ("x.png", "x (2).png"));
    assert_eq!(crow(&f, a.id).1, f.p("Foo_extras/x.png").to_str().unwrap());
    let owner: i64 = f.ops.db.query_row("SELECT file_id FROM companion_files WHERE id=?1", [b.id], |r| r.get(0)).unwrap();
    assert_eq!(owner, 2);
}

// ---- Sperren ----

#[test]
fn what_cannot_be_trusted_is_refused_and_nothing_is_written() {
    type Setup = fn(&Fx);
    let cases: &[(&str, Setup)] = &[
        ("the folder is a link", |f| {
            fs::create_dir(f.p("real")).unwrap();
            symlink(f.p("real"), f.p("Foo_extras")).unwrap();
        }),
        ("the folder is a dangling link", |f| symlink(f.p("nowhere"), f.p("Foo_extras")).unwrap()),
        ("the folder is a file", |f| fs::write(f.p("Foo_extras"), b"x").unwrap()),
    ];
    for (what, setup) in cases {
        let mut f = bare();
        setup(&f);
        let before = view(&f);
        assert_eq!(up(&mut f, 1, "a.png", &png(1)), Err(Error::CompanionsConflict), "{what}");
        assert_eq!(view(&f), before, "{what}");
        assert!(f.ops.journal.is_empty() && leftovers(&f).is_empty(), "{what}");
    }
}

#[test]
fn a_model_that_is_gone_trashed_unknown_or_in_a_protected_place_is_refused() {
    let mut f = bare();
    assert_eq!(up(&mut f, 404, "a.png", &png(1)), Err(Error::NotFound));
    assert_eq!(up(&mut f, 0, "a.png", &png(1)), Err(Error::InvalidArguments));
    f.ops.delete_file(1).unwrap();
    assert_eq!(up(&mut f, 1, "a.png", &png(1)), Err(Error::Conflict), "the model is in the trash");
    assert!(!f.p("Foo_extras").exists());
    let mut g = Fx::with_protected(&[("up", Protection::Upload)]);
    g.model(1, "up/Up.3mf");
    assert_eq!(up(&mut g, 1, "a.png", &png(1)), Err(Error::Protected));
    assert!(!g.p("up/Up_extras").exists() && g.ops.journal.is_empty());
}

#[test]
fn a_full_model_refuses_before_anything_is_journalled_or_made() {
    let mut f = bare();
    for n in 0..100 {
        f.row_for(1, &f.p(&format!("Foo_extras/n{n}.png")));
    }
    let reached = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = reached.clone();
    before_mutation(move || flag.set(true));
    assert_eq!(up(&mut f, 1, "x.png", &png(1)), Err(Error::CompanionLimit));
    assert!(!reached.get() && !f.p("Foo_extras").exists());
    // Removed rows do not count.
    f.ops.db.execute("UPDATE companion_files SET deleted_at='2026-01-01T00:00:00Z' WHERE id=1", []).unwrap();
    up(&mut f, 1, "x.png", &png(1)).unwrap();
}

#[test]
fn a_blocked_journal_refuses_an_upload() {
    let mut f = bare();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(up(&mut f, 1, "a.png", &png(1)), Err(Error::RestartRequired));
    f.ops.journal.crash_at = None;
    assert_eq!(up(&mut f, 1, "b.png", &png(1)), Err(Error::RestartRequired));
}

// ---- Konflikte mit dem Desktop ----

#[test]
fn the_desktop_changing_the_rows_during_an_upload_undoes_it() {
    for mutate in [
        "INSERT INTO companion_files(file_id,name,path,kind,size_bytes,added_at) VALUES (1,'late.png','/x/Foo_extras/late.png','image',1,'t')",
        "UPDATE companion_files SET name='other.png' WHERE id=2",
        "DELETE FROM companion_files WHERE id=2",
    ] {
        let mut f = standard();
        let before = view(&f);
        let other = other_connection(&f);
        run_at("before_commit", PointAction::Run(Box::new(move || {
            other.execute(mutate, []).unwrap();
        })));
        assert_eq!(up(&mut f, 1, "neu.png", &png(1)), Err(Error::CompanionsChanged), "{mutate}");
        assert!(!f.p("Foo_extras/neu.png").exists() && leftovers(&f).is_empty() && f.ops.journal.is_empty(), "{mutate}");
        let _ = before;
    }
    // The folder this upload made goes away with it.
    let mut g = bare();
    let other = other_connection(&g);
    run_at("before_commit", PointAction::Run(Box::new(move || {
        other.execute("UPDATE files SET deleted_at='2026-02-02T00:00:00Z' WHERE id=1", []).unwrap();
    })));
    assert_eq!(up(&mut g, 1, "neu.png", &png(1)), Err(Error::Changed));
    assert!(!g.p("Foo_extras").exists() && g.ops.journal.is_empty());
}

#[test]
fn a_name_taken_during_the_upload_is_never_overwritten() {
    let mut f = standard();
    let taken = f.p("Foo_extras/neu.png");
    run_at("after_fsync_identity", PointAction::Run(Box::new(move || fs::write(&taken, b"desktop").unwrap())));
    assert_eq!(up(&mut f, 1, "neu.png", &png(3)), Err(Error::CompanionsConflict));
    assert_eq!(fs::read(f.p("Foo_extras/neu.png")).unwrap(), b"desktop");
    assert!(leftovers(&f).is_empty() && f.ops.journal.is_empty());
    assert_eq!(f.ops.db.query_row::<i64, _, _>("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap(), 2);
}

#[test]
fn the_same_path_taken_by_another_models_row_during_the_upload_is_a_conflict() {
    let mut f = bare();
    f.model(2, "Bar.3mf");
    let other = other_connection(&f);
    let path = f.p("Foo_extras/neu.png");
    run_at("before_commit", PointAction::Run(Box::new(move || {
        // A row of another model (its rows are not part of this model's picture) claims the very path.
        crate::db::companions::insert_companion(&other, 2, "neu.png", path.to_str().unwrap(), "image", None, "t").unwrap();
    })));
    assert_eq!(up(&mut f, 1, "neu.png", &png(1)), Err(Error::CompanionsConflict));
    assert!(!f.p("Foo_extras/neu.png").exists() && leftovers(&f).is_empty() && f.ops.journal.is_empty());
    assert!(!f.p("Foo_extras").exists(), "the folder this upload made is gone");
}

#[test]
fn the_folder_swapped_for_a_link_after_planning_is_not_written_to() {
    let mut f = standard();
    let dir = f.p("Foo_extras");
    let moved = f.p("moved");
    before_mutation(move || {
        fs::rename(&dir, &moved).unwrap();
        symlink(&moved, &dir).unwrap();
    });
    assert!(up(&mut f, 1, "neu.png", &png(1)).is_err());
    assert!(!f.p("moved/neu.png").exists() && leftovers(&f).is_empty());
    assert!(f.ops.journal.is_empty());
}

#[test]
fn an_undo_removes_its_file_through_the_folder_it_holds_never_through_a_link() {
    let mut f = standard();
    let dir = f.p("Foo_extras");
    let moved = f.p("moved");
    let other = other_connection(&f);
    run_at("before_commit", PointAction::Run(Box::new(move || {
        other.execute("DELETE FROM companion_files WHERE id=2", []).unwrap();
        fs::rename(&dir, &moved).unwrap();
        symlink(&moved, &dir).unwrap();
    })));
    assert_eq!(up(&mut f, 1, "neu.png", &png(1)), Err(Error::CompanionsChanged));
    assert!(f.ops.journal.is_empty(), "the file was found in the folder the upload holds");
    assert!(!f.p("moved/neu.png").exists(), "removed by its inode in that very folder");
    assert!(fs::read_dir(f.p("moved")).unwrap().flatten().count() >= 1, "the other files stay");
}

// ---- Absturz an jedem Schritt ----

#[derive(Clone, Copy, Debug)]
enum Crash {
    Phase(Phase),
    Point(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Before,
    After,
}

fn crash_and_recover(start: fn() -> Fx, crash: Crash) -> (Fx, String, String) {
    let mut twin = start();
    up(&mut twin, 1, "neu.png", &png(40)).unwrap();
    let after = view(&twin);
    let mut f = start();
    let before = view(&f);
    match crash {
        Crash::Phase(p) => f.ops.journal.crash_at = Some(p),
        Crash::Point(name) => run_at(name, PointAction::Crash),
    }
    assert_eq!(up(&mut f, 1, "neu.png", &png(40)), Err(Error::RestartRequired), "{crash:?}");
    f.ops.journal.crash_at = None;
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty(), "{crash:?}");
    assert!(report.detach_ids.is_empty(), "the model is never detached: {crash:?}");
    assert!(f.ops.journal.is_empty(), "{crash:?}");
    (f, before, after)
}

fn check(start: fn() -> Fx, points: &[(Crash, Outcome)]) {
    for (crash, expected) in points {
        let (f, before, after) = crash_and_recover(start, *crash);
        assert_eq!(view(&f), if *expected == Outcome::Before { before } else { after }, "{crash:?}");
        f.assert_rows_match_disk();
        assert!(leftovers(&f).is_empty(), "{crash:?}: {:?}", leftovers(&f));
    }
}

const POINTS: &[(Crash, Outcome)] = &[
    (Crash::Phase(Phase::Prepared), Outcome::Before),
    (Crash::Point("after_temp_create"), Outcome::Before),
    (Crash::Point("after_temp_inode"), Outcome::Before),
    (Crash::Point("mid_write"), Outcome::Before),
    (Crash::Point("after_fsync_identity"), Outcome::Before),
    (Crash::Point("after_rename"), Outcome::Before),
    (Crash::Phase(Phase::CompanionsMoved), Outcome::Before),
    (Crash::Point("before_commit"), Outcome::Before),
    (Crash::Point("after_commit"), Outcome::After),
    (Crash::Phase(Phase::Committed), Outcome::After),
];

#[test]
fn a_crash_at_any_step_of_an_upload_into_an_existing_folder_ends_completely_before_or_after() {
    check(standard, POINTS);
}

#[test]
fn a_crash_at_any_step_of_an_upload_that_makes_the_folder_leaves_no_folder_behind_before_the_commit() {
    let mut points = POINTS.to_vec();
    points.insert(1, (Crash::Point("after_mkdir"), Outcome::Before));
    check(bare, &points);
    for (crash, expected) in &points {
        if *expected == Outcome::Before {
            let (f, _, _) = crash_and_recover(bare, *crash);
            assert!(!f.p("Foo_extras").exists(), "{crash:?}: the folder this upload made is gone");
        }
    }
}

#[test]
fn recovery_takes_only_its_own_inode_and_leaves_foreign_files_alone() {
    // After the rename a foreign file takes the final name: it stays, our entry is dropped.
    let mut f = standard();
    run_at("after_rename", PointAction::Crash);
    assert_eq!(up(&mut f, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    fs::remove_file(f.p("Foo_extras/neu.png")).unwrap();
    fs::write(f.p("Foo_extras/neu.png"), b"foreign").unwrap();
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty() && f.ops.journal.is_empty());
    assert_eq!(fs::read(f.p("Foo_extras/neu.png")).unwrap(), b"foreign");
    assert!(leftovers(&f).is_empty());
    // A hard link to our file: the file is no longer the plain inode we wrote.
    let mut g = standard();
    run_at("after_rename", PointAction::Crash);
    assert_eq!(up(&mut g, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    fs::hard_link(g.p("Foo_extras/neu.png"), g.p("alias.png")).unwrap();
    let report = g.reopen_and_recover();
    assert!(report.manual.is_empty());
    assert!(g.p("Foo_extras/neu.png").is_file() && g.p("alias.png").is_file(), "a linked file is not ours to remove");
}

#[test]
fn a_foreign_file_under_the_entrys_own_temporary_name_needs_manual_review() {
    // Written, but another inode sits under the temporary name afterwards.
    let mut f = standard();
    run_at("mid_write", PointAction::Crash);
    assert_eq!(up(&mut f, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    let temp = leftovers_path(&f);
    // The foreign file is created while the original still exists and then renamed over it, so it
    // is guaranteed to be another inode (a file written right after a removal can reuse the inode number).
    let foreign = temp.with_extension("foreign");
    fs::write(&foreign, b"foreign").unwrap();
    fs::rename(&foreign, &temp).unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1);
    assert_eq!(fs::read(&temp).unwrap(), b"foreign");
    assert!(f.ops.journal.is_blocked());
    // Created but not recorded, and not empty: not provably ours.
    let mut g = standard();
    run_at("after_temp_create", PointAction::Crash);
    assert_eq!(up(&mut g, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    let temp = leftovers_path(&g);
    fs::write(&temp, b"content").unwrap();
    assert_eq!(g.reopen_and_recover().manual.len(), 1);
    assert_eq!(fs::read(&temp).unwrap(), b"content");
}

fn leftovers_path(f: &Fx) -> std::path::PathBuf {
    fs::read_dir(f.p("Foo_extras")).unwrap().flatten().map(|e| e.path()).find(|p| p.file_name().unwrap().to_str().unwrap().starts_with(".fsops-")).unwrap()
}

#[test]
fn a_tampered_upload_entry_is_never_followed() {
    for tamper in ["temp_outside", "target_name", "model_shape", "extra_step", "size"] {
        let mut f = standard();
        f.ops.journal.crash_at = Some(Phase::Prepared);
        assert_eq!(up(&mut f, 1, "neu.png", &png(1)), Err(Error::RestartRequired));
        f.ops.journal.crash_at = None;
        {
            let intent = &mut f.ops.journal.intents[0];
            let add = intent.companion_add.as_mut().unwrap();
            match tamper {
                "temp_outside" => add.temporary = format!("elsewhere_extras/.fsops-{}.part", intent.op_id),
                "target_name" => add.target = "Foo_extras/other.png".to_owned(),
                "model_shape" => intent.source = Some("Foo.3mf".to_owned()),
                "extra_step" => intent.companions.push(CompanionStep {
                    before: CompanionState { id: 1, file_id: 1, name: "pic.png".into(), path: "/x".into(), deleted_at: None, trash_path: None },
                    after: None,
                    source: None,
                    target: None,
                    identity: None,
                }),
                _ => add.size = u64::MAX,
            }
        }
        f.ops.journal.persist().unwrap();
        let report = f.reopen_and_recover();
        assert_eq!(report.manual.len(), 1, "{tamper}");
        assert!(f.p("Foo_extras/pic.png").is_file(), "{tamper}");
    }
}

// ---- Journal-Version 3 ----

#[test]
fn an_upload_entry_is_version_3_and_the_add_record_belongs_to_the_add_operation() {
    let mut f = standard();
    f.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(up(&mut f, 1, "neu.png", &png(1)), Err(Error::RestartRequired));
    let text = f.journal_text();
    assert!(text.contains("\"version\":3") && text.contains("\"companion_add\"") && text.contains("\"companion_add\":{"), "{text}");
    f.ops.journal.crash_at = None;
    // The record cut from an add entry, or added to another operation: not ours.
    let path = f.dir.join("journal/web-fsops.json");
    let cut = text.replacen("\"companion_add\"", "\"companion_added\"", 1);
    fs::write(&path, cut).unwrap();
    assert!(matches!(Journal::open(&f.dir.join("journal")), Err(Error::RestartRequired)));
    fs::write(&path, text.replacen("\"companion_add\":{", "\"companion_x\":{", 1)).unwrap();
    assert!(matches!(Journal::open(&f.dir.join("journal")), Err(Error::RestartRequired)));
    fs::write(&path, &text).unwrap();
    assert!(f.reopen_and_recover().manual.is_empty());
    assert!(f.journal_text().contains("\"version\":1"), "back to the oldest format once it is empty");
    // Version 2 with an upload entry is not what we write.
    let mut g = standard();
    g.ops.journal.crash_at = Some(Phase::Prepared);
    assert_eq!(up(&mut g, 1, "neu.png", &png(1)), Err(Error::RestartRequired));
    let p = g.dir.join("journal/web-fsops.json");
    fs::write(&p, g.journal_text().replacen("\"version\":3", "\"version\":2", 1)).unwrap();
    assert!(matches!(Journal::open(&g.dir.join("journal")), Err(Error::RestartRequired)));
}

// ---- Lange Uploads: Ordner verschwindet, Sitzung endet, Platte voll ----

/// A body that runs `hook` before it hands out its first byte past the head, then ends or fails.
struct Hooked {
    data: std::io::Cursor<Vec<u8>>,
    hook: Option<Box<dyn FnOnce()>>,
    fail: bool,
}
impl Read for Hooked {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let at = self.data.position() as usize;
        if at >= crate::companions::CONTENT_CHECK_BYTES.min(self.data.get_ref().len()) {
            if let Some(h) = self.hook.take() {
                h();
            }
            if self.fail {
                return Err(std::io::ErrorKind::ConnectionReset.into());
            }
        }
        self.data.read(buf)
    }
}

#[test]
fn a_full_disk_is_insufficient_storage_not_an_internal_error() {
    for errno in [libc::ENOSPC, libc::EDQUOT, libc::EFBIG] {
        assert_eq!(Error::from(std::io::Error::from_raw_os_error(errno)), Error::InsufficientStorage, "errno {errno}");
    }
    assert_eq!(Error::from(std::io::Error::from_raw_os_error(libc::EIO)), Error::Internal);
}

#[test]
fn a_folder_renamed_during_the_upload_does_not_keep_a_full_size_temporary_file() {
    for start in [standard as fn() -> Fx, bare] {
        let mut f = start();
        let (dir, moved) = (f.p("Foo_extras"), f.p("moved"));
        let bytes = png(400_000);
        let mut body = Hooked {
            data: std::io::Cursor::new(bytes.clone()),
            hook: Some(Box::new(move || fs::rename(&dir, &moved).unwrap())),
            fail: true,
        };
        assert_eq!(f.ops.add_companion(1, "big.png", bytes.len() as u64, 1 << 30, &mut body), Err(Error::UploadLength));
        assert!(f.ops.journal.is_empty(), "the undo found the file through the folder it holds");
        let parts: Vec<_> = fs::read_dir(f.p("moved")).unwrap().flatten().filter(|e| e.file_name().to_str().unwrap().starts_with(".fsops-")).collect();
        assert!(parts.is_empty(), "no orphaned temporary file");
    }
}

#[test]
fn a_folder_renamed_during_the_upload_that_otherwise_succeeds_is_refused_and_cleaned_up() {
    let mut f = standard();
    let (dir, moved) = (f.p("Foo_extras"), f.p("moved"));
    let bytes = png(400_000);
    let mut body = Hooked { data: std::io::Cursor::new(bytes.clone()), hook: Some(Box::new(move || fs::rename(&dir, &moved).unwrap())), fail: false };
    assert!(f.ops.add_companion(1, "big.png", bytes.len() as u64, 1 << 30, &mut body).is_err());
    assert!(f.ops.journal.is_empty());
    assert!(fs::read_dir(f.p("moved")).unwrap().flatten().all(|e| !e.file_name().to_str().unwrap().starts_with(".fsops-") && e.file_name() != "big.png"));
}

#[test]
fn recovery_does_not_drop_the_entry_when_the_folder_of_a_recorded_file_is_gone() {
    let mut f = standard();
    run_at("mid_write", PointAction::Crash);
    assert_eq!(up(&mut f, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    fs::rename(f.p("Foo_extras"), f.p("moved")).unwrap();
    let report = f.reopen_and_recover();
    assert_eq!(report.manual.len(), 1, "a file of ours may sit in the moved folder");
    assert!(f.ops.journal.is_blocked());
    assert!(fs::read_dir(f.p("moved")).unwrap().flatten().any(|e| e.file_name().to_str().unwrap().starts_with(".fsops-")));
}

#[test]
fn the_decision_for_an_unreachable_recorded_file_is_manual() {
    for (temp, target) in [(TempSeen::Unreachable, TargetSeen::Gone), (TempSeen::Mine, TargetSeen::Unreachable)] {
        let facts = CompanionAddFacts { row_exists: false, temp, target };
        assert_eq!(companion_add_decision(facts), CompanionAddDecision::Manual);
    }
    assert_eq!(companion_add_decision(CompanionAddFacts { row_exists: true, temp: TempSeen::Unreachable, target: TargetSeen::Gone }), CompanionAddDecision::Committed);
}

#[test]
fn a_session_that_ends_before_the_commit_undoes_the_upload() {
    for start in [standard as fn() -> Fx, bare] {
        let mut f = start();
        let before = view(&f);
        let bytes = png(40);
        let mut asked = 0;
        let r = f.ops.add_companion_checked(1, "neu.png", bytes.len() as u64, 1 << 30, &mut std::io::Cursor::new(bytes), &mut || {
            asked += 1;
            false
        });
        assert_eq!(r, Err(Error::NotStarted));
        assert_eq!(asked, 1);
        assert_eq!(view(&f), before);
        assert!(leftovers(&f).is_empty() && f.ops.journal.is_empty());
    }
}

#[test]
fn the_body_can_be_streamed_while_another_operation_runs_between_the_steps() {
    let mut f = standard();
    let bytes = png(400_000);
    let size = bytes.len() as u64;
    let mut source = std::io::Cursor::new(bytes);
    let head = read_companion_head(size, &mut source).unwrap();
    let mut pending = f.ops.begin_companion_add(1, "neu.png", size, 1 << 30, &head).unwrap();
    // No lock is needed from here on; the journal already holds the entry.
    assert!(!f.ops.journal.is_empty());
    stream_companion(&mut pending, &mut source).unwrap();
    let added = f.ops.place_companion(pending, &mut || true).unwrap();
    assert_eq!(added.size, size);
    assert!(f.p("Foo_extras/neu.png").is_file() && leftovers(&f).is_empty() && f.ops.journal.is_empty());
}

#[test]
fn a_model_that_vanishes_while_the_body_streams_loses_the_upload_not_its_files() {
    let mut f = standard();
    let bytes = png(400_000);
    let size = bytes.len() as u64;
    let mut source = std::io::Cursor::new(bytes);
    let head = read_companion_head(size, &mut source).unwrap();
    let mut pending = f.ops.begin_companion_add(1, "neu.png", size, 1 << 30, &head).unwrap();
    stream_companion(&mut pending, &mut source).unwrap();
    other_connection(&f).execute("UPDATE files SET deleted_at='2026-02-02T00:00:00Z' WHERE id=1", []).unwrap();
    assert_eq!(f.ops.place_companion(pending, &mut || true), Err(Error::Changed));
    assert!(!f.p("Foo_extras/neu.png").exists() && leftovers(&f).is_empty() && f.ops.journal.is_empty());
}

#[test]
fn a_restart_after_a_finished_undo_that_removed_its_folder_is_not_a_manual_case() {
    // The undo removed the file and the folder it made, then the process died before the
    // entry was dropped (or the drop could not be saved).
    let mut f = bare();
    run_at("after_undo", PointAction::Crash);
    let bytes = png(400_000);
    let mut body = Hooked { data: std::io::Cursor::new(bytes.clone()), hook: None, fail: true };
    body.hook = Some(Box::new(|| {}));
    assert_eq!(f.ops.add_companion(1, "big.png", bytes.len() as u64, 1 << 30, &mut body), Err(Error::RestartRequired));
    assert!(!f.p("Foo_extras").exists(), "the undo took its folder away");
    let report = f.reopen_and_recover();
    assert!(report.manual.is_empty(), "{:?}", report.manual);
    assert!(f.ops.journal.is_empty() && !f.ops.journal.is_blocked());
    assert!(leftovers(&f).is_empty());
}

#[test]
fn only_entries_waiting_for_a_person_are_offered_for_acknowledgement() {
    let mut f = standard();
    run_at("mid_write", PointAction::Crash);
    assert_eq!(up(&mut f, 1, "neu.png", &png(9)), Err(Error::RestartRequired));
    assert_eq!(f.ops.journal.pending_op_ids().len(), 1);
    assert!(f.ops.journal.pending_manual_op_ids().is_empty(), "a Prepared entry is rolled back by recovery itself");
    let id = f.ops.journal.pending_op_ids().remove(0);
    f.ops.journal.block(&id);
    assert_eq!(f.ops.journal.pending_manual_op_ids(), vec![id]);
}
