use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CloudEntry {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub modified_time: String, // RFC3339
    pub size_bytes: Option<i64>,
}

#[derive(Debug)]
pub enum CloudError {
    Network(String),
    Auth(String),
    NotFound(String),
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudError::Network(msg) => write!(f, "network error: {msg}"),
            CloudError::Auth(msg) => write!(f, "authentication error: {msg}"),
            CloudError::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for CloudError {}

pub type CloudResult<T> = Result<T, CloudError>;

#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn list_folder(&self, folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>>;
    async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>>;
    async fn upload(&self, folder_id: Option<&str>, file_name: &str, data: &[u8]) -> CloudResult<CloudEntry>;
    async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockProvider {
        entries: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[async_trait]
    impl StorageProvider for MockProvider {
        async fn list_folder(&self, _folder_id: Option<&str>) -> CloudResult<Vec<CloudEntry>> {
            Ok(vec![CloudEntry {
                id: "file-1".to_string(),
                name: "cube.3mf".to_string(),
                is_folder: false,
                modified_time: "2026-09-09T12:00:00Z".to_string(),
                size_bytes: Some(1024),
            }])
        }

        async fn download(&self, file_id: &str) -> CloudResult<Vec<u8>> {
            self.entries
                .lock()
                .unwrap()
                .get(file_id)
                .cloned()
                .ok_or_else(|| CloudError::NotFound(file_id.to_string()))
        }

        async fn upload(&self, _folder_id: Option<&str>, file_name: &str, data: &[u8]) -> CloudResult<CloudEntry> {
            self.entries.lock().unwrap().insert(file_name.to_string(), data.to_vec());
            Ok(CloudEntry {
                id: file_name.to_string(),
                name: file_name.to_string(),
                is_folder: false,
                modified_time: "2026-09-09T12:00:00Z".to_string(),
                size_bytes: Some(data.len() as i64),
            })
        }

        async fn get_metadata(&self, file_id: &str) -> CloudResult<CloudEntry> {
            self.entries
                .lock()
                .unwrap()
                .get(file_id)
                .map(|data| CloudEntry {
                    id: file_id.to_string(),
                    name: file_id.to_string(),
                    is_folder: false,
                    modified_time: "2026-09-09T12:00:00Z".to_string(),
                    size_bytes: Some(data.len() as i64),
                })
                .ok_or_else(|| CloudError::NotFound(file_id.to_string()))
        }
    }

    #[tokio::test]
    async fn mock_provider_satisfies_storage_provider_as_trait_object() {
        let provider: Box<dyn StorageProvider> = Box::new(MockProvider {
            entries: Mutex::new(HashMap::new()),
        });

        let entries = provider.list_folder(None).await.expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "cube.3mf");

        let uploaded = provider.upload(None, "test.stl", b"data").await.expect("upload");
        assert_eq!(uploaded.id, "test.stl");

        let downloaded = provider.download("test.stl").await.expect("download");
        assert_eq!(downloaded, b"data");

        let meta = provider.get_metadata("test.stl").await.expect("metadata");
        assert_eq!(meta.size_bytes, Some(4));
    }
}
