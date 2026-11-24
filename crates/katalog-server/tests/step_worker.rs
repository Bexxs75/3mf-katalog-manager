//! The STEP worker as a real process: the server binary started as `step-worker`, the way the
//! server starts it. Misbehaving workers use the self-test modes, which run inside the same
//! sandbox as a real parse.
#![cfg(target_os = "linux")]

use katalog_server::step_worker::{Limits, Mode, StepWorker, WorkerError, SELFTEST_ENV, SELFTEST_PATH_ENV};
use std::fs::File;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_katalog-server"))
}

fn worker(limits: Limits) -> StepWorker {
    StepWorker::with_exe(exe(), limits, 2)
}

fn quick() -> Limits {
    Limits { address_space_mib: 1024, cpu_seconds: 2, wall: Duration::from_secs(20), max_output_bytes: 1024 * 1024 }
}

fn selftest(worker: &StepWorker, mode: &str, input: Option<File>) -> Result<Vec<u8>, WorkerError> {
    worker.run_raw(mode, input, &[(SELFTEST_ENV, "1")])
}

fn temp_file(name: &str, content: &[u8]) -> (PathBuf, File) {
    let dir = std::env::temp_dir().join(format!("step-worker-it-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    let file = File::open(&path).unwrap();
    (dir, file)
}

#[test]
fn the_probe_finds_a_working_sandbox() {
    // Network, signals to the server, writing files and starting programs are refused, threads work.
    assert_eq!(worker(quick()).run_raw("probe", None, &[]), Ok(Vec::new()));
}

#[test]
fn the_worker_reads_its_input_through_the_descriptor_inside_the_sandbox() {
    let (dir, file) = temp_file("echo.step", b"ISO-10303-21;\n");
    assert_eq!(selftest(&worker(quick()), "selftest-echo", Some(file)), Ok(b"ISO-10303-21;\n".to_vec()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_endless_loop_is_stopped_by_the_cpu_limit() {
    let started = Instant::now();
    assert_eq!(selftest(&worker(quick()), "selftest-spin", None), Err(WorkerError::TooComplex));
    // Two CPU seconds plus the hard limit's one, far below the 20-second wall deadline.
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
}

#[test]
fn a_worker_that_blocks_is_killed_at_the_wall_deadline() {
    let limits = Limits { wall: Duration::from_secs(1), ..quick() };
    let started = Instant::now();
    assert_eq!(selftest(&worker(limits), "selftest-sleep", None), Err(WorkerError::TooComplex));
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}

#[test]
fn an_abort_is_a_coded_failure_and_the_caller_lives_on() {
    // A crash is not a verdict on the file: the caller must not treat it as "unreadable".
    assert_eq!(selftest(&worker(quick()), "selftest-abort", None), Err(WorkerError::Crashed));
    assert_eq!(selftest(&worker(quick()), "selftest-panic", None), Err(WorkerError::Crashed));
    // The same worker object keeps working afterwards.
    assert_eq!(worker(quick()).run_raw("probe", None, &[]), Ok(Vec::new()));
}

#[test]
fn files_the_server_can_read_stay_closed_to_the_worker() {
    // Stands for the catalog database and the password file: same owner, readable for the
    // server, outside every rule of the worker.
    let (dir, _) = temp_file("web-auth.json", b"{\"secret\":true}");
    let secret = dir.join("web-auth.json");
    assert!(std::fs::read(&secret).is_ok());
    let read = |path: &std::path::Path| {
        worker(quick()).run_raw("selftest-read", None, &[(SELFTEST_ENV, "1"), (SELFTEST_PATH_ENV, path.to_str().unwrap())])
    };
    assert_eq!(read(&secret), Err(WorkerError::Unreadable));
    // Other processes' /proc entries too (here the server's own environment).
    let parent = PathBuf::from(format!("/proc/{}/environ", std::process::id()));
    assert_eq!(read(&parent), Err(WorkerError::Unreadable));
    // What the runtime needs stays readable, so the rules are a real allow-list.
    if std::path::Path::new("/etc/ld.so.cache").exists() {
        assert!(read(std::path::Path::new("/etc/ld.so.cache")).is_ok());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn other_processes_are_out_of_the_workers_reach() {
    // A separate process of the same user stands for the server; its limits must stay as they are.
    let mut target = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    let pid = target.id().to_string();
    let report = worker(quick())
        .run_raw("selftest-foreign", None, &[(SELFTEST_ENV, "1"), (SELFTEST_PATH_ENV, pid.as_str())])
        .map(|r| String::from_utf8(r).unwrap());
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap();
    let alive = target.try_wait().unwrap().is_none();
    let _ = target.kill();
    let _ = target.wait();
    let report = report.unwrap();
    for line in report.lines() {
        let (name, errno) = line.split_once('=').unwrap();
        assert_eq!(errno, libc::EPERM.to_string(), "{name}: {report}");
    }
    assert!(report.lines().count() >= 15, "{report}");
    assert!(alive);
    let open_files = limits.lines().find(|l| l.starts_with("Max open files")).unwrap();
    assert!(!open_files.split_whitespace().any(|w| w == "0"), "{open_files}");
}

#[test]
fn the_worker_cannot_change_the_file_it_reads() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, file) = temp_file("victim.step", b"ISO-10303-21;\n");
    let path = dir.join("victim.step");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let before = std::fs::metadata(&path).unwrap();
    let report = selftest(&worker(quick()), "selftest-mutate", Some(file)).unwrap();
    let report = String::from_utf8(report).unwrap();
    let errno = |name: &str| {
        report
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("{name} missing in {report}"))
            .to_string()
    };
    let eperm = libc::EPERM.to_string();
    // Refused by the system-call filter (EPERM) and not merely by file permissions, so the
    // check holds even where Landlock has no truncate right.
    for name in [
        "open_rdonly_trunc", "openat_rdonly_trunc", "fchmod", "fchmodat", "fchown", "futimens", "utimensat", "ftruncate",
        "ioctl_setflags", "ioctl_fssetxattr",
    ] {
        assert_eq!(errno(name), eperm, "{name}: {report}");
    }
    let fchmodat2 = errno("fchmodat2");
    assert!(fchmodat2 == eperm || fchmodat2 == libc::ENOSYS.to_string(), "{report}");
    let after = std::fs::metadata(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"ISO-10303-21;\n");
    assert_eq!(after.permissions().mode() & 0o7777, 0o644);
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn output_over_the_cap_is_refused() {
    let limits = Limits { max_output_bytes: 64 * 1024, ..quick() };
    assert_eq!(selftest(&worker(limits), "selftest-flood", None), Err(WorkerError::TooComplex));
}

#[test]
fn memory_beyond_the_address_space_limit_is_refused() {
    let limits = Limits { address_space_mib: 256, ..quick() };
    assert_eq!(selftest(&worker(limits), "selftest-alloc", None), Err(WorkerError::TooComplex));
}

#[test]
fn the_server_environment_does_not_reach_the_worker() {
    // The switch set in this process must not unlock the test modes for a worker started by
    // the server, which clears the environment.
    unsafe { std::env::set_var(SELFTEST_ENV, "1") };
    assert_eq!(worker(quick()).run_raw("selftest-spin", None, &[]), Err(WorkerError::Unavailable));
}

#[test]
fn a_file_over_the_size_limit_never_reaches_the_parser() {
    let (dir, _) = temp_file("big.step", b"");
    std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join("big.step"))
        .unwrap()
        .set_len(katalog_server::step_worker::MAX_INPUT_BYTES + 1)
        .unwrap();
    let file = File::open(dir.join("big.step")).unwrap();
    assert_eq!(worker(quick()).run(Mode::Geometry, &file, Duration::from_secs(5)), Err(WorkerError::TooLarge));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(feature = "step-preview"))]
#[test]
fn without_occt_the_worker_reports_unavailable() {
    let (dir, file) = temp_file("cube.step", b"ISO-10303-21;\n");
    assert_eq!(worker(quick()).run(Mode::Geometry, &file, Duration::from_secs(5)), Err(WorkerError::Unavailable));
    assert_eq!(worker(quick()).run(Mode::Metadata, &file, Duration::from_secs(5)), Err(WorkerError::Unavailable));
    // And the server does not offer a preview it cannot make.
    assert!(!StepWorker::start(exe(), quick()).available());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(feature = "step-preview")]
mod with_occt {
    use super::*;

    fn fixture(name: &str) -> File {
        File::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../katalog-core/tests/fixtures").join(name)).unwrap()
    }

    fn real() -> Limits {
        Limits::default()
    }

    #[test]
    fn the_server_offers_the_preview_when_the_probe_passes() {
        assert!(StepWorker::start(exe(), real()).available());
    }

    #[test]
    fn a_cube_becomes_one_mesh_in_the_viewer_format() {
        let bytes = worker(real()).run(Mode::Geometry, &fixture("wuerfel-10mm.step"), Duration::from_secs(60)).unwrap();
        let header_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let header: serde_json::Value = serde_json::from_slice(&bytes[4..4 + header_len]).unwrap();
        assert_eq!(header["meshes"].as_array().unwrap().len(), 1, "{header}");
        // A box has 12 triangles, non-indexed.
        assert_eq!(header["meshes"][0]["indexCount"], 36, "{header}");
    }

    #[test]
    fn two_bodies_stay_two_meshes() {
        let bytes = worker(real()).run(Mode::Geometry, &fixture("zwei-koerper.step"), Duration::from_secs(60)).unwrap();
        let header_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let header: serde_json::Value = serde_json::from_slice(&bytes[4..4 + header_len]).unwrap();
        assert_eq!(header["meshes"].as_array().unwrap().len(), 2, "{header}");
    }

    #[test]
    fn metadata_of_the_cube_is_ten_millimetres() {
        let bytes = worker(real()).run(Mode::Metadata, &fixture("wuerfel-10mm.step"), Duration::from_secs(60)).unwrap();
        let meta: katalog_server::step_worker::StepMetadata = serde_json::from_slice(&bytes).unwrap();
        let size = meta.dimensions_mm.unwrap();
        for side in size {
            assert!((side - 10.0).abs() < 1e-6, "{size:?}");
        }
        assert_eq!(meta.object_count, Some(1));
    }

    #[test]
    fn a_file_that_is_not_step_is_a_coded_failure() {
        let (dir, file) = temp_file("noise.step", b"this is not a STEP file at all\n");
        let result = worker(real()).run(Mode::Geometry, &file, Duration::from_secs(60));
        assert!(matches!(result, Err(WorkerError::Unreadable | WorkerError::NoGeometry)), "{result:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_dense_sphere_fits_the_limits_or_fails_coded() {
        let started = Instant::now();
        let result = worker(real()).run(Mode::Geometry, &fixture("sphere-1e6.step"), Duration::from_secs(90));
        eprintln!("sphere-1e6: {:?} after {:?}", result.as_ref().map(Vec::len), started.elapsed());
        assert!(matches!(result, Ok(_) | Err(WorkerError::TooComplex)), "{result:?}");
    }
}
