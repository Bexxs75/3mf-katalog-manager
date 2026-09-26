//! Link to the bug report form on 3mfkatalog.de with version and system filled in.

pub fn core_version(v: &str) -> &str {
    v.split(['-', '+']).next().unwrap_or(v)
}

pub fn current_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// The website only exists in German and English; every other app language
/// gets the English form.
pub fn bug_report_url(lang: &str, version: &str, os: &str, with_log: bool) -> String {
    let page = if lang == "de" {
        "https://3mfkatalog.de/fehler-melden.html"
    } else {
        "https://3mfkatalog.de/en/report-a-bug.html"
    };
    let mut url = format!("{page}?version={}&os={os}", core_version(version));
    if with_log {
        url.push_str("&log=1");
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_app_opens_the_german_form() {
        assert_eq!(
            bug_report_url("de", "0.15.0", "linux", false),
            "https://3mfkatalog.de/fehler-melden.html?version=0.15.0&os=linux"
        );
    }

    #[test]
    fn other_languages_open_the_english_form_with_log_flag() {
        assert_eq!(
            bug_report_url("fr", "0.15.0", "macos", true),
            "https://3mfkatalog.de/en/report-a-bug.html?version=0.15.0&os=macos&log=1"
        );
    }

    #[test]
    fn prerelease_suffix_is_cut() {
        assert_eq!(core_version("0.15.0-test"), "0.15.0");
        assert_eq!(core_version("0.15.0+build.3"), "0.15.0");
        assert_eq!(core_version("0.15.0"), "0.15.0");
    }
}
