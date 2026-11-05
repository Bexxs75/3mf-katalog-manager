//! Reports of sliced files from the slicer ("post-processing script" of PrusaSlicer, SuperSlicer,
//! OrcaSlicer, Bambu Studio, Creality Print).
//!
//! The slicer starts the program as `program --report-sliced <file>`. That call ([`report_sliced`]) reads the
//! file like the catalog reader does (bounded, nothing executed), writes one small ticket into the inbox
//! folder of the app data and ends. It opens no window and **no database**: it may run while the main
//! program is open, closed or busy. The main program takes the tickets over ([`ingest_inbox`]) at its start,
//! when it is woken by a second start, and whenever the interface asks for the list.
//!
//! Every ticket is untrusted input: any process of the user can drop a file into the inbox. Tickets are
//! size-limited, validated and re-serialized; the paths in them are shown but never opened before the user
//! decides, and nothing is imported, moved or assigned automatically.

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::error::DbError;
use crate::db::slicer_reports::{self, NewReport};
use crate::gcode;
use crate::printer_link::matching::{best_match, Candidate, MatchKind, ModelMatch};
use crate::threemf::print_info::Reduced;
use crate::threemf::slice_info::SliceState;

/// Flag that turns a start of the program into a report call.
pub const REPORT_FLAG: &str = "--report-sliced";
/// Environment variable the slicers set for post-processing scripts: the final name of the output file
/// (the argument is a temporary file that is renamed afterwards).
pub const OUTPUT_NAME_ENV: &str = "SLIC3R_PP_OUTPUT_NAME";
/// Folder in the app data directory that holds the tickets.
pub const INBOX_DIR: &str = "slicer-inbox";
/// Written by a failed report call; read by the settings page. Not a ticket.
pub const LAST_ERROR_FILE: &str = "last-error.txt";
pub const TICKET_VERSION: u32 = 1;
pub const MAX_TICKET_BYTES: u64 = 64 * 1024;
/// More tickets than this are not accepted by the call, and the intake drops the oldest above it.
pub const MAX_TICKETS: usize = 200;
pub const MAX_AGE_DAYS: i64 = 30;
const MAX_PATH_CHARS: usize = 1024;
const MAX_NAME_CHARS: usize = 255;
const MAX_TEXT_CHARS: usize = 200;
const MAX_FILAMENTS: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub enum ReportError {
    /// `--report-sliced` without a path.
    PathMissing,
    FileMissing,
    /// The file is no G-code the reader recognizes.
    NotGcode,
    /// A binary G-code file that is damaged or not supported (registered code in the text).
    Unreadable(&'static str),
    InboxFull,
    Io(String),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::PathMissing => write!(f, "no file path was given after {REPORT_FLAG}"),
            ReportError::FileMissing => write!(f, "the file does not exist or is not a regular file"),
            ReportError::NotGcode => write!(f, "the file is not G-code from a known slicer"),
            ReportError::Unreadable(code) => write!(f, "the file could not be read ({code})"),
            ReportError::InboxFull => write!(f, "the inbox is full; open the program to take the reports over"),
            ReportError::Io(e) => write!(f, "could not write the report: {e}"),
        }
    }
}

impl std::error::Error for ReportError {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FilamentSummary {
    pub filament_type: Option<String>,
    pub color: Option<String>,
    pub weight_g: Option<f64>,
    pub length_mm: Option<f64>,
}

/// What the slicer wrote about the print (only values the reader knows; nothing is computed).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Summary {
    pub print_seconds: Option<u64>,
    pub weight_g: Option<f64>,
    pub layer_height_mm: Option<f64>,
    pub nozzle_mm: Option<f64>,
    pub printer_model: Option<String>,
    pub filaments: Vec<FilamentSummary>,
    /// A test report of the settings page.
    pub test: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub version: u32,
    pub reported_at: String,
    /// Final path according to the slicer (environment variable), else the path of the call.
    pub reported_path: String,
    pub file_size_bytes: u64,
    pub slicer: Option<String>,
    #[serde(default)]
    pub summary: Summary,
}

/// The report request in a command line: `None` = no report call; `Some(Err)` = the flag without a path.
/// The path is the first argument after the flag that does not start with `-` (the slicer appends it), or
/// the value of `--report-sliced=<path>`.
pub fn report_request(args: &[String]) -> Option<Result<PathBuf, ReportError>> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--report-sliced=") {
            return Some(if value.is_empty() { Err(ReportError::PathMissing) } else { Ok(PathBuf::from(value)) });
        }
        if arg == REPORT_FLAG {
            let mut after_dashes = false;
            for next in iter.by_ref() {
                if next == "--" && !after_dashes {
                    after_dashes = true;
                    continue;
                }
                if !next.is_empty() && (after_dashes || !next.starts_with('-')) {
                    return Some(Ok(PathBuf::from(next)));
                }
            }
            return Some(Err(ReportError::PathMissing));
        }
    }
    None
}

fn clean_text(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max_chars || trimmed.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(trimmed.to_string())
}

fn usable_path(value: &str) -> Option<String> {
    let path = clean_text(value, MAX_PATH_CHARS)?;
    let absolute = Path::new(&path).is_absolute() || path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') || path.starts_with("\\\\");
    absolute.then_some(path)
}

/// Builds the ticket for `file`. `output_name` is the value of [`OUTPUT_NAME_ENV`]; a value that is not a
/// usable absolute path is ignored.
pub fn build_ticket(file: &Path, output_name: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> Result<Ticket, ReportError> {
    let mut handle = crate::safe_file::open_regular(file).map_err(|_| ReportError::FileMissing)?;
    let size = handle.metadata().map_err(|_| ReportError::FileMissing)?.len();
    let read = match gcode::read_gcode(&mut handle) {
        Ok(read) => read,
        Err(error) if error.is_damaged_file() => return Err(ReportError::Unreadable(error.code().as_str())),
        Err(_) => return Err(ReportError::FileMissing),
    };
    if read.print_info.slice_state == SliceState::NotRecognized {
        return Err(ReportError::NotGcode);
    }
    let info = &read.print_info;
    let weight_g = read.slice_info.as_ref().filter(|s| !s.weight_unknown()).map(|s| s.total_weight_g).filter(|w| *w > 0.0);
    let summary = Summary {
        print_seconds: info.plates.first().and_then(|p| p.print_seconds),
        weight_g,
        layer_height_mm: info.layer_height_mm,
        nozzle_mm: match info.nozzle_mm {
            Reduced::Value(v) => Some(v),
            _ => None,
        },
        printer_model: info.printer_model.as_deref().and_then(|m| clean_text(m, MAX_TEXT_CHARS)),
        filaments: read
            .filaments
            .iter()
            .take(MAX_FILAMENTS)
            .map(|f| FilamentSummary {
                filament_type: f.filament_type.clone(),
                color: f.color.clone(),
                weight_g: f.weight_g,
                length_mm: f.length_mm,
            })
            .collect(),
        test: false,
    };
    let absolute = |p: &Path| if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) };
    let reported_path = output_name
        .and_then(usable_path)
        .unwrap_or_else(|| absolute(file).to_string_lossy().into_owned());
    Ok(Ticket {
        version: TICKET_VERSION,
        reported_at: now.to_rfc3339(),
        reported_path,
        file_size_bytes: size,
        slicer: read.slicer.as_deref().and_then(|s| clean_text(s, MAX_TEXT_CHARS)),
        summary,
    })
}

fn random_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    hasher.write_u32(std::process::id());
    format!("{:016x}", hasher.finish())
}

fn is_ticket_name(name: &str) -> bool {
    name.ends_with(".json") && !name.starts_with('.')
}

/// Regular `*.json` files of the inbox (symlinks and everything else are not counted).
fn ticket_entries(inbox: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    let Ok(dir) = std::fs::read_dir(inbox) else { return Vec::new() };
    dir.flatten()
        .filter(|e| e.file_name().to_str().is_some_and(is_ticket_name))
        .filter_map(|e| {
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            meta.file_type().is_file().then_some((e.path(), meta))
        })
        .collect()
}

/// Writes the ticket atomically (temporary file, then rename; no reader sees half a file). Returns the path.
pub fn write_ticket(inbox: &Path, ticket: &Ticket) -> Result<PathBuf, ReportError> {
    let io = |e: std::io::Error| ReportError::Io(e.to_string());
    let json = serde_json::to_vec(ticket).map_err(|e| ReportError::Io(e.to_string()))?;
    if json.len() as u64 > MAX_TICKET_BYTES {
        return Err(ReportError::Io("the report is too large".into()));
    }
    std::fs::create_dir_all(inbox).map_err(io)?;
    crate::safe_file::harden_permissions(inbox);
    if ticket_entries(inbox).len() >= MAX_TICKETS {
        return Err(ReportError::InboxFull);
    }
    let name = random_name();
    let temp = inbox.join(format!(".tmp-{name}"));
    let target = inbox.join(format!("{name}.json"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options.open(&temp).and_then(|mut f| f.write_all(&json).and_then(|_| f.sync_all())).and_then(|_| std::fs::rename(&temp, &target));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(io(e));
    }
    Ok(target)
}

/// The report call: reads the file and writes the ticket. Touches nothing but the inbox folder. A request
/// without a path (`Err`) is recorded as a failure like any other.
pub fn report_sliced(request: Result<PathBuf, ReportError>, output_name: Option<&str>, inbox: &Path) -> Result<PathBuf, ReportError> {
    let result = request.and_then(|file| build_ticket(&file, output_name, chrono::Utc::now())).and_then(|ticket| write_ticket(inbox, &ticket));
    match &result {
        Ok(_) => {
            let _ = std::fs::remove_file(inbox.join(LAST_ERROR_FILE));
        }
        Err(error) => {
            // Best effort: nothing can be shown by a program without a console; the settings page reads this.
            if std::fs::create_dir_all(inbox).is_ok() {
                let _ = std::fs::write(inbox.join(LAST_ERROR_FILE), format!("{} {error}", chrono::Utc::now().to_rfc3339()));
            }
        }
    }
    result
}

/// The last failed report call (one short line), if the settings page should mention it.
pub fn last_error(inbox: &Path) -> Option<String> {
    let bytes = crate::safe_file::read_bounded(&inbox.join(LAST_ERROR_FILE), 1024).ok()?;
    clean_text(&String::from_utf8_lossy(&bytes), 400)
}

/// A test report of the settings page: it goes the same way as a real one, so the whole route is tried.
pub fn write_test_ticket(inbox: &Path) -> Result<PathBuf, ReportError> {
    let ticket = Ticket {
        version: TICKET_VERSION,
        reported_at: chrono::Utc::now().to_rfc3339(),
        reported_path: inbox.join("Test report.gcode").to_string_lossy().into_owned(),
        file_size_bytes: 0,
        slicer: None,
        summary: Summary { test: true, ..Summary::default() },
    };
    write_ticket(inbox, &ticket)
}

/// Checks and normalizes a ticket from the inbox; `None` = not acceptable.
fn accept(ticket: Ticket) -> Option<NewReport> {
    if ticket.version != TICKET_VERSION {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(&ticket.reported_at).ok()?;
    let path = usable_path(&ticket.reported_path)?;
    let file_name = clean_text(slicer_reports::last_component(&path), MAX_NAME_CHARS)?;
    let mut summary = ticket.summary;
    summary.filaments.truncate(MAX_FILAMENTS);
    let sane = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0 && *x < 1.0e9);
    summary.weight_g = sane(summary.weight_g);
    summary.layer_height_mm = sane(summary.layer_height_mm);
    summary.nozzle_mm = sane(summary.nozzle_mm);
    summary.print_seconds = summary.print_seconds.filter(|s| *s > 0 && *s < 10_u64.pow(9));
    summary.printer_model = summary.printer_model.as_deref().and_then(|m| clean_text(m, MAX_TEXT_CHARS));
    for f in &mut summary.filaments {
        f.weight_g = sane(f.weight_g);
        f.length_mm = sane(f.length_mm);
        f.filament_type = f.filament_type.as_deref().and_then(|t| clean_text(t, 40));
        f.color = f.color.as_deref().and_then(|c| (c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit())).then(|| c.to_ascii_lowercase()));
    }
    let summary_json = serde_json::to_string(&summary).ok().filter(|j| j.len() <= slicer_reports::MAX_SUMMARY_BYTES)?;
    Some(NewReport {
        reported_at: ticket.reported_at,
        reported_path: path,
        file_name,
        file_size_bytes: i64::try_from(ticket.file_size_bytes).ok()?,
        slicer: ticket.slicer.as_deref().and_then(|s| clean_text(s, MAX_TEXT_CHARS)),
        summary_json,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IngestOutcome {
    pub added: usize,
    pub merged: usize,
    /// Tickets deleted without a report: unreadable, too large, too old, over the limit, or intake off.
    pub discarded: usize,
}

/// Takes the tickets of the inbox into `slicer_reports` and deletes them. With the intake switched off the
/// tickets are deleted without being stored. Never opens a path named in a ticket.
pub fn ingest_inbox(conn: &Connection, inbox: &Path) -> Result<IngestOutcome, DbError> {
    let mut outcome = IngestOutcome::default();
    let mut entries = ticket_entries(inbox);
    if entries.is_empty() {
        return Ok(outcome);
    }
    let enabled = slicer_reports::is_enabled(conn)?;
    // Newest first, so the limit drops the oldest.
    entries.sort_by_key(|(_, meta)| std::cmp::Reverse(meta.modified().ok()));
    let oldest = std::time::SystemTime::now() - std::time::Duration::from_secs(MAX_AGE_DAYS as u64 * 86_400);
    for (index, (path, meta)) in entries.into_iter().enumerate() {
        let too_old = meta.modified().is_ok_and(|m| m < oldest);
        let ticket = if !enabled || index >= MAX_TICKETS || too_old || meta.len() > MAX_TICKET_BYTES {
            None
        } else {
            crate::safe_file::read_bounded(&path, MAX_TICKET_BYTES).ok().and_then(|bytes| serde_json::from_slice::<Ticket>(&bytes).ok()).and_then(accept)
        };
        match ticket {
            Some(report) => {
                let (id, merged) = slicer_reports::insert_or_merge(conn, &report)?;
                if merged {
                    outcome.merged += 1;
                } else {
                    outcome.added += 1;
                }
                set_suggestion_for(conn, id, &report.file_name)?;
            }
            None => outcome.discarded += 1,
        }
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!(target: "slicer_report", "could not delete a handled report: {e}");
        }
    }
    Ok(outcome)
}

/// Catalog entries a sliced file can be matched with: models only (a G-code or resin entry of the catalog
/// would win by name and take the print away from the model).
pub fn model_candidates(conn: &Connection) -> Result<Vec<Candidate>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name, imported_at FROM files WHERE deleted_at IS NULL AND file_type NOT IN ('gcode', 'resin')")?;
    let rows = stmt.query_map([], |r| Ok(Candidate { file_id: r.get(0)?, name: r.get(1)?, imported_at: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn set_suggestion_for(conn: &Connection, id: i64, file_name: &str) -> Result<(), DbError> {
    let suggestion = best_match(file_name, &model_candidates(conn)?);
    slicer_reports::set_suggestion(conn, id, suggestion.map(|m| m.file_id))
}

/// Suggestion for an open report, from the catalog as it is now.
pub fn suggestion(candidates: &[Candidate], file_name: &str) -> Option<ModelMatch> {
    best_match(file_name, candidates)
}

pub fn is_sure(m: &ModelMatch) -> bool {
    m.kind == MatchKind::Sure
}

#[cfg(test)]
#[path = "slicer_report_tests.rs"]
mod tests;
