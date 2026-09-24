//! "Reicht das Filament?": vergleicht den Filamentbedarf gesliceter Modelle
//! mit den Spulen im Lager bzw. in den Druckerfaechern. Reine Funktionen ohne
//! DB-Zugriff; den Tauri-Befehl gibt es in `commands/filament.rs`.

/// Groesster CIEDE2000-Abstand, bei dem zwei Farben noch als "dieselbe" gelten
/// (Rot/Weinrot ~12,8 ja, Weiss/Beige ~15,1 nein).
pub const COLOR_MATCH_MAX_DELTA_E: f64 = 14.0;

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
}
