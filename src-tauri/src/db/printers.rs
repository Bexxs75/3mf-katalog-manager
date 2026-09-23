// Drucker, Mehrfarbeinheiten (AMS, CFS, MMU, externe Spule …) und die
// Belegung ihrer Faecher mit Spulen aus dem Filament-Lager. Eine Spule steckt
// entweder in genau einem Fach (`unit_id` + `slot_index` gesetzt, ihr
// Lagerort liegt dann als Stammplatz in `home_location`) oder liegt im Lager
// (`location`). Nur die Funktionen hier aendern diese Zuordnung.

use rusqlite::Connection;

use super::error::DbError;

/// Farbnamen (deutsch/englisch, klein geschrieben) → Farbwert. Dieselben
/// Werte wie die Palette im Frontend (`src/lib/filamentColors.ts`).
const COLOR_NAMES: &[(&str, &str)] = &[
    ("schwarz", "#1a1a1a"),
    ("black", "#1a1a1a"),
    ("weiss", "#f2f2f2"),
    ("weiß", "#f2f2f2"),
    ("white", "#f2f2f2"),
    ("grau", "#8a8d91"),
    ("gray", "#8a8d91"),
    ("grey", "#8a8d91"),
    ("silber", "#c0c0c0"),
    ("silver", "#c0c0c0"),
    ("rot", "#c0392b"),
    ("red", "#c0392b"),
    ("orange", "#e67e22"),
    ("gelb", "#f1c40f"),
    ("yellow", "#f1c40f"),
    ("grün", "#27ae60"),
    ("gruen", "#27ae60"),
    ("green", "#27ae60"),
    ("türkis", "#16a085"),
    ("tuerkis", "#16a085"),
    ("teal", "#16a085"),
    ("turquoise", "#16a085"),
    ("blau", "#2e86de"),
    ("blue", "#2e86de"),
    ("lila", "#8e44ad"),
    ("violett", "#8e44ad"),
    ("purple", "#8e44ad"),
    ("violet", "#8e44ad"),
    ("pink", "#e84393"),
    ("rosa", "#e84393"),
    ("braun", "#8b5a2b"),
    ("brown", "#8b5a2b"),
    ("gold", "#d4af37"),
    ("natur", "#e8d5b5"),
    ("natural", "#e8d5b5"),
    ("beige", "#e8d5b5"),
    ("transparent", "#e8eef0"),
    ("klar", "#e8eef0"),
    ("clear", "#e8eef0"),
];

/// Ordnet einem freien Farbnamen einen Farbwert zu, z.B. "Galaxy Black" →
/// Schwarz, "Dunkelblau" → Blau. Woerter werden von hinten gesucht, weil die
/// eigentliche Farbe meist am Ende steht ("Silk Gold"). Deutsche Komposita
/// ("Dunkelrot") werden ueber das Wortende erkannt.
pub fn color_hex_for_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for word in words.iter().rev() {
        if let Some((_, hex)) = COLOR_NAMES.iter().find(|(key, _)| key == word) {
            return Some(hex);
        }
    }
    for word in words.iter().rev() {
        if let Some((_, hex)) = COLOR_NAMES
            .iter()
            .find(|(key, _)| key.chars().count() >= 3 && word.ends_with(key))
        {
            return Some(hex);
        }
    }
    None
}

/// `#rrggbb` (Gross-/Kleinschreibung egal).
pub fn is_valid_color_hex(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Migrationsschritt: setzt `color_hex` fuer alle Spulen ohne Farbwert, deren
/// Farbname bekannt ist. Idempotent.
pub fn backfill_color_hex(conn: &Connection) -> Result<(), DbError> {
    let mut stmt =
        conn.prepare("SELECT id, color FROM filament_spools WHERE color_hex IS NULL AND color IS NOT NULL")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    for (id, color) in rows {
        if let Some(hex) = color_hex_for_name(&color) {
            conn.execute(
                "UPDATE filament_spools SET color_hex = ?1 WHERE id = ?2",
                rusqlite::params![hex, id],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_names_map_to_hex_values_including_compounds_and_brand_names() {
        assert_eq!(color_hex_for_name("Schwarz"), Some("#1a1a1a"));
        assert_eq!(color_hex_for_name("Galaxy Black"), Some("#1a1a1a"));
        assert_eq!(color_hex_for_name("Silk Gold"), Some("#d4af37"));
        assert_eq!(color_hex_for_name("Dunkelblau"), Some("#2e86de"));
        assert_eq!(color_hex_for_name("Weiß"), Some("#f2f2f2"));
        assert_eq!(color_hex_for_name("Signal-Orange"), Some("#e67e22"));
        assert_eq!(color_hex_for_name("Mystik"), None);
        assert_eq!(color_hex_for_name(""), None);
    }

    #[test]
    fn color_hex_format_is_checked() {
        assert!(is_valid_color_hex("#1a1A1a"));
        assert!(!is_valid_color_hex("1a1a1a"));
        assert!(!is_valid_color_hex("#1a1a1"));
        assert!(!is_valid_color_hex("#1a1a1g"));
        assert!(!is_valid_color_hex("#1a1a1a; DROP"));
    }

    /// Alte Datenbank (vor den Drucker-Migrationen) mit Spulen: nach der
    /// Migration liegen alle Spulen weiter im Lager, bekannte Farbnamen haben
    /// einen Farbwert, die neuen Tabellen und der Fach-Index existieren.
    #[test]
    fn migrating_an_old_catalog_keeps_spools_in_storage_and_backfills_colors() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                created_at TEXT NOT NULL
            );
            INSERT INTO filament_spools (material, color, location, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                VALUES ('PLA', 'Galaxy Black', 'Regal 2', 1.75, 1000, 600, '2026-01-01'),
                       ('PETG', 'Mystik', NULL, 1.75, 1000, 1000, '2026-01-01'),
                       ('ASA', NULL, 'Trockenbox', 1.75, 1000, 900, '2026-01-01');",
        )
        .unwrap();
        let before_new_steps = super::super::migrations::CURRENT_SCHEMA_VERSION - 8;
        conn.pragma_update(None, "user_version", before_new_steps).unwrap();

        crate::db::run_migrations(&mut conn).unwrap();

        type Row = (String, Option<String>, Option<String>, Option<i64>, Option<i64>);
        let rows: Vec<Row> = conn
            .prepare("SELECT material, color_hex, location, unit_id, slot_index FROM filament_spools ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows[0], ("PLA".into(), Some("#1a1a1a".into()), Some("Regal 2".into()), None, None));
        assert_eq!(rows[1], ("PETG".into(), None, None, None, None));
        assert_eq!(rows[2], ("ASA".into(), None, Some("Trockenbox".into()), None, None));

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('printers', 'material_units')"), 2);
        assert_eq!(count("SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_filament_spools_slot'"), 1);
    }
}
