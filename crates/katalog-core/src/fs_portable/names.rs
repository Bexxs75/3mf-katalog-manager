use super::{nfc, Error, Result};
pub(crate) fn forbidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{ad}'|'\u{600}'..='\u{605}'|'\u{61c}'|'\u{6dd}'|'\u{70f}'|'\u{890}'..='\u{891}'|'\u{8e2}'|'\u{180e}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}'|'\u{fff9}'..='\u{fffb}'|'\u{110bd}'|'\u{110cd}'|'\u{13430}'..='\u{13455}'|'\u{1bca0}'..='\u{1bca3}'|'\u{1d173}'..='\u{1d17a}'|'\u{e0001}'|'\u{e0020}'..='\u{e007f}')
}
pub fn validate_name(value: &str) -> Result<String> {
    let n = nfc(value);
    let base = n.split('.').next().unwrap_or("").trim_end().to_uppercase();
    if n.is_empty()
        || n.len() > 255
        || n.starts_with(['.', ' '])
        || n.ends_with(['.', ' '])
        || n.chars().any(|c| forbidden(c) || "/\\<>:\"|?*".contains(c))
        || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(
                &base[3..],
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ))
    {
        return Err(Error::InvalidName);
    }
    Ok(n)
}
fn extension(name: &str) -> &str {
    if name.to_ascii_lowercase().ends_with(".gcode.3mf") {
        &name[name.len() - 10..]
    } else {
        name.rfind('.').map(|i| &name[i..]).unwrap_or("")
    }
}
pub fn rename_name(old: &str, base: &str) -> Result<String> {
    validate_name(&format!("{}{}", validate_name(base)?, extension(old)))
}
pub fn trash_name(id: i64, op: &str, disk: &str) -> Result<String> {
    trash_entry_name(&format!("{id}-"), op, disk)
}
/// Name of a companion file in the one-level web trash. The `companion-` prefix keeps
/// it apart from trashed models (`<id>-...`), and the row id keeps names of different
/// companions apart even when their file names are equal.
pub fn companion_trash_name(id: i64, op: &str, disk: &str) -> Result<String> {
    trash_entry_name(&format!("companion-{id}-"), op, disk)
}
/// The prefix every trash name of the companion with this row id starts with.
pub fn companion_trash_prefix(id: i64) -> String {
    format!("companion-{id}-")
}
fn trash_entry_name(label: &str, op: &str, disk: &str) -> Result<String> {
    if op.len() < 8 || !op.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Internal);
    }
    let clean: String = nfc(disk)
        .chars()
        .map(|c| {
            if forbidden(c) || "/\\<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let ext = extension(&clean);
    let prefix = format!("{label}{}-", &op[..8]);
    let budget = 255usize
        .checked_sub(prefix.len() + ext.len())
        .filter(|b| *b > 0)
        .ok_or(Error::TrashConflict)?;
    let stem = &clean[..clean.len() - ext.len()];
    let mut end = stem.len().min(budget);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let mut name = format!("{prefix}{}{ext}", &stem[..end]);
    // Sanitize after truncation too: it can expose a space at the end of the stem.
    let trailing = name.trim_end_matches(['.', ' ']).len();
    name.replace_range(trailing.., &"_".repeat(name.len() - trailing));
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_device_names_are_rejected_with_extensions_and_in_any_case() {
        for name in ["CONIN$", "conout$", "ConIn$.3mf", "CONOUT$.gcode.3mf", "conin$ .stl"] {
            assert_eq!(validate_name(name), Err(Error::InvalidName), "{name}");
        }
        for name in ["CONIN", "CONOUT", "CONIN$x.3mf", "xCONOUT$.stl"] {
            assert_eq!(validate_name(name).unwrap(), name);
        }
    }

    #[test]
    fn trash_names_replace_every_trailing_dot_and_space() {
        for (disk, expected) in [
            ("model. ", "model__"),
            ("model ..", "model___"),
            ("model ", "model_"),
            ("model.3mf", "model.3mf"),
            ("model.gcode.3mf", "model.gcode.3mf"),
            ("mo del.v1.3mf", "mo del.v1.3mf"),
            ("é. ", "é__"),
        ] {
            let model = trash_name(7, "abcdef0123456789", disk).unwrap();
            let companion = companion_trash_name(7, "abcdef0123456789", disk).unwrap();
            assert_eq!(model, format!("7-abcdef01-{expected}"));
            assert_eq!(companion, format!("companion-7-abcdef01-{expected}"));
            assert_eq!(validate_name(&model).unwrap(), model);
            assert_eq!(validate_name(&companion).unwrap(), companion);
        }
    }

    #[test]
    fn trash_names_stay_valid_when_truncation_exposes_a_space() {
        for label in ["7-", "companion-7-"] {
            let budget = 255 - label.len() - 9;
            let disk = format!("{} tail", "a".repeat(budget - 1));
            let name = trash_entry_name(label, "abcdef0123456789", &disk).unwrap();
            assert!(name.len() <= 255);
            assert!(!name.ends_with(['.', ' ']));
            assert_eq!(validate_name(&name).unwrap(), name);
        }
    }
}
