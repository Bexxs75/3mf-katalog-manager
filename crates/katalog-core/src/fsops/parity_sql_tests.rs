//! Parity evidence for the four SQL prefix queries in `folders.rs` (`tree`, `available`):
//! which rows a path prefix selects, with edge cases around the `'/'` and `'0'` bounds.
//! Recorded on commit b49dbc1b; golden files may only be regenerated in step S8.
//!
//! Two views: the SQL text as it is today run on a fixture database (the sets a rewritten
//! query must reproduce), and the behaviour of the production functions around one stale
//! row per edge case.
use crate::fsops::parity_golden_tests::assert_golden;
use super::*;
use rusqlite::Connection;
use std::collections::BTreeSet;

/// The queries exactly as `folders.rs` has them (the part that selects by path).
const FILES_BY_PREFIX: &str = "SELECT path FROM files WHERE path=?1 OR (path>=?1||'/' AND path<?1||'0') ORDER BY id";
const COMPANIONS_BY_PREFIX: &str = "SELECT path FROM companion_files WHERE path>=?1||'/' AND path<?1||'0' ORDER BY id";
const FOLDERS_BY_PREFIX: &str = "SELECT path FROM folders WHERE path=?1 OR (path>=?1||'/' AND path<?1||'0')";
const FILES_BELOW: &str = "SELECT path FROM files WHERE path>=?1||'/' AND path<?1||'0'";
const COMPANIONS_BELOW: &str = "SELECT path FROM companion_files WHERE path>=?1||'/' AND path<?1||'0'";

const BASES: [&str; 4] = ["/m/a", "/m/a_b", "/m/a%b", "/m/é"];
const EDGES: [&str; 14] = [
    "", "/", "0", ".b", "/b", "/b/c", "-", "1", "\u{1}", "/\u{10ffff}", "_", "x", "/b\\c", "//b",
];

fn candidates() -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for base in BASES {
        for edge in EDGES {
            all.insert(format!("{base}{edge}"));
        }
    }
    all.insert("/m/A".into());
    all.insert("/m/ab".into());
    all.insert("/m".into());
    all.insert("/".into());
    all
}

fn fixture() -> Connection {
    let c = crate::db::connect_in_memory().unwrap();
    c.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (1,'owner','/owner/o.3mf','3mf',1,'2026-01-01T00:00:00Z')", []).unwrap();
    for (i, p) in candidates().iter().enumerate() {
        let id = i as i64 + 2;
        c.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (?1,'n',?2,'3mf',1,'2026-01-01T00:00:00Z')", rusqlite::params![id, p]).unwrap();
        c.execute("INSERT INTO companion_files(file_id,name,path,kind,size_bytes,added_at) VALUES (1,'n',?1,'image',1,'2026-01-01T00:00:00Z')", [p]).unwrap();
        c.execute("INSERT INTO folders(id,name,path) VALUES (?1,'n',?2)", rusqlite::params![id, p]).unwrap();
    }
    c
}

fn paths(c: &Connection, sql: &str, base: &str) -> Vec<String> {
    let mut stmt = c.prepare(sql).unwrap();
    let mut found: Vec<String> = stmt.query_map([base], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    found.sort();
    found
}

#[test]
fn prefix_queries_select_the_golden_sets() {
    let c = fixture();
    let mut text = String::new();
    for base in BASES.iter().chain(["/m/a/"].iter()) {
        for (label, sql) in [
            ("files_by_prefix", FILES_BY_PREFIX),
            ("companions_by_prefix", COMPANIONS_BY_PREFIX),
            ("folders_by_prefix", FOLDERS_BY_PREFIX),
            ("files_below", FILES_BELOW),
            ("companions_below", COMPANIONS_BELOW),
        ] {
            text += &format!("{label} {base:?}\n");
            for p in paths(&c, sql, base) {
                text += &format!("  {p:?}\n");
            }
        }
    }
    assert_golden("sql/prefix_sets.txt", text.as_bytes());
}

#[derive(Clone, Copy)]
enum Stale {
    Folder,
    File,
    Companion,
}

fn scenario(kind: Stale, path: &str) -> Connection {
    let c = crate::db::connect_in_memory().unwrap();
    c.execute("INSERT INTO folders(id,name,path) VALUES (1,'a','/m/a')", []).unwrap();
    c.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (1,'owner','/owner/o.3mf','3mf',1,'2026-01-01T00:00:00Z')", []).unwrap();
    match kind {
        Stale::Folder => {
            c.execute("INSERT OR IGNORE INTO folders(id,name,path) VALUES (2,'n',?1)", [path]).unwrap();
        }
        Stale::File => {
            c.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES (2,'n',?1,'3mf',1,'2026-01-01T00:00:00Z')", [path]).unwrap();
        }
        Stale::Companion => {
            c.execute("INSERT INTO companion_files(file_id,name,path,kind,size_bytes,added_at) VALUES (1,'n',?1,'image',1,'2026-01-01T00:00:00Z')", [path]).unwrap();
        }
    }
    c
}

fn intent(op: FolderOperation) -> FolderIntent {
    FolderIntent {
        op_id: "0".repeat(32),
        phase: Phase::Prepared,
        operation: op,
        root_path: "/m".into(),
        root_identity: Identity { dev: 0, ino: 0 },
        source: None,
        target: Some("a".into()),
        identity: None,
        before: FolderTree::default(),
        after: FolderTree::default(),
        parent: None,
    }
}

#[test]
fn tree_and_available_around_one_stale_row_match_the_golden_behaviour() {
    let mut text = String::new();
    for (label, kind) in [("folder", Stale::Folder), ("file", Stale::File), ("companion", Stale::Companion)] {
        for edge in EDGES.iter().chain(["x/y", ".b/c"].iter()) {
            let path = format!("/m/a{edge}");
            let c = scenario(kind, &path);
            let t = match tree(&c, 1) {
                Ok(t) => format!(
                    "Ok folders={:?} files={:?} companions={:?}",
                    t.folders.iter().map(|f| f.id).collect::<Vec<_>>(),
                    t.files.iter().map(|f| f.id).collect::<Vec<_>>(),
                    t.companions.iter().map(|r| r.id).collect::<Vec<_>>()
                ),
                Err(e) => format!("Err {e}"),
            };
            let relocate = available(&c, &intent(FolderOperation::Relocate)).map(|_| "Ok".to_owned()).unwrap_or_else(|e| format!("Err {e}"));
            let other = available(&c, &intent(FolderOperation::Remove)).map(|_| "Ok".to_owned()).unwrap_or_else(|e| format!("Err {e}"));
            text += &format!("{label} {path:?}: tree {t} | available(relocate) {relocate} | available(remove) {other}\n");
        }
    }
    assert_golden("sql/prefix_behaviour.txt", text.as_bytes());
}
