pub mod db;
pub mod threemf;
pub mod stl;
pub mod obj;
#[cfg(feature = "step-preview")]
pub mod step;
pub mod archive;
pub mod geometry;
pub mod safe_file;
pub mod tagging;
pub mod filament_check;
pub mod printer_link;
pub mod backup;

#[cfg(test)]
pub(crate) fn unique_test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{name}_{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create test dir");
    dir
}

pub mod read;

pub mod write;

pub mod import;

#[cfg(unix)]
pub mod upload_fs;

#[cfg(target_os = "linux")]
pub mod fsops;
