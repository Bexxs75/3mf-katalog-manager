use serde::Serialize;

/// Transport-neutral counterpart of the desktop error contract. Logging stays
/// in the desktop adapter; classification and wire codes travel with the error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadError {
    pub message: String,
    pub expected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<GeometryErrorCode>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GeometryErrorCode {
    NotFound,
    Unreadable,
    TooLarge,
    Unsupported,
}
impl ReadError {
    pub fn expected(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected: true,
            code: None,
        }
    }
    pub fn with_code(mut self, code: GeometryErrorCode) -> Self {
        self.code = Some(code);
        self
    }
}
impl From<String> for ReadError {
    fn from(message: String) -> Self {
        Self {
            message,
            expected: false,
            code: None,
        }
    }
}
impl From<&str> for ReadError {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ReadError {}
