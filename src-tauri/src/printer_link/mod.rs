//! Printer connection: reads the filament used by finished prints from the
//! printer (Klipper/Moonraker first) and deducts it from a spool after
//! confirmation. Home network only, read-only requests, off by default.

pub mod address;
pub mod booking;
pub mod matching;
pub mod moonraker;
pub mod sync;
#[cfg(test)]
pub(crate) mod fake_moonraker;

/// Result of "Test connection".
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionInfo {
    /// Software version on the printer, e.g. "v0.8.0-209-g4235789-dirty".
    pub version: String,
    /// The base that actually works, e.g. "http://192.168.1.60".
    pub base_url: String,
}

/// Finished or ended early (cancelled, error, crash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Completed,
    Partial,
}

impl JobOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            JobOutcome::Completed => "completed",
            JobOutcome::Partial => "partial",
        }
    }
}

/// A finished print as a printer reports it (vendor-neutral).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteJob {
    pub remote_id: String,
    pub file_name: String,
    pub outcome: JobOutcome,
    pub raw_status: String,
    pub ended_at: f64,
    pub print_duration_s: f64,
    pub used_mm: f64,
    pub slicer_total_mm: Option<f64>,
    pub slicer_weight_g: Option<f64>,
    pub material: Option<String>,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Unreachable,
    AuthRequired,
    BadResponse(String),
    HistoryMissing,
    AddressNotAllowed,
}

impl LinkError {
    /// Key for the database and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            LinkError::Unreachable => "unreachable",
            LinkError::AuthRequired => "auth_required",
            LinkError::BadResponse(_) => "bad_response",
            LinkError::HistoryMissing => "history_missing",
            LinkError::AddressNotAllowed => "address_not_allowed",
        }
    }
}

pub const KIND_MOONRAKER: &str = "moonraker";

/// Common interface of all printer systems. Blocking; callers run in the background thread or in `spawn_blocking`.
pub trait PrinterLink: Send {
    /// Checks address and connection, returns the working base URL.
    fn test(&self) -> Result<ConnectionInfo, LinkError>;
    /// Finished prints with `ended_at > since` (Unix seconds).
    fn jobs_ended_since(&self, base_url: &str, since: f64) -> Result<Vec<RemoteJob>, LinkError>;
    /// Thumbnail (PNG) for a path from `RemoteJob::thumbnail_path`.
    fn thumbnail(&self, base_url: &str, path: &str) -> Result<Vec<u8>, LinkError>;
}

pub fn make_link(kind: &str, address: &str, policy: address::AddressPolicy) -> Result<Box<dyn PrinterLink>, LinkError> {
    match kind {
        KIND_MOONRAKER => Ok(Box::new(moonraker::MoonrakerLink::new(address, policy))),
        other => Err(LinkError::BadResponse(format!("unbekannter Druckertyp {other}"))),
    }
}
