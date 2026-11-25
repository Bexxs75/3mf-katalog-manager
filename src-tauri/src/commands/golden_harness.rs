//! Golden harness for the desktop file actions (issue #20, package P0).
//!
//! A scenario runs real actions against an in-memory catalog plus a real temp directory and
//! produces a normalised text snapshot: return values / error JSON per step, the rows of every
//! affected table and the file tree. The snapshot is compared with the checked-in file under
//! `src-tauri/testdata/desktop_golden/<name>.golden.txt`. Set `UPDATE_GOLDEN=1` to rewrite it.
//!
//! Add a scenario: write a `#[test]` that calls `run_golden("name", |w| { ... })`, record each
//! action with `w.step("label", result)`; run once with `UPDATE_GOLDEN=1`, review the new file.
use super::*;
use rusqlite::types::Value;
use std::fmt::Write as _;

/// Tables dumped in every snapshot. Extend (e.g. in P8) when more tables are touched.
pub const GOLDEN_TABLES: &[&str] = &[
    "folders",
    "files",
    "companion_files",
    "folder_trash_origin",
    "bundles",
    "bundle_members",
    "disk_sync_known_copies",
    // Dependents of `files` (CASCADE / SET NULL on delete) and their parents.
    "tags",
    "file_tags",
    "file_metadata",
    "file_materials",
    "collections",
    "collection_files",
    "print_log",
    "file_print_info",
    "file_print_plates",
    "printers",
    "printer_jobs",
    "slicer_reports",
];

pub struct World {
    pub conn: Connection,
    /// Real temp directory of the scenario; replaced by `<ROOT>` in the snapshot.
    pub root: PathBuf,
    steps: Vec<serde_json::Value>,
}

impl World {
    /// Creates a directory below the root and returns its path.
    pub fn dir(&self, rel: &str) -> PathBuf {
        let p = self.root.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    /// Writes a file below the root (parents created); content is part of the tree size only.
    pub fn file(&self, rel: &str, content: &str) -> PathBuf {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
        p
    }
    /// Records one action's outcome (Ok value or error JSON) in the snapshot.
    pub fn step<T: serde::Serialize>(&mut self, label: &str, result: CmdResult<T>) {
        let outcome = match result {
            Ok(v) => serde_json::json!({ "ok": serde_json::to_value(v).unwrap() }),
            Err(e) => serde_json::json!({ "err": serde_json::to_value(e).unwrap() }),
        };
        self.steps.push(serde_json::json!({ "step": label, "result": outcome }));
    }
}

fn is_time_column(name: &str) -> bool {
    name.ends_with("_at") || name.contains("mtime") || name.contains("modified")
}

fn norm_text(s: &str, root: &Path) -> String {
    let root_s = root.to_string_lossy().to_string();
    s.replace(&root_s, "<ROOT>").replace('\\', "/")
}

/// OS error texts differ per platform ("File exists (os error 17)" vs. the Windows wording);
/// map them to a stable category. Structured error codes and parameters stay untouched.
fn norm_os_error(s: &str) -> Option<String> {
    if !s.contains("(os error ") {
        return None;
    }
    let category = if s.contains("File exists") || s.contains("already exists") {
        "already exists"
    } else if s.contains("No such file") || s.contains("cannot find") {
        "not found"
    } else if s.contains("Permission denied") || s.contains("Access is denied") {
        "permission denied"
    } else {
        "other"
    };
    Some(format!("<io error: {category}>"))
}

fn norm_json(v: &mut serde_json::Value, root: &Path) {
    match v {
        serde_json::Value::String(s) => *s = norm_os_error(s).unwrap_or_else(|| norm_text(s, root)),
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| norm_json(x, root)),
        serde_json::Value::Object(o) => o.values_mut().for_each(|x| norm_json(x, root)),
        _ => {}
    }
}

fn dump_table(conn: &Connection, table: &str, root: &Path, out: &mut String) {
    let exists: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)", [table], |r| r.get(0))
        .unwrap();
    if !exists {
        let _ = writeln!(out, "[{table}] (table missing)");
        return;
    }
    let cols: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let order = (1..=cols.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY {order}")).unwrap();
    let rows: Vec<Vec<Value>> = stmt
        .query_map([], |r| (0..cols.len()).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let _ = writeln!(out, "[{table}] {} row(s)", rows.len());
    for row in rows {
        let cells: Vec<String> = cols
            .iter()
            .zip(row)
            .map(|(c, v)| {
                let text = match v {
                    Value::Null => "NULL".to_string(),
                    Value::Integer(i) => i.to_string(),
                    Value::Real(f) => f.to_string(),
                    Value::Text(t) if is_time_column(c) && !t.is_empty() => "<TS>".to_string(),
                    Value::Text(t) => norm_text(&t, root),
                    Value::Blob(b) => format!("<blob {}>", b.len()),
                };
                format!("{c}={text}")
            })
            .collect();
        let _ = writeln!(out, "  {}", cells.join(" | "));
    }
}

fn dump_tree(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        let meta = std::fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            out.push(format!("{rel}/"));
            dump_tree(&path, base, out);
        } else if meta.file_type().is_symlink() {
            // A link's size is the length of its target path, which differs per platform and temp root.
            out.push(format!("{rel} (symlink)"));
        } else {
            out.push(format!("{rel} ({} bytes)", meta.len()));
        }
    }
}

fn snapshot(w: &World) -> String {
    let mut out = String::new();
    out.push_str("== steps ==\n");
    let mut steps = serde_json::Value::Array(w.steps.clone());
    norm_json(&mut steps, &w.root);
    out.push_str(&serde_json::to_string_pretty(&steps).unwrap());
    out.push_str("\n\n== tables ==\n");
    for t in GOLDEN_TABLES {
        dump_table(&w.conn, t, &w.root, &mut out);
    }
    out.push_str("\n== tree ==\n");
    let mut tree = Vec::new();
    dump_tree(&w.root, &w.root, &mut tree);
    for l in tree {
        out.push_str(&l);
        out.push('\n');
    }
    out
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/desktop_golden").join(format!("{name}.golden.txt"))
}

/// Runs `body` in a fresh world and compares the snapshot with the golden file.
pub fn run_golden(name: &str, body: impl FnOnce(&mut World)) {
    let root = unique_test_dir(&format!("golden_{name}"));
    let root = root.canonicalize().unwrap_or(root);
    // Windows canonicalizes to `\\?\D:\..`; there `..` is an ordinary name, so the code under test would
    // no longer see parent components. Scenarios use the plain spelling, like the app does.
    #[cfg(windows)]
    let root = {
        let plain = root.to_str().and_then(|s| s.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC")).map(std::path::PathBuf::from);
        plain.unwrap_or(root)
    };
    let mut w = World {
        conn: db::connect_in_memory().unwrap(),
        root: root.clone(),
        steps: Vec::new(),
    };
    body(&mut w);
    let actual = snapshot(&w);
    let _ = std::fs::remove_dir_all(&root);
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("golden missing: {} (run with UPDATE_GOLDEN=1)", path.display()));
    assert_eq!(expected.replace("\r\n", "\n"), actual, "golden mismatch for {name} ({})", path.display());
}
