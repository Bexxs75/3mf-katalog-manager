//! Print data of sliced G-code files (`.gcode` text and `.bgcode` binary): estimated print
//! time, filament use per filament, layer height, nozzles, filament types and colors,
//! target printer, slicer name and a preview image.
//!
//! Hard rules, as for the 3MF reader:
//! - Only what the slicer wrote is used. Nothing is computed from moves or guessed; a value
//!   that is missing, a known placeholder or out of range is "unknown", never 0.
//! - Only the beginning and the end of a text file are read ([`HEAD_BYTES`], [`TAIL_BYTES`]),
//!   never the whole file; lines are capped ([`MAX_LINE_BYTES`]). A binary file is walked block
//!   by block, the G-code blocks themselves are never read.
//! - Fremd files never panic and never fail an import: a broken file leaves the entry without
//!   print data (see [`import_gcode`]), errors are registered [`ErrorCode`]s.
//!
//! Format knowledge comes from observing real files (PrusaSlicer 2.8.1 text and binary,
//! OrcaSlicer 2.4.2, Bambu Studio 02.09.01.70, Creality Print 7.3.1.6378, CuraEngine 5.13.0)
//! and from the public description of the binary format; no third-party code is used.

mod bgcode;
mod interpret;
mod text;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

use crate::db::models::PrintInfoRecord;
use crate::error_code::ErrorCode;
use crate::threemf::print_info::{PrintInfo, SettingsState, SliceState};
use crate::threemf::SliceInfo;

/// Raise whenever the text or the binary reader changes what it stores; rows of an older
/// version count as "not read yet" and are offered for a re-read. Text and binary share the
/// number because both write the same row.
pub const GCODE_READER_VERSION: u32 = 1;

/// Bytes read at the start of a text file (header, thumbnails, Bambu config block).
pub const HEAD_BYTES: u64 = 256 * 1024;
/// Bytes read at the end of a text file (sums and config block of PrusaSlicer, Orca, Creality).
pub const TAIL_BYTES: u64 = 256 * 1024;
/// A longer line is ignored as a whole (no unbounded line buffers).
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Largest preview image that is kept (decoded size).
pub const MAX_THUMBNAIL_BYTES: usize = 512 * 1024;
/// Largest unpacked metadata block of a binary file; a Deflate block that claims or produces
/// more is rejected (zip bomb).
pub const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;
/// Blocks looked at in a binary file before giving up (real files have under ten before the
/// first G-code block).
pub const MAX_BGCODE_BLOCKS: usize = 256;

/// What a read file yielded. `thumbnail_png` is the PNG as written by the slicer (signature and
/// size checked, not decoded).
#[derive(Debug, Clone)]
pub struct GcodeRead {
    /// "PrusaSlicer 2.8.1"; `None` when the file names no slicer.
    pub slicer: Option<String>,
    pub print_info: PrintInfo,
    /// Per-filament use, in the unit the slicer reported (only used filaments).
    pub filaments: Vec<FilamentFacts>,
    /// Only when the slicer reported a weight for every used filament.
    pub slice_info: Option<SliceInfo>,
    pub thumbnail_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilamentFacts {
    /// Zero-based filament (or extruder) slot.
    pub slot: usize,
    pub filament_type: Option<String>,
    /// `#rrggbb`, lowercase.
    pub color: Option<String>,
    pub length_mm: Option<f64>,
    pub weight_g: Option<f64>,
    pub volume_cm3: Option<f64>,
}

/// Why a file could not be read. [`GcodeError::Io`] says nothing about the file itself; the
/// others say the binary file is damaged or not supported.
#[derive(Debug)]
pub enum GcodeError {
    Io(std::io::Error),
    /// Not a binary G-code file (wrong magic).
    NotBgcode,
    /// Binary G-code of a version this reader does not know.
    BgcodeVersion(u32),
    /// Unknown block type or checksum type, impossible sizes, or a block that claims more than it holds.
    BgcodeCorrupt,
    /// The file ends inside a block.
    BgcodeTruncated,
    /// Checksum of a metadata block does not match.
    BgcodeChecksum,
    /// A metadata block is compressed with something other than Deflate (Heatshrink).
    BgcodeCompression,
    /// Too many blocks, or a block (or its unpacked content) over the limit.
    BgcodeTooLarge,
}

impl GcodeError {
    /// The registered code for the interface; `None` for plain I/O trouble.
    pub fn code(&self) -> ErrorCode {
        match self {
            GcodeError::Io(_) => ErrorCode::GcodeFileUnreadable,
            GcodeError::NotBgcode => ErrorCode::BgcodeNotBgcode,
            GcodeError::BgcodeVersion(_) => ErrorCode::BgcodeVersionUnsupported,
            GcodeError::BgcodeCorrupt | GcodeError::BgcodeTruncated => ErrorCode::BgcodeCorrupt,
            GcodeError::BgcodeChecksum => ErrorCode::BgcodeChecksum,
            GcodeError::BgcodeCompression => ErrorCode::BgcodeCompressionUnsupported,
            GcodeError::BgcodeTooLarge => ErrorCode::BgcodeTooLarge,
        }
    }

    /// True when the file itself is at fault (the entry then records "unreadable").
    pub fn is_damaged_file(&self) -> bool {
        !matches!(self, GcodeError::Io(_))
    }
}

impl From<std::io::Error> for GcodeError {
    fn from(e: std::io::Error) -> Self {
        // A file that ends early surfaces as UnexpectedEof from `read_exact`.
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            GcodeError::BgcodeTruncated
        } else {
            GcodeError::Io(e)
        }
    }
}

impl std::fmt::Display for GcodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code().as_str())
    }
}

impl std::error::Error for GcodeError {}

/// Reads text or binary G-code, decided by the first four bytes (`GCDE` = binary), not by the
/// file name.
pub fn read_gcode<R: Read + Seek>(source: &mut R) -> Result<GcodeRead, GcodeError> {
    let len = source.seek(SeekFrom::End(0))?;
    source.seek(SeekFrom::Start(0))?;
    let mut magic = [0u8; 4];
    let got = read_up_to(source, &mut magic)?;
    source.seek(SeekFrom::Start(0))?;
    let raw = if got == 4 && &magic == bgcode::MAGIC { bgcode::read(source, len)? } else { text::read(source, len)? };
    Ok(interpret::interpret(raw))
}

fn read_up_to<R: Read>(source: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = source.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Everything an import or a rescan stores for a G-code file.
#[derive(Debug, Default)]
pub struct GcodeImport {
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<i64>,
    pub slice_info_json: Option<String>,
    pub print_info: Option<PrintInfoRecord>,
    pub metadata: BTreeMap<String, String>,
}

/// Key of the entry metadata that carries the slicer ("PrusaSlicer 2.8.1").
pub const METADATA_APPLICATION: &str = "Application";

/// Reads a file for the catalog. Never fails: a file that cannot be read at all gives an empty
/// result (the entry stays without print data and counts as "not read yet"), a damaged binary
/// file gives a record in the state `unreadable`.
pub fn import_gcode<R: Read + Seek>(source: &mut R) -> GcodeImport {
    let read_at = chrono::Utc::now().to_rfc3339();
    match read_gcode(source) {
        Ok(read) => {
            let sliced = read.print_info.slice_state == SliceState::Sliced;
            let mut record = PrintInfoRecord::from_print_info(&read.print_info, read_at);
            record.reader_version = GCODE_READER_VERSION;
            let mut metadata = BTreeMap::new();
            if let Some(slicer) = read.slicer {
                metadata.insert(METADATA_APPLICATION.to_string(), slicer);
            }
            GcodeImport {
                thumbnail_png: read.thumbnail_png,
                plate_count: sliced.then_some(1),
                slice_info_json: read.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
                print_info: Some(record.validated()),
                metadata,
            }
        }
        Err(error) if error.is_damaged_file() => {
            log::warn!(target: "gcode", "G-code file not readable: {}", error.code().as_str());
            let info = PrintInfo {
                slice_state: SliceState::Unreadable,
                settings_state: SettingsState::NotAttempted,
                printer_model: None,
                bed_type: None,
                layer_height_mm: None,
                nozzle_mm: crate::threemf::print_info::Reduced::Unknown,
                bed_temp_c: crate::threemf::print_info::Reduced::Unknown,
                nozzle_temp_c: crate::threemf::print_info::Reduced::Unknown,
                plates: Vec::new(),
                filament_colors: Vec::new(),
            };
            let mut record = PrintInfoRecord::from_print_info(&info, read_at);
            record.reader_version = GCODE_READER_VERSION;
            GcodeImport { print_info: Some(record.validated()), ..GcodeImport::default() }
        }
        Err(error) => {
            log::warn!(target: "gcode", "G-code file could not be read: {}", error.code().as_str());
            GcodeImport::default()
        }
    }
}

/// [`import_gcode`] for a path.
pub fn import_gcode_file(path: &std::path::Path) -> GcodeImport {
    match crate::safe_file::open_regular(path) {
        Ok(mut file) => import_gcode(&mut file),
        Err(e) => {
            log::warn!(target: "gcode", "G-code file could not be opened: {e}");
            GcodeImport::default()
        }
    }
}

/// What the reader collected before it is interpreted; shared by the text and the binary reader.
#[derive(Debug, Default)]
pub(crate) struct Raw {
    /// `key -> value` of the header, the sums and the settings (text: `; key = value` and
    /// `; key: value`; binary: the INI blocks). Bounded, see [`Raw::put`].
    pub kv: BTreeMap<String, String>,
    /// "Name version" as the file says it.
    pub slicer: Option<String>,
    /// The file is slicer output (a slicer line, a settings block, or any binary metadata).
    pub recognized: bool,
    /// A settings block was found.
    pub config: bool,
    pub thumbs: Vec<Thumb>,
}

#[derive(Debug)]
pub(crate) struct Thumb {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

const MAX_KEYS: usize = 4096;
const MAX_KEY_CHARS: usize = 128;
const MAX_VALUE_CHARS: usize = 2048;

impl Raw {
    /// Stores one pair. Bounded in count and length so a prepared file cannot grow the map; a
    /// later value for the same key replaces the earlier one.
    pub fn put(&mut self, key: &str, value: &str) {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || key.chars().count() > MAX_KEY_CHARS || value.chars().count() > MAX_VALUE_CHARS {
            return;
        }
        if self.kv.len() >= MAX_KEYS && !self.kv.contains_key(key) {
            return;
        }
        self.kv.insert(key.to_string(), value.to_string());
    }

    /// Keeps the largest PNG; `png` is checked here (signature and size).
    pub fn add_thumbnail(&mut self, width: u32, height: u32, png: Vec<u8>) {
        const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
        if png.len() > MAX_THUMBNAIL_BYTES || !png.starts_with(PNG_SIGNATURE) {
            return;
        }
        self.thumbs.push(Thumb { width, height, png });
    }

    /// The slicer from a "Name version" text.
    pub fn set_slicer(&mut self, name: &str, version: Option<&str>) {
        if self.slicer.is_some() {
            return;
        }
        let name = match name {
            "BambuStudio" => "Bambu Studio",
            "Cura_SteamEngine" => "Cura",
            other => other,
        }
        .replace('_', " ");
        let version = version
            .map(|v| v.split('+').next().unwrap_or(v))
            .map(|v| if v.len() > 1 && v.starts_with('V') && v[1..].starts_with(|c: char| c.is_ascii_digit()) { &v[1..] } else { v })
            .filter(|v| !v.is_empty());
        let label = match version {
            Some(v) => format!("{name} {v}"),
            None => name,
        };
        self.slicer = crate::threemf::source::sanitize_source_value(&label);
        self.recognized = true;
    }
}
