use std::fmt;

#[derive(Debug)]
pub enum ThreeMfError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    MissingRootModel,
    InvalidTransform(String),
    /// Ein ZIP-Eintrag ueberschreitet die erlaubte entpackte Groesse -
    /// Schutz gegen Zip-Bomben (Security-Review 2026-09-19, Finding A-1).
    EntryTooLarge { path: String, size: u64, max: u64 },
}

impl fmt::Display for ThreeMfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThreeMfError::Io(e) => write!(f, "I/O error: {e}"),
            ThreeMfError::Zip(e) => write!(f, "ZIP/OPC error: {e}"),
            ThreeMfError::Xml(e) => write!(f, "XML error: {e}"),
            ThreeMfError::MissingRootModel => write!(f, "no root 3D model part found in package"),
            ThreeMfError::InvalidTransform(s) => write!(f, "invalid transform attribute: {s}"),
            ThreeMfError::EntryTooLarge { path, size, max } => write!(
                f,
                "package entry \"{path}\" is too large ({size} bytes, maximum {max} bytes)"
            ),
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
