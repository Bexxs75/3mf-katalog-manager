use chrono::{DateTime, Utc};

pub fn format_bytes(bytes: i64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{} MB", format_de_decimal(b / (1024.0 * 1024.0)))
    } else {
        format!("{} GB", format_de_decimal(b / (1024.0 * 1024.0 * 1024.0)))
    }
}

fn format_de_decimal(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

pub fn format_volume_cm3(volume: Option<f64>) -> String {
    match volume {
        Some(v) => format!("{} cm³", format_de_decimal(v)),
        None => "–".to_string(),
    }
}

pub fn format_dimensions(dims: Option<[f64; 3]>) -> String {
    match dims {
        Some([x, y, z]) => format!("{x:.0} × {y:.0} × {z:.0} mm"),
        None => "–".to_string(),
    }
}

pub fn format_date_de(rfc3339: &str) -> String {
    match DateTime::parse_from_rfc3339(rfc3339) {
        Ok(dt) => dt.format("%d.%m.%Y").to_string(),
        Err(_) => "–".to_string(),
    }
}

pub fn format_relative_time_de(rfc3339: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(rfc3339) else {
        return "–".to_string();
    };
    let delta = Utc::now().signed_duration_since(dt.with_timezone(&Utc));

    let minutes = delta.num_minutes();
    if minutes < 1 {
        "gerade eben".to_string()
    } else if minutes < 60 {
        format!("vor {minutes} Min.")
    } else if delta.num_hours() < 24 {
        format!("vor {} Std.", delta.num_hours())
    } else {
        let days = delta.num_days();
        if days == 1 {
            "vor 1 Tag".to_string()
        } else {
            format!("vor {days} Tagen")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_byte_sizes_across_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3,0 MB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2,0 GB");
    }

    #[test]
    fn formats_volume_with_german_decimal_comma() {
        assert_eq!(format_volume_cm3(Some(6.4)), "6,4 cm³");
        assert_eq!(format_volume_cm3(None), "–");
    }

    #[test]
    fn formats_dimensions_as_rounded_triplet() {
        assert_eq!(
            format_dimensions(Some([120.4, 80.0, 39.6])),
            "120 × 80 × 40 mm"
        );
        assert_eq!(format_dimensions(None), "–");
    }

    #[test]
    fn formats_date_in_german_order() {
        assert_eq!(format_date_de("2026-09-08T12:00:00Z"), "08.09.2026");
    }

    #[test]
    fn formats_recent_time_as_relative_hours() {
        let two_hours_ago = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert_eq!(format_relative_time_de(&two_hours_ago), "vor 2 Std.");
    }
}
