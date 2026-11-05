use super::*;
use crate::db::{connect_in_memory, delete_file, restore_file, soft_delete_file, test_insert_minimal_file};

fn files(conn: &Connection, n: usize) -> Vec<i64> {
    (0..n).map(|i| test_insert_minimal_file(conn, &format!("/m/{i}.3mf"), None).unwrap()).collect()
}

fn member_ids(conn: &Connection, bundle_id: i64) -> Vec<i64> {
    get_bundle(conn, bundle_id).unwrap().unwrap().members.iter().map(|m| m.file_id).collect()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

fn coded(err: DbError) -> ErrorCode {
    match err {
        DbError::Coded { code, .. } => code,
        other => panic!("expected a coded error, got {other:?}"),
    }
}

#[test]
fn a_bundle_keeps_the_order_given_and_the_first_file_is_the_title() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    let id = create_bundle(&conn, "  Vase  ", BundleKind::Variants, &[f[2], f[0], f[1]], "t").unwrap();
    let bundle = get_bundle(&conn, id).unwrap().unwrap();
    assert_eq!(bundle.name, "Vase", "the name is stored trimmed");
    assert_eq!(bundle.kind, BundleKind::Variants);
    assert_eq!(member_ids(&conn, id), vec![f[2], f[0], f[1]]);
    assert_eq!(bundle.members.iter().map(|m| m.position).collect::<Vec<_>>(), vec![0, 1, 2]);
}

#[test]
fn a_bundle_takes_two_to_two_hundred_files() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 201);
    assert_eq!(coded(create_bundle(&conn, "A", BundleKind::Parts, &f[..1], "t").unwrap_err()), ErrorCode::BundleTooFewFiles);
    assert_eq!(coded(create_bundle(&conn, "A", BundleKind::Parts, &[], "t").unwrap_err()), ErrorCode::BundleTooFewFiles);
    assert!(create_bundle(&conn, "A", BundleKind::Parts, &f[..2], "t").is_ok());
    assert!(create_bundle(&conn, "B", BundleKind::Parts, &f[1..201], "t").is_ok(), "200 files");
    assert_eq!(coded(create_bundle(&conn, "C", BundleKind::Parts, &f, "t").unwrap_err()), ErrorCode::BundleTooManyFiles);
    assert_eq!(count(&conn, "bundles"), 1, "A dissolved when its second file moved to B; the rejected ones left nothing behind");
    assert_eq!(count(&conn, "bundle_members"), 200);
}

#[test]
fn rejected_input_changes_nothing() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    soft_delete_file(&conn, f[2], None, "now").unwrap();
    for name in ["", "   ", "Tab\there", &"n".repeat(121)] {
        assert_eq!(coded(create_bundle(&conn, name, BundleKind::Parts, &f[..2], "t").unwrap_err()), ErrorCode::BundleNameInvalid, "{name:?}");
    }
    assert!(create_bundle(&conn, &"n".repeat(120), BundleKind::Parts, &f[..2], "t").is_ok(), "120 characters are fine");
    let before = count(&conn, "bundle_members");
    assert_eq!(coded(create_bundle(&conn, "A", BundleKind::Parts, &[f[0], f[0]], "t").unwrap_err()), ErrorCode::BundleDuplicateFiles);
    assert_eq!(coded(create_bundle(&conn, "A", BundleKind::Parts, &[f[0], 99_999], "t").unwrap_err()), ErrorCode::EntryNotFound);
    assert_eq!(
        coded(create_bundle(&conn, "A", BundleKind::Parts, &[f[0], f[2]], "t").unwrap_err()),
        ErrorCode::EntryNotFound,
        "a file in the trash cannot be added"
    );
    assert_eq!(count(&conn, "bundle_members"), before);
}

#[test]
fn a_file_is_in_one_bundle_only_and_moves_when_bundled_again() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 5);
    let first = create_bundle(&conn, "First", BundleKind::Parts, &[f[0], f[1], f[2]], "t").unwrap();
    let second = create_bundle(&conn, "Second", BundleKind::Variants, &[f[2], f[3]], "t").unwrap();
    assert_eq!(member_ids(&conn, first), vec![f[0], f[1]], "the file left the first bundle");
    assert_eq!(member_ids(&conn, second), vec![f[2], f[3]]);
    let in_two: i64 = conn
        .query_row("SELECT COUNT(*) FROM (SELECT file_id FROM bundle_members GROUP BY file_id HAVING COUNT(*) > 1)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(in_two, 0);

    // Moving the second member out of a pair leaves one behind: that bundle is dissolved.
    create_bundle(&conn, "Third", BundleKind::Parts, &[f[1], f[4]], "t").unwrap();
    assert!(get_bundle(&conn, first).unwrap().is_none(), "one member is no bundle");
    assert_eq!(count(&conn, "files"), 5, "no file is touched");
}

#[test]
fn adding_files_appends_them_and_moves_files_of_other_bundles() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 6);
    let a = create_bundle(&conn, "A", BundleKind::Parts, &[f[0], f[1]], "t").unwrap();
    let b = create_bundle(&conn, "B", BundleKind::Parts, &[f[2], f[3], f[4]], "t").unwrap();
    add_files_to_bundle(&conn, a, &[f[1], f[4], f[5]]).unwrap();
    assert_eq!(member_ids(&conn, a), vec![f[0], f[1], f[4], f[5]], "a member stays where it was, the others go to the end");
    assert_eq!(member_ids(&conn, b), vec![f[2], f[3]]);
    assert_eq!(coded(add_files_to_bundle(&conn, 9_999, &[f[2]]).unwrap_err()), ErrorCode::BundleNotFound);
    assert_eq!(coded(add_files_to_bundle(&conn, a, &[f[2], f[2]]).unwrap_err()), ErrorCode::BundleDuplicateFiles);
}

#[test]
fn a_bundle_cannot_grow_past_two_hundred_members() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 202);
    let id = create_bundle(&conn, "Big", BundleKind::Parts, &f[..200], "t").unwrap();
    assert_eq!(coded(add_files_to_bundle(&conn, id, &f[200..201]).unwrap_err()), ErrorCode::BundleTooManyFiles);
    add_files_to_bundle(&conn, id, &f[..3]).unwrap();
    assert_eq!(count(&conn, "bundle_members"), 200, "files that are in already do not count twice");
}

#[test]
fn removing_members_dissolves_a_bundle_that_drops_below_two() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 4);
    let id = create_bundle(&conn, "A", BundleKind::Parts, &[f[0], f[1], f[2]], "t").unwrap();
    assert_eq!(remove_files_from_bundle(&conn, &[f[1], f[3]]).unwrap(), 1, "a file outside every bundle is ignored");
    assert_eq!(member_ids(&conn, id), vec![f[0], f[2]]);
    remove_files_from_bundle(&conn, &[f[0]]).unwrap();
    assert!(get_bundle(&conn, id).unwrap().is_none());
    assert_eq!(count(&conn, "bundles"), 0);
    assert_eq!(count(&conn, "bundle_members"), 0, "the last member left with the bundle");
    assert_eq!(count(&conn, "files"), 4);
}

#[test]
fn dissolving_touches_no_row_of_the_four_companion_tables() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    conn.execute("INSERT INTO tags (id, name, color_hue) VALUES (1, 'deko', 10)", []).unwrap();
    conn.execute("INSERT INTO file_tags VALUES (?1, 1)", [f[0]]).unwrap();
    let snapshot = |conn: &Connection| -> String {
        ["SELECT group_concat(id || '|' || name || '|' || path || '|' || ifnull(license, '') || '|' || print_status, ';') FROM (SELECT * FROM files ORDER BY id)",
         "SELECT group_concat(id || '|' || name, ';') FROM (SELECT * FROM tags ORDER BY id)",
         "SELECT group_concat(file_id || '|' || tag_id, ';') FROM (SELECT * FROM file_tags ORDER BY file_id, tag_id)",
         "SELECT group_concat(id || '|' || path, ';') FROM (SELECT * FROM folders ORDER BY id)"]
            .iter()
            .map(|sql| conn.query_row(sql, [], |r| r.get::<_, Option<String>>(0)).unwrap().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("#")
    };
    let before = snapshot(&conn);
    let id = create_bundle(&conn, "A", BundleKind::Parts, &f, "t").unwrap();
    assert_eq!(snapshot(&conn), before, "bundling touches no file row");
    dissolve_bundle(&conn, id).unwrap();
    assert_eq!(snapshot(&conn), before, "dissolving touches no file row");
    assert_eq!(count(&conn, "bundle_members"), 0);
    assert_eq!(coded(dissolve_bundle(&conn, id).unwrap_err()), ErrorCode::BundleNotFound);
}

#[test]
fn name_kind_label_and_title_can_be_changed() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    let id = create_bundle(&conn, "A", BundleKind::Parts, &f, "t").unwrap();
    update_bundle(&conn, id, Some(" Renamed "), Some(BundleKind::Variants)).unwrap();
    let bundle = get_bundle(&conn, id).unwrap().unwrap();
    assert_eq!((bundle.name.as_str(), bundle.kind), ("Renamed", BundleKind::Variants));
    update_bundle(&conn, id, None, None).unwrap();
    assert_eq!(coded(update_bundle(&conn, id, Some(""), None).unwrap_err()), ErrorCode::BundleNameInvalid);
    assert_eq!(coded(update_bundle(&conn, 9_999, Some("x"), None).unwrap_err()), ErrorCode::BundleNotFound);

    update_bundle_member(&conn, f[1], Some("0.6 mm"), false).unwrap();
    let labels = |conn: &Connection| get_bundle(conn, id).unwrap().unwrap().members.into_iter().map(|m| m.label).collect::<Vec<_>>();
    assert_eq!(labels(&conn), vec![None, Some("0.6 mm".to_string()), None]);
    update_bundle_member(&conn, f[1], Some("  "), false).unwrap();
    assert_eq!(labels(&conn), vec![None, None, None], "an empty text clears the label");
    update_bundle_member(&conn, f[1], None, false).unwrap();
    assert_eq!(coded(update_bundle_member(&conn, f[1], Some(&"l".repeat(61)), false).unwrap_err()), ErrorCode::BundleLabelInvalid);
    assert_eq!(coded(update_bundle_member(&conn, f[1], Some("a\nb"), false).unwrap_err()), ErrorCode::BundleLabelInvalid);
    assert_eq!(coded(update_bundle_member(&conn, 9_999, None, false).unwrap_err()), ErrorCode::EntryNotFound);

    update_bundle_member(&conn, f[2], None, true).unwrap();
    assert_eq!(member_ids(&conn, id), vec![f[2], f[1], f[0]], "the title and the member swap places");
    update_bundle_member(&conn, f[2], None, true).unwrap();
    assert_eq!(member_ids(&conn, id), vec![f[2], f[1], f[0]], "making the title the title changes nothing");
}

#[test]
fn the_list_shows_trashed_members_and_hides_bundles_with_fewer_than_two_rows() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 5);
    let a = create_bundle(&conn, "A", BundleKind::Parts, &[f[0], f[1]], "t1").unwrap();
    let b = create_bundle(&conn, "B", BundleKind::Variants, &[f[2], f[3]], "t2").unwrap();
    soft_delete_file(&conn, f[1], None, "now").unwrap();
    let listed = list_bundles(&conn).unwrap();
    assert_eq!(listed.iter().map(|x| x.id).collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(listed[0].members.iter().map(|m| m.in_trash).collect::<Vec<_>>(), vec![false, true]);

    // A one-member leftover (a deletion path that did not prune) is invisible, and goes at the next prune.
    conn.execute("DELETE FROM bundle_members WHERE file_id = ?1", [f[3]]).unwrap();
    assert_eq!(list_bundles(&conn).unwrap().iter().map(|x| x.id).collect::<Vec<_>>(), vec![a]);
    assert!(get_bundle(&conn, b).unwrap().is_none());
    assert_eq!(count(&conn, "bundles"), 2);
    assert_eq!(prune_bundles(&conn).unwrap(), 1);
    assert_eq!(count(&conn, "bundles"), 1);
    assert_eq!(count(&conn, "bundle_members"), 2, "the leftover's last member went with it");
}

#[test]
fn the_trash_keeps_the_membership_and_deleting_for_good_prunes() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    let id = create_bundle(&conn, "A", BundleKind::Parts, &f, "t").unwrap();

    soft_delete_file(&conn, f[0], None, "now").unwrap();
    assert_eq!(member_ids(&conn, id), vec![f[0], f[1], f[2]], "a file in the trash is still a member");
    restore_file(&conn, f[0], None).unwrap();
    assert!(!get_bundle(&conn, id).unwrap().unwrap().members[0].in_trash, "restoring needs nothing else");

    delete_file(&conn, f[0]).unwrap();
    assert_eq!(member_ids(&conn, id), vec![f[1], f[2]]);
    delete_file(&conn, f[1]).unwrap();
    assert_eq!(count(&conn, "bundles"), 0, "one member left: the bundle goes");
    assert_eq!(count(&conn, "bundle_members"), 0);
    assert_eq!(count(&conn, "files"), 1);
}

#[test]
fn a_catalog_start_removes_leftover_bundles() {
    let mut conn = connect_in_memory().unwrap();
    let f = files(&conn, 2);
    let id = create_bundle(&conn, "A", BundleKind::Parts, &f, "t").unwrap();
    conn.execute("DELETE FROM bundle_members WHERE file_id = ?1", [f[1]]).unwrap();
    assert_eq!(count(&conn, "bundles"), 1);
    crate::db::repository::init(&mut conn).unwrap();
    assert_eq!(count(&conn, "bundles"), 0, "bundle {id} was pruned at start");
    assert_eq!(count(&conn, "bundle_members"), 0);
}

#[test]
fn the_kind_text_round_trips() {
    for kind in [BundleKind::Variants, BundleKind::Parts] {
        assert_eq!(BundleKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(BundleKind::parse("other"), None);
}

#[test]
fn restoring_a_bundle_brings_back_members_in_the_trash_with_position_and_label() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 4);
    soft_delete_file(&conn, f[2], None, "now").unwrap();
    let members: Vec<(i64, String)> = vec![(f[1], "A".into()), (f[2], "Lid".into()), (f[0], String::new())];
    let (id, left_out) = restore_bundle(&conn, "Vase", BundleKind::Parts, &members, "t").unwrap();
    assert_eq!(left_out, 0);
    let bundle = get_bundle(&conn, id).unwrap().unwrap();
    let rows: Vec<_> = bundle.members.iter().map(|m| (m.file_id, m.position, m.label.clone(), m.in_trash)).collect();
    assert_eq!(rows, vec![(f[1], 0, Some("A".into()), false), (f[2], 1, Some("Lid".into()), true), (f[0], 2, None, false)]);
}

#[test]
fn restoring_leaves_out_deleted_and_re_bundled_members_and_counts_them() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 5);
    delete_file(&conn, f[1]).unwrap();
    create_bundle(&conn, "Other", BundleKind::Parts, &[f[2], f[4]], "t").unwrap();
    let members: Vec<(i64, String)> = f[..4].iter().map(|id| (*id, String::new())).collect();
    let (id, left_out) = restore_bundle(&conn, "Vase", BundleKind::Variants, &members, "t").unwrap();
    assert_eq!(left_out, 2);
    assert_eq!(member_ids(&conn, id), vec![f[0], f[3]]);
    assert_eq!(member_ids(&conn, 1), vec![f[2], f[4]], "the other bundle keeps its members");
}

#[test]
fn restoring_with_fewer_than_two_members_left_writes_nothing() {
    let conn = connect_in_memory().unwrap();
    let f = files(&conn, 3);
    delete_file(&conn, f[1]).unwrap();
    let members: Vec<(i64, String)> = vec![(f[0], "x".into()), (f[1], String::new())];
    assert_eq!(coded(restore_bundle(&conn, "Vase", BundleKind::Parts, &members, "t").unwrap_err()), ErrorCode::BundleTooFewFiles);
    assert_eq!((count(&conn, "bundles"), count(&conn, "bundle_members")), (0, 0));
    let bad_label = vec![(f[0], "l".repeat(61)), (f[2], String::new())];
    assert_eq!(coded(restore_bundle(&conn, "Vase", BundleKind::Parts, &bad_label, "t").unwrap_err()), ErrorCode::BundleLabelInvalid);
    assert_eq!(count(&conn, "bundles"), 0);
}
