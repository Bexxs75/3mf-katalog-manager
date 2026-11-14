//! Recognises the download platform of a 3MF from its model metadata.
//!
//! Only Makerworld is recognised: its files carry a model and a profile ID in
//! `3D/3dmodel.model`. Other platforms (Printables, Thingiverse) leave no
//! reliable trace, and guessing would label files with a wrong origin. A file
//! that was saved again in a slicer keeps the keys but with empty values, which
//! counts as "unknown" as well.

use std::collections::BTreeMap;

use crate::db::models::SourceInfo;

pub const PLATFORM_MAKERWORLD: &str = "makerworld";

/// The values come from foreign files and end up in the catalog and the UI.
const MAX_ID_CHARS: usize = 128;

/// Zero-width and bidirectional control characters are not `is_control` but are
/// invisible, so an ID could otherwise look different from what is stored.
fn is_invisible_format_char(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Keeps printable characters only, trims, and caps the length. Text is never
/// interpreted (no markup, no URL building), so removing characters is enough.
/// `None` when nothing is left.
pub fn sanitize_source_value(raw: &str) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| !c.is_control() && !is_invisible_format_char(*c)).collect();
    let value: String = cleaned.trim().chars().take(MAX_ID_CHARS).collect();
    let value = value.trim_end().to_string();
    (!value.is_empty()).then_some(value)
}

/// Checks a stored value (restored from a backup) against the limits that
/// [`sanitize_source_value`] enforces on freshly read ones.
pub fn validate_source_value(value: &str) -> Result<(), String> {
    if value.chars().count() > MAX_ID_CHARS || value.chars().any(|c| c.is_control() || is_invisible_format_char(c)) {
        return Err(format!("Herkunftsangaben dürfen höchstens {MAX_ID_CHARS} Zeichen und keine Steuerzeichen enthalten"));
    }
    Ok(())
}

/// Makerworld when both `DesignModelId` and `DesignProfileId` hold a value;
/// anything else yields an empty [`SourceInfo`].
pub fn detect_source(metadata: &BTreeMap<String, String>) -> SourceInfo {
    let read = |key: &str| metadata.get(key).and_then(|v| sanitize_source_value(v));
    match (read("DesignModelId"), read("DesignProfileId")) {
        (Some(model_id), Some(profile_id)) => SourceInfo {
            platform: Some(PLATFORM_MAKERWORLD.to_string()),
            model_id: Some(model_id),
            profile_id: Some(profile_id),
        },
        _ => SourceInfo::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn makerworld_needs_both_ids() {
        let found = detect_source(&meta(&[("DesignModelId", "USfa963547e39de4"), ("DesignProfileId", "1067399466"), ("Designer", "x")]));
        assert_eq!(found.platform.as_deref(), Some("makerworld"));
        assert_eq!(found.model_id.as_deref(), Some("USfa963547e39de4"));
        assert_eq!(found.profile_id.as_deref(), Some("1067399466"));
    }

    #[test]
    fn a_single_id_is_not_enough() {
        assert_eq!(detect_source(&meta(&[("DesignModelId", "USfa963547e39de4")])), SourceInfo::default());
        assert_eq!(detect_source(&meta(&[("DesignProfileId", "1067399466")])), SourceInfo::default());
    }

    #[test]
    fn empty_or_whitespace_values_and_missing_keys_are_not_recognised() {
        assert_eq!(detect_source(&meta(&[("DesignModelId", ""), ("DesignProfileId", ""), ("Designer", ""), ("Origin", "")])), SourceInfo::default());
        assert_eq!(detect_source(&meta(&[("DesignModelId", "  \t"), ("DesignProfileId", "1")])), SourceInfo::default());
        assert_eq!(detect_source(&meta(&[("Application", "PrusaSlicer-2.8")])), SourceInfo::default());
        assert_eq!(detect_source(&BTreeMap::new()), SourceInfo::default());
    }

    #[test]
    fn values_made_only_of_control_characters_count_as_empty() {
        assert_eq!(detect_source(&meta(&[("DesignModelId", "\u{0}\u{7}"), ("DesignProfileId", "1")])), SourceInfo::default());
    }

    #[test]
    fn sanitizing_removes_control_and_invisible_characters_and_trims() {
        assert_eq!(sanitize_source_value(" US\u{0}fa\n96\u{202E}35\u{200B} ").as_deref(), Some("USfa9635"));
        assert_eq!(sanitize_source_value("<b>1067</b>").as_deref(), Some("<b>1067</b>"), "markup is kept as plain text");
    }

    #[test]
    fn sanitizing_caps_the_length_in_characters() {
        let long = "é".repeat(500);
        assert_eq!(sanitize_source_value(&long).unwrap().chars().count(), 128);
        let info = detect_source(&meta(&[("DesignModelId", &"a".repeat(300)), ("DesignProfileId", "1")]));
        assert_eq!(info.model_id.unwrap().len(), 128);
    }
}
