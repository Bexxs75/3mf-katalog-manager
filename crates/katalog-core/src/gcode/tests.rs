use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use flate2::write::ZlibEncoder;
use flate2::{Compression, Crc};

use super::*;
use crate::threemf::print_info::Reduced;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gcode").join(name)
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture(name)).expect("text fixture")
}

fn read_bytes(bytes: &[u8]) -> Result<GcodeRead, GcodeError> {
    read_gcode(&mut Cursor::new(bytes.to_vec()))
}

fn read_text_fixture(name: &str) -> GcodeRead {
    read_bytes(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn plate(read: &GcodeRead) -> &crate::threemf::print_info::PlatePrintInfo {
    read.print_info.plates.first().expect("plate row")
}

/// Width from the IHDR chunk of a PNG.
fn png_width(png: &[u8]) -> u32 {
    u32::from_be_bytes(png[16..20].try_into().unwrap())
}

// ---------------------------------------------------------------- text dialects (spec test 2)

#[test]
fn prusa_style_file_gives_time_use_settings_and_the_larger_png() {
    let read = read_text_fixture("prusa_one_filament.gcode");
    assert_eq!(read.slicer.as_deref(), Some("PrusaSlicer 9.9.9"));
    assert_eq!(read.print_info.slice_state, SliceState::Sliced);
    assert_eq!(read.print_info.settings_state, SettingsState::Read);
    assert_eq!(plate(&read).print_seconds, Some(4177), "1h 9m 37s");
    assert_eq!(plate(&read).first_layer_seconds, Some(95.0));
    assert_eq!(read.print_info.layer_height_mm, Some(0.2));
    assert_eq!(read.print_info.nozzle_mm, Reduced::Value(0.4));
    assert_eq!(read.print_info.bed_temp_c, Reduced::Value(60.0));
    assert_eq!(read.print_info.nozzle_temp_c, Reduced::Value(215.0));
    assert_eq!(read.print_info.printer_model.as_deref(), Some("MK4S"));
    assert_eq!(read.print_info.filament_colors, vec!["#e4002b"]);
    assert_eq!(read.filaments.len(), 1);
    let f = &read.filaments[0];
    assert_eq!((f.length_mm, f.weight_g, f.volume_cm3), (Some(1372.73), Some(4.09), Some(3.30)));
    assert_eq!(f.filament_type.as_deref(), Some("PLA"));
    let info = read.slice_info.expect("weights reported");
    assert!((info.total_weight_g - 4.09).abs() < 1e-9);
    assert!((info.plates[0].filaments[0].used_m - 1.37273).abs() < 1e-9);
    let png = read.thumbnail_png.expect("preview");
    assert_eq!(png_width(&png), 2, "the 220x124 block, not the 16x16 QOI block (which is not read)");
}

#[test]
fn prusa_two_filaments_use_semicolon_and_comma_lists() {
    let read = read_text_fixture("prusa_two_filaments.gcode");
    assert_eq!(read.filaments.len(), 2);
    assert_eq!(read.filaments[0].filament_type.as_deref(), Some("PLA"));
    assert_eq!(read.filaments[1].filament_type.as_deref(), Some("PETG"));
    assert_eq!(read.filaments[1].weight_g, Some(4.19));
    assert_eq!(read.print_info.filament_colors, vec!["#e4002b", "#00aa00"], "quotes around a color are removed");
    assert_eq!(read.print_info.bed_temp_c, Reduced::Mixed);
    assert_eq!(read.print_info.nozzle_temp_c, Reduced::Mixed);
    assert_eq!(read.print_info.nozzle_mm, Reduced::Value(0.4));
    let info = read.slice_info.unwrap();
    assert!((info.total_weight_g - 8.28).abs() < 1e-9);
    assert_eq!(info.plates[0].filaments.len(), 2);
    assert_eq!(info.plates[0].filaments[1].color.as_deref(), Some("#00aa00"));
}

#[test]
fn orca_style_file_reads_sums_after_the_executable_block_and_the_bed_of_the_chosen_plate() {
    let read = read_text_fixture("orca_style.gcode");
    assert_eq!(read.slicer.as_deref(), Some("OrcaSlicer 9.9.9"));
    assert_eq!(plate(&read).print_seconds, Some(919));
    assert_eq!(plate(&read).first_layer_seconds, Some(10.0));
    assert_eq!(read.print_info.bed_type.as_deref(), Some("High Temp Plate"));
    assert_eq!(read.print_info.bed_temp_c, Reduced::Value(55.0), "hot_plate_temp, not textured_plate_temp");
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Example Printer X"));
    assert_eq!(read.filaments[0].weight_g, Some(3.61));
    assert_eq!(png_width(&read.thumbnail_png.unwrap()), 3, "the 300x300 block");
}

#[test]
fn orca_header_without_sums_is_sliced_without_time_or_use() {
    let read = read_text_fixture("orca_header_only.gcode");
    assert_eq!(read.print_info.slice_state, SliceState::Sliced);
    assert_eq!(read.print_info.settings_state, SettingsState::Missing);
    assert_eq!(plate(&read).print_seconds, None);
    assert!(read.filaments.is_empty());
    assert!(read.slice_info.is_none());
    assert!(read.thumbnail_png.is_some());
}

#[test]
fn bambu_style_header_holds_the_sums_in_one_line_and_mislabelled_volumes() {
    let read = read_text_fixture("bambu_style.gcode");
    assert_eq!(read.slicer.as_deref(), Some("Bambu Studio 99.99.99.99"));
    assert_eq!(plate(&read).print_seconds, Some(8332), "total estimated time 2h 18m 52s, not the model time");
    assert_eq!(read.filaments.len(), 2);
    assert_eq!(read.filaments[0].weight_g, Some(15.75));
    assert!((read.filaments[0].length_mm.unwrap() - 5197.96).abs() < 1e-9);
    assert!((read.filaments[0].volume_cm3.unwrap() - 12.50257).abs() < 1e-9, "labelled cm^3, really mm^3");
    assert_eq!(read.print_info.bed_temp_c, Reduced::Value(35.0), "Cool Plate -> cool_plate_temp");
    assert_eq!(read.print_info.nozzle_temp_c, Reduced::Mixed);
    let info = read.slice_info.unwrap();
    assert!((info.total_weight_g - 37.43).abs() < 1e-9, "sum when the file names no total");
}

#[test]
fn cura_placeholders_are_not_values() {
    let read = read_text_fixture("cura_style.gcode");
    assert_eq!(read.slicer.as_deref(), Some("Cura 99.99.9"));
    assert_eq!(plate(&read).print_seconds, Some(70), "last ;TIME_ELAPSED, not the placeholder 6666");
    assert!(read.filaments.is_empty(), ";Filament used: 0m is not a use");
    assert!(read.slice_info.is_none());
    assert_eq!(read.print_info.layer_height_mm, Some(0.2));
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Example Cura Printer"));
    assert_eq!(read.print_info.settings_state, SettingsState::Missing);

    // A real estimate written by the interface is used as it is.
    let real = fixture_text("cura_style.gcode").replace(";TIME:6666", ";TIME:3600").replace(";Filament used: 0m", ";Filament used: 1.5m");
    let read = read_bytes(real.as_bytes()).unwrap();
    assert_eq!(plate(&read).print_seconds, Some(3600));
    assert_eq!(read.filaments[0].length_mm, Some(1500.0));
    // A length alone is never turned into a weight; it is kept as a length next to 0 g.
    let info = read.slice_info.expect("the length is kept");
    assert!(info.weight_unknown());
    assert_eq!(info.total_weight_g, 0.0);
    let used = &info.plates[0].filaments[0];
    assert_eq!(used.used_g, 0.0);
    assert!((used.used_m - 1.5).abs() < 1e-9);
}

#[test]
fn a_mix_of_filaments_with_and_without_weight_keeps_no_usage() {
    let text = "; generated by PrusaSlicer 9.9.9 on 2026-01-01 at 00:00:00 UTC\n\
        ; filament used [mm] = 100.00, 200.00\n; filament used [g] = 3.00\n\
        ; estimated printing time (normal mode) = 10m 0s\n";
    let read = read_bytes(text.as_bytes()).unwrap();
    assert_eq!(read.filaments.len(), 2);
    assert!(read.slice_info.is_none(), "neither a weight for one nor a length for the other is a usage");
}

#[test]
fn the_filament_check_works_on_the_usage_of_a_g_code_file_and_skips_a_length_only_one() {
    use crate::filament_check::{check_models, CheckStatus, SlotRef, SpoolInput};
    let spool = |id: &str, material: &str, hex: &str, grams: f64| SpoolInput {
        id: id.to_string(), material: material.to_string(), manufacturer: None, color_name: None,
        color_hex: Some(hex.to_string()), remaining_g: grams, original_g: 1000.0,
        slot: None::<SlotRef>, location: None,
    };
    let two = read_text_fixture("prusa_two_filaments.gcode").slice_info;
    let enough = check_models(&[("1".to_string(), two.clone())], &[spool("red", "PLA", "#e4002b", 500.0), spool("green", "PETG", "#00aa00", 500.0)]);
    assert_eq!(enough[0].status, CheckStatus::Ok);
    assert_eq!(enough[0].needs.len(), 2);
    // PETG needs 4.19 g; a nearly empty spool of the right color is short, not ok.
    let short = check_models(&[("1".to_string(), two)], &[spool("red", "PLA", "#e4002b", 500.0), spool("green", "PETG", "#00aa00", 1.0)]);
    assert_eq!(short[0].status, CheckStatus::Short);

    let real = fixture_text("cura_style.gcode").replace(";TIME:6666", ";TIME:3600").replace(";Filament used: 0m", ";Filament used: 1.5m");
    let cura = read_bytes(real.as_bytes()).unwrap().slice_info;
    let none = check_models(&[("2".to_string(), cura)], &[spool("red", "PLA", "#e4002b", 500.0)]);
    assert_eq!(none[0].status, CheckStatus::NoData, "no weight, no check");
    assert!(none[0].needs.is_empty());
}

#[test]
fn a_file_without_a_slicer_marker_is_not_recognized_and_its_text_is_not_trusted() {
    let read = read_text_fixture("no_marker.gcode");
    assert_eq!(read.print_info.slice_state, SliceState::NotRecognized);
    assert!(read.print_info.plates.is_empty());
    assert_eq!(read.print_info.layer_height_mm, None, "a layer height in a hand-written comment is not read");
    let import = import_gcode(&mut Cursor::new(fixture("no_marker.gcode")));
    let record = import.print_info.unwrap();
    assert_eq!(record.slice_state, SliceState::NotRecognized);
    assert!(record.plates.is_empty());
    assert_eq!(import.plate_count, None);
}

#[test]
fn preview_blocks_with_wrong_length_bad_base64_or_too_large_are_dropped_without_error() {
    let text = fixture_text("orca_style.gcode");
    // The first block announces 96x96 and its length; the second is the 300x300 one.
    let first_len = text.lines().find(|l| l.starts_with("; thumbnail begin 96x96")).unwrap().rsplit(' ').next().unwrap().to_string();
    let wrong_length = text.replace(&format!("96x96 {first_len}"), &format!("96x96 {}", first_len.parse::<usize>().unwrap() + 1));
    let read = read_bytes(wrong_length.as_bytes()).unwrap();
    assert_eq!(png_width(&read.thumbnail_png.unwrap()), 3, "the other block is still used");

    let both_wrong = wrong_length.replace("300x300", "300x300 7 x").replace("; thumbnail begin 300x300 7 x", "; thumbnail begin 300x300 1");
    let read = read_bytes(both_wrong.as_bytes()).unwrap();
    assert!(read.thumbnail_png.is_none());
    assert_eq!(read.print_info.slice_state, SliceState::Sliced, "numbers survive a bad preview");

    let bad_base64 = text.replace("; iVBOR", "; !!!!R");
    let read = read_bytes(bad_base64.as_bytes()).unwrap();
    assert!(read.thumbnail_png.is_none(), "invalid Base64 gives no image");
    assert_eq!(plate(&read).print_seconds, Some(919));

    // Valid Base64 that is not a PNG.
    let body = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"GIF89a-not-a-png");
    let not_png = format!("; generated by OrcaSlicer 1\n; thumbnail begin 10x10 {}\n; {body}\n; thumbnail end\n", body.len());
    assert!(read_bytes(not_png.as_bytes()).unwrap().thumbnail_png.is_none());

    // Announced larger than the limit: the block is skipped as a whole (the lines are not collected).
    let huge = format!("; generated by OrcaSlicer 1\n; thumbnail begin 10x10 {}\n; AAAA\n; thumbnail end\n", MAX_THUMBNAIL_BYTES * 2);
    assert!(read_bytes(huge.as_bytes()).unwrap().thumbnail_png.is_none());

    // A PNG over the limit that announces its true length is dropped too.
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(MAX_THUMBNAIL_BYTES + 1, 0);
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &big);
    let lines: String = b64.as_bytes().chunks(76).map(|c| format!("; {}\n", std::str::from_utf8(c).unwrap())).collect();
    let big_file = format!("; generated by OrcaSlicer 1\n; thumbnail begin 10x10 {}\n{lines}; thumbnail end\n", b64.len());
    assert!(b64.len() <= MAX_THUMBNAIL_BYTES / 3 * 4 + 4, "announced length stays within the line limit, so the decoded size decides");
    assert!(read_bytes(big_file.as_bytes()).unwrap().thumbnail_png.is_none());
}

#[test]
fn jpg_and_qoi_previews_are_not_read_and_not_an_error() {
    let text = fixture_text("prusa_one_filament.gcode");
    // Remove the PNG block: only the QOI-labelled block is left.
    let start = text.find("; thumbnail begin 220x124").unwrap();
    let end = text[start..].find("; thumbnail end\n").unwrap() + start + "; thumbnail end\n".len();
    let only_qoi = format!("{}{}", &text[..start], &text[end..]);
    let read = read_bytes(only_qoi.as_bytes()).unwrap();
    assert!(read.thumbnail_png.is_none());
    assert_eq!(read.print_info.slice_state, SliceState::Sliced);
}

// ---------------------------------------------------------------- limits (spec test 3)

/// A file of `len` bytes that is zeros except for a prefix and a suffix; counts the bytes read.
struct SparseFile {
    len: u64,
    pos: u64,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    read_bytes: u64,
}

impl Read for SparseFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = (self.len.saturating_sub(self.pos)).min(buf.len() as u64) as usize;
        for (i, slot) in buf[..n].iter_mut().enumerate() {
            let at = self.pos + i as u64;
            *slot = if at < self.prefix.len() as u64 {
                self.prefix[at as usize]
            } else if at >= self.len - self.suffix.len() as u64 {
                self.suffix[(at - (self.len - self.suffix.len() as u64)) as usize]
            } else {
                0
            };
        }
        self.pos += n as u64;
        self.read_bytes += n as u64;
        Ok(n)
    }
}

impl Seek for SparseFile {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::End(d) => (self.len as i64 + d) as u64,
            SeekFrom::Current(d) => (self.pos as i64 + d) as u64,
        };
        Ok(self.pos)
    }
}

#[test]
fn a_three_gib_file_is_read_at_both_ends_only() {
    let text = fixture_text("prusa_one_filament.gcode");
    let split = text.find("G28\n").unwrap() + 4;
    let (head, tail) = text.split_at(split);
    // The tail starts with the body lines, which also checks that a window cut out of the middle
    // does not break the sums behind it.
    let mut file = SparseFile { len: 3 * 1024 * 1024 * 1024, pos: 0, prefix: head.as_bytes().to_vec(), suffix: tail.as_bytes().to_vec(), read_bytes: 0 };
    let read = read_gcode(&mut file).unwrap();
    assert!(file.read_bytes <= HEAD_BYTES + TAIL_BYTES + 16, "read {} bytes", file.read_bytes);
    assert!(file.read_bytes >= HEAD_BYTES, "the head is read");
    assert_eq!(plate(&read).print_seconds, Some(4177));
    assert_eq!(read.filaments.len(), 1);
    assert!(read.thumbnail_png.is_some());
}

#[test]
fn a_line_without_newline_over_the_limit_is_ignored_and_its_neighbours_are_read() {
    let mut text = String::from("; generated by PrusaSlicer 1.2.3 on x\n");
    text.push_str("; layer_height = 0.3\n");
    text.push_str("; start_gcode = ");
    text.push_str(&"x".repeat(MAX_LINE_BYTES + 10));
    text.push_str("\n; printer_model = After\n; prusaslicer_config = begin\n");
    // And a last line over the limit with no line break at all.
    text.push_str("; end_gcode = ");
    text.push_str(&"y".repeat(MAX_LINE_BYTES * 2));
    let read = read_bytes(text.as_bytes()).unwrap();
    assert_eq!(read.print_info.layer_height_mm, Some(0.3));
    assert_eq!(read.print_info.printer_model.as_deref(), Some("After"));
}

#[test]
fn a_file_shorter_than_head_plus_tail_is_read_once() {
    let mut cursor = Cursor::new(fixture("prusa_one_filament.gcode"));
    let len = cursor.get_ref().len() as u64;
    let raw = text::read(&mut cursor, len).unwrap();
    assert_eq!(raw.thumbs.len(), 1, "a preview in the overlap of head and tail is counted once");
    // Just above the window sizes the two windows do not overlap and every line is seen.
    let filler = vec![b'\n'; (HEAD_BYTES + TAIL_BYTES) as usize];
    let mut text = fixture("prusa_one_filament.gcode");
    let split = String::from_utf8_lossy(&text).find("G28\n").unwrap() + 4;
    let tail = text.split_off(split);
    let mut file = text;
    file.extend_from_slice(&filler);
    file.extend_from_slice(&tail);
    let read = read_bytes(&file).unwrap();
    assert_eq!(plate(&read).print_seconds, Some(4177));
    assert_eq!(read.filaments.len(), 1);
}

#[test]
fn a_tail_cut_inside_a_multi_byte_character_keeps_the_earlier_value() {
    let mut bytes = fixture("prusa_one_filament.gcode");
    bytes.extend_from_slice(b"; printer_model = Caf\xC3");
    let read = read_bytes(&bytes).unwrap();
    assert_eq!(read.print_info.printer_model.as_deref(), Some("MK4S"));
    // A window that starts inside a character: the partial first line is dropped.
    let mut padded = "é".repeat(10).into_bytes();
    padded.remove(0);
    padded.extend_from_slice(b"\n; generated by PrusaSlicer 1\n; layer_height = 0.25\n");
    let read = read_bytes(&padded).unwrap();
    assert_eq!(read.print_info.layer_height_mm, Some(0.25));
}

#[test]
fn an_empty_or_binary_garbage_file_is_not_recognized_and_does_not_panic() {
    assert_eq!(read_bytes(b"").unwrap().print_info.slice_state, SliceState::NotRecognized);
    let garbage: Vec<u8> = (0..70_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    assert_eq!(read_bytes(&garbage).unwrap().print_info.slice_state, SliceState::NotRecognized);
}

#[test]
fn multi_byte_characters_in_a_time_value_are_no_time_and_do_not_panic() {
    for value in ["1\u{b5}", "\u{b5}", "1h 5\u{e9}", "12\u{2603}", "1h \u{1F600}"] {
        let text = format!("; generated by PrusaSlicer 2.8.1\n; estimated printing time (normal mode) = {value}\n");
        let read = read_bytes(text.as_bytes()).unwrap();
        assert_eq!(plate(&read).print_seconds, None, "{value}");
    }
}

#[test]
fn multi_byte_characters_in_binary_metadata_times_are_no_time_and_do_not_panic() {
    let print = "estimated printing time (normal mode)=1\u{b5}\nfirst layer printing time=3\u{e9}\n";
    let file = Blocks::new()
        .add(ini_block(0, false, "Producer=PrusaSlicer 5.0.0+x\n"))
        .add(ini_block(4, false, print))
        .add(gcode_block())
        .0;
    let read = read_bytes(&file).unwrap();
    assert_eq!(plate(&read).print_seconds, None);
    assert_eq!(plate(&read).first_layer_seconds, None);
}

// ---------------------------------------------------------------- binary G-code (spec test 4)

struct Blocks(Vec<u8>);

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn block(kind: u16, compression: u16, params: &[u8], data: &[u8], uncompressed: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&compression.to_le_bytes());
    out.extend_from_slice(&uncompressed.to_le_bytes());
    if compression != 0 {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(params);
    out.extend_from_slice(data);
    let sum = crc32(&out);
    out.extend_from_slice(&sum.to_le_bytes());
    out
}

fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn ini_block(kind: u16, compressed: bool, ini: &str) -> Vec<u8> {
    if compressed {
        block(kind, 1, &[0, 0], &deflate(ini.as_bytes()), ini.len() as u32)
    } else {
        block(kind, 0, &[0, 0], ini.as_bytes(), ini.len() as u32)
    }
}

fn gcode_block() -> Vec<u8> {
    block(1, 0, &[0, 0], b"G1 X1\n", 6)
}

impl Blocks {
    fn new() -> Self {
        let mut v = b"GCDE".to_vec();
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        Blocks(v)
    }
    fn add(mut self, block: Vec<u8>) -> Self {
        self.0.extend(block);
        self
    }
}

const PRINTER_INI: &str = "printer_model=Printer Q\nnozzle_diameter=0.6\nlayer_height=0.3\nfilament_type=PETG\n";
const PRINT_INI: &str = "filament used [mm]=1000.5\nfilament used [g]=3.5\nestimated printing time (normal mode)=1h 1m 1s\n";
const SLICER_INI: &str = "filament_colour=#112233\nfilament_type=PETG\n";

fn standard_blocks(order: &[usize]) -> Vec<u8> {
    let blocks = [
        ini_block(3, false, PRINTER_INI),
        ini_block(4, true, PRINT_INI),
        ini_block(2, true, SLICER_INI),
    ];
    let mut file = Blocks::new().add(ini_block(0, false, "Producer=PrusaSlicer 5.0.0+x\n"));
    for i in order {
        file = file.add(blocks[*i].clone());
    }
    file.add(gcode_block()).add(gcode_block()).0
}

fn assert_standard_values(read: &GcodeRead) {
    assert_eq!(read.slicer.as_deref(), Some("PrusaSlicer 5.0.0"));
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Printer Q"));
    assert_eq!(plate(read).print_seconds, Some(3661));
    assert_eq!(read.print_info.layer_height_mm, Some(0.3));
    assert_eq!(read.print_info.filament_colors, vec!["#112233"]);
    assert_eq!(read.filaments[0].weight_g, Some(3.5));
    assert_eq!(read.print_info.settings_state, SettingsState::Read);
}

#[test]
fn binary_fixture_gives_the_metadata_and_the_png_block() {
    let read = read_text_fixture("prusa_binary.bgcode");
    assert_eq!(read.slicer.as_deref(), Some("PrusaSlicer 99.9.9"));
    assert_eq!(read.print_info.slice_state, SliceState::Sliced);
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Example Printer Z"));
    assert_eq!(plate(&read).print_seconds, Some(2395));
    assert_eq!(plate(&read).first_layer_seconds, Some(95.0), "from the deflated print metadata");
    assert_eq!(read.print_info.filament_colors, vec!["#e4002b", "#00aa00"], "from the deflated slicer metadata");
    assert_eq!(read.print_info.bed_temp_c, Reduced::Mixed);
    assert_eq!(read.filaments.len(), 2);
    assert!((read.slice_info.unwrap().total_weight_g - 8.28).abs() < 1e-9);
    assert_eq!(png_width(&read.thumbnail_png.unwrap()), 2, "the PNG block; the QOI block is not read");
}

#[test]
fn metadata_blocks_may_come_in_any_order() {
    let mut reference = None;
    for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0], [1, 0, 2], [2, 0, 1], [0, 2, 1]] {
        let read = read_bytes(&standard_blocks(&order)).unwrap();
        assert_standard_values(&read);
        let key = (plate(&read).print_seconds, read.print_info.printer_model.clone(), read.print_info.layer_height_mm);
        assert_eq!(*reference.get_or_insert(key.clone()), key);
    }
}

#[test]
fn file_without_checksums_and_with_only_the_header_blocks_is_read() {
    let mut v = b"GCDE".to_vec();
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    // Without checksums a block has no trailing CRC.
    let mut b = ini_block(3, false, PRINTER_INI);
    b.truncate(b.len() - 4);
    v.extend(b);
    let read = read_bytes(&v).unwrap();
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Printer Q"));
}

#[test]
fn a_deflate_block_that_lies_about_its_size_is_rejected() {
    // Claims 100 bytes, unpacks to 20 MiB: the output is cut at the claim plus one byte.
    let bomb = deflate(&vec![0u8; 20 * 1024 * 1024]);
    assert!(bomb.len() < 100 * 1024, "the bomb itself is small");
    let file = Blocks::new().add(block(4, 1, &[0, 0], &bomb, 100)).add(gcode_block()).0;
    assert!(matches!(read_bytes(&file), Err(GcodeError::BgcodeTooLarge)));
    // Claims more than the limit: rejected before anything is unpacked.
    let file = Blocks::new().add(block(4, 1, &[0, 0], &bomb, (MAX_METADATA_BYTES + 1) as u32)).add(gcode_block()).0;
    assert!(matches!(read_bytes(&file), Err(GcodeError::BgcodeTooLarge)));
    // Honest size but over the limit as well.
    let big = deflate(&vec![b'a'; (MAX_METADATA_BYTES + 10) as usize]);
    let file = Blocks::new().add(block(4, 1, &[0, 0], &big, (MAX_METADATA_BYTES + 10) as u32)).add(gcode_block()).0;
    assert!(matches!(read_bytes(&file), Err(GcodeError::BgcodeTooLarge)));
    // Claims more than it holds.
    let small = deflate(b"printer_model=A\n");
    let file = Blocks::new().add(block(3, 1, &[0, 0], &small, 5000)).add(gcode_block()).0;
    assert!(matches!(read_bytes(&file), Err(GcodeError::BgcodeCorrupt)));
    // Not Deflate data at all.
    let file = Blocks::new().add(block(3, 1, &[0, 0], b"not deflate at all", 17)).add(gcode_block()).0;
    assert!(matches!(read_bytes(&file), Err(GcodeError::BgcodeCorrupt)));
}

#[test]
fn too_many_blocks_before_the_gcode_are_rejected() {
    let mut file = Blocks::new();
    for _ in 0..(MAX_BGCODE_BLOCKS + 5) {
        file = file.add(ini_block(0, false, "Producer=X 1\n"));
    }
    assert!(matches!(read_bytes(&file.add(gcode_block()).0), Err(GcodeError::BgcodeTooLarge)));
    // The same number of blocks ending exactly at the limit is fine.
    let mut file = Blocks::new();
    for _ in 0..(MAX_BGCODE_BLOCKS - 1) {
        file = file.add(ini_block(0, false, "Producer=X 1\n"));
    }
    assert!(read_bytes(&file.add(gcode_block()).0).is_ok());
}

#[test]
fn a_wrong_checksum_makes_the_file_unreadable_but_not_a_preview() {
    let good = standard_blocks(&[0, 1, 2]);
    // Flip a byte inside the printer metadata text.
    let at = good.windows(9).position(|w| w == b"Printer Q").unwrap();
    let mut bad = good.clone();
    bad[at] ^= 0x01;
    let error = read_bytes(&bad).unwrap_err();
    assert!(matches!(error, GcodeError::BgcodeChecksum));
    assert_eq!(error.code(), ErrorCode::BgcodeChecksum);
    let import = import_gcode(&mut Cursor::new(bad));
    let record = import.print_info.expect("a record that says unreadable");
    assert_eq!(record.slice_state, SliceState::Unreadable);
    assert!(record.plates.is_empty() && record.nozzle_mm.is_none());
    assert_eq!(record.reader_version, GCODE_READER_VERSION);
    assert!(import.slice_info_json.is_none());

    // A preview with a wrong checksum is only dropped.
    let png = {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend_from_slice(&[7u8; 20]);
        p
    };
    let params = [0u8, 0, 16, 0, 16, 0];
    let mut thumb = block(5, 0, &params, &png, png.len() as u32);
    let file_ok = Blocks::new().add(thumb.clone()).add(ini_block(3, false, PRINTER_INI)).add(gcode_block()).0;
    assert!(read_bytes(&file_ok).unwrap().thumbnail_png.is_some());
    thumb[20] ^= 0xFF;
    let file_bad = Blocks::new().add(thumb).add(ini_block(3, false, PRINTER_INI)).add(gcode_block()).0;
    let read = read_bytes(&file_bad).unwrap();
    assert!(read.thumbnail_png.is_none());
    assert_eq!(read.print_info.printer_model.as_deref(), Some("Printer Q"));
}

#[test]
fn heatshrink_metadata_is_unreadable_with_its_own_code() {
    let file = Blocks::new().add(block(3, 3, &[0, 0], b"whatever", 100)).add(gcode_block()).0;
    let error = read_bytes(&file).unwrap_err();
    assert!(matches!(error, GcodeError::BgcodeCompression));
    assert_eq!(error.code(), ErrorCode::BgcodeCompressionUnsupported);
    // Heatshrink G-code blocks are never touched.
    let gcode_hs = block(1, 3, &[0, 0], b"\x00\x01\x02", 4000);
    let file = Blocks::new().add(ini_block(3, false, PRINTER_INI)).add(gcode_hs).0;
    assert_eq!(read_bytes(&file).unwrap().print_info.printer_model.as_deref(), Some("Printer Q"));
}

#[test]
fn magic_version_and_structure_errors_have_their_own_codes() {
    let good = standard_blocks(&[0, 1, 2]);

    let mut wrong_magic = good.clone();
    wrong_magic[0] = b'X';
    assert!(matches!(bgcode::read(&mut Cursor::new(wrong_magic.clone()), wrong_magic.len() as u64), Err(GcodeError::NotBgcode)));
    // Through the common entry the file is judged as text: nothing recognized, no error.
    assert_eq!(read_bytes(&wrong_magic).unwrap().print_info.slice_state, SliceState::NotRecognized);

    let mut version2 = good.clone();
    version2[4] = 2;
    assert!(matches!(read_bytes(&version2), Err(GcodeError::BgcodeVersion(2))));

    let mut checksum_type = good.clone();
    checksum_type[8] = 7;
    assert!(matches!(read_bytes(&checksum_type), Err(GcodeError::BgcodeCorrupt)));

    // Unknown block type.
    let unknown = Blocks::new().add(block(9, 0, &[0, 0], b"x", 1)).0;
    assert!(matches!(read_bytes(&unknown), Err(GcodeError::BgcodeCorrupt)));

    // A block that claims 4 GiB.
    let mut huge = Blocks::new().0;
    huge.extend_from_slice(&3u16.to_le_bytes());
    huge.extend_from_slice(&0u16.to_le_bytes());
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    huge.extend_from_slice(&[0, 0, b'a', b'b']);
    assert!(matches!(read_bytes(&huge), Err(GcodeError::BgcodeTruncated)));

    // The file ends in the middle of a metadata block, and right after the file header.
    for cut in [good.len() - 40, 14, 11, 10] {
        let mut chopped = good.clone();
        chopped.truncate(cut.min(good.len()));
        let result = read_bytes(&chopped);
        assert!(result.is_ok() || matches!(result, Err(GcodeError::BgcodeTruncated)), "cut at {cut}");
    }
    let first_block_end = 10 + ini_block(0, false, "Producer=PrusaSlicer 5.0.0+x\n").len();
    let mut mid = good.clone();
    mid.truncate(first_block_end + 20);
    assert!(matches!(read_bytes(&mid), Err(GcodeError::BgcodeTruncated)));
    let mut header_only = good.clone();
    header_only.truncate(10);
    assert!(read_bytes(&header_only).is_ok(), "no blocks is not a damaged file");
    let mut short = good;
    short.truncate(6);
    assert!(matches!(read_bytes(&short), Err(GcodeError::BgcodeTruncated)));
}

#[test]
fn every_error_has_a_registered_code() {
    let all = [
        GcodeError::Io(std::io::Error::other("x")),
        GcodeError::NotBgcode,
        GcodeError::BgcodeVersion(2),
        GcodeError::BgcodeCorrupt,
        GcodeError::BgcodeTruncated,
        GcodeError::BgcodeChecksum,
        GcodeError::BgcodeCompression,
        GcodeError::BgcodeTooLarge,
    ];
    for error in all {
        assert!(ErrorCode::ALL.contains(&error.code()), "{error:?}");
    }
    assert!(!GcodeError::Io(std::io::Error::other("x")).is_damaged_file());
    assert!(GcodeError::BgcodeChecksum.is_damaged_file());
}

// ---------------------------------------------------------------- storing (spec test 5)

#[test]
fn the_import_result_carries_the_gcode_reader_version_and_the_catalog_fields() {
    let import = import_gcode(&mut Cursor::new(fixture("prusa_two_filaments.gcode")));
    let record = import.print_info.unwrap();
    assert_eq!(record.reader_version, GCODE_READER_VERSION);
    assert_ne!(GCODE_READER_VERSION, crate::threemf::print_info::PRINT_INFO_READER_VERSION, "separate numbers");
    assert_eq!(record.slice_state, SliceState::Sliced);
    assert_eq!(record.plates.len(), 1);
    assert_eq!(record.plates[0].plate_index, 1);
    assert_eq!(record.plates[0].print_seconds, Some(4177));
    assert_eq!(record.filament_colors, vec!["#e4002b", "#00aa00"]);
    assert_eq!(import.plate_count, Some(1));
    assert_eq!(import.metadata.get(METADATA_APPLICATION).map(String::as_str), Some("PrusaSlicer 9.9.9"));
    assert!(import.thumbnail_png.is_some());
    let info: SliceInfo = serde_json::from_str(&import.slice_info_json.unwrap()).unwrap();
    assert_eq!(info.plates[0].plate_index, 1);
    assert_eq!(info.plates[0].filaments.len(), 2);
}

#[test]
fn a_record_that_claims_a_time_without_sliced_is_cleaned_by_validation() {
    let mut read = read_text_fixture("prusa_one_filament.gcode");
    read.print_info.slice_state = SliceState::NotRecognized;
    let record = PrintInfoRecord::from_print_info(&read.print_info, "now".into());
    assert!(record.plates.is_empty(), "plates only for sliced / no_slice_data");
    assert_eq!(record.nozzle_temp_c, None);
    assert_eq!(record.bed_temp_c, None);
}

#[test]
fn damaged_binary_and_unreadable_files_leave_the_entry_without_crashing() {
    // Cannot be opened at all: an empty result, no record (the entry counts as "not read yet").
    let missing = import_gcode_file(&fixture_path("does-not-exist.gcode"));
    assert!(missing.print_info.is_none() && missing.thumbnail_png.is_none());
    let from_file = import_gcode_file(&fixture_path("prusa_binary.bgcode"));
    assert_eq!(from_file.print_info.unwrap().slice_state, SliceState::Sliced);
}

#[test]
fn desktop_and_web_import_store_the_print_data_of_a_gcode_file() {
    use crate::db::models::FileType;
    let path = fixture_path("prusa_two_filaments.gcode");
    let desktop = crate::import::files::read_model_file(&path, None).unwrap();
    assert_eq!(desktop.file_type, FileType::Gcode);
    assert_eq!(desktop.plate_count, Some(1));
    assert!(desktop.thumbnail_png.is_some());
    assert!(desktop.slice_info_json.is_some());
    assert_eq!(desktop.metadata.get(METADATA_APPLICATION).map(String::as_str), Some("PrusaSlicer 9.9.9"));
    let record = desktop.print_info.as_ref().unwrap();
    assert_eq!((record.reader_version, record.slice_state), (GCODE_READER_VERSION, SliceState::Sliced));

    let file = std::fs::File::open(&path).unwrap();
    let web = crate::import::files::read_web_model(file, &path, "x.gcode", crate::import::files::WebImportLimits::default()).unwrap();
    assert_eq!(web.print_info.as_ref().unwrap().plates[0].print_seconds, Some(4177));
    assert_eq!(web.slice_info_json, desktop.slice_info_json);

    // A binary file the same way, and a damaged one keeps its entry with state "unreadable".
    let binary = crate::import::files::read_model_file(&fixture_path("prusa_binary.bgcode"), None).unwrap();
    assert_eq!(binary.print_info.unwrap().plates[0].print_seconds, Some(2395));
    let dir = crate::unique_test_dir("gcode_damaged");
    let damaged = dir.join("damaged.bgcode");
    let mut bytes = fixture("prusa_binary.bgcode");
    bytes[30] ^= 0xFF;
    std::fs::write(&damaged, bytes).unwrap();
    let entry = crate::import::files::read_model_file(&damaged, None).unwrap();
    assert_eq!(entry.file_type, FileType::Gcode);
    assert_eq!(entry.print_info.unwrap().slice_state, SliceState::Unreadable);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn expected_reader_version_is_kept_per_file_type() {
    use crate::db::models::FileType;
    assert_eq!(FileType::ThreeMf.expected_reader_version(), Some(crate::threemf::print_info::PRINT_INFO_READER_VERSION));
    assert_eq!(FileType::Gcode.expected_reader_version(), Some(GCODE_READER_VERSION));
    for other in [FileType::Stl, FileType::Stp, FileType::Obj, FileType::Resin] {
        assert_eq!(other.expected_reader_version(), None);
    }
}

// ---------------------------------------------------------------- robustness

/// Small deterministic generator so the mutation test has fixed starting values.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn two_thousand_mutations_of_the_fixtures_never_panic_and_run_quickly() {
    let names = [
        "prusa_one_filament.gcode",
        "prusa_two_filaments.gcode",
        "orca_style.gcode",
        "orca_header_only.gcode",
        "bambu_style.gcode",
        "cura_style.gcode",
        "no_marker.gcode",
        "prusa_binary.bgcode",
    ];
    let originals: Vec<Vec<u8>> = names.iter().map(|n| fixture(n)).collect();
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let started = std::time::Instant::now();
    for round in 0..2000usize {
        let mut bytes = originals[round % originals.len()].clone();
        for _ in 0..(1 + rng.next() % 6) {
            let at = (rng.next() % bytes.len() as u64) as usize;
            match rng.next() % 4 {
                0 => bytes[at] = rng.next() as u8,
                1 => bytes[at] ^= 1 << (rng.next() % 8),
                2 => bytes.truncate(at.max(1)),
                _ => {
                    let extra = (rng.next() % 64) as usize;
                    bytes.splice(at..at, std::iter::repeat_n(rng.next() as u8, extra));
                }
            }
        }
        // Whatever comes out, it is a value or an error, never a panic.
        let _ = read_bytes(&bytes);
        let _ = import_gcode(&mut Cursor::new(bytes));
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "took {:?}", started.elapsed());
}

// ---------------------------------------------------------------- real files (optional)

/// Reads every file below `GCODE_SAMPLES_DIR` (when the variable is set) and checks only what
/// holds for every real slicer output: the file is recognized, a time and a use are read where
/// the slicer wrote them, and nothing panics. The files are not part of the repository.
#[test]
fn real_sample_files_are_read_when_a_samples_folder_is_given() {
    let Some(dir) = std::env::var_os("GCODE_SAMPLES_DIR") else { return };
    let mut checked = Vec::new();
    let mut stack = vec![PathBuf::from(dir)];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "auszuege" && n != "_erzeugung") {
                    stack.push(path);
                }
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !(name.ends_with(".gcode") || name.ends_with(".bgcode")) {
                continue;
            }
            let mut file = std::fs::File::open(&path).unwrap();
            let read = read_gcode(&mut file).unwrap_or_else(|e| panic!("{name}: {e}"));
            let is_cura = name.contains("curaengine");
            assert_eq!(read.print_info.slice_state, SliceState::Sliced, "{name}");
            assert!(read.slicer.is_some(), "{name}");
            let seconds = plate(&read).print_seconds;
            assert!(seconds.is_some_and(|s| s > 60), "{name}: print time {seconds:?}");
            assert!(read.print_info.layer_height_mm.is_some() || name.contains("bambu"), "{name}");
            if !is_cura {
                assert!(!read.filaments.is_empty(), "{name}: filament use");
                // A slicer profile without a filament density reports 0.00 g; that is no weight.
                let weighed = read.filaments.iter().all(|f| f.weight_g.is_some_and(|g| g > 0.0));
                assert_eq!(read.slice_info.is_some(), weighed, "{name}: weights");
            } else {
                assert!(seconds != Some(6666), "{name}: placeholder used");
                assert!(read.filaments.is_empty() && read.slice_info.is_none(), "{name}");
            }
            let expect_thumbnail = !name.starts_with("cli_") && !is_cura && !name.contains("qoi_jpg");
            if expect_thumbnail {
                assert!(read.thumbnail_png.is_some(), "{name}: preview");
            }
            checked.push(format!(
                "{name}: {} | {}s | {} filament(s) | preview={}",
                read.slicer.as_deref().unwrap_or("?"),
                seconds.unwrap_or(0),
                read.filaments.len(),
                read.thumbnail_png.is_some()
            ));
        }
    }
    checked.sort();
    println!("read {} real files:\n{}", checked.len(), checked.join("\n"));
}
