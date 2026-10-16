use super::error::DbError;
use rusqlite::{params, Connection};

#[cfg(test)]
#[derive(serde::Deserialize)]
struct Preset {
    code: String,
    usage: String,
}

// Both runtimes read the same table so adding a preset cannot change its meaning between views.
#[cfg(test)]
static PRESETS: std::sync::LazyLock<Vec<Preset>> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/lib/licensePresets.json"))).expect("valid license presets")
});

#[cfg(test)]
fn license_usage(license: Option<&str>) -> &str {
    PRESETS.iter().find(|preset| Some(preset.code.as_str()) == license)
        .map(|preset| preset.usage.as_str()).unwrap_or("unknown")
}

pub fn validate_license(license: Option<&str>) -> Result<Option<&str>, DbError> {
    match license {
        Some(value) if value.chars().count() > 200 || value.chars().any(char::is_control) =>
            Err(DbError::Other("Lizenz darf höchstens 200 Zeichen und keine Steuerzeichen enthalten".into())),
        None | Some("") => Ok(None),
        value => Ok(value),
    }
}

pub fn set_license(conn: &mut Connection, ids: &[i64], license: Option<&str>) -> Result<(), DbError> {
    let license = validate_license(license)?;
    let tx = conn.transaction()?;
    for id in ids {
        if tx.execute("UPDATE files SET license = ?1 WHERE id = ?2 AND deleted_at IS NULL", params![license, id])? != 1 {
            return Err(DbError::Other(format!("Modell {id} wurde nicht gefunden")));
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_only_claimed_for_exact_known_codes() {
        for code in ["cc-by-nc", "cc-by-nc-sa", "cc-by-nc-nd", "personal"] {
            assert_eq!(license_usage(Some(code)), "private");
        }
        for code in ["cc-by", "cc-by-sa", "cc-by-nd", "cc0", "commercial", "own"] {
            assert_eq!(license_usage(Some(code)), "commercial");
        }
        for value in [None, Some(""), Some("CC BY"), Some(" cc-by "), Some("commercial license")] {
            assert_eq!(license_usage(value), "unknown");
        }
    }

    #[test]
    fn set_license_validates_clears_and_rolls_back_the_whole_selection() {
        let mut conn = super::super::repository::connect_in_memory().unwrap();
        let id = super::super::test_insert_minimal_file(&conn, "/one.3mf", None).unwrap();
        set_license(&mut conn, &[id], Some("cc-by-nc")).unwrap();
        assert_eq!(super::super::get_file(&conn, id).unwrap().unwrap().license.as_deref(), Some("cc-by-nc"));
        assert!(set_license(&mut conn, &[id, id + 1], Some("cc0")).is_err());
        assert_eq!(super::super::get_file(&conn, id).unwrap().unwrap().license.as_deref(), Some("cc-by-nc"));
        assert!(set_license(&mut conn, &[id], Some(&"a".repeat(201))).is_err());
        assert!(set_license(&mut conn, &[id], Some("bad\0value")).is_err());
        set_license(&mut conn, &[id], Some(&"é".repeat(200))).unwrap();
        set_license(&mut conn, &[id], Some(" Free license ")).unwrap();
        assert_eq!(super::super::list_file_summaries(&conn).unwrap()[0].license.as_deref(), Some(" Free license "));
        let second = super::super::test_insert_minimal_file(&conn, "/two.3mf", None).unwrap();
        set_license(&mut conn, &[id, second], Some("own")).unwrap();
        assert!(super::super::list_files(&conn).unwrap().iter().all(|file| file.license.as_deref() == Some("own")));
        for value in [None, Some("")] {
            set_license(&mut conn, &[id], value).unwrap();
            assert_eq!(super::super::get_file(&conn, id).unwrap().unwrap().license, None);
        }
    }
}
