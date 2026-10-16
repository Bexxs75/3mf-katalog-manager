#[test]
fn server_dependency_tree_excludes_desktop_and_occt() {
    let output = std::process::Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "tree",
            "-p",
            "katalog-server",
            "--offline",
            "--no-default-features",
            "--prefix",
            "none",
        ])
        .output()
        .expect("offline dependency tree");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    for line in tree.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        assert!(
            ![
                "tauri",
                "gtk",
                "gtk-sys",
                "webkit2gtk",
                "webkit2gtk-sys",
                "wry",
                "tao",
                "opencascade-sys"
            ]
            .contains(&name),
            "unexpected server dependency {name}"
        );
    }
}
