use super::*;

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolDto {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
    pub image_png: Option<String>,
    #[serde(default)]
    pub color_hex: Option<String>,
    /// Nur lesend: Stammplatz, Einheit und Fach aendern sich ausschliesslich
    /// ueber `load_spool`/`unload_spool` (commands/printers.rs).
    #[serde(default)]
    pub home_location: Option<String>,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub slot_index: Option<i64>,
}
fn validate_color_hex(color_hex: &Option<String>) -> CmdResult<()> {
    match color_hex {
        Some(value) if !db::printers::is_valid_color_hex(value) => {
            Err(format!("ungueltiger Farbwert: {value}"))
        }
        _ => Ok(()),
    }
}
fn filament_dto_to_record(spool: &FilamentSpoolDto) -> db::models::NewFilamentSpool {
    use base64::Engine;
    db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        location: spool.location.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
        image_png: spool
            .image_png
            .as_ref()
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()),
        color_hex: spool.color_hex.as_ref().map(|c| c.to_lowercase()),
    }
}
#[tauri::command]
pub fn list_filament_spools(state: State<AppState>) -> CmdResult<Vec<FilamentSpoolDto>> {
    let conn = lock_db(&state)?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(spools
        .into_iter()
        .map(|s| {
            use base64::Engine;
            FilamentSpoolDto {
                id: s.id.to_string(),
                material: s.material,
                manufacturer: s.manufacturer,
                color: s.color,
                location: s.location,
                diameter_mm: s.diameter_mm,
                original_weight_g: s.original_weight_g,
                remaining_weight_g: s.remaining_weight_g,
                price: s.price,
                image_png: s
                    .image_png
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
                color_hex: s.color_hex,
                home_location: s.home_location,
                unit_id: s.unit_id.map(|id| id.to_string()),
                slot_index: s.slot_index,
            }
        })
        .collect())
}
#[tauri::command]
pub fn add_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    validate_color_hex(&spool.color_hex)?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    let id = db::insert_filament_spool(&conn, &new_spool).map_err(|e| e.to_string())?;
    // Neue Spulen liegen immer im Lager, auch wenn der Aufrufer ein Fach
    // mitschickt.
    Ok(FilamentSpoolDto {
        id: id.to_string(),
        color_hex: new_spool.color_hex,
        home_location: None,
        unit_id: None,
        slot_index: None,
        ..spool
    })
}
#[tauri::command]
pub fn update_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    validate_color_hex(&spool.color_hex)?;
    let id: i64 = spool.id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    db::update_filament_spool(&conn, id, &new_spool).map_err(|e| e.to_string())?;
    Ok(spool)
}
#[tauri::command]
pub fn delete_filament_spool(state: State<AppState>, spool_id: String) -> CmdResult<()> {
    let id: i64 = spool_id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_filament_spool(&conn, id).map_err(|e| e.to_string())
}
