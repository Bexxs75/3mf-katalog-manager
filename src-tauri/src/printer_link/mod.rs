//! Druckeranbindung: liest den Filamentverbrauch beendeter Drucke vom Drucker
//! (zuerst Klipper/Moonraker) und bucht ihn nach Bestätigung von einer Spule
//! ab. Nur Heimnetz, nur lesende Anfragen, standardmäßig ausgeschaltet.

/// Ergebnis von "Verbindung testen".
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionInfo {
    /// Softwareversion am Drucker, z. B. "v0.8.0-209-g4235789-dirty".
    pub version: String,
    /// Tatsächlich funktionierende Basis, z. B. "http://192.168.1.60".
    pub base_url: String,
}

/// Fertig oder vorzeitig beendet (abgebrochen, Fehler, Absturz).
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

/// Ein beendeter Druck, wie ihn ein Drucker meldet (herstellerneutral).
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
    /// Kennung für Datenbank und Oberfläche.
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
