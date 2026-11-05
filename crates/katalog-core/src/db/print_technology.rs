//! "What do you print with?": which kinds of printing the catalog shows. The
//! choice only hides parts of the interface; no row is ever removed, so a
//! change of mind brings everything back.

use rusqlite::Connection;

use super::error::DbError;
use super::printer_link::{get_setting, set_setting};
use crate::error_code::ErrorCode;

pub const SETTING_PRINT_TECHNOLOGY: &str = "print_technology";
/// Set once the question was shown, so an existing installation is asked only once.
pub const SETTING_PRINT_TECHNOLOGY_ASKED: &str = "print_technology_asked";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintTechnology {
    Filament,
    Resin,
    Both,
}

impl PrintTechnology {
    pub fn as_str(self) -> &'static str {
        match self {
            PrintTechnology::Filament => "filament",
            PrintTechnology::Resin => "resin",
            PrintTechnology::Both => "both",
        }
    }

    /// Strict parse for values coming from outside.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "filament" => Some(PrintTechnology::Filament),
            "resin" => Some(PrintTechnology::Resin),
            "both" => Some(PrintTechnology::Both),
            _ => None,
        }
    }
}

/// Missing or unknown values count as "both": a damaged or newer setting must
/// never hide parts of the catalog.
pub fn print_technology(conn: &Connection) -> Result<PrintTechnology, DbError> {
    Ok(get_setting(conn, SETTING_PRINT_TECHNOLOGY)?
        .as_deref()
        .and_then(PrintTechnology::parse)
        .unwrap_or(PrintTechnology::Both))
}

/// Stores the choice and notes that the question has been answered.
pub fn set_print_technology(conn: &Connection, value: &str) -> Result<PrintTechnology, DbError> {
    let technology = PrintTechnology::parse(value).ok_or_else(|| DbError::coded(ErrorCode::PrintTechnologyInvalid))?;
    set_setting(conn, SETTING_PRINT_TECHNOLOGY, technology.as_str())?;
    set_setting(conn, SETTING_PRINT_TECHNOLOGY_ASKED, "1")?;
    Ok(technology)
}

/// Whether the question was already answered or dismissed. The notice for
/// existing installations stays until this is true.
pub fn print_technology_asked(conn: &Connection) -> Result<bool, DbError> {
    Ok(get_setting(conn, SETTING_PRINT_TECHNOLOGY_ASKED)?.as_deref() == Some("1"))
}

/// Remembers that the question was shown without changing the choice.
pub fn mark_print_technology_asked(conn: &Connection) -> Result<(), DbError> {
    set_setting(conn, SETTING_PRINT_TECHNOLOGY_ASKED, "1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_in_memory;

    #[test]
    fn missing_or_unknown_value_means_both() {
        let conn = connect_in_memory().unwrap();
        assert_eq!(print_technology(&conn).unwrap(), PrintTechnology::Both);
        set_setting(&conn, SETTING_PRINT_TECHNOLOGY, "laser").unwrap();
        assert_eq!(print_technology(&conn).unwrap(), PrintTechnology::Both);
    }

    #[test]
    fn set_stores_each_value_and_marks_the_question_answered() {
        let conn = connect_in_memory().unwrap();
        assert!(!print_technology_asked(&conn).unwrap());
        for v in ["filament", "resin", "both"] {
            assert_eq!(set_print_technology(&conn, v).unwrap().as_str(), v);
            assert_eq!(print_technology(&conn).unwrap().as_str(), v);
        }
        assert!(print_technology_asked(&conn).unwrap());
    }

    #[test]
    fn set_rejects_unknown_values_and_keeps_the_old_one() {
        let conn = connect_in_memory().unwrap();
        set_print_technology(&conn, "resin").unwrap();
        for bad in ["", "Resin", "laser", "both "] {
            assert!(set_print_technology(&conn, bad).is_err(), "{bad:?}");
        }
        assert_eq!(print_technology(&conn).unwrap(), PrintTechnology::Resin);
    }

    #[test]
    fn marking_asked_keeps_the_choice() {
        let conn = connect_in_memory().unwrap();
        mark_print_technology_asked(&conn).unwrap();
        assert!(print_technology_asked(&conn).unwrap());
        assert_eq!(print_technology(&conn).unwrap(), PrintTechnology::Both);
    }

    #[test]
    fn read_answer_has_the_camel_case_shape_the_frontend_expects() {
        let conn = connect_in_memory().unwrap();
        let json = |c: &Connection| serde_json::to_value(crate::read::get_print_technology(c).unwrap()).unwrap();
        assert_eq!(json(&conn), serde_json::json!({"value": "both", "asked": false}));
        set_print_technology(&conn, "resin").unwrap();
        assert_eq!(json(&conn), serde_json::json!({"value": "resin", "asked": true}));
    }
}
