//! Creates an empty catalog database with the current schema (for the web image smoke test).
//!   cargo run -p katalog-core --no-default-features --example make_smoke_catalog -- <path/catalog.db>
//! The web server never migrates, so the file must match the schema of this build exactly.

use std::path::Path;

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("usage: make_smoke_catalog <path/catalog.db>");
        std::process::exit(2);
    };
    let path = Path::new(&arg);
    if path.exists() {
        eprintln!("refusing to overwrite {}", path.display());
        std::process::exit(1);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).expect("cannot create the target directory");
    }
    // connect() creates the file, applies the base schema and runs all migrations.
    let conn = katalog_core::db::connect(path).expect("cannot create the catalog");
    drop(conn);
    println!("created {}", path.display());
}
