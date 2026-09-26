//! Maps a G-code file name to a catalog model.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub file_id: i64,
    pub name: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Sure,
    Unsure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMatch {
    pub file_id: i64,
    pub file_name: String,
    pub kind: MatchKind,
}

const EXTENSIONS: &[&str] = &[".gcode", ".bgcode", ".3mf", ".stl", ".obj", ".step", ".stp"];
const MATERIAL_WORDS: &[&str] = &["pla", "petg", "abs", "asa", "tpu", "pa", "pc", "cf", "hf", "matt", "silk", "pet", "hips", "pva"];

pub fn normalize_name(name: &str) -> String {
    let mut s = name.trim().to_lowercase();
    for ext in EXTENSIONS {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
            break;
        }
    }
    let s = s.replace('ä', "a").replace('ö', "o").replace('ü', "u").replace('ß', "ss");
    let s = s.replace("ae", "a").replace("oe", "o").replace("ue", "u");
    let s: String = s.chars().map(|c| if c == '_' || c == '-' { ' ' } else { c }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_time_token(w: &str) -> bool {
    let mut rest = w;
    let mut groups = 0;
    for unit in ['d', 'h', 'm', 's'] {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(unit) {
            rest = &rest[digits + 1..];
            groups += 1;
        }
    }
    groups > 0 && rest.is_empty()
}

fn is_layer_token(w: &str) -> bool {
    match w.split_once(['.', ',']) {
        Some((a, b)) => !a.is_empty() && !b.is_empty() && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// "plate(01)" as appended by the Anycubic slicer.
fn is_plate_token(w: &str) -> bool {
    w.strip_prefix("plate(")
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Copy counter such as "(1)" or "(01)" that the Anycubic slicer appends, e.g.
/// "slide(01)" or "plate_1(2)"; returns the word without it.
fn strip_copy_counter(w: &str) -> Option<&str> {
    let (head, rest) = w.split_once('(')?;
    let n = rest.strip_suffix(')')?;
    (!head.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then_some(head)
}

fn is_four_digits(w: &str) -> bool {
    w.len() == 4 && w.chars().all(|c| c.is_ascii_digit())
}

/// Like `normalize_name`, additionally strips appended slicer parts (print time,
/// layer height, material, "plate N", copy counters) from the end and the
/// "MMDD-HHMM-" date prefix of the Anycubic slicer from the start.
pub fn normalize_gcode_name(name: &str) -> String {
    let normalized = normalize_name(name);
    let mut words: Vec<&str> = normalized.split(' ').collect();
    while let Some(last) = words.last().copied() {
        if is_time_token(last) || is_layer_token(last) || is_plate_token(last) || MATERIAL_WORDS.contains(&last) {
            words.pop();
        } else if last.chars().all(|c| c.is_ascii_digit()) && words.len() >= 2 && words[words.len() - 2] == "plate" {
            words.truncate(words.len() - 2);
        } else if let Some(head) = strip_copy_counter(last) {
            *words.last_mut().expect("loop only runs with a last word") = head;
        } else {
            break;
        }
    }
    // Only when a name remains, so a model really called "2024-1234" keeps its name.
    if words.len() > 2 && is_four_digits(words[0]) && is_four_digits(words[1]) {
        words.drain(..2);
    }
    words.join(" ")
}

pub fn best_match(gcode_file_name: &str, candidates: &[Candidate]) -> Option<ModelMatch> {
    let g = normalize_gcode_name(gcode_file_name);
    if g.is_empty() {
        return None;
    }
    let g_words: HashSet<&str> = g.split(' ').collect();

    let sure = candidates
        .iter()
        .filter(|c| normalize_name(&c.name) == g)
        .max_by(|a, b| a.imported_at.cmp(&b.imported_at));
    if let Some(c) = sure {
        return Some(ModelMatch { file_id: c.file_id, file_name: c.name.clone(), kind: MatchKind::Sure });
    }

    let mut best: Option<(f64, usize, &Candidate)> = None;
    for c in candidates {
        let n = normalize_name(&c.name);
        let n_words: HashSet<&str> = n.split(' ').collect();
        let common = g_words.intersection(&n_words).count();
        let overlap = common as f64 / g_words.len() as f64;
        let (shorter, longer) = if n.len() <= g.len() { (&n, &g) } else { (&g, &n) };
        let prefix = shorter.chars().count() >= 6 && longer.starts_with(shorter.as_str());
        // The catalog name appears as whole words in the G-code name, e.g. when the
        // slicer prepends the printer model or a date ("S1_<name>_PLA_12m.gcode").
        let contained = n.chars().count() >= 6 && format!(" {g} ").contains(&format!(" {n} "));
        let qualifies = (overlap >= 0.6 && common >= 2) || prefix || contained;
        if !qualifies {
            continue;
        }
        let score = if prefix || contained { overlap.max(0.6) } else { overlap };
        let distance = n.len().abs_diff(g.len());
        let better = match best {
            None => true,
            Some((s, d, _)) => score > s || (score == s && distance < d),
        };
        if better {
            best = Some((score, distance, c));
        }
    }
    best.map(|(_, _, c)| ModelMatch { file_id: c.file_id, file_name: c.name.clone(), kind: MatchKind::Unsure })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i64, name: &str, imported_at: &str) -> Candidate {
        Candidate { file_id: id, name: name.into(), imported_at: imported_at.into() }
    }

    #[test]
    fn slicer_suffixes_are_removed_from_gcode_names() {
        assert_eq!(normalize_gcode_name("Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode"), "distanzhulse 13,40mm");
        assert_eq!(normalize_gcode_name("Ubiquity_NanoStation_Bracket_Extended_PLA_0.2_1h42m.gcode"), "ubiquity nanostation bracket extended");
        assert_eq!(normalize_gcode_name("USB-3DBenchy_Fast_12m.gcode"), "usb 3dbenchy fast");
        assert_eq!(normalize_gcode_name("Rakete_plate_1.gcode"), "rakete");
        assert_eq!(normalize_gcode_name("Box_PETG_0,28_2d3h.bgcode"), "box");
    }

    #[test]
    fn anycubic_copy_and_plate_suffixes_are_removed() {
        assert_eq!(normalize_gcode_name("Axle Cleaning Tool_plate_1(1).gcode"), "axle cleaning tool");
        assert_eq!(normalize_gcode_name("Cartridge+nozzle_plate_1(2).gcode"), "cartridge+nozzle");
        assert_eq!(normalize_gcode_name("0926-1506-slide(01)_PETG_0.12_2h56m50s.gcode"), "slide");
        assert_eq!(
            normalize_gcode_name("0920-1338-Kobra S1 Front Z-Axis Spacer V2_plate(01)_ABS_0.2_16m46s.gcode"),
            "kobra s1 front z axis spacer v2"
        );
        assert_eq!(normalize_gcode_name("0912-1742-OrcaToleranceTest_plate(01)_PLA_0.2_14m11s.gcode"), "orcatolerancetest");
    }

    #[test]
    fn a_leading_number_pair_is_kept_when_it_is_the_whole_name() {
        assert_eq!(normalize_gcode_name("2024-1234.gcode"), "2024 1234");
    }

    #[test]
    fn anycubic_suffixes_give_a_sure_match() {
        let cands = [c(1, "Axle Cleaning Tool.3mf", "2026-09-01T00:00:00Z"), c(2, "slide.stl", "2026-09-01T00:00:00Z")];
        let m = best_match("Axle Cleaning Tool_plate_1(1).gcode", &cands).unwrap();
        assert_eq!((m.file_id, m.kind), (1, MatchKind::Sure));
        let m = best_match("0926-1506-slide(01)_PETG_0.12_2h56m50s.gcode", &cands).unwrap();
        assert_eq!((m.file_id, m.kind), (2, MatchKind::Sure));
    }

    #[test]
    fn umlauts_are_unified() {
        assert_eq!(normalize_name("Distanzhülse 13,40mm.3mf"), "distanzhulse 13,40mm");
        assert_eq!(normalize_name("Distanzhuelse 13,40mm.stl"), "distanzhulse 13,40mm");
        assert_eq!(normalize_name("Große_Schale.3mf"), "grosse schale");
    }

    #[test]
    fn equal_names_are_a_sure_match_and_newest_wins() {
        let cands = [c(1, "Distanzhülse 13,40mm.3mf", "2026-09-01T00:00:00Z"), c(2, "Distanzhuelse 13,40mm.stl", "2026-09-10T00:00:00Z")];
        let m = best_match("Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode", &cands).unwrap();
        assert_eq!(m.file_id, 2);
        assert_eq!(m.kind, MatchKind::Sure);
    }

    #[test]
    fn mostly_equal_words_are_an_unsure_match() {
        let cands = [c(7, "Ubiquity NanoStation Bracket.3mf", "2026-09-01T00:00:00Z"), c(8, "Kabelclip.stl", "2026-09-01T00:00:00Z")];
        let m = best_match("Ubiquity_NanoStation_Bracket_Extended_PLA_0.2_1h42m.gcode", &cands).unwrap();
        assert_eq!(m.file_id, 7);
        assert_eq!(m.kind, MatchKind::Unsure);
        assert_eq!(m.file_name, "Ubiquity NanoStation Bracket.3mf");
    }

    #[test]
    fn prefix_of_at_least_six_characters_is_unsure() {
        let cands = [c(3, "Wandhaken.3mf", "2026-09-01T00:00:00Z")];
        assert_eq!(best_match("Wandhaken_gross_PLA_0.2_30m.gcode", &cands).unwrap().kind, MatchKind::Unsure);
        let cands = [c(4, "Box.3mf", "2026-09-01T00:00:00Z")];
        assert_eq!(best_match("Boxdeckel_PLA_0.2_30m.gcode", &cands), None);
    }

    #[test]
    fn slicer_plate_in_parentheses_is_removed() {
        assert_eq!(normalize_gcode_name("OrcaToleranceTest_plate(01)_PLA_0.2_14m11s.gcode"), "orcatolerancetest");
    }

    #[test]
    fn prefixed_gcode_names_match_the_catalog_name() {
        // The Anycubic slicer (Kobra S1) prepends the printer model or date/time to the name.
        // The model prefix can't be told apart from a real word, so that stays unsure;
        // the "MMDD-HHMM-" date prefix is removed, so that is a sure match.
        let cands = [c(5, "OrcaToleranceTest.3mf", "2026-09-01T00:00:00Z"), c(6, "Kabelclip.stl", "2026-09-01T00:00:00Z")];
        let m = best_match("S1_OrcaToleranceTest_PLA_12m28s.gcode", &cands).unwrap();
        assert_eq!((m.file_id, m.kind), (5, MatchKind::Unsure));
        let m = best_match("0912-1742-OrcaToleranceTest_plate(01)_PLA_0.2_14m11s.gcode", &cands).unwrap();
        assert_eq!((m.file_id, m.kind), (5, MatchKind::Sure));
        let cands = [c(7, "Gehäuse Deckel.3mf", "2026-09-01T00:00:00Z")];
        assert_eq!(best_match("S1_Gehäuse Deckel_PLA_20m28s.gcode", &cands).unwrap().file_id, 7);
    }

    #[test]
    fn short_catalog_names_are_not_found_inside_other_names() {
        let cands = [c(8, "Box.3mf", "2026-09-01T00:00:00Z")];
        assert_eq!(best_match("S1_Box_Deckel_PLA_20m.gcode", &cands), None);
    }

    #[test]
    fn unrelated_names_do_not_match() {
        let cands = [c(1, "Kabelclip.stl", "2026-09-01T00:00:00Z"), c(2, "Rakete.3mf", "2026-09-01T00:00:00Z")];
        assert_eq!(best_match("USB-3DBenchy_Fast_12m.gcode", &cands), None);
    }
}
