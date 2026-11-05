//! Why a print ended early. The codes are stored as text without a CHECK in the database, so the
//! list can grow without a migration; the code and the backup check are what keep it clean.

/// Codes in the order the interface lists them. Shared with the interface through a gold list.
pub const FAILURE_REASONS: &[&str] = &[
    "adhesion",
    "clog",
    "filament",
    "power_network",
    "slicer",
    "mechanical",
    "spaghetti",
    "intentional",
    "other",
];

/// Longest note, counted in characters.
pub const MAX_NOTE_CHARS: usize = 200;

pub fn is_failure_reason(code: &str) -> bool {
    FAILURE_REASONS.contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_the_one_the_interface_uses() {
        let gold: serde_json::Value = serde_json::from_str(include_str!("../../../../src/lib/printFailureReasons.json")).unwrap();
        let reasons: Vec<&str> = gold["reasons"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(reasons, FAILURE_REASONS);
        assert_eq!(gold["maxNoteChars"].as_u64().unwrap() as usize, MAX_NOTE_CHARS);
    }

    #[test]
    fn only_listed_codes_are_reasons() {
        assert!(is_failure_reason("clog"));
        assert!(!is_failure_reason(""));
        assert!(!is_failure_reason("Clog"));
        assert!(!is_failure_reason("made_up"));
    }
}
