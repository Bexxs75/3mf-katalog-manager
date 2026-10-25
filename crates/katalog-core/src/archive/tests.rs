use super::*;
use super::paths::sanitize_component;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn unique_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "archive_{name}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}


fn allow_all(_: &Path) -> bool {
    true
}

fn is_model(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "stl" | "3mf" | "obj" | "stp" | "step"))
}

/// Content every round-trip test packs and expects back.
const SAMPLE: &[(&str, &[u8])] = &[
    ("Benchy/benchy.stl", b"solid benchy\nendsolid benchy\n"),
    ("Benchy/README.txt", b"Lizenz: CC-BY"),
    ("Benchy/teile/rumpf.3mf", b"kein echtes 3mf, nur Bytes"),
];

fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn tar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.into_inner().unwrap()
}

fn make_tar(path: &Path, format: ArchiveFormat, entries: &[(&str, &[u8])]) {
    let raw = tar_bytes(entries);
    let packed: Vec<u8> = match format {
        ArchiveFormat::Tar => raw,
        ArchiveFormat::TarGz => {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(&raw).unwrap();
            enc.finish().unwrap()
        }
        ArchiveFormat::TarBz2 => {
            let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            enc.write_all(&raw).unwrap();
            enc.finish().unwrap()
        }
        ArchiveFormat::TarXz => {
            let mut enc = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(6)).unwrap();
            enc.write_all(&raw).unwrap();
            enc.finish().unwrap()
        }
        ArchiveFormat::TarZst => {
            ruzstd::encoding::compress_to_vec(&raw[..], ruzstd::encoding::CompressionLevel::Fastest)
        }
        other => panic!("kein tar-Format: {other:?}"),
    };
    fs::write(path, packed).unwrap();
}


fn assert_sample_extracted(dest: &Path) {
    for (name, data) in SAMPLE {
        assert_eq!(fs::read(dest.join(name.strip_prefix("Benchy/").unwrap())).unwrap(), *data, "{name}");
    }
}

// ---------- Format detection & names ----------

#[test]
fn detect_format_prefers_multi_part_suffixes_and_ignores_case() {
    assert_eq!(detect_format(Path::new("a.TAR.GZ")), Some(ArchiveFormat::TarGz));
    assert_eq!(detect_format(Path::new("a.tgz")), Some(ArchiveFormat::TarGz));
    assert_eq!(detect_format(Path::new("a.tar.zst")), Some(ArchiveFormat::TarZst));
    assert_eq!(detect_format(Path::new("a.7z")), Some(ArchiveFormat::SevenZ));
    assert_eq!(detect_format(Path::new("a.Rar")), Some(ArchiveFormat::Rar));
    assert_eq!(detect_format(Path::new("a.zip")), Some(ArchiveFormat::Zip));
    assert_eq!(detect_format(Path::new("modell.stl.gz")), None);
    assert_eq!(detect_format(Path::new("modell.stl")), None);
    assert_eq!(detect_format(Path::new(".zip")), None);
}

#[test]
fn folder_name_for_strips_archive_suffix_and_sanitizes() {
    assert_eq!(folder_name_for(Path::new("/dl/Benchy_v2.zip")), "Benchy_v2");
    assert_eq!(folder_name_for(Path::new("/dl/Drache.tar.gz")), "Drache");
    assert_eq!(folder_name_for(Path::new("/dl/a:b?.7z")), "a_b_");
    assert_eq!(folder_name_for(Path::new("/dl/CON.zip")), "_CON");
    assert_eq!(folder_name_for(Path::new("/dl/....zip")), "Archiv");
}

// ---------- Path safety ----------

#[test]
fn safe_relative_path_rejects_traversal_absolute_and_drive_paths() {
    assert_eq!(safe_relative_path("a/b.stl"), Some(PathBuf::from("a/b.stl")));
    assert_eq!(safe_relative_path("a\\b.stl"), Some(PathBuf::from("a/b.stl")));
    assert_eq!(safe_relative_path("./a//b.stl"), Some(PathBuf::from("a/b.stl")));
    assert_eq!(safe_relative_path("../evil.stl"), None);
    assert_eq!(safe_relative_path("a/../../evil.stl"), None);
    assert_eq!(safe_relative_path("/etc/evil.stl"), None);
    assert_eq!(safe_relative_path("\\\\server\\share\\x.stl"), None);
    assert_eq!(safe_relative_path("C:/Windows/evil.stl"), None);
    assert_eq!(safe_relative_path("c:evil.stl"), None);
    assert_eq!(safe_relative_path(""), None);
    assert_eq!(safe_relative_path("a/.../b"), None);
}

#[test]
fn sanitize_component_replaces_invalid_chars_and_reserved_names() {
    assert_eq!(sanitize_component("te:st?.stl"), "te_st_.stl");
    assert_eq!(sanitize_component("name. "), "name");
    assert_eq!(sanitize_component("nul.txt"), "_nul.txt");
    assert_eq!(sanitize_component("Console.stl"), "Console.stl");
    assert_eq!(sanitize_component("tab\there"), "tab_here");
}

#[test]
fn sanitize_component_covers_all_windows_device_names_and_trailing_stem_padding() {
    assert_eq!(sanitize_component("CONIN$"), "_CONIN$");
    assert_eq!(sanitize_component("conout$.txt"), "_conout$.txt");
    assert_eq!(sanitize_component("COM0.stl"), "_COM0.stl");
    assert_eq!(sanitize_component("lpt0"), "_lpt0");
    // Windows ignores trailing spaces/dots of the stem: "CON .txt" is CON.
    assert_eq!(sanitize_component("CON .txt"), "_CON .txt");
    assert_eq!(sanitize_component("aux..stl"), "_aux..stl");
    assert_eq!(sanitize_component("CONSOLE .txt"), "CONSOLE .txt");
}

// ---------- Round trip per format ----------

fn roundtrip(archive_name: &str, build: impl FnOnce(&Path)) {
    let dir = unique_dir("roundtrip");
    let archive = dir.join(archive_name);
    build(&archive);
    let format = detect_format(&archive).unwrap();

    let summary = inspect(&archive, is_model);
    assert_eq!(summary.status, InspectStatus::Ok, "{archive_name}");
    assert_eq!(summary.model_count, 2, "{archive_name}");

    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, format, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.written_files, 3, "{archive_name}");
    assert_eq!(extraction.stripped_root.as_deref(), Some("Benchy"));
    assert_sample_extracted(&dest);
}

#[test]
fn roundtrip_zip() {
    roundtrip("a.zip", |p| make_zip(p, SAMPLE));
}
#[test]
fn roundtrip_tar() {
    roundtrip("a.tar", |p| make_tar(p, ArchiveFormat::Tar, SAMPLE));
}
#[test]
fn roundtrip_tar_gz() {
    roundtrip("a.tar.gz", |p| make_tar(p, ArchiveFormat::TarGz, SAMPLE));
}
#[test]
fn roundtrip_tar_bz2() {
    roundtrip("a.tar.bz2", |p| make_tar(p, ArchiveFormat::TarBz2, SAMPLE));
}
#[test]
fn roundtrip_tar_xz() {
    roundtrip("a.tar.xz", |p| make_tar(p, ArchiveFormat::TarXz, SAMPLE));
}
#[test]
fn roundtrip_tar_zst() {
    roundtrip("a.tar.zst", |p| make_tar(p, ArchiveFormat::TarZst, SAMPLE));
}
// ---------- inspect ----------

#[test]
fn inspect_reports_no_models_unsupported_and_unreadable() {
    let dir = unique_dir("inspect");
    let only_text = dir.join("text.zip");
    make_zip(&only_text, &[("README.txt", b"x")]);
    assert_eq!(inspect(&only_text, is_model).status, InspectStatus::NoModels);

    assert_eq!(inspect(&dir.join("x.stl.gz"), is_model).status, InspectStatus::Unsupported);

    let broken = dir.join("kaputt.zip");
    fs::write(&broken, b"PK\x03\x04 abgeschnitten").unwrap();
    assert_eq!(inspect(&broken, is_model).status, InspectStatus::Unreadable);

}

#[test]
fn inspect_reports_encrypted_zip() {
    let dir = unique_dir("encrypted");
    let zip_path = dir.join("geheim.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    let options = zip::write::SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw");
    zip.start_file("teil.stl", options).unwrap();
    zip.write_all(b"solid").unwrap();
    zip.finish().unwrap();
    assert_eq!(inspect(&zip_path, is_model).status, InspectStatus::Encrypted);
}

#[test]
fn inspect_reports_too_large_for_too_many_entries() {
    let dir = unique_dir("too_many");
    let path = dir.join("viele.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..=MAX_ENTRIES {
        zip.start_file(format!("f{i}.stl"), options).unwrap();
    }
    zip.finish().unwrap();
    assert_eq!(inspect(&path, is_model).status, InspectStatus::TooLarge);
}

// ---------- Safety while extracting ----------

#[test]
fn zip_slip_entries_are_skipped_and_nothing_escapes() {
    let dir = unique_dir("zipslip");
    let archive = dir.join("boese.zip");
    make_zip(
        &archive,
        &[
            ("../evil.stl", b"x"),
            ("/abs.stl", b"x"),
            ("C:/win.stl", b"x"),
            ("ok.stl", b"solid"),
        ],
    );
    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.unsafe_skipped, 3);
    assert_eq!(extraction.stats.written_files, 1);
    assert!(!dir.join("evil.stl").exists());
    assert!(dest.join("ok.stl").exists());
}

/// Written once with the previous ZIP library (default options, as the app
/// always did): the current library must keep reading what older versions made.
#[test]
fn zip_written_by_the_previous_library_still_extracts() {
    let dir = unique_dir("zip_legacy");
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/archive/testdata/archive_zip2.zip");
    assert_eq!(inspect(&archive, is_model).status, InspectStatus::Ok);
    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    // The symlink entry is skipped, the directory entry is not counted as a file.
    assert_eq!(extraction.stats.written_files, 2);
    assert_eq!(extraction.stats.unsafe_skipped, 1);
    assert_eq!(extraction.stripped_root.as_deref(), Some("Gartenhaus"));
    assert_eq!(fs::read(dest.join("readme.txt")).unwrap(), b"hallo");
    assert!(fs::read(dest.join("Größe-Übersicht.stl")).unwrap().starts_with(b"solid x"));
    assert!(fs::symlink_metadata(dest.join("link.stl")).is_err());
}

#[test]
fn zip_unpacked_size_limit_applies_with_a_counter_check() {
    let dir = unique_dir("zip_budget");
    let archive = dir.join("a.zip");
    make_zip(&archive, &[("big.stl", &[b'a'; 100])]);
    let tight = extract_archive(&archive, ArchiveFormat::Zip, &dir.join("eng"), false, 10, &allow_all);
    assert!(matches!(tight, Err(ArchiveError::LimitExceeded)));
    // Counter-check: the same archive passes with enough budget.
    let roomy = extract_archive(&archive, ArchiveFormat::Zip, &dir.join("weit"), false, 1000, &allow_all);
    assert_eq!(roomy.unwrap().stats.written_files, 1);
}

#[test]
fn tar_traversal_entry_is_skipped() {
    let dir = unique_dir("tarslip");
    let archive = dir.join("boese.tar");
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_old();
    // append_data rejects ".." - so write the name raw into the header field.
    let name = b"../evil.stl";
    header.as_old_mut().name[..name.len()].copy_from_slice(name);
    header.set_size(1);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, &b"x"[..]).unwrap();
    fs::write(&archive, builder.into_inner().unwrap()).unwrap();

    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Tar, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.unsafe_skipped, 1);
    assert!(!dir.join("evil.stl").exists());
}

#[test]
fn symlink_entries_are_never_created() {
    let dir = unique_dir("symlink");

    let zip_path = dir.join("link.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.add_symlink("link.stl", "/etc/passwd", options).unwrap();
    zip.finish().unwrap();
    let zip_dest = dir.join("zip_ziel");
    let stats = extract_archive(&zip_path, ArchiveFormat::Zip, &zip_dest, false, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.unsafe_skipped, 1);
    assert!(fs::symlink_metadata(zip_dest.join("link.stl")).is_err());

    let tar_path = dir.join("link.tar");
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    builder.append_link(&mut header, "link.stl", "/etc/passwd").unwrap();
    fs::write(&tar_path, builder.into_inner().unwrap()).unwrap();
    let tar_dest = dir.join("tar_ziel");
    let stats = extract_archive(&tar_path, ArchiveFormat::Tar, &tar_dest, false, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.unsafe_skipped, 1);
    assert!(fs::symlink_metadata(tar_dest.join("link.stl")).is_err());
}

#[cfg(unix)]
#[test]
fn merge_does_not_follow_an_existing_symlinked_subfolder() {
    let dir = unique_dir("merge_symlink");
    let outside = dir.join("draussen");
    fs::create_dir_all(&outside).unwrap();
    let dest = dir.join("ziel");
    fs::create_dir_all(&dest).unwrap();
    std::os::unix::fs::symlink(&outside, dest.join("Benchy")).unwrap();

    let archive = dir.join("a.zip");
    let names: Vec<String> = SAMPLE.iter().map(|(name, _)| format!("Download/{name}")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().zip(SAMPLE).map(|(name, (_, data))| (name.as_str(), *data)).collect();
    make_zip(&archive, &entries);
    let stats = extract_archive(&archive, ArchiveFormat::Zip, &dest, true, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.unsafe_skipped, 3);
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

// ---------- Conflicts ----------

#[test]
fn new_mode_refuses_an_existing_destination() {
    let dir = unique_dir("new_exists");
    let archive = dir.join("a.zip");
    make_zip(&archive, SAMPLE);
    let dest = dir.join("ziel");
    fs::create_dir_all(&dest).unwrap();
    assert!(extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).is_err());
    assert!(dest.exists(), "vorhandener Ordner darf nicht entfernt werden");
}

#[test]
fn merge_keeps_existing_files_byte_identical_and_counts_them() {
    let dir = unique_dir("merge");
    let archive = dir.join("a.zip");
    make_zip(&archive, SAMPLE);
    let dest = dir.join("ziel");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("benchy.stl"), b"MEINE VERSION").unwrap();

    let stats = extract_archive(&archive, ArchiveFormat::Zip, &dest, true, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.existing_skipped, 1);
    assert_eq!(stats.written_files, 2);
    assert_eq!(fs::read(dest.join("benchy.stl")).unwrap(), b"MEINE VERSION");
    assert!(dest.join("teile/rumpf.3mf").exists());
}

// ---------- Byte budget & cleanup ----------

#[test]
fn exceeding_the_byte_budget_aborts_and_removes_a_new_destination() {
    let dir = unique_dir("budget_new");
    let archive = dir.join("a.tar.gz");
    make_tar(&archive, ArchiveFormat::TarGz, SAMPLE);
    let dest = dir.join("ziel");
    let result = extract_archive(&archive, ArchiveFormat::TarGz, &dest, false, 20, &allow_all);
    assert!(matches!(result, Err(ArchiveError::LimitExceeded)));
    assert!(!dest.exists());
}

#[test]
fn exceeding_the_byte_budget_in_merge_mode_restores_the_previous_state() {
    let dir = unique_dir("budget_merge");
    let archive = dir.join("a.zip");
    make_zip(&archive, SAMPLE);
    let dest = dir.join("ziel");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("alt.txt"), b"alt").unwrap();

    let result = extract_archive(&archive, ArchiveFormat::Zip, &dest, true, 20, &allow_all);
    assert!(matches!(result, Err(ArchiveError::LimitExceeded)));
    let remaining: Vec<_> = fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(remaining, vec![std::ffi::OsString::from("alt.txt")]);
}

// ---------- Hardening ----------

#[test]
fn sanitize_component_neutralizes_bidi_and_zero_width_chars_and_superscript_devices() {
    assert_eq!(sanitize_component("rechnung\u{202E}lts.exe"), "rechnung_lts.exe");
    assert_eq!(sanitize_component("a\u{200B}b"), "a_b");
    assert_eq!(sanitize_component("COM\u{b9}.txt"), "_COM\u{b9}.txt");
}

#[test]
fn folder_names_never_start_with_a_dot() {
    assert_eq!(safe_folder_name(".local"), "local");
    assert_eq!(safe_folder_name("..config"), "config");
    assert_eq!(folder_name_for(Path::new("/dl/.local.zip")), "local");
    assert_eq!(folder_name_for(Path::new("/dl/..zip")), "Archiv");
}

#[test]
fn blocked_file_types_are_never_extracted() {
    let dir = unique_dir("blocked");
    let archive = dir.join("a.zip");
    make_zip(
        &archive,
        &[
            ("setup.EXE", b"MZ"),
            ("Drache/desktop.ini", b"[.ShellClassInfo]"),
            ("Drache/Hilfe.url", b"[InternetShortcut]"),
            ("Start.app/Contents/MacOS/start", b"#!"),
            ("run.desktop", b"[Desktop Entry]"),
            ("Drache/drache.stl", b"solid"),
        ],
    );
    let dest = dir.join("ziel");
    let stats = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.blocked_skipped, 5);
    assert_eq!(stats.written_files, 1);
    assert!(dest.join("Drache/drache.stl").exists());
    assert!(!dest.join("setup.EXE").exists());
    assert!(!dest.join("Start.app").exists());
}

#[test]
fn guard_blocks_protected_destinations_and_entries() {
    let dir = unique_dir("guard");
    let archive = dir.join("a.zip");
    make_zip(&archive, &[("geschuetzt/x.stl", b"solid"), ("ok.stl", b"solid")]);
    let deny = |p: &Path| !p.to_string_lossy().contains("geschuetzt");

    let dest = dir.join("ziel");
    let stats = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &deny)
        .unwrap()
        .stats;
    assert_eq!(stats.unsafe_skipped, 1);
    assert!(!dest.join("geschuetzt").exists());
    assert!(dest.join("ok.stl").exists());

    let protected_dest = dir.join("geschuetzt_ziel");
    assert!(extract_archive(&archive, ArchiveFormat::Zip, &protected_dest, false, MAX_UNPACKED_BYTES, &deny).is_err());
    assert!(!protected_dest.exists());
}

#[test]
fn entry_limit_is_enforced_during_extraction_even_without_inspect() {
    let dir = unique_dir("entry_limit");
    let path = dir.join("viele.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..=MAX_ENTRIES {
        zip.start_file(format!("f{i}.txt"), options).unwrap();
    }
    zip.finish().unwrap();
    let dest = dir.join("ziel");
    let result = extract_archive(&path, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all);
    assert!(matches!(result, Err(ArchiveError::TooManyEntries)));
    assert!(!dest.exists());
}

#[test]
fn zip_entries_with_lzma_or_xz_compression_are_unsupported() {
    let dir = unique_dir("zip_xz");
    let path = dir.join("xz.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Xz);
    zip.start_file("teil.stl", options).unwrap();
    zip.write_all(b"solid").unwrap();
    zip.finish().unwrap();
    assert_eq!(inspect(&path, is_model).status, InspectStatus::Unsupported);
}

#[cfg(unix)]
#[test]
fn extracted_files_are_never_executable_and_symlinked_merge_roots_are_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = unique_dir("perms");
    let archive = dir.join("a.tar");
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(5);
    header.set_mode(0o4755);
    header.set_cksum();
    builder.append_data(&mut header, "teil.stl", &b"solid"[..]).unwrap();
    fs::write(&archive, builder.into_inner().unwrap()).unwrap();

    let dest = dir.join("ziel");
    extract_archive(&archive, ArchiveFormat::Tar, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    let mode = fs::metadata(dest.join("teil.stl")).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode & 0o111, 0, "keine Ausfuehrungsrechte");
    assert_eq!(mode & 0o7000, 0, "kein setuid/setgid/sticky");

    let elsewhere = dir.join("woanders");
    fs::create_dir_all(&elsewhere).unwrap();
    let link = dir.join("link_ziel");
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    assert!(extract_archive(&archive, ArchiveFormat::Tar, &link, true, MAX_UNPACKED_BYTES, &allow_all).is_err());
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn rollback_removes_everything_a_successful_extraction_created() {
    let dir = unique_dir("rollback");
    let archive = dir.join("a.zip");
    make_zip(&archive, SAMPLE);
    let dest = dir.join("ziel");
    let extraction =
        extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    extraction.rollback();
    assert!(!dest.exists());
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/archives")
        .join(name)
}

fn make_7z(path: &Path, entries: &[(&str, &[u8])], password: Option<&str>) {
    let src = unique_dir("7z_src");
    for (name, data) in entries {
        let file = src.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, data).unwrap();
    }
    match password {
        None => sevenz_rust2::compress_to_path(&src, path).unwrap(),
        Some(pw) => sevenz_rust2::compress_to_path_encrypted(&src, path, pw.into()).unwrap(),
    }
}

#[test]
fn roundtrip_7z() {
    roundtrip("a.7z", |p| make_7z(p, SAMPLE, None));
}

#[test]
fn rar4_and_rar5_fixtures_extract() {
    let dir = unique_dir("rar");
    let dest4 = dir.join("rar4");
    extract_archive(&fixture("rar4-plain.rar"), ArchiveFormat::Rar, &dest4, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(fs::read_to_string(dest4.join("VERSION")).unwrap().len(), 11);

    let dest5 = dir.join("rar5");
    let extraction =
        extract_archive(&fixture("rar5-solid.rar"), ArchiveFormat::Rar, &dest5, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.written_files, 1);
    assert_eq!(fs::metadata(dest5.join(".gitignore")).unwrap().len(), 18);
}

#[test]
fn skipped_entries_in_a_solid_7z_do_not_corrupt_later_entries() {
    let dir = unique_dir("solid_skip");
    let archive = dir.join("a.7z");
    make_7z(&archive, SAMPLE, None);
    let dest = dir.join("ziel");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("README.txt"), b"vorher").unwrap();
    let stats = extract_archive(&archive, ArchiveFormat::SevenZ, &dest, true, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.existing_skipped, 1);
    assert_eq!(fs::read(dest.join("benchy.stl")).unwrap(), SAMPLE[0].1);
    assert_eq!(fs::read(dest.join("teile/rumpf.3mf")).unwrap(), SAMPLE[2].1);
}

#[test]
fn inspect_reports_encrypted_7z_and_rar_and_unsupported_multipart_rar() {
    let dir = unique_dir("encrypted_7z_rar");
    let sevenz_path = dir.join("geheim.7z");
    make_7z(&sevenz_path, SAMPLE, Some("pw"));
    assert_eq!(inspect(&sevenz_path, is_model).status, InspectStatus::Encrypted);

    assert_eq!(inspect(&fixture("rar4-encrypted.rar"), is_model).status, InspectStatus::Encrypted);
    assert_eq!(
        inspect(&fixture("rar5-encrypted-headers.rar"), is_model).status,
        InspectStatus::Encrypted
    );
    assert_eq!(inspect(&fixture("multi.part1.rar"), is_model).status, InspectStatus::Unsupported);
}

#[test]
fn rar_budget_check_prevents_oversized_entry() {
    let dir = unique_dir("rar_budget");
    let dest = dir.join("ziel");
    // rar5-solid.rar contains a .gitignore (18 bytes), but budget is only 10 bytes
    let result = extract_archive(&fixture("rar5-solid.rar"), ArchiveFormat::Rar, &dest, false, 10, &allow_all);
    assert!(matches!(result, Err(ArchiveError::LimitExceeded)));
    assert!(!dest.exists(), "Destination should not be created on budget exceeded");
}

#[test]
fn strips_single_root_in_zip_and_all_tar_formats() {
    for format in [ArchiveFormat::Zip, ArchiveFormat::Tar, ArchiveFormat::TarGz,
        ArchiveFormat::TarBz2, ArchiveFormat::TarXz, ArchiveFormat::TarZst] {
        let dir = unique_dir("single_root");
        let archive = dir.join("Anderer-Name.archive");
        let entries: &[(&str, &[u8])] = &[("Garten-Paket/a.3mf", b"a"), ("Garten-Paket/sub/b.stl", b"b")];
        if format == ArchiveFormat::Zip { make_zip(&archive, entries); }
        else { make_tar(&archive, format, entries); }
        let dest = dir.join("ziel");
        let extraction = extract_archive(&archive, format, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
        assert_eq!(extraction.stripped_root.as_deref(), Some("Garten-Paket"));
        assert_eq!(fs::read(dest.join("a.3mf")).unwrap(), b"a");
        assert_eq!(fs::read(dest.join("sub/b.stl")).unwrap(), b"b");
        assert!(!dest.join("Garten-Paket").exists());
        extraction.rollback();
        assert!(!dest.exists());
    }
}

#[test]
fn root_detection_ignores_platform_metadata() {
    let dir = unique_dir("root_metadata");
    let archive = dir.join("a.zip");
    make_zip(&archive, &[("__MACOSX/._a", b"meta"), (".DS_Store", b"meta"),
        ("Thumbs.db", b"meta"), ("desktop.ini", b"meta"), ("Garten-Paket/a.3mf", b"a")]);
    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(fs::read(dest.join("a.3mf")).unwrap(), b"a");
    assert!(!dest.join("Garten-Paket").exists());
    assert_eq!(extraction.stats.blocked_skipped, 1);
}

#[test]
fn preserves_multiple_roots_and_top_level_files() {
    for entries in [vec![("A/a.3mf", b"a".as_slice()), ("B/b.stl", b"b".as_slice())],
        vec![("a.3mf", b"a".as_slice()), ("B/b.stl", b"b".as_slice())],
        vec![("a.3mf", b"a".as_slice())]] {
        let dir = unique_dir("multiple_roots");
        let archive = dir.join("a.zip");
        make_zip(&archive, &entries);
        let dest = dir.join("ziel");
        extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
        for (name, bytes) in entries { assert_eq!(fs::read(dest.join(name)).unwrap(), bytes); }
    }
}

#[test]
fn stripped_paths_still_reject_traversal_absolute_drives_and_executables() {
    let dir = unique_dir("strip_security");
    let archive = dir.join("a.zip");
    make_zip(&archive, &[("Garten-Paket/../x", b"bad"), ("Garten-Paket//absolute.stl", b"bad"),
        ("Garten-Paket/C:/drive.stl", b"bad"), ("Garten-Paket/run.exe", b"bad"), ("Garten-Paket/a.3mf", b"a")]);
    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.unsafe_skipped, 3);
    assert_eq!(extraction.stats.blocked_skipped, 1);
    assert_eq!(fs::read(dest.join("a.3mf")).unwrap(), b"a");
    assert!(!dir.join("x").exists());
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
}

#[test]
fn explicit_root_directory_and_windows_separators_are_stripped() {
    let dir = unique_dir("explicit_root");
    let archive = dir.join("a.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    zip.add_directory("Garten-Paket/", zip::write::SimpleFileOptions::default()).unwrap();
    zip.start_file("Garten-Paket\\sub\\a.stl", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(b"a").unwrap();
    zip.finish().unwrap();
    let dest = dir.join("ziel");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stripped_root.as_deref(), Some("Garten-Paket"));
    assert_eq!(extraction.stats.unsafe_skipped, 0);
    assert_eq!(fs::read(dest.join("sub/a.stl")).unwrap(), b"a");
    assert!(!dest.join("Garten-Paket").exists());
}

#[test]
fn stripping_does_not_unblock_executable_bundles_or_protected_targets() {
    let dir = unique_dir("root_guards");
    let archive = dir.join("a.zip");
    make_zip(&archive, &[("Start.app/a.stl", b"bad")]);
    let dest = dir.join("blocked");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.blocked_skipped, 1);
    assert_eq!(extraction.stats.written_files, 0);

    make_zip(&archive, &[("Garten-Paket/geschuetzt/a.stl", b"bad"), ("Garten-Paket/a.stl", b"a")]);
    let dest = dir.join("protected");
    let guard = |path: &Path| !path.components().any(|c| c.as_os_str() == "geschuetzt");
    let extraction = extract_archive(&archive, ArchiveFormat::Zip, &dest, false, MAX_UNPACKED_BYTES, &guard).unwrap();
    assert_eq!(extraction.stats.unsafe_skipped, 1);
    assert_eq!(extraction.stats.written_files, 1);
    assert!(!dest.join("geschuetzt").exists());
}

// ---------- Restrictions for server use (ExtractOptions) ----------

/// One SOLID 7z block, unlike `make_7z`, which writes one block per file.
fn make_solid_7z(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    let headers = entries
        .iter()
        .map(|(name, _)| sevenz_rust2::ArchiveEntry::new_file(name))
        .collect();
    let readers = entries
        .iter()
        .map(|(_, data)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(data.to_vec())))
        .collect();
    writer.push_archive_entries(headers, readers).unwrap();
    writer.finish().unwrap();
}

const MIXED: &[(&str, &[u8])] = &[
    ("a.stl", b"solid a\nendsolid a\n"),
    ("bild.png", b"\x89PNG not really a picture but long enough to matter"),
    ("sub/c.3mf", b"PK\x03\x04 pretend 3mf"),
];

fn only_models(path: &Path) -> bool {
    is_model(path)
}

#[test]
fn keep_filter_skips_payload_in_zip_and_counts_it() {
    let dir = unique_dir("keep_zip");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    let dest = dir.join("ziel");
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.keep = Some(&only_models);
    let extraction = extract_archive_with(&archive, ArchiveFormat::Zip, &dest, false, &options, &allow_all).unwrap();
    assert_eq!(extraction.stats.written_files, 2);
    assert_eq!(extraction.stats.filtered_skipped, 1);
    assert!(!dest.join("bild.png").exists());
    assert_eq!(fs::read(dest.join("sub/c.3mf")).unwrap(), MIXED[2].1);
}

#[test]
fn keep_filter_in_a_solid_7z_block_does_not_shift_later_entries() {
    let dir = unique_dir("keep_solid_7z");
    let archive = dir.join("a.7z");
    make_solid_7z(&archive, MIXED);
    let dest = dir.join("ziel");
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.keep = Some(&only_models);
    let extraction = extract_archive_with(&archive, ArchiveFormat::SevenZ, &dest, false, &options, &allow_all).unwrap();
    assert_eq!(extraction.stats.filtered_skipped, 1);
    assert_eq!(fs::read(dest.join("a.stl")).unwrap(), MIXED[0].1);
    // The PNG sits between the models inside the one compressed block.
    assert_eq!(fs::read(dest.join("sub/c.3mf")).unwrap(), MIXED[2].1);
}

#[test]
fn skipped_blocked_entry_in_a_solid_7z_does_not_shift_later_entries() {
    let dir = unique_dir("blocked_solid_7z");
    let archive = dir.join("a.7z");
    make_solid_7z(
        &archive,
        &[("x.stl", b"solid x\nendsolid x\n"), ("evil.exe", b"MZ0123456789"), ("y.stl", b"solid y\nendsolid y\n")],
    );
    let dest = dir.join("ziel");
    let extraction =
        extract_archive(&archive, ArchiveFormat::SevenZ, &dest, false, MAX_UNPACKED_BYTES, &allow_all).unwrap();
    assert_eq!(extraction.stats.blocked_skipped, 1);
    assert_eq!(fs::read(dest.join("y.stl")).unwrap(), b"solid y\nendsolid y\n");
}

#[test]
fn inspect_reports_the_size_of_the_model_entries() {
    let dir = unique_dir("model_bytes");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    let summary = inspect(&archive, is_model);
    assert_eq!(summary.model_count, 2);
    assert_eq!(summary.model_bytes, (MIXED[0].1.len() + MIXED[2].1.len()) as u64);
    assert_eq!(summary.unpacked_size, MIXED.iter().map(|(_, d)| d.len() as u64).sum::<u64>());
}

#[test]
fn sevenz_decoder_memory_bound_is_enforced_before_anything_is_written() {
    let dir = unique_dir("sevenz_memory");
    let archive = dir.join("a.7z");
    make_7z(&archive, SAMPLE, None);
    // The default writer declares a multi-MiB LZMA2 dictionary.
    assert_eq!(inspect_with(&archive, is_model, Some(64 * 1024)).status, InspectStatus::MemoryLimit);
    assert_eq!(inspect_with(&archive, is_model, Some(256 * 1024 * 1024)).status, InspectStatus::Ok);
    let dest = dir.join("ziel");
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.sevenz_memory_limit = Some(64 * 1024);
    let result = extract_archive_with(&archive, ArchiveFormat::SevenZ, &dest, false, &options, &allow_all);
    assert!(matches!(result, Err(ArchiveError::MemoryLimit)));
    assert!(!dest.exists());
}

#[test]
fn sevenz_decoder_memory_adds_up_over_a_coder_chain() {
    use sevenz_rust2::encoder_options::{Lzma2Options, LzmaOptions};
    use sevenz_rust2::EncoderConfiguration;
    let dir = unique_dir("sevenz_chain");
    let archive = dir.join("a.7z");
    let mut lzma2 = Lzma2Options::from_level(1);
    lzma2.set_dictionary_size(4 * 1024 * 1024);
    let mut lzma = LzmaOptions::from_level(1);
    lzma.set_dictionary_size(4 * 1024 * 1024);
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
    writer.set_content_methods(vec![
        EncoderConfiguration::from(lzma2),
        EncoderConfiguration::from(lzma),
    ]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("a.stl"),
            Some(std::io::Cursor::new(b"solid a\nendsolid a\n".to_vec())),
        )
        .unwrap();
    writer.finish().unwrap();
    // Each coder alone reserves 4 MiB; both run at once while the block is decoded.
    assert_eq!(inspect_with(&archive, is_model, Some(6 * 1024 * 1024)).status, InspectStatus::MemoryLimit);
    assert_eq!(inspect_with(&archive, is_model, Some(9 * 1024 * 1024)).status, InspectStatus::Ok);
}

#[test]
fn an_expired_deadline_stops_extraction_and_cleans_up() {
    let dir = unique_dir("deadline");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    let dest = dir.join("ziel");
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.deadline = Some(std::time::Instant::now());
    let result = extract_archive_with(&archive, ArchiveFormat::Zip, &dest, false, &options, &allow_all);
    assert!(matches!(result, Err(ArchiveError::TimedOut)));
    assert!(!dest.exists());
}

#[test]
fn restricted_callers_refuse_a_7z_header_that_could_hold_millions_of_entries() {
    let dir = unique_dir("sevenz_header");
    let archive = dir.join("viele.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
    // 4,000 empty files with 400-character names: far below the entry limit, but a
    // header of about 3 MiB once decoded.
    for i in 0..4_000 {
        let name = format!("{}-{i:06}.stl", "n".repeat(390));
        writer
            .push_archive_entry::<&[u8]>(sevenz_rust2::ArchiveEntry::new_file(&name), None)
            .unwrap();
    }
    writer.finish().unwrap();
    assert_eq!(inspect_with(&archive, is_model, Some(256 * 1024 * 1024)).status, InspectStatus::TooLarge);
    // Without a bound (desktop) the header is still accepted up to its own limit.
    assert_eq!(inspect(&archive, is_model).status, InspectStatus::Ok);
}

#[test]
fn a_raised_cancel_flag_stops_before_anything_is_created() {
    let dir = unique_dir("cancel_before");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    let dest = dir.join("ziel");
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.cancel = Some(&cancel);
    let result = extract_archive_with(&archive, ArchiveFormat::Zip, &dest, false, &options, &allow_all);
    assert!(matches!(result, Err(ArchiveError::Cancelled)));
    assert!(!dest.exists());
}

#[test]
fn cancelling_in_the_middle_removes_what_was_already_written() {
    let dir = unique_dir("cancel_middle");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    let dest = dir.join("ziel");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    // The filter runs for every file entry; raising the flag on the second one
    // happens after the first file was written completely.
    let calls = std::cell::Cell::new(0);
    let keep = |_: &Path| {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            cancel.store(true, std::sync::atomic::Ordering::Release);
        }
        true
    };
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.cancel = Some(&cancel);
    options.keep = Some(&keep);
    let result = extract_archive_with(&archive, ArchiveFormat::Zip, &dest, false, &options, &allow_all);
    assert!(matches!(result, Err(ArchiveError::Cancelled)));
    assert_eq!(calls.get(), 2);
    assert!(!dest.exists());
}

#[test]
fn zip_with_more_declared_entries_than_allowed_is_refused_from_the_end_record() {
    let dir = unique_dir("zip_declared");
    let archive = dir.join("a.zip");
    make_zip(&archive, MIXED);
    // Forge the entry count in the end record; the real directory stays tiny, so
    // only the pre-check can be what refuses it before the zip library parses.
    let mut bytes = fs::read(&archive).unwrap();
    let eocd = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
    bytes[eocd + 10..eocd + 12].copy_from_slice(&60_000u16.to_le_bytes());
    fs::write(&archive, bytes).unwrap();
    assert!(matches!(list_entries(&archive, ArchiveFormat::Zip), Err(ArchiveError::TooManyEntries)));
}

// ---------- Memory peak of 7z decoding ----------

#[cfg(target_os = "linux")]
fn vm_hwm_kb() -> u64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse().ok())
        .unwrap()
}

/// Runs in a process of its own (see the test below): the peak resident size of a
/// process only ever grows, so inside the shared test process earlier tests would
/// hide what this extraction costs.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "child of sevenz_extraction_does_not_multiply_memory_with_worker_threads"]
fn rss_child_extract_7z() {
    let archive = PathBuf::from(std::env::var("KATALOG_RSS_ARCHIVE").unwrap());
    let dest = PathBuf::from(std::env::var("KATALOG_RSS_DEST").unwrap());
    let before = vm_hwm_kb();
    let mut options = ExtractOptions::new(MAX_UNPACKED_BYTES);
    options.sevenz_memory_limit = Some(256 * 1024 * 1024);
    extract_archive_with(&archive, ArchiveFormat::SevenZ, &dest, false, &options, &allow_all).unwrap();
    println!("RSS_DELTA_KB={}", vm_hwm_kb().saturating_sub(before));
}

/// 256 MiB of zeros in a few KiB of 7z, compressed as independent LZMA2 packets:
/// the shape for which the library starts one decoding worker per CPU, each
/// holding its packet fully decoded in memory.
#[cfg(target_os = "linux")]
#[test]
fn sevenz_extraction_does_not_multiply_memory_with_worker_threads() {
    use std::io::Read;
    let dir = unique_dir("rss");
    let archive = dir.join("nullen.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
    writer.set_content_methods(vec![sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(6, 4, 64 * 1024 * 1024).into()]);
    let zeros = std::io::repeat(0).take(256 * 1024 * 1024);
    writer
        .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file("nullen.bin"), Some(zeros))
        .unwrap();
    writer.finish().unwrap();
    let dest = dir.join("ziel");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "archive::tests::rss_child_extract_7z", "--ignored", "--nocapture", "--test-threads=1"])
        .env("KATALOG_RSS_ARCHIVE", &archive)
        .env("KATALOG_RSS_DEST", &dest)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}{}", String::from_utf8_lossy(&output.stderr));
    // The harness prints "test <name> ... " in front of the child's own output.
    let delta: u64 = text
        .split("RSS_DELTA_KB=")
        .nth(1)
        .expect("child reports its peak")
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    println!("7z extraction peak growth: {} MiB", delta / 1024);
    assert_eq!(fs::metadata(dest.join("nullen.bin")).unwrap().len(), 256 * 1024 * 1024);
    assert!(delta < 64 * 1024, "peak resident memory grew by {} MiB", delta / 1024);
}
