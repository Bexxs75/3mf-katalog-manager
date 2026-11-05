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

/// Group of a print in the history and the statistics (`succeeded`, `cancelled`, `failed`,
/// `printer_error`, `interrupted`, `other`). A reason set by the user overrides the printer's
/// status: `intentional` stays cancelled, every other reason counts as failed. Shared with
/// the interface through a gold table (`src/lib/printStatusGroups.json`).
pub fn status_group(raw_status: &str, failure_reason: Option<&str>) -> &'static str {
    if let Some(reason) = failure_reason {
        return if reason == "intentional" { "cancelled" } else { "failed" };
    }
    match raw_status.trim().to_lowercase().as_str() {
        "completed" => "succeeded",
        "cancelled" => "cancelled",
        "klippy_shutdown" | "klippy_disconnect" | "error" => "printer_error",
        "interrupted" => "interrupted",
        _ => "other",
    }
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

    #[test]
    fn the_status_groups_are_the_ones_the_interface_uses() {
        let gold: serde_json::Value = serde_json::from_str(include_str!("../../../../src/lib/printStatusGroups.json")).unwrap();
        let cases = gold["cases"].as_array().unwrap();
        assert!(cases.len() >= 10, "the table must keep its cases");
        for case in cases {
            let status = case["rawStatus"].as_str().unwrap();
            let reason = case["reason"].as_str();
            assert_eq!(status_group(status, reason), case["group"].as_str().unwrap(), "{status:?} with reason {reason:?}");
        }
        // Every status the table names maps the same way, and every group it lists is one this function can return.
        for (status, group) in gold["statusGroups"].as_object().unwrap() {
            assert_eq!(status_group(status, None), group.as_str().unwrap(), "{status}");
        }
        let groups: Vec<&str> = gold["groups"].as_array().unwrap().iter().map(|g| g.as_str().unwrap()).collect();
        for group in ["succeeded", "cancelled", "failed", "printer_error", "interrupted", "other"] {
            assert!(groups.contains(&group), "{group}");
        }
        assert_eq!(groups.len(), 6);
    }
}
