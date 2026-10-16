//! Explicit read-only command allowlist and bounded blocking geometry work.
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::{
    db,
    read::{self, GeometryErrorCode, ReadError},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct GeometryGate {
    workers: Arc<Semaphore>,
    manager_reads: Arc<Semaphore>,
    admission: Arc<Semaphore>,
}
impl Default for GeometryGate {
    fn default() -> Self {
        Self {
            workers: Arc::new(Semaphore::new(2)),
            manager_reads: Arc::new(Semaphore::new(4)),
            admission: Arc::new(Semaphore::new(10)),
        }
    }
}
impl GeometryGate {
    async fn acquire(&self) -> Result<(OwnedSemaphorePermit, OwnedSemaphorePermit), Box<Response>> {
        let admission = self.admission.clone().try_acquire_owned().map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Zu viele Geometrie-Anfragen",
                true,
            )
        })?;
        let worker = self.workers.clone().acquire_owned().await.map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Geometrie nicht verfügbar",
                true,
            )
        })?;
        Ok((admission, worker))
    }
}
fn error(status: StatusCode, message: &str, expected: bool) -> Box<Response> {
    Box::new(
        (
            status,
            Json(json!({"message": message, "expected": expected})),
        )
            .into_response(),
    )
}
pub(crate) fn read_error(e: ReadError) -> Response {
    let status = if e.busy {
        StatusCode::SERVICE_UNAVAILABLE
    } else if e.code == Some(GeometryErrorCode::NotFound) {
        StatusCode::NOT_FOUND
    } else if e.expected {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        // Read errors contain catalog/SQLite diagnostics, never authentication inputs.
        eprintln!(
            "Interner Lesefehler: {}",
            e.message
                .chars()
                .take(1024)
                .flat_map(char::escape_default)
                .collect::<String>()
        );
        return *error(status, "Interner Fehler", false);
    }
    (status, Json(e)).into_response()
}
fn encode(value: impl Serialize) -> Result<Response, ReadError> {
    serde_json::to_value(value)
        .map(|v| Json(v).into_response())
        .map_err(|_| ReadError::from("Antwort konnte nicht kodiert werden"))
}
fn id(args: &Value, key: &str) -> Result<String, Box<Response>> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| s.parse::<i64>().is_ok())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "ID muss eine numerische Zeichenfolge sein",
                true,
            )
        })
}
fn ids(args: &Value, images: bool) -> Result<Vec<String>, Box<Response>> {
    let values = args
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "ids muss eine Liste sein", true))?;
    if images && values.len() > 200 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Höchstens 200 Bild-IDs erlaubt",
            true,
        ));
    }
    values
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| s.parse::<i64>().is_ok())
                .map(str::to_owned)
                .ok_or_else(|| {
                    error(
                        StatusCode::BAD_REQUEST,
                        "IDs müssen numerische Zeichenfolgen sein",
                        true,
                    )
                })
        })
        .collect()
}

pub const READ_COMMANDS: &[&str] = &[
    "get_import_state",
    "get_import_result",

    "list_file_summaries",
    "list_all_file_tags",
    "list_files_by_ids",
    "list_tag_counts",
    "list_print_log_entries",
    "list_folders",
    "list_collections",
    "list_collection_files",
    "list_trash",
    "list_file_images",
    "get_model_geometry",
    "get_app_version",
    "is_preview_build",
    "has_step_preview",
    "list_filament_spools",
    "list_printers",
    "list_printer_history",
    "get_last_printer_for_file",
    "check_filament",
    "get_filament_spool_image",
];

pub async fn invoke(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    command: String,
    args: Value,
    gate: Arc<GeometryGate>,
) -> Response {
    if !READ_COMMANDS.contains(&command.as_str()) {
        return *error(StatusCode::NOT_FOUND, "Befehl im Web nicht verfügbar", true);
    }
    if !args.is_object() {
        return *error(
            StatusCode::BAD_REQUEST,
            "Argumente müssen ein JSON-Objekt sein",
            true,
        );
    }
    let file_id = match command.as_str() {
        "list_print_log_entries" | "get_model_geometry" | "get_last_printer_for_file" => {
            match id(&args, "fileId") {
                Ok(v) => Some(v),
                Err(e) => return *e,
            }
        }
        "list_printer_history" => match id(&args, "printerId") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        "get_filament_spool_image" => match id(&args, "id") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        "list_collection_files" => match id(&args, "collectionId") {
            Ok(v) => Some(v),
            Err(e) => return *e,
        },
        _ => None,
    };
    let file_ids = match command.as_str() {
        "list_file_images" | "list_files_by_ids" => match ids(&args, command == "list_file_images")
        {
            Ok(v) => v,
            Err(e) => return *e,
        },
        _ => Vec::new(),
    };
    match command.as_str() {
        "get_app_version" => return Json(json!(env!("CARGO_PKG_VERSION"))).into_response(),
        "is_preview_build" | "has_step_preview" => return Json(json!(false)).into_response(),
        _ => {}
    }
    let check_ids = if command == "check_filament" {
        let Some(values) = args.get("fileIds").and_then(Value::as_array) else {
            return *error(
                StatusCode::BAD_REQUEST,
                "fileIds muss eine Liste sein",
                true,
            );
        };
        if values.len() > 1000 {
            return *error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Höchstens 1000 IDs erlaubt",
                true,
            );
        }
        let Some(ids) = values
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        else {
            return *error(
                StatusCode::BAD_REQUEST,
                "IDs müssen Zeichenfolgen sein",
                true,
            );
        };
        ids
    } else {
        Vec::new()
    };
    let manager_permit = if matches!(
        command.as_str(),
        "list_filament_spools"
            | "list_printers"
            | "list_printer_history"
            | "get_last_printer_for_file"
            | "check_filament"
            | "get_filament_spool_image"
    ) {
        match gate.manager_reads.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => {
                let mut r = *error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Zu viele Manager-Anfragen",
                    true,
                );
                r.headers_mut().insert("retry-after", "1".parse().unwrap());
                return r;
            }
        }
    } else {
        None
    };
    let permits = if command == "get_model_geometry" {
        match gate.acquire().await {
            Ok(p) => Some(p),
            Err(e) => return *e,
        }
    } else {
        None
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Response, ReadError> {
        // The permits belong to this closure: cancelling HTTP does not release a running parser.
        let _permits = permits;
        let _manager_permit = manager_permit;
        let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
        match command.as_str() {
            "list_filament_spools" => encode(read::manager::list_filament_spools_web(&conn)?),
            "list_printers" => encode(read::manager::list_printers_with_conn(&conn)?),
            "list_printer_history" => encode(read::manager::list_printer_history_limit(
                &conn,
                file_id.unwrap().parse().unwrap(),
                500,
            )?),
            "get_last_printer_for_file" => {
                encode(read::manager::get_last_printer_for_file_with_conn(
                    &conn,
                    file_id.unwrap().parse().unwrap(),
                )?)
            }
            "get_filament_spool_image" => encode(read::manager::get_filament_spool_image(
                &conn,
                file_id.unwrap().parse().unwrap(),
            )?),
            "check_filament" => encode(read::manager::check_filament_with_conn(&conn, &check_ids)?),

            "list_file_summaries" => encode(read::list_file_summaries(&conn)?),
            "list_all_file_tags" => encode(read::list_all_file_tags(&conn)?),
            "list_files_by_ids" => encode(read::list_files_by_ids(&conn, file_ids)?),
            "list_tag_counts" => encode(read::list_tag_counts(&conn)?),
            "list_print_log_entries" => {
                encode(read::list_print_log_entries(&conn, file_id.unwrap())?)
            }
            "list_folders" => encode(read::list_folders(&conn)?),
            "list_collections" => encode(read::list_collections(&conn)?),
            "list_collection_files" => {
                encode(read::list_collection_files(&conn, file_id.unwrap())?)
            }
            "list_trash" => encode(read::list_trash(&conn)?),
            "list_file_images" => {
                let numeric = file_ids
                    .iter()
                    .map(|s| s.parse::<i64>().expect("validated ID"))
                    .collect::<Vec<_>>();
                let images = read::load_file_images(&conn, &numeric)?;
                drop(conn);
                encode(read::encode_file_images(images))
            }
            "get_model_geometry" => {
                let source = read::load_geometry_source(&conn, &file_id.unwrap())?;
                drop(conn);
                let bytes = read::get_model_geometry(&source.path, Some(&roots))?;
                Ok(([("content-type", "application/octet-stream")], bytes).into_response())
            }
            _ => unreachable!("allowlist checked"),
        }
    })
    .await;
    match result {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => read_error(e),
        Err(_) => *error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Anfrage konnte nicht verarbeitet werden",
            false,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use sha2::{Digest, Sha256};
    struct Catalog {
        dir: PathBuf,
        db: PathBuf,
        id: String,
        collection: String,
    }
    impl Drop for Catalog {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn catalog() -> Catalog {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let dir =
            std::env::temp_dir().join(format!("server-invoke-{:x}", u128::from_le_bytes(random)));
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("catalog.db");
        let conn = db::connect(&db).unwrap();
        let path = dir.join("one.stl");
        let mut stl = vec![0u8; 80];
        stl.extend(1u32.to_le_bytes());
        stl.extend([0u8; 12]);
        for c in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            stl.extend(c.to_le_bytes());
        }
        stl.extend([0u8; 2]);
        std::fs::write(&path, stl).unwrap();
        let folder =
            db::insert_folder_with_parent(&conn, "Ordner", None, dir.to_str().unwrap()).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), Some(folder)).unwrap();
        conn.execute("UPDATE files SET file_type='stl', thumbnail_png=x'010203', render_snapshot_png=x'04', custom_image_png=x'05'", []).unwrap();
        db::add_tag_to_file(&conn, id, "Tag").unwrap();
        let collection = db::create_collection(&conn, "Sammlung", "now").unwrap();
        db::add_file_to_collection(&conn, collection, id, 0).unwrap();
        db::insert_print_log_entry(
            &conn,
            &db::models::NewPrintLogEntry {
                file_id: id,
                printed_at: "2026-10-16".into(),
                note: Some("Druck".into()),
                photo_png: Some(vec![1, 2]),
            },
        )
        .unwrap();
        let trash =
            db::test_insert_minimal_file(&conn, "/missing/original.stl", Some(folder)).unwrap();
        db::soft_delete_file(&conn, trash, Some(path.to_str().unwrap()), "2026-10-16").unwrap();
        Catalog {
            dir,
            db,
            id: id.to_string(),
            collection: collection.to_string(),
        }
    }
    async fn call(c: &Catalog, command: &str, args: Value) -> Response {
        invoke(
            c.db.clone(),
            vec![c.dir.clone()],
            command.into(),
            args,
            Arc::new(GeometryGate::default()),
        )
        .await
    }
    async fn body(response: Response) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }
    fn entries(c: &Catalog) -> Vec<std::ffi::OsString> {
        let mut v = std::fs::read_dir(&c.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>();
        v.sort();
        v
    }
    #[tokio::test]
    async fn unexpected_errors_hide_details_and_expected_errors_keep_them() {
        for (e, status, message) in [
            (
                ReadError::from("no such table: private_table"),
                500,
                "Interner Fehler",
            ),
            (ReadError::expected("Fachfehler"), 422, "Fachfehler"),
        ] {
            let response = read_error(e);
            assert_eq!(response.status(), status);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["message"], message);
        }
    }
    #[tokio::test]
    async fn all_commands_match_core_and_catalog_end_state_is_unchanged() {
        let c = catalog();
        let before = Sha256::digest(std::fs::read(&c.db).unwrap());
        let names = entries(&c);
        let conn = db::open_read_only(&c.db).unwrap();
        let cases = vec![
            (
                "list_file_summaries",
                json!({}),
                serde_json::to_value(read::list_file_summaries(&conn).unwrap()).unwrap(),
            ),
            (
                "list_all_file_tags",
                json!({}),
                serde_json::to_value(read::list_all_file_tags(&conn).unwrap()).unwrap(),
            ),
            (
                "list_files_by_ids",
                json!({"ids":[c.id]}),
                serde_json::to_value(read::list_files_by_ids(&conn, vec![c.id.clone()]).unwrap())
                    .unwrap(),
            ),
            (
                "list_tag_counts",
                json!({}),
                serde_json::to_value(read::list_tag_counts(&conn).unwrap()).unwrap(),
            ),
            (
                "list_print_log_entries",
                json!({"fileId":c.id}),
                serde_json::to_value(read::list_print_log_entries(&conn, c.id.clone()).unwrap())
                    .unwrap(),
            ),
            (
                "list_folders",
                json!({}),
                serde_json::to_value(read::list_folders(&conn).unwrap()).unwrap(),
            ),
            (
                "list_collections",
                json!({}),
                serde_json::to_value(read::list_collections(&conn).unwrap()).unwrap(),
            ),
            (
                "list_collection_files",
                json!({"collectionId":c.collection}),
                serde_json::to_value(
                    read::list_collection_files(&conn, c.collection.clone()).unwrap(),
                )
                .unwrap(),
            ),
            (
                "list_trash",
                json!({}),
                serde_json::to_value(read::list_trash(&conn).unwrap()).unwrap(),
            ),
            (
                "list_file_images",
                json!({"ids":[c.id]}),
                serde_json::to_value(read::encode_file_images(
                    read::load_file_images(&conn, &[c.id.parse().unwrap()]).unwrap(),
                ))
                .unwrap(),
            ),
            (
                "get_app_version",
                json!({}),
                json!(env!("CARGO_PKG_VERSION")),
            ),
            ("is_preview_build", json!({}), json!(false)),
            ("has_step_preview", json!({}), json!(false)),
        ];
        drop(conn);
        for (command, args, expected) in cases {
            let response = call(&c, command, args).await;
            assert_eq!(response.status(), StatusCode::OK, "{command}");
            assert_eq!(
                serde_json::from_slice::<Value>(&body(response).await).unwrap(),
                expected,
                "{command}"
            );
        }
        let response = call(&c, "get_model_geometry", json!({"fileId":c.id})).await;
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
        assert_eq!(
            body(response).await,
            read::get_model_geometry(&c.dir.join("one.stl"), Some(std::slice::from_ref(&c.dir)))
                .unwrap()
        );
        assert_eq!(before, Sha256::digest(std::fs::read(&c.db).unwrap()));
        assert_eq!(names, entries(&c));
    }
    #[tokio::test]
    async fn invalid_arguments_and_unavailable_commands() {
        let c = catalog();
        for command in ["unknown", "add_tag", "delete_file", "start_import"] {
            let r = call(&c, command, json!({})).await;
            assert_eq!(r.status(), 404);
            assert_eq!(
                serde_json::from_slice::<Value>(&body(r).await).unwrap(),
                json!({"message":"Befehl im Web nicht verfügbar","expected":true})
            );
        }
        for (cmd, args) in [
            ("list_file_summaries", json!([])),
            ("get_model_geometry", json!({"fileId":"no"})),
            ("list_files_by_ids", json!({"ids":[1]})),
            ("list_collection_files", json!({"collectionId":"1.0"})),
            ("list_file_images", json!({"ids":vec!["1";201]})),
        ] {
            assert_eq!(call(&c, cmd, args).await.status(), 400);
        }
    }
    #[tokio::test]
    async fn geometry_roots_reject_outside_and_symlink() {
        let c = catalog();
        let root = c.dir.join("allowed");
        std::fs::create_dir(&root).unwrap();
        let r = invoke(
            c.db.clone(),
            vec![root.clone()],
            "get_model_geometry".into(),
            json!({"fileId":c.id}),
            Arc::new(GeometryGate::default()),
        )
        .await;
        assert_eq!(r.status(), 422);
        #[cfg(unix)]
        {
            let link = root.join("escape.stl");
            std::os::unix::fs::symlink(c.dir.join("one.stl"), &link).unwrap();
            let conn = db::connect(&c.db).unwrap();
            conn.execute(
                "UPDATE files SET path=?1 WHERE deleted_at IS NULL",
                [link.to_str().unwrap()],
            )
            .unwrap();
            drop(conn);
            let r = invoke(
                c.db.clone(),
                vec![root],
                "get_model_geometry".into(),
                json!({"fileId":c.id}),
                Arc::new(GeometryGate::default()),
            )
            .await;
            assert_eq!(r.status(), 422);
        }
    }
    #[tokio::test]
    async fn busy_marker_maps_to_503_without_message_parsing() {
        let mut e = ReadError::from("nicht textabhängig");
        e.busy = true;
        assert_eq!(read_error(e).status(), 503);
        assert_eq!(read_error(ReadError::from("database locked")).status(), 500);
    }
    #[tokio::test]
    async fn geometry_queue_is_bounded_and_cancellation_retains_running_permits() {
        let gate = Arc::new(GeometryGate::default());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let g = gate.clone();
        let running = tokio::spawn(async move {
            let permits = g.acquire().await.unwrap();
            tokio::task::spawn_blocking(move || {
                let _permits = permits;
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
            .unwrap();
        });
        started_rx.await.unwrap();
        running.abort();
        let second = gate.acquire().await.unwrap();
        assert_eq!(gate.workers.available_permits(), 0);
        let mut waiters = Vec::new();
        for _ in 0..8 {
            let g = gate.clone();
            waiters.push(tokio::spawn(async move { g.acquire().await }));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while gate.admission.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(gate.acquire().await.unwrap_err().status(), 503);
        for waiter in waiters {
            waiter.abort();
            let _ = waiter.await;
        }
        assert_eq!(gate.workers.available_permits(), 0);
        drop(second);
        release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while gate.workers.available_permits() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    #[tokio::test]
    async fn manager_read_concurrency_is_bounded_and_returns_retry_after() {
        let gate = Arc::new(GeometryGate::default());
        let permits = gate
            .manager_reads
            .clone()
            .acquire_many_owned(4)
            .await
            .unwrap();
        for command in [
            "list_filament_spools",
            "get_filament_spool_image",
            "check_filament",
            "list_printers",
            "list_printer_history",
            "get_last_printer_for_file",
        ] {
            let r = invoke(
                PathBuf::from("unused"),
                vec![],
                command.into(),
                json!({"id":"1","fileIds":[],"printerId":"1","fileId":"1"}),
                gate.clone(),
            )
            .await;
            assert_eq!(r.status(), 503, "{command}");
            assert_eq!(r.headers()["retry-after"], "1");
        }
        drop(permits);
        assert_eq!(gate.manager_reads.available_permits(), 4);
    }
}
