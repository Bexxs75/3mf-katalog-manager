use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    dir: PathBuf,
    roots: Vec<PathBuf>,
    conn: Connection,
}
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let dir =
            std::env::temp_dir().join(format!("srcdirs-{:032x}", u128::from_ne_bytes(random)));
        let root = dir.join("source");
        std::fs::create_dir_all(&root).unwrap();
        Self {
            dir,
            roots: vec![root],
            conn: db::connect_in_memory().unwrap(),
        }
    }
    fn fs(&self) -> SourceAdapter<'_> {
        SourceAdapter { roots: &self.roots }
    }
    fn read(&self, cmd: &str, args: Value) -> ImportResult<Value> {
        read(&self.conn, &self.roots, cmd, &args)
    }
    fn write(&self, cmd: &str, args: Value) -> ImportResult<(Value, u64)> {
        write(&self.conn, &self.roots, cmd, &args)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn source_dirs_reject_traversal_absolute_and_free_path_arguments() {
    let f = Fixture::new();
    for rel in [
        "..",
        "child/../other",
        "/etc",
        "C:\\Users",
        "./x",
        "x//y",
        "x/",
    ] {
        assert_eq!(
            f.read("list_source_dirs", json!({"rootIdx":0,"rel":rel}))
                .unwrap_err(),
            denied()
        );
        assert_eq!(
            f.write("add_source_folder", json!({"rootIdx":0,"rel":rel}))
                .unwrap_err(),
            denied()
        );
    }
    assert!(f
        .write("add_source_folder", json!({"path":f.roots[0]}))
        .is_err());
    assert_eq!(
        f.read("list_source_dirs", json!({"rootIdx":99}))
            .unwrap_err(),
        denied()
    );
    assert_eq!(
        read(&f.conn, &[], "list_source_dirs", &json!({})).unwrap()["roots"],
        json!([])
    );
}

#[test]
fn source_dirs_skip_symlinks_fifo_and_device_without_opening_them() {
    let f = Fixture::new();
    let root = &f.roots[0];
    std::fs::create_dir(f.dir.join("outside")).unwrap();
    std::fs::write(f.dir.join("outside/secret.stl"), "secret").unwrap();
    symlink(f.dir.join("outside"), root.join("escape")).unwrap();
    symlink("/proc", root.join("proc")).unwrap();
    symlink("/dev/null", root.join("device.stl")).unwrap();
    symlink(f.dir.join("outside/secret.stl"), root.join("link.stl")).unwrap();
    assert!(std::process::Command::new("mkfifo")
        .arg(root.join("pipe.stl"))
        .status()
        .unwrap()
        .success());
    std::fs::write(root.join("ok.stl"), "model").unwrap();
    std::fs::write(root.join("ignored.txt"), "text").unwrap();
    let result = f.read("list_source_dirs", json!({"rootIdx":0})).unwrap();
    assert_eq!(result["entries"].as_array().unwrap().len(), 1);
    assert_eq!(result["entries"][0]["rel"], "ok.stl");
    for rel in ["escape", "proc", "pipe.stl", "device.stl"] {
        assert!(f
            .read("list_source_dirs", json!({"rootIdx":0,"rel":rel}))
            .is_err());
    }
    let devices = [PathBuf::from("/dev")];
    let fs = SourceAdapter { roots: &devices };
    assert_eq!(
        fs.stat(Path::new("/dev/null"), false).unwrap().kind,
        SourceKind::Other
    );
    assert!(fs.content_hash(Path::new("/dev/null")).is_err());
    assert!(f.fs().content_hash(&root.join("pipe.stl")).is_err());
    assert!(f.fs().content_hash(&root.join("link.stl")).is_err());
}

#[test]
fn source_dirs_replaced_root_and_intermediate_link_become_unreachable() {
    let f = Fixture::new();
    std::fs::create_dir(f.roots[0].join("child")).unwrap();
    let added = f
        .write("add_source_folder", json!({"rootIdx":0,"rel":"child"}))
        .unwrap()
        .0;
    std::fs::rename(&f.roots[0], f.dir.join("old")).unwrap();
    symlink(f.dir.join("old"), &f.roots[0]).unwrap();
    assert!(f.read("list_source_dirs", json!({"rootIdx":0})).is_err());
    let listed = f.read("list_source_folders", json!({})).unwrap();
    assert_eq!(listed[0]["id"], added["id"]);
    assert_eq!(listed[0]["reachable"], false);
    assert_eq!(
        f.read("scan_source_folders", json!({})).unwrap()[0]["unreachable"],
        true
    );
    std::fs::remove_file(&f.roots[0]).unwrap();
    std::fs::rename(f.dir.join("old"), &f.roots[0]).unwrap();
    std::fs::rename(f.roots[0].join("child"), f.roots[0].join("old-child")).unwrap();
    symlink("old-child", f.roots[0].join("child")).unwrap();
    assert_eq!(
        f.read("list_source_folders", json!({})).unwrap()[0]["reachable"],
        false
    );
}

#[test]
fn source_dirs_counts_and_limits_are_bounded_and_signalled() {
    let f = Fixture::new();
    let root = &f.roots[0];
    std::fs::create_dir_all(root.join("a/b/c/d/e")).unwrap();
    std::fs::write(root.join("a/one.stl"), "model").unwrap();
    std::fs::write(root.join("a/b/two.3mf"), "model").unwrap();
    std::fs::write(root.join("a/b/c/d/e/deep.stl"), "model").unwrap();
    let result = f.read("list_source_dirs", json!({"rootIdx":0})).unwrap();
    assert_eq!(result["entries"][0]["fileCount"], 2);
    assert_eq!(result["truncated"], true);
    assert_eq!(result["entries"][0]["truncated"], true);
    let result = browse(
        &f.fs(),
        Browse {
            root_idx: Some(0),
            rel: String::new(),
        },
        3,
        1,
    )
    .unwrap();
    assert_eq!(result["truncated"], true);
    assert_eq!(result["entries"].as_array().unwrap().len(), 1);
    for i in 0..MAX_SCAN_ENTRIES + 1 {
        std::fs::write(root.join(format!("{i}.txt")), "").unwrap();
    }
    let result = f.read("list_source_dirs", json!({"rootIdx":0})).unwrap();
    assert_eq!(result["truncated"], true);
    assert!(result["entries"].as_array().unwrap().len() <= MAX_SCAN_ENTRIES);
}

#[test]
fn source_dirs_roundtrip_keeps_sources_and_hides_server_paths() {
    let f = Fixture::new();
    std::fs::write(f.roots[0].join("one.stl"), "model").unwrap();
    let added = f
        .write("add_source_folder", json!({"rootIdx":0,"rel":""}))
        .unwrap()
        .0;
    assert_eq!(added["path"], "source");
    assert_eq!(added["rootIdx"], 0);
    let scans = f.read("scan_source_folders", json!({})).unwrap();
    assert_eq!(scans[0]["files"][0]["path"], "0/one.stl");
    assert!(!scans.to_string().contains(f.dir.to_str().unwrap()));
    assert_eq!(
        f.write("reject_source_files", json!({"paths":["0/one.stl"]}))
            .unwrap()
            .0,
        1
    );
    assert!(
        f.read("scan_source_folders", json!({})).unwrap()[0]["files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.write("reset_rejected_source_files", json!({})).unwrap().0,
        1
    );
    assert_eq!(
        f.read("scan_source_folders", json!({})).unwrap()[0]["files"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(f
        .write(
            "reject_source_files",
            json!({"paths":[f.roots[0].join("one.stl")]})
        )
        .is_err());
    assert_eq!(
        f.read("get_source_check_on_start", json!({})).unwrap(),
        false
    );
    f.write("set_source_check_on_start", json!({"enabled":true}))
        .unwrap();
    assert_eq!(
        f.read("get_source_check_on_start", json!({})).unwrap(),
        true
    );
    f.write("remove_source_folder", json!({"id":added["id"]}))
        .unwrap();
    assert_eq!(f.read("list_source_folders", json!({})).unwrap(), json!([]));
    assert_eq!(
        std::fs::read_to_string(f.roots[0].join("one.stl")).unwrap(),
        "model"
    );
}

#[test]
fn source_dirs_restored_desktop_sources_are_preserved_unreachable_and_redacted() {
    let f = Fixture::new();
    for path in ["/private/desktop/downloads", "C:\\Users\\Person\\models"] {
        store::insert_source_folder(&f.conn, path, "now").unwrap();
    }
    let listed = f.read("list_source_folders", json!({})).unwrap();
    for folder in listed.as_array().unwrap() {
        assert_eq!(folder["reachable"], false);
        assert_eq!(folder["rootIdx"], Value::Null);
        assert!(!folder["path"].as_str().unwrap().contains(['/', '\\']));
    }
    let scans = f.read("scan_source_folders", json!({})).unwrap();
    assert!(scans
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["unreachable"] == true));
    assert_eq!(store::list_source_folders(&f.conn).unwrap().len(), 2);
}
