//! Parity evidence for the `fsops` refactoring: the journal file (`web-fsops.json`) after
//! every reachable phase of every operation, recorded on the unchanged code of commit
//! b49dbc1b (branch `kern/k0-bereinigung`), compared byte for byte.
//!
//! The golden files in `testdata/fsops_golden/` may only be regenerated in step S8, and
//! only for the cases that step names. A failing comparison means the behaviour changed.
//! Regenerating (`FSOPS_GOLDEN_UPDATE=1`) is for that step alone.
//!
//! What is masked before comparing, because the OS decides it: the temporary directory,
//! device and inode numbers, and the RFC 3339 timestamps `deleted_at` / `added_at`
//! (`chrono::Utc::now()`). Everything else is the byte stream the journal wrote. Operation
//! ids are made deterministic by the `cfg(test)` hook `journal::FIXED_OP_IDS`.
use super::companion_carry_tests::{Fx, Ledger, PNG};
use super::journal::FIXED_OP_IDS;
use super::tests::{PointAction, POINT};
use super::*;
use std::{collections::BTreeMap, fs, path::PathBuf};

pub(super) fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/fsops_golden")
}

/// Compares `bytes` with the checked-in golden file `name`.
pub(super) fn assert_golden(name: &str, bytes: &[u8]) {
    let path = golden_dir().join(name);
    if std::env::var_os("FSOPS_GOLDEN_UPDATE").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        return;
    }
    let want = fs::read(&path).unwrap_or_else(|_| panic!("golden file missing: {name}"));
    assert!(
        want == bytes,
        "golden file differs: {name}\n--- want\n{}\n--- got\n{}",
        String::from_utf8_lossy(&want),
        String::from_utf8_lossy(bytes)
    );
}

fn replace_after(text: &mut String, key: &str, open: &str, stop: &str, with: &str) {
    let marker = format!("\"{key}\":{open}");
    let mut from = 0;
    while let Some(at) = text[from..].find(&marker) {
        let start = from + at + marker.len();
        let end = start + text[start..].find(stop).expect("closing delimiter");
        text.replace_range(start..end, with);
        from = start + with.len();
    }
}

/// Masks what the operating system decides (see the module comment).
pub(super) fn normalize(bytes: &[u8], dir: &std::path::Path) -> Vec<u8> {
    let mut text = String::from_utf8(bytes.to_vec()).expect("journal is UTF-8");
    for d in [fs::canonicalize(dir).ok(), Some(dir.to_path_buf())].into_iter().flatten() {
        text = text.replace(d.to_str().unwrap(), "<DIR>");
    }
    for key in ["dev", "ino"] {
        // Plain numbers end at `,` or `}`; the first stop found is the nearer one.
        let marker = format!("\"{key}\":");
        let mut from = 0;
        while let Some(at) = text[from..].find(&marker) {
            let start = from + at + marker.len();
            let end = start + text[start..].find(|c: char| !c.is_ascii_digit()).unwrap();
            text.replace_range(start..end, "0");
            from = start + 1;
        }
    }
    replace_after(&mut text, "temp_inode", "[", "]", "0,0");
    replace_after(&mut text, "deleted_at", "\"", "\"", "<TIME>");
    replace_after(&mut text, "added_at", "\"", "\"", "<TIME>");
    text.into_bytes()
}

const PHASES: [Phase; 6] = [
    Phase::Prepared,
    Phase::TmpRenamed,
    Phase::Renamed,
    Phase::CompanionsMoved,
    Phase::Unlinked,
    Phase::Committed,
];

type Run = fn(&mut Fx) -> Result<()>;

struct Case {
    name: &'static str,
    setup: fn() -> Fx,
    run: Run,
    /// Crash points of the production code (`at_point`) that this operation can pass.
    points: &'static [(&'static str, usize)],
}

/// Same layout as `Fx::new`, but with an in-memory catalog: the evidence runs several
/// hundred operations and a database file with its migrations makes each one slow.
fn fx() -> Fx {
    let dir = crate::unique_test_dir("fsops_parity_journal");
    fs::create_dir_all(dir.join("root/sub")).unwrap();
    fs::create_dir(dir.join("journal")).unwrap();
    let db = crate::db::connect_in_memory().unwrap();
    db.execute("INSERT INTO folders(id,name,path) VALUES (1,'sub',?1)", [dir.join("root/sub").to_str().unwrap()]).unwrap();
    let context = FsContext::new(&[dir.join("root")], &[]).unwrap();
    let journal = Journal::open(&dir.join("journal")).unwrap();
    Fx { dir: dir.clone(), db_path: dir.join("catalog.db"), ops: FileOps::new(context, journal, db, Ledger) }
}
/// Foo.3mf (id 1) with two companions in Foo_extras.
fn standard() -> Fx {
    let mut f = bare();
    f.companion(1, "Foo_extras/pic.png");
    f.companion(1, "Foo_extras/zzz.png");
    f
}
fn plain() -> Fx {
    let mut f = fx();
    f.model(1, "a.stl");
    f
}
fn bare() -> Fx {
    let mut f = fx();
    f.model(1, "Foo.3mf");
    f
}
fn trashed(mut f: Fx) -> Fx {
    f.ops.delete_file(1).unwrap();
    f
}
fn plain_trashed() -> Fx {
    trashed(plain())
}
fn standard_trashed() -> Fx {
    trashed(standard())
}
fn companion_removed() -> Fx {
    let mut f = standard();
    f.ops.trash_companion(1).unwrap();
    f
}
fn in_folder() -> Fx {
    let f = plain();
    f.ops.db.execute("UPDATE files SET folder_id=1", []).unwrap();
    f
}
fn missing_file() -> Fx {
    let f = plain();
    fs::remove_file(f.p("a.stl")).unwrap();
    f
}
fn missing_file_trashed() -> Fx {
    let f = plain_trashed();
    let (path,): (String,) = f
        .ops
        .db
        .query_row("SELECT trash_path FROM files WHERE id=1", [], |r| Ok((r.get(0)?,)))
        .unwrap();
    fs::remove_file(path).unwrap();
    f
}
/// Folder 1 (`sub`) holds a model with a companion; folder 2 is `sub/inner`; folder 3 is `other`.
fn folders() -> Fx {
    let mut f = fx();
    f.ops.create_folder(Some(1), "inner").unwrap();
    f.ops.create_folder(Some(1), "other").unwrap();
    f.model(1, "sub/M.3mf");
    f.ops.db.execute("UPDATE files SET folder_id=1", []).unwrap();
    f.companion(1, "sub/M_extras/p.png");
    f
}
/// Folder 2 (`sub/inner`) holds the model with its companion; folder 3 is `sub/other`.
fn folders_nested() -> Fx {
    let mut f = fx();
    f.ops.create_folder(Some(1), "inner").unwrap();
    f.ops.create_folder(Some(1), "other").unwrap();
    f.model(1, "sub/inner/M.3mf");
    f.ops.db.execute("UPDATE files SET folder_id=2", []).unwrap();
    f.companion(1, "sub/inner/M_extras/p.png");
    f
}
fn folders_empty() -> Fx {
    let mut f = fx();
    f.ops.create_folder(Some(1), "inner").unwrap();
    f.ops.create_folder(Some(1), "other").unwrap();
    f
}
fn ok<T>(r: Result<T>) -> Result<()> {
    r.map(|_| ())
}

const MODEL_POINTS: &[(&str, usize)] = &[("before_commit", 0), ("after_mkdir", 0), ("after_companion", 0), ("after_companion", 1)];
const UNLINK_POINTS: &[(&str, usize)] = &[("after_unlink", 0), ("after_unlink", 1), ("after_companion", 0)];
const EDIT_POINTS: &[(&str, usize)] = &[("before_commit", 0), ("after_companion", 0), ("before_unlink", 0), ("after_unlink", 0)];
const ADD_POINTS: &[(&str, usize)] = &[
    ("after_temp_create", 0),
    ("after_temp_inode", 0),
    ("mid_write", 0),
    ("after_fsync_identity", 0),
    ("after_rename", 0),
    ("before_commit", 0),
    ("after_commit", 0),
    ("after_undo", 0),
];

fn cases() -> Vec<Case> {
    vec![
        // Models without companions.
        Case { name: "file_rename", setup: plain, run: |f| f.ops.rename_file(1, "b"), points: MODEL_POINTS },
        Case { name: "file_rename_case", setup: plain, run: |f| f.ops.rename_file(1, "A"), points: MODEL_POINTS },
        Case { name: "file_move", setup: plain, run: |f| f.ops.move_file_to_folder(1, Some(1)), points: MODEL_POINTS },
        Case { name: "file_trash", setup: plain, run: |f| f.ops.delete_file(1), points: MODEL_POINTS },
        Case { name: "file_restore", setup: plain_trashed, run: |f| f.ops.restore_file(1), points: MODEL_POINTS },
        Case { name: "file_delete_permanently", setup: plain_trashed, run: |f| ok(f.ops.delete_file_permanently(1)), points: UNLINK_POINTS },
        // DbOnly: no disk move is involved.
        Case { name: "dbonly_move_to_no_folder", setup: in_folder, run: |f| f.ops.move_file_to_folder(1, None), points: &[] },
        Case { name: "dbonly_trash_of_missing_file", setup: missing_file, run: |f| f.ops.delete_file(1), points: &[] },
        Case { name: "dbonly_delete_of_missing_trash_file", setup: missing_file_trashed, run: |f| ok(f.ops.delete_file_permanently(1)), points: &[] },
        // Models with companions.
        Case { name: "file_rename_with_companions", setup: standard, run: |f| f.ops.rename_file(1, "Bar"), points: MODEL_POINTS },
        Case { name: "file_rename_case_with_companions", setup: standard, run: |f| f.ops.rename_file(1, "FOO"), points: MODEL_POINTS },
        Case { name: "file_move_with_companions", setup: standard, run: |f| f.ops.move_file_to_folder(1, Some(1)), points: MODEL_POINTS },
        Case { name: "file_trash_with_companions", setup: standard, run: |f| f.ops.delete_file(1), points: MODEL_POINTS },
        Case { name: "file_restore_with_companions", setup: standard_trashed, run: |f| f.ops.restore_file(1), points: MODEL_POINTS },
        Case { name: "file_delete_permanently_with_companions", setup: standard_trashed, run: |f| ok(f.ops.delete_file_permanently(1)), points: UNLINK_POINTS },
        // Folders.
        Case { name: "folder_create", setup: fx, run: |f| ok(f.ops.create_folder(Some(1), "new")), points: &[] },
        Case { name: "folder_rename", setup: folders, run: |f| f.ops.rename_folder(2, "inner2"), points: &[] },
        Case { name: "folder_move", setup: folders_empty, run: |f| f.ops.move_folder(2, Some(3)), points: &[] },
        Case { name: "folder_move_with_models", setup: folders_nested, run: |f| f.ops.move_folder(2, Some(3)), points: &[] },
        Case { name: "folder_rename_with_models", setup: folders, run: |f| f.ops.rename_folder(1, "sub2"), points: &[] },
        Case { name: "folder_remove", setup: folders_empty, run: |f| ok(f.ops.remove_folder_from_catalog(2)), points: &[] },
        Case { name: "folder_remove_with_models", setup: folders, run: |f| ok(f.ops.remove_folder_from_catalog(1)), points: &[] },
        // Single companion files.
        Case { name: "companion_trash", setup: standard, run: |f| f.ops.trash_companion(1), points: EDIT_POINTS },
        Case { name: "companion_trash_last_file", setup: companion_removed, run: |f| f.ops.trash_companion(2), points: EDIT_POINTS },
        Case { name: "companion_restore", setup: companion_removed, run: |f| f.ops.restore_companion(1), points: EDIT_POINTS },
        Case { name: "companion_purge", setup: companion_removed, run: |f| f.ops.purge_loose_companion(1, None), points: EDIT_POINTS },
        Case {
            name: "companion_add",
            setup: bare,
            run: |f| ok(f.ops.add_companion(1, "pic.png", PNG.len() as u64, 50 << 20, &mut std::io::Cursor::new(PNG.to_vec()))),
            points: ADD_POINTS,
        },
        Case {
            name: "companion_add_into_existing_folder",
            setup: standard,
            run: |f| ok(f.ops.add_companion(1, "new.png", PNG.len() as u64, 50 << 20, &mut std::io::Cursor::new(PNG.to_vec()))),
            points: ADD_POINTS,
        },
    ]
}

fn journal_after(f: &Fx) -> Option<Vec<u8>> {
    fs::read(f.dir.join("journal/web-fsops.json")).ok().map(|b| normalize(&b, &f.dir))
}

/// Runs `case` with one crash armed and reports how it ended.
fn run_armed(case: &Case, arm: impl FnOnce(&mut Fx)) -> (String, Option<Vec<u8>>) {
    FIXED_OP_IDS.with(|c| c.set(Some(1)));
    let mut f = (case.setup)();
    arm(&mut f);
    let result = (case.run)(&mut f);
    POINT.with(|p| *p.borrow_mut() = None);
    let bytes = journal_after(&f);
    FIXED_OP_IDS.with(|c| c.set(None));
    match result {
        Err(Error::RestartRequired) => ("crashed".into(), bytes),
        Ok(()) => ("completed".into(), bytes),
        Err(e) => (format!("failed {e}"), bytes),
    }
}

/// Every golden file of the journal evidence, keyed by file name, plus the index that
/// lists which phase or crash point of which operation can not be reached.
fn generate() -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut index = String::new();
    for case in cases() {
        let (end, bytes) = run_armed(&case, |_| {});
        assert_eq!(end, "completed", "{}", case.name);
        index += &format!("{} end completed\n", case.name);
        out.insert(format!("journal/{}.end.json", case.name), bytes.expect("journal file after a finished operation"));
        for phase in PHASES {
            let (end, bytes) = run_armed(&case, |f| f.ops.journal.crash_at = Some(phase));
            let label = format!("{} phase {phase:?}", case.name);
            match (end.as_str(), bytes) {
                ("crashed", Some(bytes)) => {
                    index += &format!("{label} crashed\n");
                    out.insert(format!("journal/{}.{phase:?}.json", case.name), bytes);
                }
                (end, _) => index += &format!("{label} NOT REACHED ({end})\n"),
            }
        }
        for (point, k) in case.points {
            let (end, bytes) = run_armed(&case, |_| {
                POINT.with(|p| *p.borrow_mut() = Some((point, *k, PointAction::Crash)));
            });
            let label = format!("{} point {point}#{k}", case.name);
            match (end.as_str(), bytes) {
                ("crashed", Some(bytes)) => {
                    index += &format!("{label} crashed\n");
                    out.insert(format!("journal/{}.at_{point}_{k}.json", case.name), bytes);
                }
                (end, _) => index += &format!("{label} NOT REACHED ({end})\n"),
            }
        }
    }
    out.insert("journal/INDEX.txt".into(), index.into_bytes());
    out
}

/// The evidence takes a while (every state is a real operation with fsync), so the tests
/// that only read it share one run.
fn generated_once() -> &'static BTreeMap<String, Vec<u8>> {
    static ONCE: std::sync::OnceLock<BTreeMap<String, Vec<u8>>> = std::sync::OnceLock::new();
    ONCE.get_or_init(generate)
}

#[test]
fn journal_after_every_reachable_phase_matches_the_golden_files() {
    let generated = generated_once();
    for (name, bytes) in generated {
        assert_golden(name, bytes);
    }
    // A golden file nobody produces any more is stale evidence.
    let mut stale = Vec::new();
    for entry in fs::read_dir(golden_dir().join("journal")).unwrap().flatten() {
        let name = format!("journal/{}", entry.file_name().to_str().unwrap());
        if !generated.contains_key(&name) {
            stale.push(name);
        }
    }
    assert!(stale.is_empty() || std::env::var_os("FSOPS_GOLDEN_UPDATE").is_some(), "stale golden files: {stale:?}");
}

#[test]
fn journal_evidence_is_deterministic_run_to_run() {
    assert_eq!(&generate(), generated_once());
}

#[test]
fn every_golden_journal_is_a_valid_document_of_the_current_format() {
    for (name, bytes) in generated_once() {
        if name.ends_with(".json") {
            let value: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(value["version"].is_u64(), "{name}");
        }
    }
}
