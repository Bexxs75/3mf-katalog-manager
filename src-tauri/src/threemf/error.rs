use std::fmt;

#[derive(Debug)]
pub enum ThreeMfError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    MissingRootModel,
    InvalidTransform(String),
    /// A ZIP entry exceeds the allowed unpacked size (zip bomb).
    EntryTooLarge { path: String, size: u64, max: u64 },
    /// Objects reference each other via <component> elements.
    ComponentCycle,
    /// The component chain exceeds MAX_COMPONENT_DEPTH although acyclic - protects against extremely deep but valid graphs.
    MaxDepthExceeded,
    /// The sum of all unpacked resources exceeds the total budget, or too many
    /// referenced model files were loaded.
    ResourceLimitExceeded(String),
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
            ThreeMfError::ComponentCycle => {
                write!(f, "3mf component graph contains a cycle")
            }
            ThreeMfError::MaxDepthExceeded => {
                write!(f, "3mf component graph exceeds maximum nesting depth")
            }
            ThreeMfError::ResourceLimitExceeded(msg) => write!(f, "{msg}"),
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
