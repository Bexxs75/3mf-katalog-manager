// Drucker, Mehrfarbeinheiten (AMS, CFS, MMU, externe Spule …) und die
// Belegung ihrer Faecher mit Spulen aus dem Filament-Lager. Eine Spule steckt
// entweder in genau einem Fach (`unit_id` + `slot_index` gesetzt, ihr
// Lagerort liegt dann als Stammplatz in `home_location`) oder liegt im Lager
// (`location`). Nur die Funktionen hier aendern diese Zuordnung.

use rusqlite::{params, Connection, OptionalExtension};

use super::error::DbError;
use super::models::{MaterialUnitRecord, PrinterRecord};

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

/// Rundet auf eine Nachkommastelle (Gramm-Angaben im Filament-Lager).
pub fn round_tenth(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
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

pub const UNIT_KINDS: &[&str] = &[
    "bambu_ams",
    "bambu_ams_lite",
    "bambu_ams_ht",
    "creality_cfs",
    "prusa_mmu3",
    "anycubic_ace",
    "external",
    "custom",
    "resin_vat",
];

/// Druckerart (v0.14.0). Wird beim Anlegen gewaehlt und ist danach fest.
pub const PRINTER_KIND_FILAMENT: &str = "filament";
pub const PRINTER_KIND_RESIN: &str = "resin";
pub const PRINTER_KINDS: &[&str] = &[PRINTER_KIND_FILAMENT, PRINTER_KIND_RESIN];

/// Einzige Einheit eines Resin-Druckers: die Harzwanne mit genau einem Platz.
/// Sie entsteht nur ueber `insert_resin_vat` und ist weder umbenennbar noch
/// loeschbar. Resin-Flaschen duerfen nur hierhin, Filament nie.
pub const UNIT_KIND_RESIN_VAT: &str = "resin_vat";

const MAX_NAME_LEN: usize = 60;
const MAX_SLOTS: i64 = 16;
/// Bambu nummeriert AMS und AMS lite pro Drucker mit 0-3.
const MAX_BAMBU_AMS: i64 = 4;

/// Feste Fachanzahl der Vorlagen; `None` = frei waehlbar ("Eigene…").
pub fn template_slot_count(kind: &str) -> Option<i64> {
    match kind {
        "bambu_ams" | "bambu_ams_lite" | "creality_cfs" | "anycubic_ace" => Some(4),
        "bambu_ams_ht" | "external" | "resin_vat" => Some(1),
        "prusa_mmu3" => Some(5),
        _ => None,
    }
}

fn uses_bambu_ams_index(kind: &str) -> bool {
    matches!(kind, "bambu_ams" | "bambu_ams_lite")
}

fn clean_name(name: &str) -> Result<String, DbError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DbError::Other("Name darf nicht leer sein".into()));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(DbError::Other(format!("Name darf hoechstens {MAX_NAME_LEN} Zeichen haben")));
    }
    Ok(trimmed.to_string())
}

pub fn list_printers(conn: &Connection) -> Result<Vec<PrinterRecord>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name, kind FROM printers ORDER BY position, id")?;
    let rows = stmt
        .query_map([], |r| Ok(PrinterRecord { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_units(conn: &Connection) -> Result<Vec<MaterialUnitRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, printer_id, name, kind, slot_count, bambu_ams_index
         FROM material_units ORDER BY printer_id, position, id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(MaterialUnitRecord {
                id: r.get(0)?,
                printer_id: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                slot_count: r.get(4)?,
                bambu_ams_index: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn get_unit(conn: &Connection, unit_id: i64) -> Result<MaterialUnitRecord, DbError> {
    list_units(conn)?
        .into_iter()
        .find(|u| u.id == unit_id)
        .ok_or_else(|| DbError::Other(format!("Einheit {unit_id} existiert nicht")))
}

/// Legt einen Filament-Drucker an (Kurzform fuer Tests; die App geht ueber
/// `insert_printer_of_kind`).
#[cfg(test)]
pub fn insert_printer(conn: &Connection, name: &str) -> Result<i64, DbError> {
    insert_printer_of_kind(conn, name, PRINTER_KIND_FILAMENT)
}

/// Legt einen Drucker der Art `kind` ("filament"/"resin") an - ohne
/// Einheiten; die legt der Aufrufer in derselben Transaktion an.
pub fn insert_printer_of_kind(conn: &Connection, name: &str, kind: &str) -> Result<i64, DbError> {
    if !PRINTER_KINDS.contains(&kind) {
        return Err(DbError::Other(format!("unbekannte Druckerart: {kind}")));
    }
    let name = clean_name(name)?;
    conn.execute(
        "INSERT INTO printers (name, kind, position) VALUES (?1, ?2, (SELECT COALESCE(MAX(position), -1) + 1 FROM printers))",
        params![name, kind],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Art des Druckers; Fehler, wenn es ihn nicht gibt.
pub fn printer_kind(conn: &Connection, printer_id: i64) -> Result<String, DbError> {
    conn.query_row("SELECT kind FROM printers WHERE id = ?1", params![printer_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| DbError::Other(format!("Drucker {printer_id} existiert nicht")))
}

/// Druckeranbindung, "Reicht das Filament?" usw. gibt es nur fuer
/// Filament-Drucker. Lehnt Resin-Drucker und unbekannte IDs ab.
pub fn ensure_filament_printer(conn: &Connection, printer_id: i64) -> Result<(), DbError> {
    if printer_kind(conn, printer_id)? == PRINTER_KIND_RESIN {
        return Err(DbError::Other("Resin-Drucker haben keine Druckeranbindung".into()));
    }
    Ok(())
}

/// Legt die Harzwanne (1 Platz) eines Resin-Druckers an. Nur fuer
/// Resin-Drucker und nur einmal pro Drucker. Aufrufer haelt eine Transaktion.
pub fn insert_resin_vat(conn: &Connection, printer_id: i64, name: &str) -> Result<i64, DbError> {
    if printer_kind(conn, printer_id)? != PRINTER_KIND_RESIN {
        return Err(DbError::Other("Eine Harzwanne gibt es nur bei Resin-Druckern".into()));
    }
    let has_units: bool =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM material_units WHERE printer_id = ?1)", params![printer_id], |r| r.get(0))?;
    if has_units {
        return Err(DbError::Other("Ein Resin-Drucker hat genau eine Harzwanne".into()));
    }
    insert_unit_row(conn, printer_id, UNIT_KIND_RESIN_VAT, name, None)
}

pub fn rename_printer(conn: &Connection, printer_id: i64, name: &str) -> Result<(), DbError> {
    let name = clean_name(name)?;
    let changed = conn.execute("UPDATE printers SET name = ?1 WHERE id = ?2", params![name, printer_id])?;
    if changed == 0 {
        return Err(DbError::Other(format!("Drucker {printer_id} existiert nicht")));
    }
    Ok(())
}

/// Legt alle Spulen, die die Bedingung erfuellen, zurueck an ihren
/// Stammplatz. Liefert die Anzahl.
fn return_spools_home(conn: &Connection, where_clause: &str, args: &[&dyn rusqlite::ToSql]) -> Result<usize, DbError> {
    let sql = format!(
        "UPDATE filament_spools
         SET location = home_location, home_location = NULL, unit_id = NULL, slot_index = NULL
         WHERE unit_id IS NOT NULL AND {where_clause}"
    );
    Ok(conn.execute(&sql, args)?)
}

/// Loescht einen Drucker samt Einheiten; deren Spulen kehren vorher an ihren
/// Stammplatz zurueck. Aufrufer muss eine Transaktion halten.
pub fn delete_printer(conn: &Connection, printer_id: i64) -> Result<usize, DbError> {
    let returned = return_spools_home(
        conn,
        "unit_id IN (SELECT id FROM material_units WHERE printer_id = ?1)",
        &[&printer_id],
    )?;
    let deleted = conn.execute("DELETE FROM printers WHERE id = ?1", params![printer_id])?;
    if deleted == 0 {
        return Err(DbError::Other(format!("Drucker {printer_id} existiert nicht")));
    }
    Ok(returned)
}

/// Fuegt einem Filament-Drucker eine Einheit hinzu. Vorlagen haben eine
/// feste Fachanzahl (der Parameter wird dann ignoriert); AMS/AMS lite
/// bekommen die naechste freie Bambu-AMS-Nummer des Druckers. Harzwannen
/// entstehen nur ueber `insert_resin_vat`, Resin-Drucker bekommen keine
/// weiteren Einheiten.
pub fn insert_unit(
    conn: &Connection,
    printer_id: i64,
    kind: &str,
    name: &str,
    slot_count: Option<i64>,
) -> Result<i64, DbError> {
    if kind == UNIT_KIND_RESIN_VAT {
        return Err(DbError::Other("Eine Harzwanne entsteht nur mit dem Resin-Drucker".into()));
    }
    if printer_kind(conn, printer_id)? == PRINTER_KIND_RESIN {
        return Err(DbError::Other("Resin-Drucker haben nur ihre Harzwanne".into()));
    }
    insert_unit_row(conn, printer_id, kind, name, slot_count)
}

fn insert_unit_row(
    conn: &Connection,
    printer_id: i64,
    kind: &str,
    name: &str,
    slot_count: Option<i64>,
) -> Result<i64, DbError> {
    if !UNIT_KINDS.contains(&kind) {
        return Err(DbError::Other(format!("unbekannter Einheitstyp: {kind}")));
    }
    let name = clean_name(name)?;
    let slots = match template_slot_count(kind) {
        Some(fixed) => fixed,
        None => slot_count.ok_or_else(|| DbError::Other("Fachanzahl fehlt".into()))?,
    };
    if !(1..=MAX_SLOTS).contains(&slots) {
        return Err(DbError::Other(format!("Fachanzahl muss zwischen 1 und {MAX_SLOTS} liegen")));
    }
    let printer_exists: bool =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM printers WHERE id = ?1)", params![printer_id], |r| r.get(0))?;
    if !printer_exists {
        return Err(DbError::Other(format!("Drucker {printer_id} existiert nicht")));
    }
    let ams_index = if uses_bambu_ams_index(kind) {
        let used: Vec<i64> = conn
            .prepare("SELECT bambu_ams_index FROM material_units WHERE printer_id = ?1 AND bambu_ams_index IS NOT NULL")?
            .query_map(params![printer_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let free = (0..MAX_BAMBU_AMS).find(|i| !used.contains(i));
        Some(free.ok_or_else(|| {
            DbError::Other(format!("Ein Drucker kann hoechstens {MAX_BAMBU_AMS} AMS haben"))
        })?)
    } else {
        None
    };
    conn.execute(
        "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
         VALUES (?1, ?2, ?3, ?4, ?5,
                 (SELECT COALESCE(MAX(position), -1) + 1 FROM material_units WHERE printer_id = ?1))",
        params![printer_id, name, kind, slots, ams_index],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Aendert Name und (nur bei "custom") Fachanzahl. Spulen in wegfallenden
/// Faechern kehren an ihren Stammplatz zurueck. Aufrufer haelt eine
/// Transaktion.
pub fn update_unit(conn: &Connection, unit_id: i64, name: &str, slot_count: Option<i64>) -> Result<usize, DbError> {
    let unit = get_unit(conn, unit_id)?;
    if unit.kind == UNIT_KIND_RESIN_VAT {
        return Err(DbError::Other("Die Harzwanne kann nicht geaendert werden".into()));
    }
    let name = clean_name(name)?;
    let slots = match template_slot_count(&unit.kind) {
        Some(fixed) => fixed,
        None => slot_count.unwrap_or(unit.slot_count),
    };
    if !(1..=MAX_SLOTS).contains(&slots) {
        return Err(DbError::Other(format!("Fachanzahl muss zwischen 1 und {MAX_SLOTS} liegen")));
    }
    let returned = return_spools_home(conn, "unit_id = ?1 AND slot_index >= ?2", &[&unit_id, &slots])?;
    conn.execute(
        "UPDATE material_units SET name = ?1, slot_count = ?2 WHERE id = ?3",
        params![name, slots, unit_id],
    )?;
    Ok(returned)
}

/// Loescht eine Einheit; ihre Spulen kehren vorher an ihren Stammplatz
/// zurueck. Aufrufer haelt eine Transaktion.
pub fn delete_unit(conn: &Connection, unit_id: i64) -> Result<usize, DbError> {
    if get_unit(conn, unit_id)?.kind == UNIT_KIND_RESIN_VAT {
        return Err(DbError::Other("Die Harzwanne kann nicht geloescht werden".into()));
    }
    let returned = return_spools_home(conn, "unit_id = ?1", &[&unit_id])?;
    conn.execute("DELETE FROM material_units WHERE id = ?1", params![unit_id])?;
    Ok(returned)
}

/// Setzt die Reihenfolge der Einheiten eines Druckers. `unit_ids` muss genau
/// dessen Einheiten enthalten.
pub fn reorder_units(conn: &Connection, printer_id: i64, unit_ids: &[i64]) -> Result<(), DbError> {
    let mut current: Vec<i64> = list_units(conn)?
        .into_iter()
        .filter(|u| u.printer_id == printer_id)
        .map(|u| u.id)
        .collect();
    let mut requested = unit_ids.to_vec();
    current.sort_unstable();
    requested.sort_unstable();
    if current != requested {
        return Err(DbError::Other("Reihenfolge passt nicht zu den Einheiten des Druckers".into()));
    }
    for (position, id) in unit_ids.iter().enumerate() {
        conn.execute("UPDATE material_units SET position = ?1 WHERE id = ?2", params![position as i64, id])?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadOutcome {
    /// Spule, die das Fach vorher belegt hat und an ihren Stammplatz zurueckging.
    pub displaced_spool_id: Option<i64>,
}

/// Legt eine Spule in ein Fach. Belegtes Fach → die bisherige Spule kehrt an
/// ihren Stammplatz zurueck (Tausch). Steckt die Spule schon in einem
/// anderen Fach, wird sie verschoben und behaelt ihren Stammplatz; kommt sie
/// aus dem Lager, wird ihr Lagerort zum Stammplatz. Aufrufer haelt eine
/// Transaktion.
pub fn load_spool(conn: &Connection, spool_id: i64, unit_id: i64, slot_index: i64) -> Result<LoadOutcome, DbError> {
    let unit = get_unit(conn, unit_id)?;
    if slot_index < 0 || slot_index >= unit.slot_count {
        return Err(DbError::Other(format!("Fach {} existiert in {} nicht", slot_index + 1, unit.name)));
    }
    let (current_unit, current_slot, kind): (Option<i64>, Option<i64>, String) = conn
        .query_row(
            "SELECT unit_id, slot_index, kind FROM filament_spools WHERE id = ?1",
            params![spool_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| DbError::Other(format!("Spule {spool_id} existiert nicht")))?;
    // Resin-Flaschen nur in eine Harzwanne, Filament nie (v0.14.0). Eine
    // Harzwanne gibt es nur an Resin-Druckern (`insert_resin_vat`).
    let is_resin = kind == crate::db::models::SPOOL_KIND_RESIN;
    let is_vat = unit.kind == UNIT_KIND_RESIN_VAT;
    if is_resin && !is_vat {
        return Err(DbError::Other("Resin-Flaschen passen nur in die Harzwanne eines Resin-Druckers".to_string()));
    }
    if !is_resin && is_vat {
        return Err(DbError::Other("Filament-Spulen passen nicht in eine Harzwanne".to_string()));
    }
    if current_unit == Some(unit_id) && current_slot == Some(slot_index) {
        return Ok(LoadOutcome { displaced_spool_id: None });
    }
    let occupant: Option<i64> = conn
        .query_row(
            "SELECT id FROM filament_spools WHERE unit_id = ?1 AND slot_index = ?2",
            params![unit_id, slot_index],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(occupant_id) = occupant {
        return_spools_home(conn, "id = ?1", &[&occupant_id])?;
    }
    if current_unit.is_some() {
        conn.execute(
            "UPDATE filament_spools SET unit_id = ?1, slot_index = ?2 WHERE id = ?3",
            params![unit_id, slot_index, spool_id],
        )?;
    } else {
        conn.execute(
            "UPDATE filament_spools
             SET home_location = location, location = NULL, unit_id = ?1, slot_index = ?2
             WHERE id = ?3",
            params![unit_id, slot_index, spool_id],
        )?;
    }
    Ok(LoadOutcome { displaced_spool_id: occupant })
}

/// Nimmt eine Spule aus ihrem Fach. Ziel ist `location` (falls nicht leer)
/// oder ihr Stammplatz. Liefert den neuen Lagerort. Aufrufer haelt eine
/// Transaktion.
pub fn unload_spool(conn: &Connection, spool_id: i64, location: Option<&str>) -> Result<Option<String>, DbError> {
    let (unit_id, home): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT unit_id, home_location FROM filament_spools WHERE id = ?1",
            params![spool_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| DbError::Other(format!("Spule {spool_id} existiert nicht")))?;
    if unit_id.is_none() {
        return Err(DbError::Other("Spule steckt in keinem Fach".into()));
    }
    let target = match location.map(str::trim).filter(|l| !l.is_empty()) {
        Some(l) => Some(l.to_string()),
        None => home,
    };
    conn.execute(
        "UPDATE filament_spools
         SET location = ?1, home_location = NULL, unit_id = NULL, slot_index = NULL
         WHERE id = ?2",
        params![target, spool_id],
    )?;
    Ok(target)
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
        conn.pragma_update(None, "user_version", super::super::migrations::FIRST_PRINTER_MIGRATION_VERSION).unwrap();

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

    fn spool(conn: &Connection, material: &str, location: Option<&str>) -> i64 {
        crate::db::insert_filament_spool(
            conn,
            &crate::db::models::NewFilamentSpool {
                material: material.to_string(),
                manufacturer: None,
                color: None,
                location: location.map(str::to_string),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 800.0,
                price: None,
                image_png: None,
                color_hex: None,
                kind: "filament".into(),
            },
        )
        .unwrap()
    }

    /// (location, home_location, unit_id, slot_index)
    fn placement(conn: &Connection, id: i64) -> (Option<String>, Option<String>, Option<i64>, Option<i64>) {
        conn.query_row(
            "SELECT location, home_location, unit_id, slot_index FROM filament_spools WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    }

    fn x1c_with_two_ams(conn: &Connection) -> (i64, i64, i64) {
        let printer = insert_printer(conn, "X1C").unwrap();
        let ams_a = insert_unit(conn, printer, "bambu_ams", "AMS A", None).unwrap();
        let ams_b = insert_unit(conn, printer, "bambu_ams", "AMS B", None).unwrap();
        (printer, ams_a, ams_b)
    }

    #[test]
    fn units_get_template_slot_counts_and_consecutive_bambu_ams_numbers() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, ams_a, ams_b) = x1c_with_two_ams(&conn);
        let custom = insert_unit(&conn, printer, "custom", "Box Turtle", Some(8)).unwrap();
        let mmu = insert_unit(&conn, printer, "prusa_mmu3", "MMU", Some(2)).unwrap();

        let units = list_units(&conn).unwrap();
        let by_id = |id: i64| units.iter().find(|u| u.id == id).unwrap().clone();
        assert_eq!((by_id(ams_a).slot_count, by_id(ams_a).bambu_ams_index), (4, Some(0)));
        assert_eq!((by_id(ams_b).slot_count, by_id(ams_b).bambu_ams_index), (4, Some(1)));
        assert_eq!((by_id(custom).slot_count, by_id(custom).bambu_ams_index), (8, None));
        assert_eq!(by_id(mmu).slot_count, 5, "Vorlagen ignorieren eine abweichende Fachanzahl");
        assert_eq!(
            units.iter().filter(|u| u.printer_id == printer).map(|u| u.id).collect::<Vec<_>>(),
            vec![ams_a, ams_b, custom, mmu],
            "neue Einheiten kommen ans Ende"
        );
    }

    #[test]
    fn invalid_units_and_names_are_rejected() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, _, _) = x1c_with_two_ams(&conn);
        insert_unit(&conn, printer, "bambu_ams_lite", "C", None).unwrap();
        insert_unit(&conn, printer, "bambu_ams", "D", None).unwrap();
        assert!(insert_unit(&conn, printer, "bambu_ams", "E", None).is_err(), "hoechstens 4 AMS");
        assert!(insert_unit(&conn, printer, "toaster", "X", None).is_err());
        assert!(insert_unit(&conn, printer, "custom", "X", None).is_err(), "Eigene braucht Fachanzahl");
        assert!(insert_unit(&conn, printer, "custom", "X", Some(17)).is_err());
        assert!(insert_unit(&conn, 999, "external", "X", None).is_err());
        assert!(insert_printer(&conn, "   ").is_err());
        assert!(insert_printer(&conn, &"x".repeat(61)).is_err());
    }

    #[test]
    fn loading_from_storage_remembers_the_home_location() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));

        let outcome = load_spool(&conn, pla, ams_a, 2).unwrap();

        assert_eq!(outcome.displaced_spool_id, None);
        assert_eq!(placement(&conn, pla), (None, Some("Regal 2".into()), Some(ams_a), Some(2)));
    }

    #[test]
    fn loading_into_an_occupied_slot_swaps_and_sends_the_old_spool_home() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));
        let petg = spool(&conn, "PETG", Some("Trockenbox"));
        load_spool(&conn, pla, ams_a, 0).unwrap();

        let outcome = load_spool(&conn, petg, ams_a, 0).unwrap();

        assert_eq!(outcome.displaced_spool_id, Some(pla));
        assert_eq!(placement(&conn, pla), (Some("Regal 2".into()), None, None, None));
        assert_eq!(placement(&conn, petg), (None, Some("Trockenbox".into()), Some(ams_a), Some(0)));
    }

    #[test]
    fn moving_between_slots_keeps_the_home_location() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, ams_b) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));
        load_spool(&conn, pla, ams_a, 0).unwrap();

        load_spool(&conn, pla, ams_b, 3).unwrap();

        assert_eq!(placement(&conn, pla), (None, Some("Regal 2".into()), Some(ams_b), Some(3)));
        assert_eq!(load_spool(&conn, pla, ams_b, 3).unwrap().displaced_spool_id, None, "gleiches Fach = nichts tun");
    }

    #[test]
    fn slot_index_must_be_inside_the_unit() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", None);
        assert!(load_spool(&conn, pla, ams_a, 4).is_err());
        assert!(load_spool(&conn, pla, ams_a, -1).is_err());
        assert!(load_spool(&conn, 999, ams_a, 0).is_err());
        assert!(load_spool(&conn, pla, 999, 0).is_err());
    }

    #[test]
    fn the_database_itself_refuses_two_spools_in_one_slot() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", None);
        let petg = spool(&conn, "PETG", None);
        load_spool(&conn, pla, ams_a, 1).unwrap();
        let direct = conn.execute(
            "UPDATE filament_spools SET unit_id = ?1, slot_index = 1 WHERE id = ?2",
            params![ams_a, petg],
        );
        assert!(direct.is_err());
    }

    #[test]
    fn unloading_goes_home_or_to_a_given_location() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));
        let petg = spool(&conn, "PETG", Some("Regal 1"));
        load_spool(&conn, pla, ams_a, 0).unwrap();
        load_spool(&conn, petg, ams_a, 1).unwrap();

        assert_eq!(unload_spool(&conn, pla, None).unwrap(), Some("Regal 2".into()));
        assert_eq!(placement(&conn, pla), (Some("Regal 2".into()), None, None, None));
        assert_eq!(unload_spool(&conn, petg, Some("  Trockenbox ")).unwrap(), Some("Trockenbox".into()));
        assert!(unload_spool(&conn, petg, None).is_err(), "liegt schon im Lager");
    }

    #[test]
    fn shrinking_or_deleting_units_and_printers_sends_spools_home() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, ams_a, ams_b) = x1c_with_two_ams(&conn);
        let custom = insert_unit(&conn, printer, "custom", "Box", Some(6)).unwrap();
        let a = spool(&conn, "A", Some("R1"));
        let b = spool(&conn, "B", Some("R2"));
        let c = spool(&conn, "C", Some("R3"));
        let d = spool(&conn, "D", Some("R4"));
        load_spool(&conn, a, custom, 5).unwrap();
        load_spool(&conn, b, custom, 0).unwrap();
        load_spool(&conn, c, ams_a, 0).unwrap();
        load_spool(&conn, d, ams_b, 0).unwrap();

        assert_eq!(update_unit(&conn, custom, "Box 4", Some(4)).unwrap(), 1);
        assert_eq!(placement(&conn, a), (Some("R1".into()), None, None, None));
        assert_eq!(placement(&conn, b).2, Some(custom));
        assert_eq!(update_unit(&conn, ams_a, "AMS Links", Some(9)).unwrap(), 0, "Vorlagen behalten ihre Fachanzahl");
        assert_eq!(list_units(&conn).unwrap().iter().find(|u| u.id == ams_a).unwrap().slot_count, 4);

        assert_eq!(delete_unit(&conn, custom).unwrap(), 1);
        assert_eq!(placement(&conn, b), (Some("R2".into()), None, None, None));

        assert_eq!(delete_printer(&conn, printer).unwrap(), 2);
        assert_eq!(placement(&conn, c), (Some("R3".into()), None, None, None));
        assert_eq!(placement(&conn, d), (Some("R4".into()), None, None, None));
        assert!(list_units(&conn).unwrap().is_empty());
        assert!(list_printers(&conn).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_spool_frees_its_slot() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", None);
        let petg = spool(&conn, "PETG", None);
        load_spool(&conn, pla, ams_a, 0).unwrap();
        crate::db::delete_filament_spool(&conn, pla).unwrap();
        assert_eq!(load_spool(&conn, petg, ams_a, 0).unwrap().displaced_spool_id, None);
    }

    #[test]
    fn units_can_be_reordered_only_with_the_printers_own_units() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, ams_a, ams_b) = x1c_with_two_ams(&conn);
        reorder_units(&conn, printer, &[ams_b, ams_a]).unwrap();
        let order: Vec<i64> = list_units(&conn).unwrap().iter().map(|u| u.id).collect();
        assert_eq!(order, vec![ams_b, ams_a]);
        assert!(reorder_units(&conn, printer, &[ams_b]).is_err());
        assert!(reorder_units(&conn, printer, &[ams_b, ams_a, 999]).is_err());
    }

    #[test]
    fn editing_a_loaded_spool_changes_its_home_location_not_its_slot() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));
        load_spool(&conn, pla, ams_a, 0).unwrap();
        let edited = crate::db::models::NewFilamentSpool {
            material: "PLA+".into(),
            manufacturer: None,
            color: Some("Schwarz".into()),
            location: Some("Regal 5".into()),
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 700.0,
            price: None,
            image_png: None,
            color_hex: Some("#1a1a1a".into()),
            kind: "filament".into(),
        };
        crate::db::update_filament_spool(&conn, pla, &edited).unwrap();
        assert_eq!(placement(&conn, pla), (None, Some("Regal 5".into()), Some(ams_a), Some(0)));
    }

    fn bottle(conn: &Connection, location: &str) -> i64 {
        crate::db::insert_filament_spool(
            conn,
            &crate::db::models::NewFilamentSpool {
                material: "Standard".into(),
                manufacturer: None,
                color: Some("Grau".into()),
                location: Some(location.into()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 620.0,
                price: None,
                image_png: None,
                color_hex: None,
                kind: crate::db::models::SPOOL_KIND_RESIN.into(),
            },
        )
        .unwrap()
    }

    /// Resin-Drucker samt Harzwanne (wie `add_printer` sie anlegt).
    fn saturn_with_vat(conn: &Connection) -> (i64, i64) {
        let printer = insert_printer_of_kind(conn, "Saturn 4", PRINTER_KIND_RESIN).unwrap();
        let vat = insert_resin_vat(conn, printer, "Harzwanne").unwrap();
        (printer, vat)
    }

    #[test]
    fn printers_have_a_kind_and_default_to_filament() {
        let conn = crate::db::connect_in_memory().unwrap();
        insert_printer(&conn, "X1C").unwrap();
        insert_printer_of_kind(&conn, "Saturn 4", PRINTER_KIND_RESIN).unwrap();
        assert!(insert_printer_of_kind(&conn, "Toaster", "toast").is_err());

        let kinds: Vec<(String, String)> = list_printers(&conn).unwrap().into_iter().map(|p| (p.name, p.kind)).collect();
        assert_eq!(kinds, vec![("X1C".into(), "filament".into()), ("Saturn 4".into(), "resin".into())]);
    }

    #[test]
    fn a_resin_printer_gets_exactly_one_vat_and_no_other_units() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, vat) = saturn_with_vat(&conn);

        let unit = list_units(&conn).unwrap().into_iter().find(|u| u.id == vat).unwrap();
        assert_eq!((unit.kind.as_str(), unit.slot_count, unit.bambu_ams_index), ("resin_vat", 1, None));
        assert!(insert_resin_vat(&conn, printer, "Zweite Wanne").is_err(), "nur eine Wanne");
        assert!(insert_unit(&conn, printer, "external", "Halter", None).is_err(), "keine weiteren Einheiten");
        assert!(insert_unit(&conn, printer, "custom", "Box", Some(2)).is_err());
    }

    #[test]
    fn vats_exist_only_on_resin_printers() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (x1c, _, _) = x1c_with_two_ams(&conn);
        assert!(insert_resin_vat(&conn, x1c, "Harzwanne").is_err());
        assert!(insert_unit(&conn, x1c, "resin_vat", "Harzwanne", None).is_err(), "nicht ueber add_unit");
        assert!(insert_resin_vat(&conn, 999, "Harzwanne").is_err());
    }

    #[test]
    fn the_vat_can_be_neither_renamed_nor_deleted() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, vat) = saturn_with_vat(&conn);
        assert!(update_unit(&conn, vat, "Wanne 2", None).is_err());
        assert!(delete_unit(&conn, vat).is_err());
        assert_eq!(list_units(&conn).unwrap().len(), 1);
    }

    #[test]
    fn a_resin_bottle_goes_into_the_vat_and_back_home() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, vat) = saturn_with_vat(&conn);
        let grey = bottle(&conn, "Resin-Schrank");
        let clear = bottle(&conn, "Keller");

        assert_eq!(load_spool(&conn, grey, vat, 0).unwrap().displaced_spool_id, None);
        assert_eq!(placement(&conn, grey), (None, Some("Resin-Schrank".into()), Some(vat), Some(0)));
        assert_eq!(load_spool(&conn, clear, vat, 0).unwrap().displaced_spool_id, Some(grey), "Tausch");
        assert_eq!(placement(&conn, grey), (Some("Resin-Schrank".into()), None, None, None));
        assert_eq!(unload_spool(&conn, clear, None).unwrap(), Some("Keller".into()));
    }

    #[test]
    fn a_resin_bottle_can_never_be_loaded_into_a_filament_unit() {
        let conn = crate::db::connect_in_memory().unwrap();
        let printer = insert_printer(&conn, "Mars").unwrap();
        let holder = insert_unit(&conn, printer, "external", "Halter", None).unwrap();
        let ams = insert_unit(&conn, printer, "bambu_ams", "AMS", None).unwrap();
        let resin = bottle(&conn, "Resin-Schrank");

        assert!(load_spool(&conn, resin, holder, 0).is_err());
        assert!(load_spool(&conn, resin, ams, 2).is_err());
        assert_eq!(placement(&conn, resin), (Some("Resin-Schrank".into()), None, None, None));
    }

    #[test]
    fn a_filament_spool_can_never_be_loaded_into_a_vat() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, vat) = saturn_with_vat(&conn);
        let (_, ams_a, _) = x1c_with_two_ams(&conn);
        let pla = spool(&conn, "PLA", Some("Regal 2"));
        load_spool(&conn, pla, ams_a, 0).unwrap();

        assert!(load_spool(&conn, pla, vat, 0).is_err(), "auch nicht aus einem Fach heraus");
        assert_eq!(placement(&conn, pla), (None, Some("Regal 2".into()), Some(ams_a), Some(0)));
    }

    #[test]
    fn a_loaded_entry_cannot_change_its_kind() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (_, vat) = saturn_with_vat(&conn);
        let grey = bottle(&conn, "Resin-Schrank");
        load_spool(&conn, grey, vat, 0).unwrap();
        let as_filament = crate::db::models::NewFilamentSpool {
            material: "PLA".into(),
            manufacturer: None,
            color: None,
            location: Some("Regal".into()),
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 1000.0,
            price: None,
            image_png: None,
            color_hex: None,
            kind: "filament".into(),
        };
        assert!(crate::db::update_filament_spool(&conn, grey, &as_filament).is_err());
        assert_eq!(crate::db::get_filament_spool(&conn, grey).unwrap().kind, "resin");
        let as_resin = crate::db::models::NewFilamentSpool { kind: "resin".into(), ..as_filament };
        crate::db::update_filament_spool(&conn, grey, &as_resin).expect("Art bleibt gleich: erlaubt");
    }

    #[test]
    fn deleting_a_resin_printer_sends_the_bottle_home() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (printer, vat) = saturn_with_vat(&conn);
        let grey = bottle(&conn, "Resin-Schrank");
        load_spool(&conn, grey, vat, 0).unwrap();

        assert_eq!(delete_printer(&conn, printer).unwrap(), 1);
        assert_eq!(placement(&conn, grey), (Some("Resin-Schrank".into()), None, None, None));
    }

    #[test]
    fn only_filament_printers_accept_a_connection() {
        let conn = crate::db::connect_in_memory().unwrap();
        let (saturn, _) = saturn_with_vat(&conn);
        let x1c = insert_printer(&conn, "X1C").unwrap();
        assert!(ensure_filament_printer(&conn, x1c).is_ok());
        assert!(ensure_filament_printer(&conn, saturn).is_err());
        assert!(ensure_filament_printer(&conn, 999).is_err());
    }
}
