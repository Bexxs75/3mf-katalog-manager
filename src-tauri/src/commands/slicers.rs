use super::*;

// AppImage exportiert beim Start diverse Umgebungsvariablen, die nur fuer die
// eigene AppImage-Laufzeit gedacht sind und nicht an unabhaengig gestartete
// externe Programme (Slicer) weitergegeben werden duerfen.
const APPIMAGE_ENV_VARS_TO_STRIP: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "OWD",
    "ARGV0",
    "LD_LIBRARY_PATH",
    "GTK_EXE_PREFIX",
    "GTK_DATA_PREFIX",
    "GTK_THEME",
    "GTK_PATH",
    "GTK_IM_MODULE_FILE",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_BACKEND",
    "GIO_EXTRA_MODULES",
    "GSETTINGS_SCHEMA_DIR",
    "XDG_DATA_DIRS",
    "PYTHONPATH",
    "QT_PLUGIN_PATH",
    "GST_PLUGIN_SYSTEM_PATH",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

/// Ueber die Slicer-Registry (M-06, Task 11) an das Frontend zurueckgegebene
/// Sicht auf einen `registered_slicers`-Eintrag. `is_auto_detected` ist
/// hier bewusst NICHT enthalten - das Frontend braucht diese Unterscheidung
/// aktuell nicht, jeder registrierte Eintrag (ob manuell oder automatisch
/// erkannt) ist gleichermassen vertrauenswuerdig, sobald er in der Tabelle
/// steht.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlicerDto {
    pub id: String,
    pub name: String,
    pub executable_path: String,
}
// Muss async sein, obwohl kein .await im Rumpf steht: eine synchrone
// Tauri-Command-Funktion laeuft direkt auf dem IPC-Dispatch-Thread (siehe
// import_files/import_folder, die aus demselben Grund schon async sind).
// blocking_pick_file() blockiert diesen Thread, bis der native Dialog
// geschlossen wird - lief die Funktion synchron, waere das genau der
// Thread, den GTK fuer die eigene Fensterschleife (und damit fuer den
// Dialog selbst) braucht: ein Deadlock, der die App komplett einfrieren
// liess. Als async fn dispatcht Tauri sie stattdessen auf den
// Async-Runtime-Thread-Pool.
/// Kernlogik, getrennt vom Tauri-Dialog gehalten (gleiche Konvention wie
/// `move_file_to_folder_with_conn` etc.), damit sie direkt testbar ist - der
/// Dialog selbst laesst sich nicht sinnvoll unit-testen.
pub(crate) fn register_slicer_with_conn(conn: &Connection, name: String, executable_path: String) -> CmdResult<SlicerDto> {
    validate_slicer_path(&executable_path)?; // bestehende Pruefung wiederverwendet, nicht ersetzt
    let id = db::insert_registered_slicer(conn, &name, &executable_path, false).map_err(|e| e.to_string())?;
    Ok(SlicerDto { id: id.to_string(), name, executable_path })
}
/// Oeffnet den nativen Datei-Dialog IM BACKEND (M-06/P0-Korrektur, zweite
/// Review-Runde) - das Frontend uebergibt hier an keiner Stelle einen selbst
/// konstruierten Pfad-String, einzige Quelle fuer `executable_path` ist die
/// vom Nutzer im Dialog getroffene Auswahl. Analog zu `pick_and_read_image`,
/// das denselben `blocking_pick_file()`-Ansatz bereits fuer Bilder nutzt.
/// Muss aus demselben Grund wie `pick_and_read_image` async sein:
/// `blocking_pick_file()` blockiert den aufrufenden Thread, bis der native
/// Dialog geschlossen wird.
#[tauri::command]
pub async fn pick_and_register_slicer(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<Option<SlicerDto>> {
    let dialog = app.dialog().file();
    #[cfg(target_os = "windows")]
    let dialog = dialog.add_filter("Programme", &["exe"]);
    let picked = dialog.blocking_pick_file();
    let Some(picked) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let name = picked
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Slicer")
        .to_string();
    let executable_path = picked.to_string_lossy().to_string();
    let conn = lock_db(&state)?;
    register_slicer_with_conn(&conn, name, executable_path).map(Some)
}
#[tauri::command]
pub fn list_registered_slicers(state: State<AppState>) -> CmdResult<Vec<SlicerDto>> {
    let conn = lock_db(&state)?;
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| SlicerDto { id: r.id.to_string(), name: r.name, executable_path: r.executable_path })
                .collect()
        })
}
// Security-Review 2026-09-19, Finding Z-1 (M-06): eine echte Pfad-Allowlist
// (nur automatisch erkannte ODER nachweislich per Datei-Dialog gewaehlte
// Binaries) braucht eine backend-seitig persistierte Slicer-Liste. Das ist
// jetzt `registered_slicers` (Task 11, siehe migrations.rs) statt des
// frueheren, ausschliesslich im Frontend-localStorage gefuehrten Zustands -
// `open_in_slicer` nimmt seitdem keinen freien Pfad mehr entgegen, sondern
// ausschliesslich eine zuvor registrierte `slicer_id`
// (`resolve_registered_slicer_and_model` unten). `validate_slicer_path`
// bleibt zusaetzlich bestehen und wird sowohl bei der Registrierung als auch
// bei jeder Aufloesung erneut angewendet: sie stellt sicher, dass der
// registrierte Pfad tatsaechlich (noch) auf eine existierende, ausfuehrbare
// Datei zeigt, statt jeden gespeicherten String klaglos an process::Command
// zu uebergeben.
fn validate_slicer_path(slicer_path: &str) -> CmdResult<()> {
    let path = Path::new(slicer_path);
    let metadata = std::fs::metadata(path)
        .map_err(|_| "slicer executable not found".to_string())?;
    if !metadata.is_file() {
        return Err("slicer path is not a file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("slicer path is not executable".to_string());
        }
    }
    #[cfg(windows)]
    {
        let is_exe = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false);
        if !is_exe {
            return Err("slicer path must be an .exe file".to_string());
        }
    }
    Ok(())
}
/// Prueft das ZWEITE `Command`-Argument von `open_in_slicer`. Auch wenn der
/// Slicer-Pfad selbst schon durch `validate_slicer_path` geht, ist der
/// uebergebene Modell-Pfad bisher ungeprueft an den externen Prozess
/// gewandert (Security-Review 2026-09-19, Finding I-5): ein mit "-"
/// beginnender Wert wuerde vom Slicer als CLI-Flag gedeutet, ein beliebiger
/// anderer Pfad/eine andere Endung hat in einem "im Slicer oeffnen"-Aufruf
/// nichts zu suchen.
fn validate_slicer_target_file(file_path: &str) -> CmdResult<()> {
    let path = Path::new(file_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid model file path".to_string())?;
    if file_path.starts_with('-') || file_name.starts_with('-') {
        return Err("model file path must not start with '-'".to_string());
    }
    if !is_sliceable_extension(path) {
        return Err("model file must be a .3mf or .stl file".to_string());
    }
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return Err("model file not found".to_string());
    }
    Ok(())
}
/// Ergebnis der Registry-/Sicherheits-Aufloesung: beide Pfade sind bereits
/// durch `validate_slicer_path`/`validate_slicer_target_file` geprueft.
struct ResolvedSlicerLaunch {
    executable_path: String,
    model_path: String,
}
/// Reine Aufloesungs- und Validierungslogik, GETRENNT vom eigentlichen
/// Prozessstart (Korrektur, vierte Review-Runde): dadurch ist der
/// sicherheitsrelevante Teil (Slicer registriert? Pfade gueltig? Datei
/// existiert und hat eine erlaubte Endung?) ohne Seiteneffekt (kein echter
/// `Command::spawn()`) unit-testbar.
fn resolve_registered_slicer_and_model(
    conn: &Connection,
    file_id: &str,
    slicer_id: &str,
) -> CmdResult<ResolvedSlicerLaunch> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let sid: i64 = slicer_id.parse().map_err(|_| "invalid slicer id".to_string())?;
    let file = db::get_file(conn, id).map_err(|e| e.to_string())?.ok_or_else(|| "file not found".to_string())?;
    let slicer = db::get_registered_slicer(conn, sid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "slicer not registered".to_string())?;
    validate_slicer_path(&slicer.executable_path)?;
    validate_slicer_target_file(&file.path)?;
    Ok(ResolvedSlicerLaunch { executable_path: slicer.executable_path, model_path: file.path })
}
/// Echter Prozessstart - bewusst NICHT unit-getestet (siehe
/// `resolve_registered_slicer_and_model`); bei Bedarf kann ein
/// plattformspezifischer Launch-Test separat ergaenzt werden.
fn launch_slicer(resolved: &ResolvedSlicerLaunch) -> CmdResult<()> {
    let mut cmd = std::process::Command::new(&resolved.executable_path);
    cmd.arg(&resolved.model_path);
    for var in APPIMAGE_ENV_VARS_TO_STRIP {
        cmd.env_remove(var);
    }
    if let Some(parent) = std::path::Path::new(&resolved.executable_path).parent() {
        cmd.current_dir(parent);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn open_in_slicer_with_conn(conn: &Connection, file_id: &str, slicer_id: &str) -> CmdResult<()> {
    let resolved = resolve_registered_slicer_and_model(conn, file_id, slicer_id)?;
    launch_slicer(&resolved)
}
#[tauri::command]
pub fn open_in_slicer(state: State<AppState>, file_id: String, slicer_id: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    open_in_slicer_with_conn(&conn, &file_id, &slicer_id)
}
/// Fuehrt die bestehende Best-Effort-Autoerkennung (`slicers::detect_slicers`)
/// aus und traegt neu gefundene Slicer ueber `db::insert_registered_slicer`
/// (`is_auto_detected = true`) in dieselbe Registry ein wie manuell per
/// Dialog hinzugefuegte - vorher landete die Autoerkennung ausschliesslich
/// im Frontend-localStorage, ohne Backend-Vertrauensgrenze. Bereits bekannte
/// Pfade (egal ob zuvor automatisch oder manuell registriert) werden nicht
/// erneut eingefuegt, da `executable_path` `UNIQUE` ist und ein wiederholter
/// Insert sonst bei jedem Scan fehlschlagen wuerde. Gibt die vollstaendige,
/// aktuelle Registry zurueck (nicht nur die neu gefundenen Eintraege), damit
/// das Frontend mit einem einzigen Aufruf sowohl den Scan ausloest als auch
/// die anzuzeigende Liste erhaelt.
#[tauri::command]
pub fn scan_installed_slicers(state: State<AppState>) -> CmdResult<Vec<SlicerDto>> {
    let conn = lock_db(&state)?;
    let existing = db::list_registered_slicers(&conn).map_err(|e| e.to_string())?;
    let known_paths: HashSet<String> = existing.iter().map(|s| s.executable_path.clone()).collect();
    for detected in detect_slicers() {
        if known_paths.contains(&detected.path) {
            continue;
        }
        // Best-effort: ein einzelner fehlschlagender Insert (z.B. Race mit
        // einem parallelen Scan) darf die Erkennung der uebrigen Slicer
        // nicht abbrechen.
        let _ = db::insert_registered_slicer(&conn, &detected.name, &detected.path, true);
    }
    db::list_registered_slicers(&conn)
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| SlicerDto { id: r.id.to_string(), name: r.name, executable_path: r.executable_path })
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_slicer_path_rejects_missing_file() {
        assert!(validate_slicer_path("/does/not/exist/slicer").is_err());
    }
    #[test]
    fn validate_slicer_path_rejects_directory() {
        let dir = std::env::temp_dir();
        assert!(validate_slicer_path(dir.to_str().unwrap()).is_err());
    }
    #[test]
    fn open_in_slicer_rejects_an_unregistered_slicer_id() {
        let dir = unique_test_dir("open_in_slicer_rejects");
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("model.3mf");
        std::fs::write(&model_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &model_path.to_string_lossy(), None).unwrap();
        let result = resolve_registered_slicer_and_model(&conn, &file_id.to_string(), "not-a-registered-id");
        assert!(result.is_err());
    }
    #[test]
    fn a_registered_slicer_resolves_to_a_validated_executable_and_model_path() {
        // Testet ausschliesslich die Registry-/Sicherheits-Aufloesung
        // (resolve_registered_slicer_and_model), OHNE tatsaechlich einen
        // Prozess zu starten - dafuer wurde open_in_slicer_with_conn in
        // resolve_registered_slicer_and_model() (validiert, testbar ohne
        // Seiteneffekt) und launch_slicer() (echter Prozessstart, bewusst
        // NICHT hier unit-getestet) aufgeteilt. Ein echter Launch-Test kann
        // bei Bedarf separat und plattformspezifisch ergaenzt werden.
        let dir = unique_test_dir("open_in_slicer_resolve");
        std::fs::create_dir_all(&dir).unwrap();
        let model_path = dir.join("model.3mf");
        std::fs::write(&model_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &model_path.to_string_lossy(), None).unwrap();
        // std::env::current_exe() liefert plattformunabhaengig einen
        // tatsaechlich existierenden UND ausfuehrbaren Pfad (unter Windows
        // automatisch mit .exe-Endung) - register_slicer_with_conn()/
        // validate_slicer_path() pruefen reale Existenz + Ausfuehrbarkeit, ein
        // hartkodierter Unix-Pfad wie "/bin/true" existiert unter Windows nicht
        // und wuerde dort jeden Test scheitern lassen, der ihn nutzt.
        let fake_slicer = std::env::current_exe().unwrap();
        let slicer = register_slicer_with_conn(&conn, "Test Slicer".into(), fake_slicer.to_string_lossy().to_string()).unwrap();

        let resolved = resolve_registered_slicer_and_model(&conn, &file_id.to_string(), &slicer.id).unwrap();

        assert_eq!(resolved.executable_path, fake_slicer.to_string_lossy());
        assert_eq!(resolved.model_path, model_path.to_string_lossy());
    }
    #[test]
    fn validate_slicer_target_file_rejects_flags_unsupported_types_and_missing_files() {
        let dir = unique_test_dir("validate_slicer_target");
        let model = dir.join("modell.3mf");
        std::fs::write(&model, b"x").unwrap();
        let stl = dir.join("modell.STL");
        std::fs::write(&stl, b"x").unwrap();
        let other = dir.join("notiz.txt");
        std::fs::write(&other, b"x").unwrap();
        let flag = dir.join("--export-gcode.3mf");
        std::fs::write(&flag, b"x").unwrap();
        let stp = dir.join("modell.stp");
        std::fs::write(&stp, b"x").unwrap();

        let ok_3mf = validate_slicer_target_file(&model.to_string_lossy());
        let ok_stl = validate_slicer_target_file(&stl.to_string_lossy());
        let bad_ext = validate_slicer_target_file(&other.to_string_lossy());
        let bad_flag = validate_slicer_target_file(&flag.to_string_lossy());
        let bad_bare_flag = validate_slicer_target_file("--version");
        let bad_missing = validate_slicer_target_file(&dir.join("weg.3mf").to_string_lossy());
        let bad_dir = validate_slicer_target_file(&dir.to_string_lossy());
        let bad_stp = validate_slicer_target_file(&stp.to_string_lossy());

        let _ = std::fs::remove_dir_all(&dir);
        assert!(ok_3mf.is_ok(), "a real .3mf file must still open: {ok_3mf:?}");
        assert!(ok_stl.is_ok(), "extension check must be case-insensitive: {ok_stl:?}");
        assert!(bad_ext.is_err(), "unsupported extension must be rejected");
        assert!(bad_flag.is_err(), "a file name starting with '-' must be rejected");
        assert!(bad_bare_flag.is_err(), "a bare CLI flag must be rejected");
        assert!(bad_missing.is_err(), "a non-existent file must be rejected");
        assert!(bad_dir.is_err(), "a directory must be rejected");
        // Regressionsschutz: .stp ist seit der STP/STEP-Katalogisierung ueber
        // is_supported_extension importierbar, darf aber NICHT ueber
        // is_sliceable_extension zum Slicer-Start zugelassen werden (siehe
        // Kommentar an is_sliceable_extension in files.rs).
        assert!(bad_stp.is_err(), "a .stp file must not be launchable in a slicer");
    }
}
