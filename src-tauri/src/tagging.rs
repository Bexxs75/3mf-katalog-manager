use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::db::models::MaterialRecord;

const MIN_TOKEN_LEN: usize = 3;
const MAX_FILENAME_TAGS: usize = 4;
const MINIATURE_MAX_DIM_MM: f64 = 30.0;
const LARGE_MIN_DIM_MM: f64 = 200.0;

const FILENAME_STOPWORDS: &[&str] = &[
    "kopie", "copy", "neu", "new", "final", "fertig", "export", "test", "scan", "model", "modell",
    "copia", "nuevo", "nueva", "modelo", "prueba", "copie", "nouveau", "nouvelle", "modèle", "essai",
    "untitled", "sans", "titre",
];

// Name table of the automatic tags, shared with the frontend
// (src/lib/autoTags.json). The key is the German name and is what the database
// stores; the frontend only translates the display.
const AUTO_TAGS_JSON: &str = include_str!("../../src/lib/autoTags.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoTagNames {
    de: String,
    en: String,
    es: String,
    fr: String,
}

impl AutoTagNames {
    fn all(&self) -> [&str; 4] {
        [&self.de, &self.en, &self.es, &self.fr]
    }
}

fn auto_tags() -> &'static BTreeMap<String, AutoTagNames> {
    static TABLE: OnceLock<BTreeMap<String, AutoTagNames>> = OnceLock::new();
    TABLE.get_or_init(|| serde_json::from_str(AUTO_TAGS_JSON).expect("src/lib/autoTags.json ist ungueltig"))
}

fn alias_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Maps the name of an automatic tag in any language (case and surrounding
/// whitespace don't matter) to its key, e.g. "Multipart" -> "mehrteilig". All
/// other tags stay unchanged.
pub fn canonical_tag(name: &str) -> String {
    let key = alias_key(name);
    auto_tags()
        .iter()
        .find(|(_, names)| names.all().iter().any(|n| alias_key(n) == key))
        .map(|(canonical, _)| canonical.clone())
        .unwrap_or_else(|| name.to_string())
}

/// Ambiguous aliases: "mini" also appears e.g. in "Bambu A1 mini", and "large"
/// and "grande" show up in file names otherwise too. Compared via `alias_key`.
const AMBIGUOUS_ALIASES: &[&str] = &["mini", "large", "grande"];

fn is_ambiguous_alias(name: &str) -> bool {
    let key = alias_key(name);
    AMBIGUOUS_ALIASES.iter().any(|alias| *alias == key)
}

/// Like [`canonical_tag`], but does NOT map the ambiguous aliases from
/// `AMBIGUOUS_ALIASES` - the input is returned unchanged. Meant for all automatic
/// sources (file name tokens, material names, merging at startup); manual input
/// (`add_tag` command, frontend `canonicalTag`) still uses the full `canonical_tag`.
pub fn canonical_tag_unambiguous(name: &str) -> String {
    if is_ambiguous_alias(name) {
        return name.to_string();
    }
    canonical_tag(name)
}

pub struct TaggingContext<'a> {
    pub file_name: &'a str,
    pub dimensions_mm: Option<[f64; 3]>,
    pub object_count: Option<i64>,
    pub materials: &'a [MaterialRecord],
}

/// Suggests catalog tags from filename tokens, bounding-box/object-count
/// heuristics, and (for 3MF files) the parsed material list. Duplicate tags
/// across sources are merged, keeping the first occurrence's position.
pub fn suggest_tags(ctx: &TaggingContext) -> Vec<String> {
    let mut tags = Vec::new();
    add_unique(&mut tags, filename_tags(ctx.file_name));
    add_unique(&mut tags, geometry_tags(ctx.dimensions_mm, ctx.object_count));
    add_unique(&mut tags, material_tags(ctx.materials));
    tags
}

fn add_unique(tags: &mut Vec<String>, new_tags: Vec<String>) {
    for tag in new_tags {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
}

fn filename_tags(file_name: &str) -> Vec<String> {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);

    stem.split(|c: char| !c.is_alphanumeric())
        .map(|token| token.to_lowercase())
        .filter(|token| is_meaningful_token(token))
        .map(|token| canonical_tag_unambiguous(&token))
        .take(MAX_FILENAME_TAGS)
        .collect()
}

fn is_meaningful_token(token: &str) -> bool {
    if token.chars().count() < MIN_TOKEN_LEN {
        return false;
    }
    if token.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if is_version_token(token) {
        return false;
    }
    !FILENAME_STOPWORDS.contains(&token)
}

/// Matches tokens like "v1", "v12" — sequential version suffixes that
/// carry no descriptive meaning as a catalog tag.
fn is_version_token(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some('v')) && chars.clone().count() > 0 && chars.all(|c| c.is_ascii_digit())
}

fn geometry_tags(dimensions_mm: Option<[f64; 3]>, object_count: Option<i64>) -> Vec<String> {
    let mut tags = Vec::new();

    if object_count.unwrap_or(1) > 1 {
        tags.push("mehrteilig".to_string());
    }

    if let Some(dims) = dimensions_mm {
        let max_dim = dims.iter().copied().fold(0.0_f64, f64::max);
        if max_dim <= MINIATURE_MAX_DIM_MM {
            tags.push("miniatur".to_string());
        } else if max_dim >= LARGE_MIN_DIM_MM {
            tags.push("grossformat".to_string());
        }
    }

    tags
}

fn material_tags(materials: &[MaterialRecord]) -> Vec<String> {
    let mut tags: Vec<String> = materials
        .iter()
        .map(|m| canonical_tag_unambiguous(&normalize_material_name(&m.name)))
        .filter(|name| !name.is_empty())
        .collect();
    tags.dedup();

    // `contains` check instead of a blind push: a material name may already have
    // been mapped to "mehrfarbig" (e.g. the material name "Multicolor"), a second
    // entry would be a duplicate.
    if tags.len() > 1 && !tags.contains(&"mehrfarbig".to_string()) {
        tags.push("mehrfarbig".to_string());
    }

    tags
}

fn normalize_material_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        file_name: &'a str,
        dimensions_mm: Option<[f64; 3]>,
        object_count: Option<i64>,
        materials: &'a [MaterialRecord],
    ) -> TaggingContext<'a> {
        TaggingContext {
            file_name,
            dimensions_mm,
            object_count,
            materials,
        }
    }

    #[test]
    fn extracts_meaningful_filename_tokens() {
        let tags = suggest_tags(&ctx("Kabelhalter_v3_final.3mf", None, None, &[]));
        assert_eq!(tags, vec!["kabelhalter".to_string()]);
    }

    #[test]
    fn drops_numeric_short_and_stopword_tokens() {
        let tags = suggest_tags(&ctx("zahnrad_modul2_kopie.3mf", None, None, &[]));
        assert_eq!(tags, vec!["zahnrad".to_string(), "modul2".to_string()]);
    }

    #[test]
    fn works_the_same_for_an_stp_filename_as_any_other_extension() {
        // Tag suggestions don't depend on the file extension.
        let tags = suggest_tags(&ctx("kabelhalter_v3_final.stp", None, None, &[]));
        assert_eq!(tags, vec!["kabelhalter".to_string()]);
    }
    #[test]
    fn caps_filename_tags_at_max() {
        let tags = suggest_tags(&ctx(
            "alpha_bravo_charlie_delta_echo_foxtrot.stl",
            None,
            None,
            &[],
        ));
        assert_eq!(tags.len(), MAX_FILENAME_TAGS);
    }

    #[test]
    fn tags_multi_part_files() {
        let tags = suggest_tags(&ctx("set.3mf", None, Some(3), &[]));
        assert!(tags.contains(&"mehrteilig".to_string()));
    }

    #[test]
    fn tags_miniature_by_bounding_box() {
        let tags = suggest_tags(&ctx("figur.3mf", Some([20.0, 18.0, 25.0]), None, &[]));
        assert!(tags.contains(&"miniatur".to_string()));
    }

    #[test]
    fn tags_large_format_by_bounding_box() {
        let tags = suggest_tags(&ctx("gehaeuse.3mf", Some([250.0, 80.0, 40.0]), None, &[]));
        assert!(tags.contains(&"grossformat".to_string()));
    }

    #[test]
    fn mid_size_gets_no_size_tag() {
        let tags = suggest_tags(&ctx("teil.3mf", Some([80.0, 60.0, 40.0]), None, &[]));
        assert!(!tags.contains(&"miniatur".to_string()));
        assert!(!tags.contains(&"grossformat".to_string()));
    }

    #[test]
    fn normalizes_material_names_into_tags() {
        let materials = vec![
            MaterialRecord {
                name: "PETG".to_string(),
                display_color: None,
            },
            MaterialRecord {
                name: "PLA Silk".to_string(),
                display_color: None,
            },
        ];
        let tags = suggest_tags(&ctx("teil.3mf", None, None, &materials));
        assert!(tags.contains(&"petg".to_string()));
        assert!(tags.contains(&"pla-silk".to_string()));
        assert!(tags.contains(&"mehrfarbig".to_string()));
    }

    #[test]
    fn single_material_has_no_multi_color_tag() {
        let materials = vec![MaterialRecord {
            name: "PLA".to_string(),
            display_color: None,
        }];
        let tags = suggest_tags(&ctx("teil.3mf", None, None, &materials));
        assert!(tags.contains(&"pla".to_string()));
        assert!(!tags.contains(&"mehrfarbig".to_string()));
    }

    #[test]
    fn combines_and_deduplicates_tags_from_all_sources() {
        let materials = vec![MaterialRecord {
            name: "PLA".to_string(),
            display_color: None,
        }];
        let tags = suggest_tags(&ctx(
            "vase_pla.3mf",
            Some([80.0, 80.0, 140.0]),
            Some(1),
            &materials,
        ));
        assert_eq!(tags.iter().filter(|t| *t == "pla").count(), 1);
    }

    #[test]
    fn auto_tag_table_is_consistent() {
        let table = auto_tags();
        assert_eq!(table.len(), 4);
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (canonical, names) in table {
            assert_eq!(&names.de, canonical, "DE-Name muss der Kennung entsprechen");
            for name in names.all() {
                assert!(!name.trim().is_empty(), "leerer Name bei {canonical}");
                if let Some(other) = seen.insert(alias_key(name), canonical.clone()) {
                    assert_eq!(&other, canonical, "Alias {name} gehoert zu zwei Kennungen");
                }
            }
        }
    }

    #[test]
    fn canonical_tag_maps_every_alias_in_every_language() {
        for (alias, expected) in [
            ("mehrteilig", "mehrteilig"), ("multipart", "mehrteilig"), ("multipieza", "mehrteilig"), ("multipièce", "mehrteilig"),
            ("miniatur", "miniatur"), ("mini", "miniatur"), ("miniatura", "miniatur"), ("miniature", "miniatur"),
            ("grossformat", "grossformat"), ("large", "grossformat"), ("grande", "grossformat"), ("grand format", "grossformat"),
            ("mehrfarbig", "mehrfarbig"), ("multicolor", "mehrfarbig"), ("multicolore", "mehrfarbig"),
        ] {
            assert_eq!(canonical_tag(alias), expected, "Alias {alias}");
        }
    }

    #[test]
    fn canonical_tag_ignores_case_and_surrounding_whitespace() {
        assert_eq!(canonical_tag("  Multipart "), "mehrteilig");
        assert_eq!(canonical_tag("MULTIPIÈCE"), "mehrteilig");
        assert_eq!(canonical_tag("Grand Format"), "grossformat");
    }

    #[test]
    fn canonical_tag_leaves_other_tags_untouched() {
        assert_eq!(canonical_tag("Vase"), "Vase");
        assert_eq!(canonical_tag("pla-silk"), "pla-silk");
        assert_eq!(canonical_tag("minis"), "minis");
    }

    #[test]
    fn filename_tokens_that_are_auto_tag_aliases_become_the_canonical_tag() {
        let tags = suggest_tags(&ctx("Board_multipart.3mf", None, None, &[]));
        assert_eq!(tags, vec!["board".to_string(), "mehrteilig".to_string()]);
    }

    #[test]
    fn filename_alias_and_geometry_tag_are_not_duplicated() {
        let tags = suggest_tags(&ctx("set_multipart.3mf", None, Some(3), &[]));
        assert_eq!(tags.iter().filter(|t| *t == "mehrteilig").count(), 1);
    }

    #[test]
    fn drops_spanish_french_and_english_filler_words() {
        let tags = suggest_tags(&ctx("vase_copia_nuevo_modèle_untitled.stl", None, None, &[]));
        assert_eq!(tags, vec!["vase".to_string()]);
    }

    #[test]
    fn canonical_tag_unambiguous_leaves_ambiguous_aliases_untouched() {
        for alias in ["mini", "large", "grande", "Mini", "LARGE", "Grande"] {
            assert_eq!(canonical_tag_unambiguous(alias), alias, "Alias {alias}");
        }
    }

    #[test]
    fn canonical_tag_unambiguous_still_maps_unambiguous_aliases() {
        for (alias, expected) in [
            ("multipart", "mehrteilig"),
            ("Multipart", "mehrteilig"),
            ("miniature", "miniatur"),
            ("grand format", "grossformat"),
            ("multicolor", "mehrfarbig"),
        ] {
            assert_eq!(canonical_tag_unambiguous(alias), expected, "Alias {alias}");
        }
    }

    #[test]
    fn filename_token_a1_mini_stays_mini_not_miniatur() {
        let tags = suggest_tags(&ctx("A1_mini_halter.3mf", None, None, &[]));
        assert!(tags.contains(&"mini".to_string()));
        assert!(!tags.contains(&"miniatur".to_string()));
    }

    #[test]
    fn filename_token_large_stays_large_not_grossformat() {
        let tags = suggest_tags(&ctx("gehaeuse_large_v2.3mf", None, None, &[]));
        assert!(tags.contains(&"large".to_string()));
        assert!(!tags.contains(&"grossformat".to_string()));
    }

    #[test]
    fn filename_token_multipart_still_becomes_mehrteilig() {
        let tags = suggest_tags(&ctx("Board_multipart.3mf", None, None, &[]));
        assert!(tags.contains(&"mehrteilig".to_string()));
    }

    #[test]
    fn material_named_multicolor_maps_to_the_canonical_tag_without_duplicating_it() {
        let materials = vec![
            MaterialRecord {
                name: "Multicolor".to_string(),
                display_color: None,
            },
            MaterialRecord {
                name: "PLA".to_string(),
                display_color: None,
            },
        ];
        let tags = suggest_tags(&ctx("teil.3mf", None, None, &materials));
        assert_eq!(tags.iter().filter(|t| *t == "mehrfarbig").count(), 1);
        assert!(!tags.contains(&"multicolor".to_string()));
    }
}
