//! Measures the import of 200 synthetic 3MF files (see `threemf_gen`), to compare the speed of
//! one build against another. Only `import_many_with_conn` is used, so the file compiles on the
//! base commit as well. Parameters by environment: `MI_KIND` (`sliced` | `nodata`), `MI_TRIS`
//! (triangles per model), `MI_FILES`, `MI_RUNS`.
//!
//! ```text
//! MI_KIND=sliced cargo test --no-default-features --locked -p mf-katalog-manager \
//!   import_speed -- --ignored --nocapture
//! ```
use super::threemf_gen::{self as gen, Kind};
use super::*;
use std::time::Instant;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
#[ignore = "measurement: see the module comment"]
fn import_speed_of_synthetic_3mf_files() {
    let kind = match std::env::var("MI_KIND").as_deref() {
        Ok("nodata") => Kind::NoPrintData,
        _ => Kind::Sliced,
    };
    let (files, triangles, runs) = (env_usize("MI_FILES", 200), env_usize("MI_TRIS", 3000), env_usize("MI_RUNS", 4));
    let dir = unique_test_dir("import_speed");
    let files_dir = dir.join("files");
    std::fs::create_dir_all(&files_dir).unwrap();
    for i in 0..files {
        gen::write_file(&files_dir.join(format!("m{i:04}.3mf")), kind, i, triangles);
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    eprintln!("IMPORT_SPEED kind={kind:?} files={files} tris={triangles} load_at_start={}", load.trim());
    // The first run warms the file cache and is not counted.
    for run in 0..=runs {
        // Parsing alone (hash is not part of the import's parse step), to split the import time.
        let parse_started = Instant::now();
        for i in 0..files {
            katalog_core::threemf::parse_3mf_file(&files_dir.join(format!("m{i:04}.3mf"))).expect("parse");
        }
        let parse_elapsed = parse_started.elapsed();
        let mut conn = crate::db::connect(&dir.join(format!("run{run}.db"))).expect("connect");
        let started = Instant::now();
        let result = import_many_with_conn(&mut conn, vec![files_dir.clone()]).expect("import");
        let elapsed = started.elapsed();
        assert_eq!(result.imported.len(), files, "skipped: {:?}", result.skipped.len());
        let label = if run == 0 { "warmup".to_string() } else { format!("run{run}") };
        eprintln!("IMPORT_SPEED {label} {:.1} ms", elapsed.as_secs_f64() * 1000.0);
        eprintln!("PARSE_ONLY {label} {:.1} ms", parse_elapsed.as_secs_f64() * 1000.0);
    }
}
