use super::*;

async fn fixture() -> Fixture {
    let mut f = Fixture::new(false).await;
    let root = f.dir.join("source");
    std::fs::create_dir(&root).unwrap();
    Arc::get_mut(&mut f.app).unwrap().config.source_roots = vec![root];
    f
}
async fn value(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn source_dirs_http_rejects_traversal_absolute_symlinks_and_replaced_root() {
    let f = fixture().await;
    let root = f.dir.join("source");
    std::os::unix::fs::symlink("/proc", root.join("proc")).unwrap();
    std::os::unix::fs::symlink(f.dir.join("models"), root.join("outside")).unwrap();
    for rel in ["..", "x/../y", "/etc", "proc", "outside"] {
        for command in ["list_source_dirs", "add_source_folder"] {
            let response = f.invoke(command, json!({"rootIdx":0,"rel":rel})).await;
            assert_eq!(response.status(), 422, "{command} {rel}");
            assert_eq!(value(response).await["message"], "sourceRootNotAllowed");
        }
    }
    let response = f.invoke("add_source_folder", json!({"path":root})).await;
    assert_eq!(response.status(), 422);
    assert_eq!(
        f.invoke("add_source_folder", json!({"rootIdx":0,"rel":""}))
            .await
            .status(),
        200
    );
    std::fs::rename(&root, f.dir.join("old-source")).unwrap();
    std::os::unix::fs::symlink(f.dir.join("old-source"), &root).unwrap();
    assert_eq!(
        f.invoke("list_source_dirs", json!({"rootIdx":0}))
            .await
            .status(),
        422
    );
    let rows = value(f.invoke("list_source_folders", json!({})).await).await;
    assert_eq!(rows[0]["reachable"], false);
    assert!(!rows.to_string().contains(f.dir.to_str().unwrap()));
}

#[tokio::test]
async fn source_dirs_http_skips_special_files_and_reports_depth_and_entry_limits() {
    let f = fixture().await;
    let root = f.dir.join("source");
    assert!(std::process::Command::new("mkfifo")
        .arg(root.join("pipe.stl"))
        .status()
        .unwrap()
        .success());
    std::os::unix::fs::symlink("/dev/null", root.join("device.stl")).unwrap();
    std::fs::create_dir_all(root.join("a/b/c/d/e")).unwrap();
    std::fs::write(root.join("a/visible.stl"), "model").unwrap();
    std::fs::write(root.join("a/b/c/d/e/hidden.stl"), "model").unwrap();
    let body = value(f.invoke("list_source_dirs", json!({"rootIdx":0})).await).await;
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(body["entries"][0]["fileCount"], 1);
    assert_eq!(body["truncated"], true);
    for i in 0..5001 {
        std::fs::write(root.join(format!("{i}.stl")), "").unwrap();
    }
    let body = value(f.invoke("list_source_dirs", json!({"rootIdx":0})).await).await;
    assert_eq!(body["truncated"], true);
    assert!(body["entries"].as_array().unwrap().len() <= 5000);
}

#[tokio::test]
async fn source_dirs_http_write_gate_csrf_and_roundtrip() {
    let mut f = fixture().await;
    std::fs::write(f.dir.join("source/model.stl"), "model").unwrap();
    Arc::get_mut(&mut f.app).unwrap().config.write = false;
    assert_eq!(
        f.invoke("add_source_folder", json!({"rootIdx":0,"rel":""}))
            .await
            .status(),
        404
    );
    assert_eq!(f.invoke("list_source_dirs", json!({})).await.status(), 200);
    Arc::get_mut(&mut f.app).unwrap().config.write = true;
    let mut req = f.request(
        "POST",
        "/api/invoke/add_source_folder",
        Body::from(r#"{"rootIdx":0,"rel":""}"#),
    );
    req.headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    req.headers_mut().remove("x-csrf-token");
    assert_eq!(
        router(f.app.clone()).oneshot(req).await.unwrap().status(),
        403
    );
    let added = value(
        f.invoke("add_source_folder", json!({"rootIdx":0,"rel":""}))
            .await,
    )
    .await;
    let scan = value(f.invoke("scan_source_folders", json!({})).await).await;
    let token = scan[0]["files"][0]["path"].clone();
    assert_eq!(
        value(
            f.invoke("reject_source_files", json!({"paths":[token]}))
                .await
        )
        .await,
        1
    );
    assert_eq!(
        value(f.invoke("reset_rejected_source_files", json!({})).await).await,
        1
    );
    assert_eq!(
        f.invoke("set_source_check_on_start", json!({"enabled":true}))
            .await
            .status(),
        200
    );
    assert_eq!(
        value(f.invoke("get_source_check_on_start", json!({})).await).await,
        true
    );
    assert_eq!(
        f.invoke("remove_source_folder", json!({"id":added["id"]}))
            .await
            .status(),
        200
    );
    assert!(f.dir.join("source/model.stl").is_file());
}
