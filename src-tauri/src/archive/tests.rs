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
        assert_eq!(fs::read(dest.join(name)).unwrap(), *data, "{name}");
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
    make_zip(&archive, SAMPLE);
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
    fs::create_dir_all(dest.join("Benchy")).unwrap();
    fs::write(dest.join("Benchy/benchy.stl"), b"MEINE VERSION").unwrap();

    let stats = extract_archive(&archive, ArchiveFormat::Zip, &dest, true, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.existing_skipped, 1);
    assert_eq!(stats.written_files, 2);
    assert_eq!(fs::read(dest.join("Benchy/benchy.stl")).unwrap(), b"MEINE VERSION");
    assert!(dest.join("Benchy/teile/rumpf.3mf").exists());
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
    fs::create_dir_all(dest.join("Benchy")).unwrap();
    fs::write(dest.join("Benchy/README.txt"), b"vorher").unwrap();
    let stats = extract_archive(&archive, ArchiveFormat::SevenZ, &dest, true, MAX_UNPACKED_BYTES, &allow_all)
        .unwrap()
        .stats;
    assert_eq!(stats.existing_skipped, 1);
    assert_eq!(fs::read(dest.join("Benchy/benchy.stl")).unwrap(), SAMPLE[0].1);
    assert_eq!(fs::read(dest.join("Benchy/teile/rumpf.3mf")).unwrap(), SAMPLE[2].1);
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
