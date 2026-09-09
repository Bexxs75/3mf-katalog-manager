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
    }

    let parent = folder_id.unwrap_or("root");
    let query = format!(
        "'{parent}' in parents and trashed = false and \
         (mimeType = 'application/vnd.google-apps.folder' or name contains '.3mf' or name contains '.stl')"
    );

    let response = client
        .get(format!("{base_url}/files"))
        .bearer_auth(access_token)
        .query(&[
            ("q", query.as_str()),
            ("fields", "files(id,name,mimeType,modifiedTime,size)"),
            ("pageSize", "1000"),
        ])
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

    Ok(parsed.files.into_iter().map(CloudEntry::from).collect())
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
