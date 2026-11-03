//! Tests of the portable module. They use nothing but this module, so they can run on every host.
use super::names::{rename_name, validate_name};
use super::recovery::{recovery_decision, RecoveryDecision, RecoveryFacts};
use super::{nfc, Error};

#[test]
fn nfc_composes_decomposed_text() {
    assert_eq!(nfc("e\u{301}"), "\u{e9}");
    assert_eq!(nfc("plain"), "plain");
}

#[test]
fn validate_name_accepts_and_rejects_like_the_linux_module() {
    assert_eq!(validate_name("model.3mf").unwrap(), "model.3mf");
    assert_eq!(validate_name("e\u{301}.3mf").unwrap(), "\u{e9}.3mf");
    for bad in ["", ".a", " a", "a.", "a ", "a/b", "a\\b", "a:b", "CON", "con.txt", "COM1", "LPT\u{b9}.x", "a\u{200b}b"] {
        assert_eq!(validate_name(bad), Err(Error::InvalidName), "{bad:?}");
    }
    assert_eq!(validate_name(&"a".repeat(256)), Err(Error::InvalidName));
    assert!(validate_name(&"a".repeat(255)).is_ok());
    assert_eq!(rename_name("old.gcode.3mf", "new").unwrap(), "new.gcode.3mf");
}

#[test]
fn error_display_is_the_serde_name() {
    assert_eq!(Error::PathUnresolvable.to_string(), "path_unresolvable");
    assert_eq!(serde_json::to_string(&Error::Conflict).unwrap(), "\"conflict\"");
}

#[test]
fn recovery_decision_untrusted_state_is_manual() {
    let facts = RecoveryFacts {
        trusted: false,
        db_before: true,
        db_after: false,
        source_matches: true,
        source_absent: false,
        target_matches: false,
        temporary_matches: false,
        unlink: false,
        db_only: false,
        companions_db_before: true,
        companions_db_after: false,
        companions_all_source: true,
        companions_all_target: false,
        companions_mixed: false,
        companions_any: false,
    };
    assert_eq!(recovery_decision(facts), RecoveryDecision::Manual);
    assert_eq!(recovery_decision(RecoveryFacts { trusted: true, ..facts }), RecoveryDecision::Discard);
}

#[cfg(unix)]
#[test]
fn io_errors_map_by_errno() {
    let e = |n| Error::from(std::io::Error::from_raw_os_error(n));
    assert_eq!(e(libc::EXDEV), Error::CrossDevice);
    assert_eq!(e(libc::ENOENT), Error::NotFound);
    assert_eq!(e(libc::ENOSPC), Error::InsufficientStorage);
    assert_eq!(Error::from(std::io::Error::from(std::io::ErrorKind::InvalidData)), Error::PathUnresolvable);
}
