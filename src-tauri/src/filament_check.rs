//! "Reicht das Filament?": vergleicht den Filamentbedarf gesliceter Modelle
//! mit den Spulen im Lager bzw. in den Druckerfaechern. Reine Funktionen ohne
//! DB-Zugriff; den Tauri-Befehl gibt es in `commands/filament.rs`.

use std::collections::HashMap;

use serde::Serialize;

use crate::threemf::SliceInfo;

/// Groesster CIEDE2000-Abstand, bei dem zwei Farben noch als "dieselbe" gelten
/// (Rot/Weinrot ~12,8 ja, Weiss/Beige ~15,1 nein).
pub const COLOR_MATCH_MAX_DELTA_E: f64 = 14.0;

/// Toleranz fuer Gewichtsvergleiche in Gramm, gegen Rundungsrauschen aus
/// aufsummierten/abgezogenen Fliesskommazahlen (z. B. 3 x 11.1 g != exakt 33.3 g).
const EPS: f64 = 1e-6;

/// "#RRGGBB" oder "#RRGGBBAA" (Gross/klein egal, Leerzeichen rundherum egal)
/// -> "#RRGGBB" in Grossbuchstaben; sonst None.
pub(crate) fn color_key(hex: &str) -> Option<String> {
    let h = hex.trim().strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", h[..6].to_ascii_uppercase()))
}

fn parse_rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let key = color_key(hex)?;
    let c = |i: usize| u8::from_str_radix(&key[i..i + 2], 16).ok().map(|v| f64::from(v) / 255.0);
    Some((c(1)?, c(3)?, c(5)?))
}

fn to_lab((r, g, b): (f64, f64, f64)) -> (f64, f64, f64) {
    let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let (r, g, b) = (lin(r), lin(g), lin(b));
    let x = (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047;
    let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
    let z = (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883;
    let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

/// Farbabstand CIEDE2000 zweier Hex-Farben; None bei ungueltigem Hex.
pub fn delta_e_2000(a: &str, b: &str) -> Option<f64> {
    let (l1, a1, b1) = to_lab(parse_rgb(a)?);
    let (l2, a2, b2) = to_lab(parse_rgb(b)?);
    let pow7 = |v: f64| v.powi(7);
    let c1 = a1.hypot(b1);
    let c2 = a2.hypot(b2);
    let cm = (c1 + c2) / 2.0;
    let g = 0.5 * (1.0 - (pow7(cm) / (pow7(cm) + pow7(25.0))).sqrt());
    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = a1p.hypot(b1);
    let c2p = a2p.hypot(b2);
    let hue = |b: f64, ap: f64| {
        let h = b.atan2(ap).to_degrees();
        if h < 0.0 { h + 360.0 } else { h }
    };
    let h1 = hue(b1, a1p);
    let h2 = hue(b2, a2p);
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh_deg = if c1p * c2p == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 > h1 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let dh = 2.0 * (c1p * c2p).sqrt() * (dh_deg.to_radians() / 2.0).sin();
    let lm = (l1 + l2) / 2.0;
    let cmp = (c1p + c2p) / 2.0;
    let hm = if c1p * c2p == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (hm - 30.0).to_radians().cos() + 0.24 * (2.0 * hm).to_radians().cos()
        + 0.32 * (3.0 * hm + 6.0).to_radians().cos()
        - 0.20 * (4.0 * hm - 63.0).to_radians().cos();
    let dtheta = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (pow7(cmp) / (pow7(cmp) + pow7(25.0))).sqrt();
    let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cmp;
    let sh = 1.0 + 0.015 * cmp * t;
    let rt = -(2.0 * dtheta).to_radians().sin() * rc;
    Some(((dl / sl).powi(2) + (dc / sc).powi(2) + (dh / sh).powi(2) + rt * (dc / sc) * (dh / sh)).sqrt())
}

/// Kleinbuchstaben, nur Buchstaben und Ziffern ("PETG-HF" -> "petghf").
pub(crate) fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Passt eine Spule im Material? "PLA Silk" enthaelt "PLA", "PETG HF" == "PETG-HF".
pub fn material_matches(spool_material: &str, filament_type: &str) -> bool {
    let wanted = norm(filament_type);
    !wanted.is_empty() && norm(spool_material).contains(&wanted)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Swap,
    Short,
    Unknown,
    NoData,
}

impl CheckStatus {
    /// Rangfolge fuer den Modellstatus: der schlechteste Bedarf gewinnt.
    fn severity(self) -> u8 {
        match self {
            CheckStatus::Ok | CheckStatus::NoData => 0,
            CheckStatus::Swap => 1,
            CheckStatus::Unknown => 2,
            CheckStatus::Short => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotRef {
    pub printer: String,
    pub unit: String,
    pub slot_number: i64,
}

#[derive(Debug, Clone)]
pub struct SpoolInput {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color_name: Option<String>,
    pub color_hex: Option<String>,
    pub remaining_g: f64,
    pub original_g: f64,
    pub slot: Option<SlotRef>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpoolUse {
    pub spool_id: String,
    pub label: String,
    pub color_name: Option<String>,
    pub remaining_g: f64,
    pub original_g: f64,
    pub slot: Option<SlotRef>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedCheck {
    pub filament_type: String,
    pub color: Option<String>,
    pub needed_g: f64,
    pub status: CheckStatus,
    pub missing_g: f64,
    pub spools: Vec<SpoolUse>,
    pub possible: Vec<SpoolUse>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCheck {
    pub file_id: String,
    pub status: CheckStatus,
    pub needs: Vec<NeedCheck>,
}

const MAX_POSSIBLE: usize = 3;

struct Need {
    filament_type: String,
    color: Option<String>,
    needed_g: f64,
}

/// Summiert den Bedarf ueber alle Platten, gruppiert nach (norm(Typ), Farbe);
/// Reihenfolge = erstes Auftreten.
fn collect_needs(slice: &SliceInfo) -> Vec<Need> {
    let mut needs: Vec<Need> = Vec::new();
    for plate in &slice.plates {
        for f in &plate.filaments {
            if f.filament_type.trim().is_empty() || f.used_g <= 0.0 {
                continue;
            }
            let color = f.color.as_deref().and_then(color_key);
            let key = norm(&f.filament_type);
            match needs.iter_mut().find(|n| norm(&n.filament_type) == key && n.color == color) {
                Some(n) => n.needed_g += f.used_g,
                None => needs.push(Need { filament_type: f.filament_type.trim().to_string(), color, needed_g: f.used_g }),
            }
        }
    }
    needs
}

fn spool_label(s: &SpoolInput) -> String {
    let head: Vec<&str> = [s.manufacturer.as_deref(), Some(s.material.as_str())]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut label = head.join(" ");
    if let Some(color) = s.color_name.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        label.push_str(" · ");
        label.push_str(color);
    }
    label
}

fn spool_use(s: &SpoolInput, available: f64) -> SpoolUse {
    SpoolUse {
        spool_id: s.id.clone(),
        label: spool_label(s),
        color_name: s.color_name.clone(),
        remaining_g: available,
        original_g: s.original_g,
        slot: s.slot.clone(),
        location: if s.slot.is_some() { None } else { s.location.clone().filter(|l| !l.trim().is_empty()) },
    }
}

/// Noch verfuegbares Gewicht einer Spule im gedachten Abbuchungsstand.
fn left(s: &SpoolInput, available: &HashMap<String, f64>) -> f64 {
    available.get(&s.id).copied().unwrap_or(0.0)
}

fn check_need(need: &Need, spools: &[SpoolInput], available: &mut HashMap<String, f64>) -> NeedCheck {
    // Kandidaten werden am gespeicherten remaining_g gefiltert, nicht am schon
    // gedanklich abgebuchten Stand (left()): eine Spule, die ein frueherer
    // Warteschlangen-Eintrag geleert hat, bleibt Kandidat, damit ein spaeterer
    // Bedarf als "short" (mit korrektem Fehlbetrag) statt faelschlich
    // "unknown" gemeldet wird. left() bleibt die Quelle fuer die Betraege.
    let material_ok: Vec<&SpoolInput> = spools
        .iter()
        .filter(|s| s.remaining_g > 0.0 && material_matches(&s.material, &need.filament_type))
        .collect();

    let spool_color = |s: &SpoolInput| s.color_hex.as_deref().and_then(color_key);
    let mut hits: Vec<&SpoolInput> = match &need.color {
        Some(want) => material_ok
            .iter()
            .copied()
            .filter(|s| {
                spool_color(s)
                    .and_then(|have| delta_e_2000(want, &have))
                    .is_some_and(|d| d <= COLOR_MATCH_MAX_DELTA_E)
            })
            .collect(),
        None => Vec::new(),
    };
    hits.sort_by(|a, b| {
        b.slot.is_some()
            .cmp(&a.slot.is_some())
            .then(left(b, available).total_cmp(&left(a, available)))
            .then(a.id.cmp(&b.id))
    });

    let mut result = NeedCheck {
        filament_type: need.filament_type.clone(),
        color: need.color.clone(),
        needed_g: need.needed_g,
        status: CheckStatus::Unknown,
        missing_g: 0.0,
        spools: Vec::new(),
        possible: Vec::new(),
    };

    if hits.is_empty() {
        let mut possible: Vec<&SpoolInput> = material_ok
            .into_iter()
            .filter(|s| need.color.is_none() || spool_color(s).is_none())
            .collect();
        possible.sort_by(|a, b| left(b, available).total_cmp(&left(a, available)).then(a.id.cmp(&b.id)));
        result.possible = possible.into_iter().take(MAX_POSSIBLE).map(|s| spool_use(s, left(s, available))).collect();
        return result;
    }

    if let Some(single) = hits.iter().copied().find(|s| left(s, available) >= need.needed_g - EPS) {
        let before = left(single, available);
        result.status = CheckStatus::Ok;
        result.spools.push(spool_use(single, before));
        // .max(0.0): before kann wegen der EPS-Toleranz oben minimal unter
        // need.needed_g liegen, das Ergebnis bleibt nicht negativ.
        available.insert(single.id.clone(), (before - need.needed_g).max(0.0));
        return result;
    }

    let total: f64 = hits.iter().map(|s| left(s, available)).sum();
    if total >= need.needed_g - EPS {
        result.status = CheckStatus::Swap;
        let mut still = need.needed_g;
        for s in hits {
            if still <= EPS {
                break;
            }
            let before = left(s, available);
            // Eine bereits geleerte Spule (z. B. durch einen frueheren
            // Warteschlangen-Eintrag) wird nicht als Wechselpartner genannt.
            if before <= 0.0 {
                continue;
            }
            let take = before.min(still);
            result.spools.push(spool_use(s, before));
            available.insert(s.id.clone(), before - take);
            still -= take;
        }
    } else {
        result.status = CheckStatus::Short;
        result.missing_g = (need.needed_g - total).max(0.0);
        // Alle Treffer werden genannt, auch eine dabei bereits geleerte Spule
        // (remaining 0) - konsistent mit "alle Treffer werden auf 0 abgebucht".
        for s in hits {
            result.spools.push(spool_use(s, left(s, available)));
            available.insert(s.id.clone(), 0.0);
        }
    }
    result
}

/// Prueft Modelle in der uebergebenen Reihenfolge (= Warteschlange) mit einem
/// gemeinsamen, nur gedachten Abbuchungsstand. Aendert nichts in der DB.
pub fn check_models(models: &[(String, Option<SliceInfo>)], spools: &[SpoolInput]) -> Vec<ModelCheck> {
    let mut available: HashMap<String, f64> = spools.iter().map(|s| (s.id.clone(), s.remaining_g.max(0.0))).collect();
    models
        .iter()
        .map(|(file_id, slice)| {
            let needs: Vec<NeedCheck> = slice
                .as_ref()
                .map(collect_needs)
                .unwrap_or_default()
                .iter()
                .map(|n| check_need(n, spools, &mut available))
                .collect();
            let status = if needs.is_empty() {
                CheckStatus::NoData
            } else {
                needs.iter().map(|n| n.status).max_by_key(|s| s.severity()).unwrap_or(CheckStatus::Ok)
            };
            ModelCheck { file_id: file_id.clone(), status, needs }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn de(a: &str, b: &str) -> f64 {
        delta_e_2000(a, b).expect("gueltige Farben")
    }

    #[test]
    fn delta_e_matches_reference_values() {
        for (a, b, expected) in [
            ("#C0392B", "#B03020", 4.1),
            ("#C0392B", "#8B1A1A", 12.8),
            ("#000000", "#2E2E2E", 11.8),
            ("#3A7BD5", "#2E86C1", 6.9),
            ("#C0392B", "#E67E22", 25.0),
            ("#000000", "#545454", 24.2),
            ("#FFFFFF", "#E8D5B7", 15.1),
            ("#3A7BD5", "#5DADE2", 16.6),
        ] {
            let got = de(a, b);
            assert!((got - expected).abs() <= 0.3, "{a}/{b}: {got} statt ~{expected}");
        }
    }

    #[test]
    fn reference_pairs_fall_on_the_expected_side_of_the_threshold() {
        assert!(de("#C0392B", "#8B1A1A") <= COLOR_MATCH_MAX_DELTA_E);
        assert!(de("#000000", "#2E2E2E") <= COLOR_MATCH_MAX_DELTA_E);
        assert!(de("#FFFFFF", "#E8D5B7") > COLOR_MATCH_MAX_DELTA_E);
        assert!(de("#C0392B", "#E67E22") > COLOR_MATCH_MAX_DELTA_E);
    }

    #[test]
    fn delta_e_is_zero_for_identical_colors_and_ignores_alpha_and_case() {
        assert!(de("#C0392B", "#c0392b") < 1e-9);
        assert!(de("#C0392BFF", "#C0392B") < 1e-9);
    }

    #[test]
    fn delta_e_rejects_invalid_hex() {
        assert_eq!(delta_e_2000("rot", "#C0392B"), None);
        assert_eq!(delta_e_2000("#C0392", "#C0392B"), None);
        assert_eq!(delta_e_2000("#GG392B", "#C0392B"), None);
    }

    #[test]
    fn color_key_normalizes_valid_hex() {
        assert_eq!(color_key("#c0392bff").as_deref(), Some("#C0392B"));
        assert_eq!(color_key(" #C0392B ").as_deref(), Some("#C0392B"));
        assert_eq!(color_key("#C039"), None);
    }

    #[test]
    fn material_matching_ignores_case_spaces_and_dashes() {
        assert!(material_matches("PLA Silk", "PLA"));
        assert!(material_matches("PETG HF", "PETG-HF"));
        assert!(material_matches("pla basic", "PLA"));
        assert!(!material_matches("PETG", "PLA"));
        assert!(!material_matches("PLA", ""));
    }

    fn slice(json: &str) -> Option<SliceInfo> {
        Some(serde_json::from_str(json).expect("slice json"))
    }

    fn one_plate(filaments: &str) -> Option<SliceInfo> {
        slice(&format!(r#"{{"total_weight_g":0,"plates":[{{"plate_index":1,"weight_g":0,"filaments":[{filaments}]}}]}}"#))
    }

    fn fil(t: &str, color: &str, g: f64) -> String {
        format!(r#"{{"filament_type":"{t}","color":"{color}","used_g":{g},"used_m":1}}"#)
    }

    fn spool(id: &str, material: &str, hex: Option<&str>, g: f64, loaded: bool) -> SpoolInput {
        SpoolInput {
            id: id.to_string(),
            material: material.to_string(),
            manufacturer: Some("Bambu".to_string()),
            color_name: Some("Rot".to_string()),
            color_hex: hex.map(str::to_string),
            remaining_g: g,
            original_g: 1000.0,
            slot: loaded.then(|| SlotRef { printer: "X1C".into(), unit: "AMS 1".into(), slot_number: 2 }),
            location: (!loaded).then(|| "Regal A".to_string()),
        }
    }

    fn check_one(model: Option<SliceInfo>, spools: &[SpoolInput]) -> ModelCheck {
        check_models(&[("1".to_string(), model)], spools).remove(0)
    }

    const RED: &str = "#C0392B";

    #[test]
    fn ok_when_one_matching_spool_has_enough() {
        let r = check_one(one_plate(&fil("PLA", RED, 100.0)), &[spool("s1", "PLA", Some("#B03020"), 640.0, false)]);
        assert_eq!(r.status, CheckStatus::Ok);
        let need = &r.needs[0];
        assert_eq!(need.status, CheckStatus::Ok);
        assert_eq!(need.spools.len(), 1);
        assert_eq!(need.spools[0].spool_id, "s1");
        assert_eq!(need.spools[0].remaining_g, 640.0);
        assert_eq!(need.spools[0].label, "Bambu PLA · Rot");
        assert_eq!(need.spools[0].location.as_deref(), Some("Regal A"));
    }

    #[test]
    fn loaded_spool_is_preferred_when_it_suffices() {
        let spools = [spool("big", "PLA", Some(RED), 900.0, false), spool("ams", "PLA", Some(RED), 200.0, true)];
        let r = check_one(one_plate(&fil("PLA", RED, 150.0)), &spools);
        assert_eq!(r.needs[0].spools[0].spool_id, "ams");
        assert!(r.needs[0].spools[0].slot.is_some());
    }

    #[test]
    fn larger_unloaded_spool_is_used_when_loaded_one_is_too_small() {
        let spools = [spool("big", "PLA", Some(RED), 900.0, false), spool("ams", "PLA", Some(RED), 50.0, true)];
        let r = check_one(one_plate(&fil("PLA", RED, 150.0)), &spools);
        assert_eq!(r.needs[0].status, CheckStatus::Ok);
        assert_eq!(r.needs[0].spools[0].spool_id, "big");
    }

    #[test]
    fn swap_when_only_the_sum_of_spools_suffices() {
        let spools = [spool("a", "PLA", Some(RED), 100.0, true), spool("b", "PLA", Some(RED), 100.0, false)];
        let r = check_one(one_plate(&fil("PLA", RED, 150.0)), &spools);
        assert_eq!(r.status, CheckStatus::Swap);
        let ids: Vec<&str> = r.needs[0].spools.iter().map(|s| s.spool_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn short_reports_the_missing_amount() {
        let r = check_one(one_plate(&fil("PLA", RED, 150.0)), &[spool("a", "PLA", Some(RED), 40.0, false)]);
        assert_eq!(r.status, CheckStatus::Short);
        assert!((r.needs[0].missing_g - 110.0).abs() < 1e-9);
    }

    #[test]
    fn wrong_color_does_not_count_and_yields_unknown() {
        let r = check_one(one_plate(&fil("PLA", RED, 10.0)), &[spool("orange", "PLA", Some("#E67E22"), 900.0, false)]);
        assert_eq!(r.status, CheckStatus::Unknown);
        assert!(r.needs[0].spools.is_empty());
        assert!(r.needs[0].possible.is_empty());
    }

    #[test]
    fn spool_without_color_is_only_a_possible_match() {
        let r = check_one(one_plate(&fil("PLA", RED, 10.0)), &[spool("nocolor", "PLA", None, 900.0, false)]);
        assert_eq!(r.needs[0].status, CheckStatus::Unknown);
        assert_eq!(r.needs[0].possible.len(), 1);
        assert_eq!(r.needs[0].possible[0].spool_id, "nocolor");
    }

    #[test]
    fn filament_without_color_makes_every_material_match_possible_only() {
        let json = r#"{"filament_type":"PLA","color":null,"used_g":10,"used_m":1}"#;
        let r = check_one(one_plate(json), &[spool("a", "PLA", Some(RED), 900.0, false)]);
        assert_eq!(r.needs[0].status, CheckStatus::Unknown);
        assert_eq!(r.needs[0].possible.len(), 1);
    }

    #[test]
    fn possible_matches_are_capped_at_three_by_weight() {
        let spools = [
            spool("a", "PLA", None, 100.0, false),
            spool("b", "PLA", None, 400.0, false),
            spool("c", "PLA", None, 300.0, false),
            spool("d", "PLA", None, 200.0, false),
        ];
        let r = check_one(one_plate(&fil("PLA", RED, 10.0)), &spools);
        let ids: Vec<&str> = r.needs[0].possible.iter().map(|s| s.spool_id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "d"]);
    }

    #[test]
    fn empty_spools_are_ignored() {
        let r = check_one(one_plate(&fil("PLA", RED, 10.0)), &[spool("empty", "PLA", Some(RED), 0.0, false)]);
        assert_eq!(r.status, CheckStatus::Unknown);
        assert!(r.needs[0].possible.is_empty());
    }

    #[test]
    fn need_is_summed_over_plates_and_split_by_color() {
        let json = format!(
            r#"{{"total_weight_g":0,"plates":[
                {{"plate_index":1,"weight_g":0,"filaments":[{},{}]}},
                {{"plate_index":2,"weight_g":0,"filaments":[{}]}}]}}"#,
            fil("PLA", RED, 50.0),
            fil("PLA", "#000000", 5.0),
            fil("PLA", "#c0392bff", 30.0)
        );
        let r = check_one(slice(&json), &[]);
        assert_eq!(r.needs.len(), 2);
        assert_eq!(r.needs[0].color.as_deref(), Some("#C0392B"));
        assert!((r.needs[0].needed_g - 80.0).abs() < 1e-9);
        assert_eq!(r.needs[1].color.as_deref(), Some("#000000"));
    }

    #[test]
    fn empty_type_and_zero_usage_are_ignored_and_yield_no_data() {
        let r = check_one(one_plate(&format!("{},{}", fil("", RED, 10.0), fil("PLA", RED, 0.0))), &[]);
        assert_eq!(r.status, CheckStatus::NoData);
        assert!(r.needs.is_empty());
    }

    #[test]
    fn model_without_slice_info_has_no_data() {
        assert_eq!(check_one(None, &[]).status, CheckStatus::NoData);
    }

    #[test]
    fn model_status_is_the_worst_need_status() {
        let json = format!("{},{}", fil("PLA", RED, 10.0), fil("PETG", "#000000", 10.0));
        let spools = [spool("a", "PLA", Some(RED), 900.0, false), spool("b", "PETG", Some("#000000"), 5.0, false)];
        assert_eq!(check_one(one_plate(&json), &spools).status, CheckStatus::Short);
    }

    #[test]
    fn short_outranks_unknown() {
        let json = format!("{},{}", fil("PLA", RED, 100.0), fil("TPU", "#3A7BD5", 5.0));
        let spools = [spool("a", "PLA", Some(RED), 10.0, false)];
        assert_eq!(check_one(one_plate(&json), &spools).status, CheckStatus::Short);
    }

    #[test]
    fn queue_deducts_cumulatively_in_order() {
        let m = || one_plate(&fil("PLA", RED, 200.0));
        let models = vec![("1".to_string(), m()), ("2".to_string(), m()), ("3".to_string(), m())];
        let r = check_models(&models, &[spool("a", "PLA", Some(RED), 500.0, false)]);
        let statuses: Vec<CheckStatus> = r.iter().map(|m| m.status).collect();
        assert_eq!(statuses, vec![CheckStatus::Ok, CheckStatus::Ok, CheckStatus::Short]);
        assert!((r[2].needs[0].missing_g - 100.0).abs() < 1e-9);
        assert_eq!(r[1].needs[0].spools[0].remaining_g, 300.0);
    }

    #[test]
    fn status_serializes_as_snake_case() {
        assert_eq!(serde_json::to_string(&CheckStatus::NoData).unwrap(), "\"no_data\"");
        assert_eq!(serde_json::to_string(&CheckStatus::Swap).unwrap(), "\"swap\"");
    }

    #[test]
    fn drained_spool_still_counts_as_short_instead_of_unknown() {
        // Review-Fund 1: eine durch den ersten Warteschlangen-Eintrag komplett
        // geleerte Spule muss beim zweiten Eintrag als "short" (mit korrektem
        // Fehlbetrag) gemeldet werden, nicht als "unknown".
        let models = vec![
            ("1".to_string(), one_plate(&fil("PLA", RED, 150.0))),
            ("2".to_string(), one_plate(&fil("PLA", RED, 50.0))),
        ];
        let r = check_models(&models, &[spool("a", "PLA", Some(RED), 100.0, false)]);
        let statuses: Vec<CheckStatus> = r.iter().map(|m| m.status).collect();
        assert_eq!(statuses, vec![CheckStatus::Short, CheckStatus::Short]);
        assert!((r[0].needs[0].missing_g - 50.0).abs() < 1e-9);
        assert!((r[1].needs[0].missing_g - 50.0).abs() < 1e-9);
    }

    #[test]
    fn spool_emptied_by_ok_entry_yields_short_not_unknown_for_the_next_entry() {
        // Review-Fund 1, zweites Beispiel: die erste Pruefung passt genau
        // ("ok"), die zweite muss trotzdem den fehlenden Betrag als "short"
        // melden statt "unknown", weil die Spule als Kandidat bestehen bleibt.
        let models = vec![
            ("1".to_string(), one_plate(&fil("PLA", RED, 100.0))),
            ("2".to_string(), one_plate(&fil("PLA", RED, 30.0))),
        ];
        let r = check_models(&models, &[spool("a", "PLA", Some(RED), 100.0, false)]);
        let statuses: Vec<CheckStatus> = r.iter().map(|m| m.status).collect();
        assert_eq!(statuses, vec![CheckStatus::Ok, CheckStatus::Short]);
        assert!((r[1].needs[0].missing_g - 30.0).abs() < 1e-9);
    }

    #[test]
    fn drained_spool_is_not_listed_as_a_swap_partner() {
        // Review-Fund 1 (Swap-Zweig): eine bereits geleerte Spule zaehlt als
        // Kandidat (Material passt, gespeichertes remaining_g > 0), darf im
        // Swap-Fall aber nicht mit 0 g als Wechselpartner aufgelistet werden.
        // "loaded" ist eingelegt und sortiert deshalb vor den vollen,
        // nicht eingelegten Spulen - genau der Fall, in dem der Swap-Loop sie
        // sonst zuerst anfassen wuerde.
        let spools = [
            spool("loaded", "PLA", Some(RED), 100.0, true),
            spool("full", "PLA", Some(RED), 90.0, false),
            spool("extra", "PLA", Some(RED), 90.0, false),
        ];
        let mut available: HashMap<String, f64> = spools.iter().map(|s| (s.id.clone(), s.remaining_g)).collect();
        available.insert("loaded".to_string(), 0.0); // durch einen frueheren Eintrag schon geleert
        let need = Need { filament_type: "PLA".to_string(), color: color_key(RED), needed_g: 150.0 };
        let result = check_need(&need, &spools, &mut available);
        assert_eq!(result.status, CheckStatus::Swap);
        let ids: Vec<&str> = result.spools.iter().map(|s| s.spool_id.as_str()).collect();
        assert_eq!(ids, vec!["extra", "full"], "'loaded' ist geleert und darf nicht auftauchen");
    }

    #[test]
    fn tiny_float_rounding_still_counts_as_ok() {
        // Review-Fund 3: 3 x 11.1 g summiert kann minimal von 33.3 abweichen.
        let json = format!("{},{},{}", fil("PLA", RED, 11.1), fil("PLA", RED, 11.1), fil("PLA", RED, 11.1));
        let r = check_one(one_plate(&json), &[spool("a", "PLA", Some(RED), 33.3, false)]);
        assert_eq!(r.status, CheckStatus::Ok);
    }

    #[test]
    fn spool_use_carries_the_original_weight() {
        let r = check_one(one_plate(&fil("PLA", RED, 100.0)), &[spool("s1", "PLA", Some(RED), 640.0, false)]);
        assert_eq!(r.needs[0].spools[0].original_g, 1000.0);
    }
}
