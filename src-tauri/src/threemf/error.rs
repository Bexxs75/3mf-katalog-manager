use std::fmt;

#[derive(Debug)]
pub enum ThreeMfError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    MissingRootModel,
    InvalidTransform(String),
}

impl fmt::Display for ThreeMfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThreeMfError::Io(e) => write!(f, "I/O error: {e}"),
            ThreeMfError::Zip(e) => write!(f, "ZIP/OPC error: {e}"),
            ThreeMfError::Xml(e) => write!(f, "XML error: {e}"),
            ThreeMfError::MissingRootModel => write!(f, "no root 3D model part found in package"),
            ThreeMfError::InvalidTransform(s) => write!(f, "invalid transform attribute: {s}"),
        }
    }
}

impl std::error::Error for ThreeMfError {}

impl From<std::io::Error> for ThreeMfError {
    fn from(e: std::io::Error) -> Self {
        ThreeMfError::Io(e)
    }
}

impl From<zip::result::ZipError> for ThreeMfError {
    fn from(e: zip::result::ZipError) -> Self {
        ThreeMfError::Zip(e)
    }
}

impl From<quick_xml::Error> for ThreeMfError {
    fn from(e: quick_xml::Error) -> Self {
        ThreeMfError::Xml(e)
    }
}
