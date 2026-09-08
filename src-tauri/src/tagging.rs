use std::path::Path;

use crate::db::models::MaterialRecord;

const MIN_TOKEN_LEN: usize = 3;
const MAX_FILENAME_TAGS: usize = 4;
const MINIATURE_MAX_DIM_MM: f64 = 30.0;
const LARGE_MIN_DIM_MM: f64 = 200.0;

const FILENAME_STOPWORDS: &[&str] = &[
    "kopie", "copy", "neu", "new", "final", "fertig", "export", "test", "scan", "model", "modell",
];

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
        .map(|m| normalize_material_name(&m.name))
        .filter(|name| !name.is_empty())
        .collect();
    tags.dedup();

    if tags.len() > 1 {
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
}
