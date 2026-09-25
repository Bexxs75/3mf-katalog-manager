use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

fn parse_version(v: &str) -> Option<semver::Version> {
    let v = v.strip_prefix('v').unwrap_or(v);
    semver::Version::parse(v).ok()
}

/// Vergleicht die laufende Version gegen einen GitHub-Release-Tag (z.B.
/// "v0.7.9") nach echter SemVer-Praezedenz (ueber die `semver`-Crate), nicht
/// mehr nur nach major.minor.patch. Dadurch werden auch Vorabversionen wie
/// "0.14.0-gharac" korrekt eingeordnet: sie zaehlen als aelter als das
/// zugehoerige finale Release "0.14.0", aber als neuer als das vorherige
/// finale Release "0.13.1". Die GitHub-API (`/releases/latest`) liefert zwar
/// nie ein Pre-Release als "latest", aber die Logik bleibt so auch fuer
/// andere Aufrufer korrekt. Ein nicht parsbarer Tag (z.B. GitHub-API-
/// Fehlerantwort ohne echtes Release) wird bewusst als "kein Update"
/// behandelt statt einen Fehler zu werfen - der Aufrufer (die Tauri-Command-Ebene) muss so nie
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

    #[test]
    fn malformed_current_version_is_treated_as_no_update() {
        let result = compare_versions("not-a-version", "v0.7.9", "https://example.com/v0.7.9");
        assert!(!result.update_available);
        assert_eq!(result.latest_version, "not-a-version");
    }

    #[test]
    fn prerelease_current_version_is_older_than_the_matching_final_release() {
        // "0.14.0-gharac" ist eine Vorabversion von "0.14.0" - nach SemVer-
        // Praezedenz ist jede Vorabversion aelter als ihr finales Release.
        let result = compare_versions("0.14.0-gharac", "v0.14.0", "https://example.com/v0.14.0");
        assert!(result.update_available);
        assert_eq!(result.latest_version, "0.14.0");
    }

    #[test]
    fn prerelease_current_version_is_newer_than_an_older_final_release() {
        let result = compare_versions("0.14.0-gharac", "v0.13.1", "https://example.com/v0.13.1");
        assert!(!result.update_available);
        assert_eq!(result.latest_version, "0.14.0-gharac");
    }

    #[test]
    fn newer_prerelease_tag_counts_as_an_update() {
        // GitHub liefert bei /releases/latest nie ein Pre-Release, aber die
        // Vergleichslogik soll auch dann korrekt sein, wenn sie mit einem
        // Pre-Release-Tag aufgerufen wird, das nach SemVer neuer ist.
        let result = compare_versions("0.13.1", "v0.14.0-gharac", "https://example.com/v0.14.0-gharac");
        assert!(result.update_available);
        assert_eq!(result.latest_version, "0.14.0-gharac");
    }
}
