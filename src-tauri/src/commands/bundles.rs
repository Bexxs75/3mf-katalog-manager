use super::*;

// Bundles only group catalog entries: no command here moves, renames or deletes a file, so
// none of them waits for the import gate (like the collections).

#[tauri::command]
pub fn list_bundles(state: State<AppState>) -> CmdResult<Vec<BundleDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_bundles(&conn).map_err(Into::into)
}
#[tauri::command]
pub fn create_bundle(
    state: State<AppState>,
    name: String,
    kind: String,
    file_ids: Vec<String>,
    labels: Option<Vec<String>>,
    restore: Option<bool>,
) -> CmdResult<BundleDto> {
    let conn = lock_db(&state)?;
    katalog_core::write::create_bundle_with(&conn, name, kind, file_ids, labels, restore).map_err(Into::into)
}
#[tauri::command]
pub fn update_bundle(state: State<AppState>, bundle_id: String, name: Option<String>, kind: Option<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    katalog_core::write::update_bundle(&conn, bundle_id, name, kind).map_err(Into::into)
}
#[tauri::command]
pub fn add_files_to_bundle(state: State<AppState>, bundle_id: String, file_ids: Vec<String>) -> CmdResult<BundleDto> {
    let conn = lock_db(&state)?;
    katalog_core::write::add_files_to_bundle(&conn, bundle_id, file_ids).map_err(Into::into)
}
#[tauri::command]
pub fn remove_files_from_bundle(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    katalog_core::write::remove_files_from_bundle(&conn, file_ids).map_err(Into::into)
}
#[tauri::command]
pub fn dissolve_bundle(state: State<AppState>, bundle_id: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    katalog_core::write::dissolve_bundle(&conn, bundle_id).map_err(Into::into)
}
#[tauri::command]
pub fn update_bundle_member(state: State<AppState>, file_id: String, label: Option<String>, make_title: Option<bool>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    katalog_core::write::update_bundle_member(&conn, file_id, label, make_title).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use katalog_core::error_code::ErrorCode;

    fn files(conn: &Connection, n: usize) -> Vec<String> {
        (0..n).map(|i| db::test_insert_minimal_file(conn, &format!("/tmp/bundle{i}.3mf"), None).unwrap().to_string()).collect()
    }

    #[test]
    fn the_desktop_functions_create_change_and_dissolve_a_bundle() {
        let conn = db::connect_in_memory().unwrap();
        let f = files(&conn, 3);
        let bundle = katalog_core::write::create_bundle(&conn, "Vase".into(), "parts".into(), vec![f[0].clone(), f[1].clone()]).unwrap();
        assert_eq!((bundle.name.as_str(), bundle.kind.as_str(), bundle.members.len()), ("Vase", "parts", 2));
        let bundle = katalog_core::write::add_files_to_bundle(&conn, bundle.id.clone(), vec![f[2].clone()]).unwrap();
        assert_eq!(bundle.members.iter().map(|m| m.file_id.clone()).collect::<Vec<_>>(), f);
        katalog_core::write::update_bundle(&conn, bundle.id.clone(), Some("Vase 2".into()), Some("variants".into())).unwrap();
        katalog_core::write::update_bundle_member(&conn, f[1].clone(), Some("Lid".into()), Some(true)).unwrap();
        let listed = katalog_core::read::list_bundles(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].name.as_str(), listed[0].kind.as_str()), ("Vase 2", "variants"));
        assert_eq!(listed[0].members[0].file_id, f[1], "made the title model");
        assert_eq!(listed[0].members[0].label.as_deref(), Some("Lid"));
        katalog_core::write::remove_files_from_bundle(&conn, vec![f[0].clone()]).unwrap();
        assert_eq!(katalog_core::read::list_bundles(&conn).unwrap()[0].members.len(), 2);
        katalog_core::write::dissolve_bundle(&conn, bundle.id).unwrap();
        assert!(katalog_core::read::list_bundles(&conn).unwrap().is_empty());
        let files_left: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(files_left, 3, "no file was touched");
    }

    #[test]
    fn rejected_input_comes_back_as_a_code_and_changes_nothing() {
        let conn = db::connect_in_memory().unwrap();
        let f = files(&conn, 2);
        let code = |e: katalog_core::write::WriteError| match e {
            katalog_core::write::WriteError::Coded { code, .. } => code,
            other => panic!("not coded: {other:?}"),
        };
        assert_eq!(code(katalog_core::write::create_bundle(&conn, "A".into(), "other".into(), f.clone()).unwrap_err()), ErrorCode::BundleKindInvalid);
        assert_eq!(code(katalog_core::write::create_bundle(&conn, "A".into(), "parts".into(), vec![f[0].clone(), "x".into()]).unwrap_err()), ErrorCode::InvalidCatalogId);
        assert_eq!(code(katalog_core::write::create_bundle(&conn, "A".into(), "parts".into(), vec![f[0].clone()]).unwrap_err()), ErrorCode::BundleTooFewFiles);
        assert_eq!(code(katalog_core::write::create_bundle(&conn, " ".into(), "parts".into(), f.clone()).unwrap_err()), ErrorCode::BundleNameInvalid);
        assert_eq!(code(katalog_core::write::dissolve_bundle(&conn, "999".into()).unwrap_err()), ErrorCode::BundleNotFound);
        assert_eq!(code(katalog_core::write::update_bundle_member(&conn, f[0].clone(), None, None).unwrap_err()), ErrorCode::EntryNotFound);
        assert!(katalog_core::read::list_bundles(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_failing_step_in_the_middle_leaves_no_half_made_bundle() {
        let conn = db::connect_in_memory().unwrap();
        let f = files(&conn, 2);
        // The second member fails after the head row and the first member are written.
        conn.execute_batch("CREATE TRIGGER stop_second BEFORE INSERT ON bundle_members WHEN NEW.position = 1 BEGIN SELECT RAISE(ABORT, 'simulated'); END;").unwrap();
        assert!(katalog_core::write::create_bundle(&conn, "A".into(), "parts".into(), f).is_err());
        let rows: i64 = conn.query_row("SELECT (SELECT COUNT(*) FROM bundles) + (SELECT COUNT(*) FROM bundle_members)", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }
}
