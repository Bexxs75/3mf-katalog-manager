use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Vergleicht die laufende Version gegen einen GitHub-Release-Tag (z.B.
/// "v0.7.9"). Ein nicht parsbarer Tag (z.B. GitHub-API-Fehlerantwort ohne
/// echtes Release) wird bewusst als "kein Update" behandelt statt einen
/// Fehler zu werfen - der Aufrufer (die Tauri-Command-Ebene) muss so nie
/// zwischen "kein Update" und "Antwort nicht verstanden" unterscheiden,
/// beides fuehrt zum selben harmlosen UI-Zustand.
pub fn compare_versions(current: &str, latest_tag: &str, release_url: &str) -> UpdateCheckResult {
    let current_parsed = parse_version(current);
    let latest_parsed = parse_version(latest_tag);

    let (latest_version_str, update_available) = match (current_parsed, latest_parsed) {
        (Some(cur), Some(lat)) if lat > cur => {
            (latest_tag.strip_prefix('v').unwrap_or(latest_tag).to_string(), true)
        }
        _ => (current.to_string(), false),
    };

    UpdateCheckResult {
        current_version: current.to_string(),
        latest_version: latest_version_str,
        update_available,
        release_url: if update_available { release_url.to_string() } else { String::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_newer_patch_version() {
        let result = compare_versions("0.7.8", "v0.7.9", "https://example.com/v0.7.9");
        assert!(result.update_available);
        assert_eq!(result.latest_version, "0.7.9");
        assert_eq!(result.current_version, "0.7.8");
        assert_eq!(result.release_url, "https://example.com/v0.7.9");
    }

    #[test]
    fn detects_a_newer_minor_version_even_with_lower_patch() {
        let result = compare_versions("0.7.9", "v0.8.0", "https://example.com/v0.8.0");
        assert!(result.update_available);
    }

    #[test]
    fn same_version_is_not_an_update() {
        let result = compare_versions("0.7.8", "v0.7.8", "https://example.com/v0.7.8");
        assert!(!result.update_available);
    }

    #[test]
    fn older_remote_tag_is_not_an_update() {
        let result = compare_versions("0.7.8", "v0.7.7", "https://example.com/v0.7.7");
        assert!(!result.update_available);
    }

    #[test]
    fn tag_without_leading_v_is_handled() {
        let result = compare_versions("0.7.8", "0.7.9", "https://example.com/0.7.9");
        assert!(result.update_available);
        assert_eq!(result.latest_version, "0.7.9");
    }

    #[test]
    fn malformed_tag_is_treated_as_no_update() {
        let result = compare_versions("0.7.8", "not-a-version", "https://example.com");
        assert!(!result.update_available);
        assert_eq!(result.latest_version, "0.7.8");
    }
}
