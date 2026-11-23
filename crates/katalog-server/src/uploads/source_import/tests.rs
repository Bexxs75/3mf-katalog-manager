use super::*;

#[test]
fn source_import_copy_preserves_source_and_collision_and_cleans_cancelled_file() {
    let mut random = [0; 8];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().join(format!("source-copy-{}", u64::from_ne_bytes(random)));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("model.stl");
    std::fs::write(&path, b"original").unwrap();
    let dir = Directory::open(&root).unwrap();
    let mut copy = copy_file(&dir, "model.stl", dir.read("model.stl").unwrap(), 8, || {
        false
    })
    .unwrap();
    assert_eq!(copy.name, "model (1).stl");
    assert_eq!(std::fs::read(root.join(&copy.name)).unwrap(), b"original");
    copy.keep = true;
    drop(copy);
    assert!(
        copy_file(&dir, "model.stl", dir.read("model.stl").unwrap(), 8, || {
            true
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_import_cancel_mid_copy_removes_partial_target() {
    let mut random = [0; 8];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().join(format!("source-cancel-{}", u64::from_ne_bytes(random)));
    std::fs::create_dir(&root).unwrap();
    let bytes = vec![42; 131072];
    std::fs::write(root.join("source.obj"), &bytes).unwrap();
    let dir = Directory::open(&root).unwrap();
    let checks = std::cell::Cell::new(0);
    let result = copy_file(
        &dir,
        "target.obj",
        dir.read("source.obj").unwrap(),
        bytes.len() as u64,
        || {
            checks.set(checks.get() + 1);
            checks.get() > 1
        },
    );
    assert!(result.is_err());
    assert!(!root.join("target.obj").exists());
    assert_eq!(std::fs::read(root.join("source.obj")).unwrap(), bytes);
    std::fs::remove_dir_all(root).unwrap();
}
