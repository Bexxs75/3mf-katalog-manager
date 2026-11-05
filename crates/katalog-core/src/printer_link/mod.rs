//! Printer connection: reads the filament used by finished prints from the
//! printer (Klipper/Moonraker first) and deducts it from a spool after
//! confirmation. Home network only, read-only requests, off by default.

pub mod address;
pub mod api_key;
pub mod booking;
pub mod matching;
pub mod moonraker;
pub mod status;
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
    /// Printer clock minus local clock in seconds; 0 when the difference is within
    /// the tolerance or unknown. Printers without NTP can be years off, and their
    /// timestamps have to be shifted before they are compared with local times.
    pub clock_offset_s: f64,
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
    /// Network failure that none of the more specific classes names (no route, unknown cause).
    Unreachable,
    /// The printer did not answer within the time limit.
    Timeout,
    /// Nothing listens on the address: the printer is off or Moonraker is not running.
    ConnectionRefused,
    /// The connection broke off without an answer.
    ConnectionReset,
    /// 502, 503 or 504: a web server in front of Moonraker is up while Moonraker restarts.
    ServerBusy,
    NameResolutionFailed,
    ContainerLocalName,
    ContainerLoopback,
    AuthRequired,
    /// The printer answered 401/403 although a key was sent.
    AuthRejected,
    BadResponse(String),
    HistoryMissing,
    AddressNotAllowed,
}

impl LinkError {
    /// Key for the database and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            LinkError::Unreachable => "unreachable",
            LinkError::Timeout => "timeout",
            LinkError::ConnectionRefused => "connection_refused",
            LinkError::ConnectionReset => "connection_reset",
            LinkError::ServerBusy => "server_busy",
            LinkError::NameResolutionFailed => "name_resolution_failed",
            LinkError::ContainerLocalName => "container_local_name",
            LinkError::ContainerLoopback => "container_loopback",
            LinkError::AuthRequired => "auth_required",
            LinkError::AuthRejected => "auth_rejected",
            LinkError::BadResponse(_) => "bad_response",
            LinkError::HistoryMissing => "history_missing",
            LinkError::AddressNotAllowed => "address_not_allowed",
        }
    }
}

impl LinkError {
    /// A failure that usually goes away by itself (printer switched off, restarting, Wi-Fi
    /// hiccup): worth another try and, for a while, no reason for a red error.
    pub fn is_transient(&self) -> bool {
        status::is_transient_code(self.code())
    }
}

pub const KIND_MOONRAKER: &str = "moonraker";

/// Common interface of all printer systems. Blocking; callers run in the background thread or in `spawn_blocking`.
pub trait PrinterLink: Send {
    /// Checks address and connection, returns the working base URL.
    fn test(&self) -> Result<ConnectionInfo, LinkError>;
    /// Finished prints with `ended_at > since`, both in local Unix seconds: the
    /// adapter converts with `info.clock_offset_s` in both directions.
    fn jobs_ended_since(&self, info: &ConnectionInfo, since: f64) -> Result<Vec<RemoteJob>, LinkError>;
    /// Thumbnail (PNG) for a path from `RemoteJob::thumbnail_path`.
    fn thumbnail(&self, base_url: &str, path: &str) -> Result<Vec<u8>, LinkError>;
    /// Hard end of everything this link still does (all printers together get one budget per pass).
    /// Adapters without time limits ignore it.
    fn set_deadline(&mut self, _deadline: std::time::Instant) {}
}

pub fn make_link(
    kind: &str,
    address: &str,
    api_key: Option<&api_key::ApiKey>,
    policy: address::AddressPolicy,
) -> Result<Box<dyn PrinterLink>, LinkError> {
    match kind {
        KIND_MOONRAKER => Ok(Box::new(moonraker::MoonrakerLink::try_new(address, api_key.cloned(), policy)?)),
        other => Err(LinkError::BadResponse(format!("unbekannter Druckertyp {other}"))),
    }
}
