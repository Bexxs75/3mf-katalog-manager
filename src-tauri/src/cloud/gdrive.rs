use async_trait::async_trait;

use crate::cloud::provider::{CloudEntry, CloudError, CloudResult, StorageProvider};

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";

pub struct GoogleDriveProvider {
    access_token: String,
    client: reqwest::Client,
}

impl GoogleDriveProvider {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl StorageProvider for GoogleDriveProvider {
    async fn list_folder(&self, folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>> {
        list_folder_from(DRIVE_API_BASE, &self.client, &self.access_token, folder_id).await
    }

    async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>> {
        download_from(DRIVE_API_BASE, &self.client, &self.access_token, file_id).await
    }

    async fn upload(&self, _folder_id: Option<&str>, _file_name: &str, _data: &[u8]) -> CloudResult<CloudEntry> {
        Err(CloudError::Network(
            "Hochladen zu Google Drive ist noch nicht implementiert".to_string(),
        ))
    }

    async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry> {
        get_metadata_from(DRIVE_API_BASE, &self.client, &self.access_token, file_id).await
    }
}

#[derive(serde::Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "modifiedTime")]
    modified_time: String,
    #[serde(default)]
    size: Option<String>,
}

impl From<DriveFile> for CloudEntry {
    fn from(f: DriveFile) -> Self {
        CloudEntry {
            id: f.id,
            name: f.name,
            is_folder: f.mime_type == "application/vnd.google-apps.folder",
            modified_time: f.modified_time,
            size_bytes: f.size.and_then(|s| s.parse::<i64>().ok()),
        }
    }
}

async fn list_folder_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    folder_id: Option<&str>,
) -> CloudResult<Vec<CloudEntry>> {
    #[derive(serde::Deserialize)]
    struct FileListResponse {
        files: Vec<DriveFile>,
        #[serde(rename = "nextPageToken", default)]
        next_page_token: Option<String>,
    }

    // Drive's `contains` operator only prefix-matches on `name`, so a
    // server-side filter like `name contains '.3mf'` would never match
    // e.g. "cube.3mf" (it would only match names STARTING with ".3mf").
    // The query below therefore only narrows by parent/trashed on the
    // server; the .3mf/.stl extension filter happens client-side below,
    // which also makes it case-insensitive (".STL" etc.).
    let parent = folder_id.unwrap_or("root");
    let query = format!("'{parent}' in parents and trashed = false");

    let mut all_files = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut query_params = vec![
            ("q", query.as_str()),
            ("fields", "files(id,name,mimeType,modifiedTime,size),nextPageToken"),
            ("pageSize", "1000"),
        ];
        if let Some(token) = page_token.as_deref() {
            query_params.push(("pageToken", token));
        }

        let response = client
            .get(format!("{base_url}/files"))
            .bearer_auth(access_token)
            .query(&query_params)
            .send()
            .await
            .map_err(|e| CloudError::Network(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
        }
        if !response.status().is_success() {
            return Err(CloudError::Network(format!(
                "Google-Drive-Anfrage fehlgeschlagen: HTTP {}",
                response.status()
            )));
        }

        let parsed: FileListResponse = response
            .json()
            .await
            .map_err(|e| CloudError::Network(format!("Antwort konnte nicht gelesen werden: {e}")))?;

        all_files.extend(parsed.files);

        match parsed.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }

    Ok(all_files
        .into_iter()
        .map(CloudEntry::from)
        .filter(|e| {
            e.is_folder || {
                let lower = e.name.to_lowercase();
                lower.ends_with(".3mf") || lower.ends_with(".stl")
            }
        })
        .collect())
}

async fn get_metadata_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    file_id: &str,
) -> CloudResult<CloudEntry> {
    let response = client
        .get(format!("{base_url}/files/{file_id}"))
        .bearer_auth(access_token)
        .query(&[("fields", "id,name,mimeType,modifiedTime,size")])
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CloudError::NotFound(file_id.to_string()));
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
    }
    if !response.status().is_success() {
        return Err(CloudError::Network(format!(
            "Google-Drive-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    let parsed: DriveFile = response
        .json()
        .await
        .map_err(|e| CloudError::Network(format!("Antwort konnte nicht gelesen werden: {e}")))?;

    Ok(CloudEntry::from(parsed))
}

async fn download_from(
    base_url: &str,
    client: &reqwest::Client,
    access_token: &str,
    file_id: &str,
) -> CloudResult<Vec<u8>> {
    let response = client
        .get(format!("{base_url}/files/{file_id}"))
        .bearer_auth(access_token)
        .query(&[("alt", "media")])
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CloudError::NotFound(file_id.to_string()));
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CloudError::Auth("Zugriffstoken abgelaufen".to_string()));
    }
    if !response.status().is_success() {
        return Err(CloudError::Network(format!(
            "Google-Drive-Download fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| CloudError::Network(e.to_string()))
}

const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

pub async fn fetch_google_account_email(access_token: &str) -> CloudResult<String> {
    fetch_google_account_email_from(USERINFO_URL, access_token).await
}

async fn fetch_google_account_email_from(base_url: &str, access_token: &str) -> CloudResult<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(base_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(CloudError::Auth(format!(
            "Google-Userinfo-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    #[derive(serde::Deserialize)]
    struct UserInfo {
        email: String,
    }

    let info: UserInfo = response
        .json()
        .await
        .map_err(|e| CloudError::Auth(format!("Antwort konnte nicht gelesen werden: {e}")))?;
    Ok(info.email)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Minimaler synchroner Mock-HTTP-Server, der genau eine Anfrage
    /// beantwortet - kein neues Test-Crate noetig, nur std::net.
    fn spawn_mock_userinfo_server(body: &'static str, status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "{status_line}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// Mock-Server, der die eingehende Anfrage (Request-Zeile + Header) in
    /// einen geteilten Puffer schreibt, bevor er die kanonische Antwort
    /// sendet - damit Tests die tatsaechlich gesendete Query-String/Header
    /// pruefen koennen, statt nur die kanonische Antwort zu konsumieren.
    fn spawn_mock_capture_server(
        body: &'static str,
        status_line: &'static str,
    ) -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        let captured = Arc::new(Mutex::new(String::new()));
        let captured_clone = Arc::clone(&captured);
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                if let Ok(n) = stream.read(&mut buf) {
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    *captured_clone.lock().expect("lock captured request") = request;
                }
                let response = format!(
                    "{status_line}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}"), captured)
    }

    /// Mock-Server, der mehrere Antworten nacheinander auf getrennten
    /// Verbindungen ausliefert (fuer Pagination-Tests). Jede Antwort traegt
    /// `Connection: close`, damit reqwest fuer die naechste Seite garantiert
    /// eine neue Verbindung aufbaut statt Keep-Alive zu versuchen.
    fn spawn_mock_paginated_server(bodies: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            for body in bodies {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 8192];
                    let _ = stream.read(&mut buf);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn parses_email_from_successful_response() {
        let url = spawn_mock_userinfo_server(r#"{"email": "user@example.com"}"#, "HTTP/1.1 200 OK");
        let email = fetch_google_account_email_from(&url, "fake-token").await.expect("fetch");
        assert_eq!(email, "user@example.com");
    }

    #[tokio::test]
    async fn returns_auth_error_for_non_success_status() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 401 Unauthorized");
        let result = fetch_google_account_email_from(&url, "expired-token").await;
        assert!(matches!(result, Err(CloudError::Auth(_))));
    }

    #[tokio::test]
    async fn list_folder_parses_files_and_folders() {
        let url = spawn_mock_userinfo_server(
            r#"{"files": [
                {"id": "folder-1", "name": "Vasen", "mimeType": "application/vnd.google-apps.folder", "modifiedTime": "2026-09-01T10:00:00Z"},
                {"id": "file-1", "name": "cube.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "2048"}
            ]}"#,
            "HTTP/1.1 200 OK",
        );
        let client = reqwest::Client::new();
        let entries = list_folder_from(&url, &client, "fake-token", None).await.expect("list");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "folder-1");
        assert!(entries[0].is_folder);
        assert_eq!(entries[0].size_bytes, None);
        assert_eq!(entries[1].id, "file-1");
        assert!(!entries[1].is_folder);
        assert_eq!(entries[1].size_bytes, Some(2048));
    }

    #[tokio::test]
    async fn list_folder_returns_auth_error_for_401() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 401 Unauthorized");
        let client = reqwest::Client::new();
        let result = list_folder_from(&url, &client, "expired-token", None).await;
        assert!(matches!(result, Err(CloudError::Auth(_))));
    }

    #[tokio::test]
    async fn list_folder_request_only_filters_by_parent_and_trashed_not_by_name_contains() {
        // Findung 1 (KRITISCH): Drive's `contains`-Operator matcht bei
        // `name` nur als Praefix - `name contains '.3mf'` traf nie auf
        // "cube.3mf" zu. Dieser Test faengt die tatsaechlich gesendete
        // Anfrage ab und prueft, dass server-seitig NICHT mehr per `contains`
        // gefiltert wird (die Erweiterungspruefung passiert client-seitig).
        let (url, captured) = spawn_mock_capture_server(r#"{"files": []}"#, "HTTP/1.1 200 OK");
        let client = reqwest::Client::new();
        let _ = list_folder_from(&url, &client, "fake-token", None)
            .await
            .expect("list");

        let request = captured.lock().expect("lock captured request").clone();
        let request_line = request.lines().next().unwrap_or_default();

        assert!(
            request_line.starts_with("GET /files?"),
            "unerwartete Request-Zeile: {request_line}"
        );
        assert!(
            request_line.contains("trashed+%3D+false") || request_line.contains("trashed = false"),
            "Query-String enthaelt keinen trashed=false-Filter: {request_line}"
        );
        assert!(
            !request_line.contains("contains"),
            "server-seitiger 'contains'-Filter sollte entfernt sein (Praefix-Bug): {request_line}"
        );
        assert!(
            request
                .to_lowercase()
                .contains("authorization: bearer fake-token"),
            "erwartete Authorization-Header mit Bearer-Token: {request}"
        );
    }

    #[tokio::test]
    async fn list_folder_filters_by_extension_client_side() {
        // Findung 1: Da der Server nicht mehr nach Erweiterung filtert, muss
        // der Client zuverlaessig .3mf/.stl (gross-/kleinschreibungs-
        // unabhaengig) behalten und alles andere (z.B. .txt) verwerfen -
        // Ordner bleiben unabhaengig vom Namen immer erhalten.
        let url = spawn_mock_userinfo_server(
            r#"{"files": [
                {"id": "folder-1", "name": "Vasen", "mimeType": "application/vnd.google-apps.folder", "modifiedTime": "2026-09-01T10:00:00Z"},
                {"id": "file-1", "name": "cube.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "2048"},
                {"id": "file-2", "name": "model.STL", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "4096"},
                {"id": "file-3", "name": "notes.txt", "mimeType": "text/plain", "modifiedTime": "2026-09-02T11:00:00Z", "size": "10"}
            ]}"#,
            "HTTP/1.1 200 OK",
        );
        let client = reqwest::Client::new();
        let entries = list_folder_from(&url, &client, "fake-token", None)
            .await
            .expect("list");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(entries.len(), 3, "erwartete Vasen, cube.3mf, model.STL: {names:?}");
        assert!(names.contains(&"Vasen"));
        assert!(names.contains(&"cube.3mf"));
        assert!(names.contains(&"model.STL"));
        assert!(!names.contains(&"notes.txt"));
    }

    #[tokio::test]
    async fn list_folder_follows_pagination_and_merges_pages() {
        // Findung 5: nextPageToken muss verfolgt und alle Seiten
        // zusammengefuehrt werden, statt nach der ersten Seite abzubrechen.
        let url = spawn_mock_paginated_server(vec![
            r#"{"files": [{"id": "file-1", "name": "a.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-01T10:00:00Z", "size": "1"}], "nextPageToken": "page2token"}"#,
            r#"{"files": [{"id": "file-2", "name": "b.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-01T10:00:00Z", "size": "2"}]}"#,
        ]);
        let client = reqwest::Client::new();
        let entries = list_folder_from(&url, &client, "fake-token", None)
            .await
            .expect("list");

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(entries.len(), 2, "erwartete beide Seiten zusammengefuehrt: {ids:?}");
        assert!(ids.contains(&"file-1"));
        assert!(ids.contains(&"file-2"));
    }

    #[tokio::test]
    async fn get_metadata_parses_a_single_file() {
        let url = spawn_mock_userinfo_server(
            r#"{"id": "file-1", "name": "cube.3mf", "mimeType": "application/octet-stream", "modifiedTime": "2026-09-02T11:00:00Z", "size": "2048"}"#,
            "HTTP/1.1 200 OK",
        );
        let client = reqwest::Client::new();
        let entry = get_metadata_from(&url, &client, "fake-token", "file-1").await.expect("metadata");
        assert_eq!(entry.name, "cube.3mf");
        assert_eq!(entry.modified_time, "2026-09-02T11:00:00Z");
        assert_eq!(entry.size_bytes, Some(2048));
    }

    #[tokio::test]
    async fn get_metadata_returns_not_found_for_404() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 404 Not Found");
        let client = reqwest::Client::new();
        let result = get_metadata_from(&url, &client, "fake-token", "missing-file").await;
        assert!(matches!(result, Err(CloudError::NotFound(_))));
    }

    #[tokio::test]
    async fn download_returns_raw_bytes() {
        let url = spawn_mock_userinfo_server("raw-file-content", "HTTP/1.1 200 OK");
        let client = reqwest::Client::new();
        let data = download_from(&url, &client, "fake-token", "file-1").await.expect("download");
        assert_eq!(data, b"raw-file-content");
    }
}
