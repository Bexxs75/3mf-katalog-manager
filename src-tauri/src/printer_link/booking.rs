//! Abbuchen: Gramm berechnen, Spule/Material pruefen, Bestaetigen/Ignorieren.

use std::f64::consts::PI;

use crate::db::printers::round_tenth;

/// Erstes Wort in Kleinbuchstaben: "PLA Matt" -> "pla", "PETG-CF" -> "petg".
pub fn normalize_material(m: &str) -> String {
    m.trim()
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '-' || c == ';' || c == '+')
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn density_g_cm3(material: &str) -> f64 {
    match normalize_material(material).as_str() {
        "pla" => 1.24,
        "petg" => 1.27,
        "abs" => 1.04,
        "asa" => 1.07,
        "tpu" => 1.21,
        "pa" => 1.14,
        "pc" => 1.20,
        _ => 1.24,
    }
}

/// Verbrauch in Gramm, auf 0,1 g gerundet. Bevorzugt die Slicer-Angaben
/// (Gewicht x gefoerdert / geschaetzte Laenge), sonst Laenge x Querschnitt x
/// Dichte.
pub fn grams(
    used_mm: f64,
    slicer_total_mm: Option<f64>,
    slicer_weight_g: Option<f64>,
    spool_diameter_mm: f64,
    spool_material: &str,
) -> f64 {
    let raw = match (slicer_weight_g, slicer_total_mm) {
        (Some(w), Some(t)) if w > 0.0 && t > 0.0 => w * used_mm / t,
        _ => {
            let r = spool_diameter_mm / 2.0;
            used_mm * PI * r * r * density_g_cm3(spool_material) / 1000.0
        }
    };
    round_tenth(raw.max(0.0))
}

pub fn partial_percent(used_mm: f64, slicer_total_mm: Option<f64>) -> Option<u8> {
    slicer_total_mm
        .filter(|t| *t > 0.0)
        .map(|t| (used_mm / t * 100.0).round().clamp(0.0, 100.0) as u8)
}

pub fn materials_match(job_material: Option<&str>, spool_material: &str) -> bool {
    match job_material {
        None => true,
        Some(m) => normalize_material(m) == normalize_material(spool_material),
    }
}

#[cfg(test)]
mod pure_tests {
    use super::*;

    #[test]
    fn grams_follow_the_slicer_weight_of_the_sv08_prints() {
        assert_eq!(grams(303.265800000015, Some(279.17), Some(0.83), 1.75, "PLA"), 0.9);
        assert_eq!(grams(18464.74639999127, Some(18440.65), Some(55.0), 1.75, "PLA"), 55.1);
        assert_eq!(grams(1331.9600600000035, Some(3440.41), Some(10.26), 1.75, "PLA"), 4.0);
    }

    #[test]
    fn grams_fall_back_to_length_diameter_and_density() {
        // 1000 mm * pi * 0.875^2 = 2405.28 mm^3 = 2.405 cm^3 * 1.24 = 2.98 g
        assert_eq!(grams(1000.0, None, None, 1.75, "PLA"), 3.0);
        assert_eq!(grams(1000.0, Some(0.0), Some(5.0), 1.75, "PLA Matt"), 3.0);
        assert_eq!(grams(1000.0, None, None, 1.75, "PETG-CF"), 3.1);
    }

    #[test]
    fn tiny_or_negative_amounts_become_zero() {
        assert_eq!(grams(0.01, None, None, 1.75, "PLA"), 0.0);
        assert_eq!(grams(-5.0, None, None, 1.75, "PLA"), 0.0);
    }

    #[test]
    fn densities_by_material() {
        assert_eq!(density_g_cm3("PETG"), 1.27);
        assert_eq!(density_g_cm3("abs"), 1.04);
        assert_eq!(density_g_cm3("ASA"), 1.07);
        assert_eq!(density_g_cm3("TPU 95A"), 1.21);
        assert_eq!(density_g_cm3("PA-CF"), 1.14);
        assert_eq!(density_g_cm3("PC"), 1.20);
        assert_eq!(density_g_cm3("Holz"), 1.24);
    }

    #[test]
    fn material_matching_ignores_variants() {
        assert!(materials_match(Some("PLA"), "PLA Matt"));
        assert!(materials_match(Some("petg"), "PETG-CF"));
        assert!(!materials_match(Some("PETG"), "PLA"));
        assert!(materials_match(None, "PLA"));
    }

    #[test]
    fn partial_percent_from_slicer_estimate() {
        assert_eq!(partial_percent(1331.96, Some(3440.41)), Some(39));
        assert_eq!(partial_percent(5000.0, Some(3440.41)), Some(100));
        assert_eq!(partial_percent(10.0, None), None);
    }
}
