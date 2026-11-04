//! Binary G-code (`.bgcode`, magic `GCDE`): walks the blocks in front of the first G-code
//! block, reads the metadata blocks (INI text) and the PNG preview blocks, and skips everything
//! else by seeking. The G-code blocks (Heatshrink/MeatPack) are never read or unpacked.
//!
//! Layout observed in files written by PrusaSlicer:
//!
//! ```text
//! file header : "GCDE", version u32 (1), checksum type u16 (0 none, 1 CRC32)      -- little endian
//! block       : type u16, compression u16, uncompressed size u32,
//!               [compressed size u32 when compression != 0],
//!               parameters (metadata: encoding u16; preview: format u16, width u16, height u16),
//!               data, [CRC32 u32 over header + parameters + data when the file has checksums]
//! types       : 0 file metadata, 1 G-code, 2 slicer metadata, 3 printer metadata,
//!               4 print metadata, 5 preview
//! compression : 0 none, 1 Deflate, 2/3 Heatshrink
//! ```
//!
//! Every size is checked against the bytes the file still has and against fixed limits before it
//! is used for an allocation or a read.

use std::io::{Read, Seek, SeekFrom};

use flate2::read::ZlibDecoder;
use flate2::Crc;

use super::{GcodeError, Raw, MAX_BGCODE_BLOCKS, MAX_METADATA_BYTES, MAX_THUMBNAIL_BYTES};

pub(super) const MAGIC: &[u8; 4] = b"GCDE";
const SUPPORTED_VERSION: u32 = 1;
const FILE_HEADER_BYTES: u64 = 10;

const TYPE_FILE: u16 = 0;
const TYPE_GCODE: u16 = 1;
const TYPE_SLICER: u16 = 2;
const TYPE_PRINTER: u16 = 3;
const TYPE_PRINT: u16 = 4;
const TYPE_THUMBNAIL: u16 = 5;

const COMPRESSION_NONE: u16 = 0;
const COMPRESSION_DEFLATE: u16 = 1;
const ENCODING_INI: u16 = 0;
const THUMBNAIL_PNG: u16 = 0;

/// All previews together.
const MAX_THUMBNAIL_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// All unpacked metadata together.
const MAX_METADATA_TOTAL_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn read<R: Read + Seek>(source: &mut R, len: u64) -> Result<Raw, GcodeError> {
    if len < FILE_HEADER_BYTES {
        return Err(if len >= 4 { GcodeError::BgcodeTruncated } else { GcodeError::NotBgcode });
    }
    source.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; FILE_HEADER_BYTES as usize];
    source.read_exact(&mut header)?;
    if &header[..4] != MAGIC {
        return Err(GcodeError::NotBgcode);
    }
    let version = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes"));
    if version != SUPPORTED_VERSION {
        return Err(GcodeError::BgcodeVersion(version));
    }
    let checksum_bytes = match u16::from_le_bytes(header[8..10].try_into().expect("2 bytes")) {
        0 => 0u64,
        1 => 4,
        _ => return Err(GcodeError::BgcodeCorrupt),
    };

    // Later blocks win for the same key in this order (print metadata is the most specific).
    let mut slicer = Vec::new();
    let mut printer = Vec::new();
    let mut print = Vec::new();
    let mut file_meta = Vec::new();
    let mut raw = Raw::default();
    let mut metadata_total = 0u64;
    let mut thumbnail_total = 0usize;

    let mut pos = FILE_HEADER_BYTES;
    let mut finished = false;
    for _ in 0..MAX_BGCODE_BLOCKS {
        if pos == len {
            finished = true;
            break;
        }
        let remaining = len - pos;
        if remaining < 8 {
            return Err(GcodeError::BgcodeTruncated);
        }
        let mut head = [0u8; 12];
        let head_len = if remaining >= 12 { 12 } else { 8 };
        source.seek(SeekFrom::Start(pos))?;
        source.read_exact(&mut head[..head_len])?;
        let block_type = u16::from_le_bytes([head[0], head[1]]);
        let compression = u16::from_le_bytes([head[2], head[3]]);
        let uncompressed = u64::from(u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
        let header_bytes = if compression == COMPRESSION_NONE { 8u64 } else { 12 };
        if remaining < header_bytes {
            return Err(GcodeError::BgcodeTruncated);
        }
        let data_bytes = if compression == COMPRESSION_NONE {
            uncompressed
        } else {
            u64::from(u32::from_le_bytes([head[8], head[9], head[10], head[11]]))
        };
        let parameter_bytes = match block_type {
            TYPE_FILE | TYPE_GCODE | TYPE_SLICER | TYPE_PRINTER | TYPE_PRINT => 2u64,
            TYPE_THUMBNAIL => 6,
            _ => return Err(GcodeError::BgcodeCorrupt),
        };
        if compression > 3 {
            return Err(GcodeError::BgcodeCorrupt);
        }
        let block_bytes = header_bytes
            .checked_add(parameter_bytes)
            .and_then(|n| n.checked_add(data_bytes))
            .and_then(|n| n.checked_add(checksum_bytes))
            .ok_or(GcodeError::BgcodeCorrupt)?;
        if block_bytes > remaining {
            return Err(GcodeError::BgcodeTruncated);
        }

        if block_type == TYPE_GCODE {
            // Everything the slicer says about the print stands in front of the G-code.
            finished = true;
            break;
        }

        if block_type == TYPE_THUMBNAIL {
            let readable = data_bytes as usize <= MAX_THUMBNAIL_BYTES
                && compression == COMPRESSION_NONE
                && thumbnail_total + data_bytes as usize <= MAX_THUMBNAIL_TOTAL_BYTES;
            if !readable {
                // A preview that is too large, packed or over budget is skipped, nothing else is lost.
                pos += block_bytes;
                continue;
            }
        } else if data_bytes > MAX_METADATA_BYTES || uncompressed > MAX_METADATA_BYTES {
            return Err(GcodeError::BgcodeTooLarge);
        }

        // The block is at most a few MiB here (limits above), so it is read as a whole.
        let mut block = vec![0u8; block_bytes as usize];
        source.seek(SeekFrom::Start(pos))?;
        source.read_exact(&mut block)?;

        let checksum_ok = if checksum_bytes == 4 {
            let body = block.len() - 4;
            let stored = u32::from_le_bytes(block[body..].try_into().expect("4 bytes"));
            let mut crc = Crc::new();
            crc.update(&block[..body]);
            crc.sum() == stored
        } else {
            true
        };
        let data_start = (header_bytes + parameter_bytes) as usize;
        let data = &block[data_start..data_start + data_bytes as usize];

        if block_type == TYPE_THUMBNAIL {
            // A preview with a wrong checksum is dropped; the numbers are not affected.
            if checksum_ok {
                let format = u16::from_le_bytes([block[8], block[9]]);
                let width = u16::from_le_bytes([block[10], block[11]]);
                let height = u16::from_le_bytes([block[12], block[13]]);
                if format == THUMBNAIL_PNG {
                    thumbnail_total += data.len();
                    raw.add_thumbnail(u32::from(width), u32::from(height), data.to_vec());
                }
            }
            pos += block_bytes;
            continue;
        }

        // Metadata: wrong numbers are worse than none, so a bad checksum makes the file unreadable.
        if !checksum_ok {
            return Err(GcodeError::BgcodeChecksum);
        }
        let encoding = u16::from_le_bytes([block[header_bytes as usize], block[header_bytes as usize + 1]]);
        if encoding != ENCODING_INI {
            return Err(GcodeError::BgcodeCorrupt);
        }
        let text = match compression {
            COMPRESSION_NONE => data.to_vec(),
            COMPRESSION_DEFLATE => inflate_bounded(data, uncompressed)?,
            _ => return Err(GcodeError::BgcodeCompression),
        };
        metadata_total += text.len() as u64;
        if metadata_total > MAX_METADATA_TOTAL_BYTES {
            return Err(GcodeError::BgcodeTooLarge);
        }
        match block_type {
            TYPE_SLICER => {
                raw.config = true;
                slicer.push(text);
            }
            TYPE_PRINTER => printer.push(text),
            TYPE_PRINT => print.push(text),
            _ => file_meta.push(text),
        }
        raw.recognized = true;
        pos += block_bytes;
    }

    if !finished {
        return Err(GcodeError::BgcodeTooLarge);
    }
    for text in slicer.iter().chain(&printer).chain(&print) {
        read_ini(&mut raw, text);
    }
    for text in &file_meta {
        read_ini(&mut raw, text);
    }
    // `Producer=PrusaSlicer 2.8.1+linux-x64-GTK3`
    if let Some(producer) = raw.kv.get("Producer").cloned() {
        let mut words = producer.split_whitespace();
        if let Some(name) = words.next() {
            raw.set_slicer(name, words.next());
        }
    }
    Ok(raw)
}

/// Unpacks at most `expected` bytes; the stored size has to be exact, so a block cannot claim a
/// small size and unpack to a large one (or the other way round).
fn inflate_bounded(data: &[u8], expected: u64) -> Result<Vec<u8>, GcodeError> {
    let mut out = Vec::with_capacity(expected.min(MAX_METADATA_BYTES) as usize);
    let mut decoder = ZlibDecoder::new(data).take(expected.min(MAX_METADATA_BYTES) + 1);
    decoder.read_to_end(&mut out).map_err(|_| GcodeError::BgcodeCorrupt)?;
    if out.len() as u64 > expected {
        return Err(GcodeError::BgcodeTooLarge);
    }
    if out.len() as u64 != expected {
        return Err(GcodeError::BgcodeCorrupt);
    }
    Ok(out)
}

/// `key=value` per line, no spaces around the `=`.
fn read_ini(raw: &mut Raw, text: &[u8]) {
    for line in text.split(|&b| b == b'\n') {
        if line.len() > super::MAX_LINE_BYTES {
            continue;
        }
        let line = String::from_utf8_lossy(line);
        if let Some((key, value)) = line.trim_end_matches('\r').split_once('=') {
            raw.put(key, value);
        }
    }
}
