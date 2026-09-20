use std::fmt;

#[derive(Debug)]
pub enum ObjError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjError::Io(e) => write!(f, "I/O error: {e}"),
            ObjError::Parse(msg) => write!(f, "OBJ parse error: {msg}"),
        }
    }
}

impl std::error::Error for ObjError {}

impl From<std::io::Error> for ObjError {
    fn from(e: std::io::Error) -> Self {
        ObjError::Io(e)
    }
}
