//! Parity evidence for the four SQL prefix queries in `folders.rs`, through the real
//! functions: `tree` (files by prefix, companions by prefix) and `available` (folders by
//! prefix, files and companions below the target). Recorded on commit b49dbc1b; golden files
//! may only be regenerated in step S8.
//!
//! Each scenario puts exactly one stale row at a boundary path next to a base path
//! (`a`, `a/`, `a0`, `a.b`, `a/b`, and more, relative to the base) and records what the
//! function does. What shows depends on the query:
//! - companions in `tree`: the selected set is returned as it is, so too many and too few
//!   rows both show;
//! - the other queries are filtered again in Rust (`under`), so only a query that selects
//!   too FEW rows shows (a row below the base that is no longer found); too many rows are
//!   absorbed by the Rust check and can not be seen from outside.
use crate::fsops::parity_golden_tests::assert_golden;
use super::*;
use rusqlite::Connection;

const EDGES: [&str; 17] = [
    "", "/", "0", ".b", "/b", "/b/c", "-", "1", "\u{1}", "/\u{10ffff}", "_", "x", "/b\\c", "//b", "x/y", ".b/c", "é",
];
/// (parent folder of the root of the operation, its name)
const BASES: [(&str, &str); 2] = [("/m", "a"), ("/m", "a_b")];

#[derive(Clone, Copy)]
enum Stale {
    Folder,
    File,
    Companion,
}
const KINDS: [(&str, Stale); 3] = [("folder", Stale::Folder), ("file", Stale::File), ("companion", Stale::Companion)];

fn scenario(root: Option<&str>, kind: Stale, path: &str) -> Connection {
    let c = crate::db::connect_in_memory().unwrap();
    if let Some(root) = root {
        c.execute("INSERT INTO folders(id,name,path) VALUES (1,'a',?1)", [root]).unwrap();
    }
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

fn intent(op: FolderOperation, parent: &str, name: &str) -> FolderIntent {
    FolderIntent {
        op_id: "0".repeat(32),
        phase: Phase::Prepared,
        operation: op,
        root_path: parent.into(),
        root_identity: Identity { dev: 0, ino: 0 },
        source: None,
        target: Some(name.into()),
        identity: None,
        before: FolderTree::default(),
        after: FolderTree::default(),
        parent: None,
    }
}

fn verdict<T>(r: Result<T>) -> String {
    r.map(|_| "Ok".to_owned()).unwrap_or_else(|e| format!("Err {e}"))
}

#[test]
fn tree_around_one_stale_row_matches_the_golden_behaviour() {
    let mut text = String::new();
    for (parent, name) in BASES {
        let base = format!("{parent}/{name}");
        for (label, kind) in KINDS {
            for edge in EDGES {
                let path = format!("{base}{edge}");
                let c = scenario(Some(&base), kind, &path);
                let t = match tree(&c, 1) {
                    Ok(t) => format!(
                        "Ok folders={:?} files={:?} companions={:?}",
                        t.folders.iter().map(|f| f.id).collect::<Vec<_>>(),
                        t.files.iter().map(|f| f.id).collect::<Vec<_>>(),
                        t.companions.iter().map(|r| r.id).collect::<Vec<_>>()
                    ),
                    Err(e) => format!("Err {e}"),
                };
                text += &format!("tree base={base:?} {label} {path:?}: {t}\n");
            }
        }
    }
    assert_golden("sql/prefix_behaviour.txt", text.as_bytes());
}

#[test]
fn available_around_one_stale_row_matches_the_golden_behaviour() {
    let mut text = String::new();
    for (parent, name) in BASES {
        let base = format!("{parent}/{name}");
        for (label, kind) in KINDS {
            for edge in EDGES {
                let path = format!("{base}{edge}");
                let c = scenario(None, kind, &path);
                let relocate = verdict(available(&c, &intent(FolderOperation::Relocate, parent, name)));
                let create = verdict(available(&c, &intent(FolderOperation::Create, parent, name)));
                text += &format!("available base={base:?} {label} {path:?}: relocate {relocate} | create {create}\n");
            }
        }
    }
    // Kept separate from the tree golden: it is a second file, so a regeneration of one
    // can never hide a change of the other.
    assert_golden("sql/available_behaviour.txt", text.as_bytes());
}
