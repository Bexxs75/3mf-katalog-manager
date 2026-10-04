use rusqlite::{params, Connection, OptionalExtension};
use super::error::DbError;

/// The completion marker and all repairs commit together; a failed run retries.
pub fn cleanup_tag_fragments(conn: &mut Connection) -> Result<Option<(usize, usize, usize)>, DbError> {
    let tx = conn.transaction()?;
    let done: Option<String> = tx.query_row(
        "SELECT value FROM app_settings WHERE key = 'tag_fragment_cleanup_done'", [], |row| row.get(0),
    ).optional()?;
    if done.is_some() { return Ok(None); }
    let files: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, name FROM files")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let (mut changed, mut removed, mut added) = (0, 0, 0);
    for (id, name) in files {
        let (fragments, correct) = crate::tagging::filename_tag_fragments(&name);
        if fragments.is_empty() { continue; }
        let (before_removed, before_added) = (removed, added);
        for fragment in fragments {
            removed += tx.prepare_cached(
                "DELETE FROM file_tags WHERE file_id = ?1 AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            )?.execute(params![id, fragment])?;
        }
        for tag in correct {
            let exists: bool = tx.prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM file_tags ft JOIN tags t ON t.id = ft.tag_id WHERE ft.file_id = ?1 AND t.name = ?2)",
            )?.query_row(params![id, tag], |row| row.get(0))?;
            if !exists {
                super::add_tag_to_file(&tx, id, &tag)?;
                added += 1;
            }
        }
        if removed != before_removed || added != before_added { changed += 1; }
    }
    super::delete_unused_tags(&tx)?;
    tx.execute("INSERT INTO app_settings (key, value) VALUES ('tag_fragment_cleanup_done', '1')", [])?;
    tx.commit()?;
    Ok(Some((changed, removed, added)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{add_tag_to_file, connect_in_memory, test_insert_minimal_file};
    use unicode_normalization::UnicodeNormalization;

    fn tags(conn: &Connection, id: i64) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT t.name FROM tags t JOIN file_tags ft ON ft.tag_id = t.id WHERE ft.file_id = ?1 ORDER BY t.name").unwrap();
        stmt.query_map([id], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    }

    #[test]
    fn repairs_nfd_fragments_per_file_preserves_other_tags_and_runs_once() {
        let mut conn = connect_in_memory().unwrap();
        let cases = [
            ("Zahnrad 20 Zähne.3mf", vec!["zahnrad", "hne"], vec!["zahnrad", "zähne"]),
            ("Einplatinen-Gehäuse.3mf", vec!["einplatinen", "geha", "use"], vec!["einplatinen", "gehäuse"]),
            ("Handyständer.3mf", vec!["handysta", "nder"], vec!["handyständer"]),
        ];
        let mut ids = Vec::new();
        for (name, old, correct) in cases {
            let id = test_insert_minimal_file(&conn, &format!("/tmp/{}", name.nfd().collect::<String>()), None).unwrap();
            for tag in old { add_tag_to_file(&conn, id, tag).unwrap(); }
            add_tag_to_file(&conn, id, "mein-manueller-tag").unwrap();
            ids.push((id, correct));
        }
        let plain = test_insert_minimal_file(&conn, "/tmp/use.3mf", None).unwrap();
        add_tag_to_file(&conn, plain, "use").unwrap();
        let untagged = test_insert_minimal_file(&conn, "/tmp/halter.3mf", None).unwrap();
        conn.execute("INSERT INTO tags (name, color_hue) VALUES ('verwaist', 0)", []).unwrap();
        assert_eq!(cleanup_tag_fragments(&mut conn).unwrap(), Some((3, 5, 3)));
        for (id, mut correct) in ids {
            correct.push("mein-manueller-tag"); correct.sort();
            assert_eq!(tags(&conn, id), correct);
        }
        assert_eq!(tags(&conn, plain), vec!["use"]);
        assert!(tags(&conn, untagged).is_empty());
        let orphan_count: i64 = conn.query_row("SELECT COUNT(*) FROM tags WHERE name IN ('verwaist', 'hne', 'geha', 'handysta', 'nder')", [], |r| r.get(0)).unwrap();
        assert_eq!(orphan_count, 0);
        add_tag_to_file(&conn, plain, "hne").unwrap();
        assert_eq!(cleanup_tag_fragments(&mut conn).unwrap(), None);
        assert!(tags(&conn, plain).contains(&"hne".to_string()));
    }

    #[test]
    fn repairs_composed_names_and_preserves_legitimate_fragment_in_same_name() {
        let mut conn = connect_in_memory().unwrap();
        let id = test_insert_minimal_file(&conn, "/tmp/use-Gehäuse.3mf", None).unwrap();
        for tag in ["use", "geha"] { add_tag_to_file(&conn, id, tag).unwrap(); }
        assert_eq!(cleanup_tag_fragments(&mut conn).unwrap(), Some((1, 1, 1)));
        assert_eq!(tags(&conn, id), vec!["gehäuse", "use"]);
    }

    #[test]
    fn failure_rolls_back_tags_and_completion_marker() {
        let mut conn = connect_in_memory().unwrap();
        let id = test_insert_minimal_file(&conn, "/tmp/Handyständer.3mf", None).unwrap();
        add_tag_to_file(&conn, id, "nder").unwrap();
        conn.execute_batch("CREATE TRIGGER reject_cleanup_marker BEFORE INSERT ON app_settings
            WHEN NEW.key = 'tag_fragment_cleanup_done' BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(cleanup_tag_fragments(&mut conn).is_err());
        assert_eq!(tags(&conn, id), vec!["nder"]);
        conn.execute_batch("DROP TRIGGER reject_cleanup_marker").unwrap();
        assert_eq!(cleanup_tag_fragments(&mut conn).unwrap(), Some((1, 1, 1)));
        assert_eq!(tags(&conn, id), vec!["handyständer"]);
    }

    #[test]
    fn repairs_5000_files_under_one_second() {
        let mut conn = connect_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0..5000 {
            let id = test_insert_minimal_file(&tx, &format!("/tmp/{i}-Handyständer.3mf"), None).unwrap();
            for tag in ["handysta", "nder"] { add_tag_to_file(&tx, id, tag).unwrap(); }
        }
        tx.commit().unwrap();
        let start = std::time::Instant::now();
        assert_eq!(cleanup_tag_fragments(&mut conn).unwrap(), Some((5000, 10000, 5000)));
        let elapsed = start.elapsed();
        eprintln!("5000-file cleanup: {elapsed:?}");
        assert!(elapsed < std::time::Duration::from_secs(1), "{elapsed:?}");
    }
}
