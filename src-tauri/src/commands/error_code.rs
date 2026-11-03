//! The registry of error codes lives in the core so the web server can name the
//! same codes; this module only keeps the old path alive for the desktop callers.
//! The register tests and the free-text scan are in `katalog_core::error_code`.

pub use katalog_core::error_code::*;
