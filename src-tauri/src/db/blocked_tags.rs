//! Automatic tags the user does not want assigned again. The list lives in its
//! own table: `tags` and `file_tags` are read directly by other tools and must
//! not change shape.

use std::collections::HashSet;

use rusqlite::{params, Connection};

use super::error::{validate_text_length, DbError};

/// Longest tag name the catalog accepts (UTF-16 units, like the tag input).
pub const MAX_TAG_NAME_LEN: usize = 100;
/// Upper bound for the list; automatic tags are a small set, so this only stops a crafted backup.
pub const MAX_BLOCKED_TAGS: usize = 500;

/// Checks a name coming from outside and returns its comparison form: trimmed,
/// mapped to the automatic tag's key in any language, lower case. Case does not
/// matter and neither does the language, so "Multicolor" and "mehrfarbig" are one entry.
pub fn normalize_blocked_name(name: &str) -> Result<String, DbError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DbError::Invalid("Tag-Name darf nicht leer sein".to_string()));
    }
    validate_text_length(trimmed, MAX_TAG_NAME_LEN, "Tag")?;
    if trimmed.chars().any(char::is_control) {
        return Err(DbError::Invalid("Tag-Name enthält ungültige Zeichen".to_string()));
    }
    Ok(comparison_key(trimmed))
}

/// The key both sides of a comparison go through, so entries written before the
/// name table learned a new alias still match.
fn comparison_key(name: &str) -> String {
    crate::tagging::canonical_tag_unambiguous(name.trim()).to_lowercase()
}

/// Adds a tag to the list and returns the stored name. Blocking twice is fine.
pub fn block_auto_tag(conn: &Connection, name: &str) -> Result<String, DbError> {
    let key = normalize_blocked_name(name)?;
    if blocked_set(conn)?.contains(&key) {
        return Ok(key);
    }
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM blocked_auto_tags", [], |r| r.get(0))?;
    if count as usize >= MAX_BLOCKED_TAGS {
        return Err(DbError::Invalid(format!("Es können höchstens {MAX_BLOCKED_TAGS} Tags gesperrt werden")));
    }
    conn.execute(
        "INSERT OR IGNORE INTO blocked_auto_tags (name, blocked_at) VALUES (?1, ?2)",
        params![key, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(key)
}

/// Removes the block. Entries are matched by comparison key, not by their text.
pub fn unblock_auto_tag(conn: &Connection, name: &str) -> Result<(), DbError> {
    let key = normalize_blocked_name(name)?;
    for stored in list_blocked_auto_tags(conn)? {
        if comparison_key(&stored) == key {
            conn.execute("DELETE FROM blocked_auto_tags WHERE name = ?1", params![stored])?;
        }
    }
    Ok(())
}

pub fn list_blocked_auto_tags(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare("SELECT name FROM blocked_auto_tags ORDER BY name")?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

fn blocked_set(conn: &Connection) -> Result<HashSet<String>, DbError> {
    Ok(list_blocked_auto_tags(conn)?.iter().map(|n| comparison_key(n)).collect())
}

/// Drops the blocked tags from a list of automatically derived ones. Tags a
/// person assigns by hand never pass through here.
pub fn retain_unblocked(conn: &Connection, tags: &mut Vec<String>) -> Result<(), DbError> {
    if tags.is_empty() {
        return Ok(());
    }
    let blocked = blocked_set(conn)?;
    if !blocked.is_empty() {
        tags.retain(|tag| !blocked.contains(&comparison_key(tag)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{add_tag_to_file, connect_in_memory, test_insert_minimal_file};

    fn conn() -> Connection {
        connect_in_memory().unwrap()
    }

    fn stored(conn: &Connection) -> Vec<String> {
        list_blocked_auto_tags(conn).unwrap()
    }

    #[test]
    fn blocking_ignores_case_and_language_and_stores_the_internal_name() {
        let conn = conn();
        assert_eq!(block_auto_tag(&conn, "Multicolor").unwrap(), "mehrfarbig");
        block_auto_tag(&conn, "MEHRFARBIG").unwrap();
        block_auto_tag(&conn, "  multicolore ").unwrap();
        assert_eq!(stored(&conn), vec!["mehrfarbig"]);
    }

    #[test]
    fn a_name_without_alias_is_kept_but_folded_to_lower_case() {
        let conn = conn();
        block_auto_tag(&conn, "Kabelhalter").unwrap();
        block_auto_tag(&conn, "GEHÄUSE").unwrap();
        block_auto_tag(&conn, "gehäuse").unwrap();
        assert_eq!(stored(&conn), vec!["gehäuse", "kabelhalter"]);
    }

    #[test]
    fn the_ambiguous_alias_mini_is_not_mapped_to_miniatur() {
        let conn = conn();
        block_auto_tag(&conn, "mini").unwrap();
        assert_eq!(stored(&conn), vec!["mini"]);
        let mut tags = vec!["miniatur".to_string(), "mini".to_string()];
        retain_unblocked(&conn, &mut tags).unwrap();
        assert_eq!(tags, vec!["miniatur"]);
    }

    #[test]
    fn unblocking_matches_by_meaning_and_leaves_other_entries() {
        let conn = conn();
        block_auto_tag(&conn, "miniatur").unwrap();
        block_auto_tag(&conn, "kabelhalter").unwrap();
        unblock_auto_tag(&conn, "Miniature").unwrap();
        assert_eq!(stored(&conn), vec!["kabelhalter"]);
        unblock_auto_tag(&conn, "never-blocked").unwrap();
        assert_eq!(stored(&conn), vec!["kabelhalter"]);
    }

    #[test]
    fn an_entry_written_under_an_older_name_table_still_matches_and_unblocks() {
        let conn = conn();
        // Stored as typed before "multipart" was known as an alias of "mehrteilig".
        conn.execute("INSERT INTO blocked_auto_tags (name, blocked_at) VALUES ('Multipart', 'x')", []).unwrap();
        let mut tags = vec!["mehrteilig".to_string(), "pla".to_string()];
        retain_unblocked(&conn, &mut tags).unwrap();
        assert_eq!(tags, vec!["pla"]);
        unblock_auto_tag(&conn, "mehrteilig").unwrap();
        assert!(stored(&conn).is_empty());
    }

    #[test]
    fn invalid_names_are_rejected() {
        let conn = conn();
        for bad in ["", "   ", "a\u{0}b", "line\nbreak", "tab\there", &"x".repeat(101)] {
            assert!(block_auto_tag(&conn, bad).is_err(), "{bad:?}");
            assert!(unblock_auto_tag(&conn, bad).is_err(), "{bad:?}");
        }
        assert!(block_auto_tag(&conn, &"x".repeat(100)).is_ok(), "counter-check: 100 characters are fine");
    }

    #[test]
    fn the_list_has_an_upper_bound() {
        let conn = conn();
        for i in 0..MAX_BLOCKED_TAGS {
            block_auto_tag(&conn, &format!("tag{i}")).unwrap();
        }
        assert!(block_auto_tag(&conn, "one-too-many").is_err());
        assert!(block_auto_tag(&conn, "tag0").is_ok(), "blocking a known tag again is not an addition");
    }

    #[test]
    fn a_tag_assigned_by_hand_survives_the_block_list() {
        let conn = conn();
        block_auto_tag(&conn, "miniatur").unwrap();
        let id = test_insert_minimal_file(&conn, "/tmp/hand.3mf", None).unwrap();
        add_tag_to_file(&conn, id, "miniatur").unwrap();
        let tags: Vec<String> = crate::db::list_all_file_tags(&conn).unwrap().into_iter().map(|(_, t)| t).collect();
        assert_eq!(tags, vec!["miniatur"]);
    }

    #[test]
    fn the_startup_repair_does_not_bring_a_blocked_tag_back() {
        use unicode_normalization::UnicodeNormalization;
        let mut conn = conn();
        let id = test_insert_minimal_file(&conn, &format!("/tmp/{}", "Zahnrad 20 Zähne.3mf".nfd().collect::<String>()), None).unwrap();
        add_tag_to_file(&conn, id, "hne").unwrap();
        block_auto_tag(&conn, "zähne").unwrap();
        crate::db::cleanup_tag_fragments(&mut conn).unwrap();
        let tags: Vec<String> = crate::db::list_all_file_tags(&conn).unwrap().into_iter().map(|(_, t)| t).collect();
        assert_eq!(tags, vec!["zahnrad"]);
    }
}
