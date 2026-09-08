use std::fmt;

#[derive(Debug)]
pub enum StlError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for StlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StlError::Io(e) => write!(f, "I/O error: {e}"),
            StlError::Parse(msg) => write!(f, "STL parse error: {msg}"),
        }
    }
}

impl std::error::Error for StlError {}

impl From<std::io::Error> for StlError {
    fn from(e: std::io::Error) -> Self {
        StlError::Io(e)
    }
}
