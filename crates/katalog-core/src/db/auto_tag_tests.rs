//! The web limit for derived tags: over it a tag is left out and the import goes on.
use super::models::FileType;
use super::print_info_tests::new_file;
use super::*;
use rusqlite::Connection;

fn tagged(path: &str, tags: &[&str]) -> models::NewFile {
    let mut file = new_file(path, FileType::ThreeMf, None);
    file.tags = tags.iter().map(|t| t.to_string()).collect();
    file
}
fn tag_names(conn: &Connection, id: i64) -> Vec<String> {
    let mut names = get_file(conn, id).unwrap().unwrap().tags;
    names.sort();
    names
}

#[test]
fn a_new_tag_fits_only_below_the_limit_an_existing_one_always() {
    let conn = connect_in_memory().unwrap();
    conn.execute_batch("INSERT INTO tags(name,color_hue) VALUES ('a',0),('b',0)").unwrap();
    assert!(!repository::auto_tag_fits(&conn, "new", 2).unwrap());
    assert!(repository::auto_tag_fits(&conn, "a", 2).unwrap());
    assert!(repository::auto_tag_fits(&conn, "new", 3).unwrap());
    // Unlimited (the desktop): always.
    assert!(repository::auto_tag_fits(&conn, "new", i64::MAX).unwrap());
}

#[test]
fn derived_tags_over_the_web_limit_are_skipped_and_the_file_is_still_imported() {
    let mut conn = connect_in_memory().unwrap();
    let before = tagged("/tmp/before.3mf", &["early"]);
    // Default (desktop): unlimited.
    let tx = conn.transaction().unwrap();
    let first = insert_file_within_tx(&tx, &before).unwrap();
    tx.commit().unwrap();
    assert_eq!(tag_names(&conn, first), vec!["early".to_string()]);
    // The web limit, with the catalog exactly full.
    let limit = crate::write::web::MAX_TAGS;
    conn.execute_batch(&format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < {limit}) INSERT OR IGNORE INTO tags(name,color_hue) SELECT 'bulk' || i, 0 FROM n WHERE (SELECT COUNT(*) FROM tags) < {limit}"
    ))
    .unwrap();
    let full: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap();
    assert!(full >= limit);
    set_auto_tag_limit(limit);
    let tx = conn.transaction().unwrap();
    let id = insert_file_within_tx(&tx, &tagged("/tmp/over.3mf", &["early", "never-seen"])).unwrap();
    tx.commit().unwrap();
    set_auto_tag_limit(i64::MAX);
    // Imported; the known tag is attached, the new one is not created.
    assert_eq!(tag_names(&conn, id), vec!["early".to_string()]);
    let none: i64 = conn.query_row("SELECT COUNT(*) FROM tags WHERE name='never-seen'", [], |r| r.get(0)).unwrap();
    assert_eq!(none, 0);
    // Gegenprobe: without the limit the same new tag is created.
    let tx = conn.transaction().unwrap();
    let again = insert_file_within_tx(&tx, &tagged("/tmp/free.3mf", &["never-seen"])).unwrap();
    tx.commit().unwrap();
    assert_eq!(tag_names(&conn, again), vec!["never-seen".to_string()]);
}
