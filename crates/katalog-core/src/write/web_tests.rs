use super::*;
#[test]
fn web_atomic_membership_and_bounds() {
    let mut c = db::connect_in_memory().unwrap();
    let a = db::test_insert_minimal_file(&c, "/a.3mf", None).unwrap();
    let b = db::test_insert_minimal_file(&c, "/b.3mf", None).unwrap();
    assert!(matches!(
        set_favorite(&mut c, 999, true),
        Err(WriteError::NotFound { .. })
    ));
    set_favorite(&mut c, a, true).unwrap();
    assert!(db::get_file(&c, a).unwrap().unwrap().favorite);
    assert!(set_print_status(&mut c, a, "other").is_err());
    set_print_status(&mut c, a, "printed").unwrap();
    let cid = create_collection(&mut c, &"😀".repeat(120))
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    assert!(create_collection(&mut c, &"😀".repeat(121)).is_err());
    assert!(add_files_to_collection(&mut c, cid, &[a, 999]).is_err());
    assert!(db::list_collection_file_ids(&c, cid).unwrap().is_empty());
    add_files_to_collection(&mut c, cid, &[a, b, a]).unwrap();
    assert_eq!(db::list_collection_file_ids(&c, cid).unwrap(), vec![a, b]);
    assert!(add_tag(&mut c, a, &"😀".repeat(51)).is_err());
    add_tag(&mut c, a, &"😀".repeat(50)).unwrap();
    add_tag(&mut c, a, "Multipart").unwrap();
    assert!(db::get_file(&c, a)
        .unwrap()
        .unwrap()
        .tags
        .contains(&"mehrteilig".into()));
    let first = add_to_queue(&mut c, a).unwrap().value;
    assert_eq!(add_to_queue(&mut c, a).unwrap().value, first);
    assert!(reorder_queue(&mut c, &[(a, 10), (b, 20)]).is_err());
    assert_eq!(
        db::get_file(&c, a).unwrap().unwrap().queue_position,
        Some(first)
    );
    c.execute_batch("CREATE TRIGGER reject_position BEFORE UPDATE ON collection_files WHEN NEW.position=21 BEGIN SELECT RAISE(ABORT, 'rollback'); END;").unwrap();
    assert!(reorder_collection(&mut c, cid, &[(a, 10), (b, 21)]).is_err());
    assert_eq!(
        c.query_row(
            "SELECT position FROM collection_files WHERE file_id=?1",
            [a],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn every_web_operation_succeeds_and_checks_missing_records() {
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/one.3mf", None).unwrap();
    let cid: i64 = create_collection(&mut c, "one")
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    macro_rules! missing {
        ($e:expr) => {
            assert!(
                matches!($e, Err(WriteError::NotFound { .. })),
                stringify!($e)
            );
        };
    }
    missing!(set_print_status(&mut c, 999, "printed"));
    missing!(add_tag(&mut c, 999, "a"));
    missing!(remove_tag(&mut c, 999, "a"));
    missing!(set_license(&mut c, &[999], None));
    missing!(set_source_url(&mut c, 999, None));
    missing!(add_print_log_entry(
        &mut c,
        999,
        "2026-10-16T00:00:00.000Z".into(),
        None,
        None
    ));
    missing!(delete_print_log_entry(&mut c, 999));
    missing!(mark_file_viewed(&mut c, 999));
    missing!(add_to_queue(&mut c, 999));
    missing!(remove_from_queue(&mut c, 999));
    missing!(rename_collection(&mut c, 999, "x"));
    missing!(delete_collection(&mut c, 999));
    missing!(add_files_to_collection(&mut c, 999, &[id]));
    missing!(remove_file_from_collection(&mut c, 999, id));
    missing!(remove_file_from_collection(&mut c, cid, 999));
    missing!(reorder_collection(&mut c, 999, &[]));
    assert!(matches!(
        reorder_queue(&mut c, &[(999, 0)]),
        Err(WriteError::Invalid { .. })
    ));
    assert!(matches!(
        reorder_collection(&mut c, cid, &[(999, 0)]),
        Err(WriteError::Invalid { .. })
    ));
    add_tag(&mut c, id, "test").unwrap();
    assert_eq!(add_tag(&mut c, id, "test").unwrap().changes, 0);
    assert_eq!(remove_tag(&mut c, id, "test").unwrap().changes, 1);
    db::add_tag_to_file(&c, id, "mini").unwrap();
    remove_tag(&mut c, id, "mini").unwrap();
    assert!(db::get_file(&c, id).unwrap().unwrap().tags.is_empty());
    set_license(&mut c, &[id, id], Some("cc0")).unwrap();
    set_source_url(&mut c, id, Some(" https://example.test ".into())).unwrap();
    mark_file_viewed(&mut c, id).unwrap();
    add_to_queue(&mut c, id).unwrap();
    reorder_queue(&mut c, &[(id, 10)]).unwrap();
    remove_from_queue(&mut c, id).unwrap();
    let entry = add_print_log_entry(
        &mut c,
        id,
        "2026-10-16T00:00:00.000Z".into(),
        Some("😀".repeat(4000)),
        Some("iVBORw0KGgo=".into()),
    )
    .unwrap()
    .value;
    delete_print_log_entry(&mut c, entry.id.parse().unwrap()).unwrap();
    rename_collection(&mut c, cid, "two").unwrap();
    add_files_to_collection(&mut c, cid, &[id]).unwrap();
    reorder_collection(&mut c, cid, &[(id, 9)]).unwrap();
    remove_file_from_collection(&mut c, cid, id).unwrap();
    delete_collection(&mut c, cid).unwrap();
    let file = db::get_file(&c, id).unwrap().unwrap();
    assert_eq!(file.license.as_deref(), Some("cc0"));
    assert_eq!(file.source_url.as_deref(), Some("https://example.test"));
    assert!(file.last_viewed_at.is_some());
    assert!(file.queue_position.is_none());
}

#[test]
fn text_and_id_limits_and_license_rollback() {
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/one.3mf", None).unwrap();
    let cid = create_collection(&mut c, "x")
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    assert!(rename_collection(&mut c, cid, &"é".repeat(121)).is_err());
    assert!(remove_tag(&mut c, id, &"😀".repeat(51)).is_ok());
    let url = format!("https://{}", "é".repeat(2040));
    set_source_url(&mut c, id, Some(url.clone())).unwrap();
    assert!(set_source_url(&mut c, id, Some(format!("{url}x"))).is_err());
    assert!(set_source_url(&mut c, id, Some("javascript:alert(1)".into())).is_err());
    assert!(add_print_log_entry(
        &mut c,
        id,
        "2026-10-16T00:00:00.000Z".into(),
        Some("é".repeat(4001)),
        None
    )
    .is_err());
    assert!(add_print_log_entry(
        &mut c,
        id,
        "2026-10-16T00:00:00.000Z".into(),
        None,
        Some("!".into())
    )
    .is_err());
    use base64::Engine;
    assert!(add_print_log_entry(
        &mut c,
        id,
        "2026-10-16T00:00:00.000Z".into(),
        None,
        Some(base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_CUSTOM_IMAGE_BYTES + 1]))
    )
    .is_err());
    for n in [500, 501] {
        assert_eq!(set_license(&mut c, &vec![id; n], None).is_ok(), n == 500);
        assert_eq!(
            add_files_to_collection(&mut c, cid, &vec![id; n]).is_ok(),
            n == 500
        );
        add_to_queue(&mut c, id).unwrap();
        assert_eq!(reorder_queue(&mut c, &vec![(id, 0); n]).is_ok(), n == 500);
        assert_eq!(
            reorder_collection(&mut c, cid, &vec![(id, 0); n]).is_ok(),
            n == 500
        );
    }
    set_license(&mut c, &[id], Some("own")).unwrap();
    assert!(set_license(&mut c, &[id, 999], Some("cc0")).is_err());
    assert_eq!(
        db::get_file(&c, id).unwrap().unwrap().license.as_deref(),
        Some("own")
    );
    assert!(set_license(&mut c, &[id], Some(&"é".repeat(201))).is_err());
    c.execute_batch("CREATE TRIGGER no_license BEFORE UPDATE OF license ON files WHEN NEW.license='cc0' BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
    assert!(set_license(&mut c, &[id], Some("cc0")).is_err());
    assert_eq!(
        db::get_file(&c, id).unwrap().unwrap().license.as_deref(),
        Some("own")
    );
}

#[test]
fn batch_insert_and_queue_roll_back_after_first_statement() {
    let mut c = db::connect_in_memory().unwrap();
    let a = db::test_insert_minimal_file(&c, "/a", None).unwrap();
    let b = db::test_insert_minimal_file(&c, "/b", None).unwrap();
    let cid = create_collection(&mut c, "x")
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    c.execute_batch(&format!("CREATE TRIGGER stop_insert BEFORE INSERT ON collection_files WHEN NEW.file_id={b} BEGIN SELECT RAISE(ABORT,'blocked'); END;")).unwrap();
    assert!(add_files_to_collection(&mut c, cid, &[a, b]).is_err());
    assert!(db::list_collection_file_ids(&c, cid).unwrap().is_empty());
    add_to_queue(&mut c, a).unwrap();
    add_to_queue(&mut c, b).unwrap();
    c.execute_batch("CREATE TRIGGER stop_queue BEFORE UPDATE OF queue_position ON files WHEN NEW.queue_position=21 BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
    assert!(reorder_queue(&mut c, &[(a, 10), (b, 21)]).is_err());
    assert_eq!(
        db::get_file(&c, a).unwrap().unwrap().queue_position,
        Some(1)
    );
}

#[test]
fn writer_open_never_creates_migrates_or_cleans() {
    let dir = crate::unique_test_dir("write_open");
    let path = dir.join("catalog.db");
    assert!(db::open_read_write(&path).is_err());
    assert!(!path.exists());
    let c = db::connect(&path).unwrap();
    let current: i64 = c
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    for version in [current - 1, current + 1] {
        c.pragma_update(None, "user_version", version).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(db::open_read_write(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    c.pragma_update(None, "user_version", current).unwrap();
    drop(c);
    let c = db::open_read_write(&path).unwrap();
    assert_eq!(
        c.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        c.pragma_query_value(None, "busy_timeout", |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
    drop(c);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn web_rejects_blank_names_and_tags() {
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/blank", None).unwrap();
    let cid = create_collection(&mut c, "original")
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    for blank in ["", " \t\n", "\u{2003}"] {
        assert!(matches!(
            add_tag(&mut c, id, blank),
            Err(WriteError::Invalid { .. })
        ));
        assert!(matches!(
            remove_tag(&mut c, id, blank),
            Err(WriteError::Invalid { .. })
        ));
        assert!(matches!(
            create_collection(&mut c, blank),
            Err(WriteError::Invalid { .. })
        ));
        assert!(matches!(
            rename_collection(&mut c, cid, blank),
            Err(WriteError::Invalid { .. })
        ));
    }
}

#[test]
fn web_source_url_trims_before_length_and_accepts_case_insensitive_scheme() {
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/url", None).unwrap();
    let url = format!("HTTPS://{}", "a".repeat(2040));
    set_source_url(&mut c, id, Some(format!("  {url}  "))).unwrap();
    assert_eq!(db::get_file(&c, id).unwrap().unwrap().source_url, Some(url));
    for url in [
        "https://a\nb",
        "http://a\u{7f}",
        "https://a\0b",
        "\nhttps://a",
        "https://a\t",
    ] {
        assert!(matches!(
            set_source_url(&mut c, id, Some(url.into())),
            Err(WriteError::Invalid { .. })
        ));
    }
}

#[test]
fn web_removes_legacy_long_tags_and_trashed_memberships() {
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/legacy", None).unwrap();
    let tag = "x".repeat(101);
    c.execute("INSERT INTO tags(name,color_hue) VALUES (?1,0)", [&tag])
        .unwrap();
    c.execute(
        "INSERT INTO file_tags(file_id,tag_id) VALUES (?1,last_insert_rowid())",
        [id],
    )
    .unwrap();
    assert_eq!(remove_tag(&mut c, id, &tag).unwrap().changes, 1);
    let cid = create_collection(&mut c, "legacy")
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    add_files_to_collection(&mut c, cid, &[id]).unwrap();
    add_to_queue(&mut c, id).unwrap();
    c.execute("UPDATE files SET deleted_at='2026-10-16' WHERE id=?1", [id])
        .unwrap();
    assert_eq!(
        remove_file_from_collection(&mut c, cid, id)
            .unwrap()
            .changes,
        1
    );
    assert_eq!(remove_from_queue(&mut c, id).unwrap().changes, 1);
    assert!(db::list_collection_file_ids(&c, cid).unwrap().is_empty());
    assert!(c
        .query_row("SELECT queue_position FROM files WHERE id=?1", [id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap()
        .is_none());
}

#[test]
fn web_print_log_validates_timestamp_and_desktop_image_signatures() {
    use base64::Engine;
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/photo", None).unwrap();
    let too_long_date = format!("2026-10-16T00:00:00.{}Z", "0".repeat(44));
    assert_eq!(too_long_date.len(), 65);
    assert!(chrono::DateTime::parse_from_rfc3339(&too_long_date).is_ok());
    for date in [
        &too_long_date,
        "today",
        "2026-02-30T00:00:00Z",
        "2026-10-16",
        &"x".repeat(65),
    ] {
        assert!(matches!(
            add_print_log_entry(&mut c, id, date.into(), None, None),
            Err(WriteError::Invalid { .. })
        ));
    }
    for bytes in [&b"GIF89a"[..], b"<svg/>", b"", b"RIFF1234NOPE", b"\x89PNG"] {
        assert!(matches!(
            add_print_log_entry(
                &mut c,
                id,
                "2026-10-16T00:00:00.000Z".into(),
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(bytes))
            ),
            Err(WriteError::Invalid { .. })
        ));
    }
    assert!(db::list_print_log_entries(&c, id).unwrap().is_empty());
    for bytes in [&b"\x89PNG\r\n\x1a\n"[..], b"\xff\xd8\xff", b"RIFF1234WEBP"] {
        add_print_log_entry(
            &mut c,
            id,
            "2026-10-16T02:00:00+02:00".into(),
            None,
            Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        )
        .unwrap();
    }
    assert_eq!(db::list_print_log_entries(&c, id).unwrap().len(), 3);
}

#[test]
fn snapshot_limits_formats_and_rollback() {
    use base64::Engine;
    let mut c = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&c, "/snapshot.stl", None).unwrap();
    let encode = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    for bytes in [
        &b"\x89PNG\r\n\x1a\n"[..],
        &b"\xff\xd8\xff"[..],
        &b"RIFF0000WEBP"[..],
    ] {
        assert_eq!(
            set_render_snapshot(&mut c, id, &encode(bytes))
                .unwrap()
                .changes,
            1
        );
        assert_eq!(
            db::get_file(&c, id)
                .unwrap()
                .unwrap()
                .render_snapshot_png
                .as_deref(),
            Some(bytes)
        );
    }
    // Invalid Base64 beyond the encoded limit must fail at the length guard.
    let err = set_render_snapshot(
        &mut c,
        id,
        &"!".repeat(MAX_RENDER_SNAPSHOT_BASE64_BYTES + 1),
    )
    .unwrap_err();
    assert!(
        matches!(err, WriteError::Invalid { message } if message == "Snapshot-Base64 ist zu groß")
    );
    for input in [
        "!".into(),
        encode(b"GIF89a"),
        encode(&vec![0; MAX_CUSTOM_IMAGE_BYTES + 1]),
    ] {
        assert!(matches!(
            set_render_snapshot(&mut c, id, &input),
            Err(WriteError::Invalid { .. })
        ));
        assert_eq!(
            db::get_file(&c, id)
                .unwrap()
                .unwrap()
                .render_snapshot_png
                .as_deref(),
            Some(&b"RIFF0000WEBP"[..])
        );
        assert!(c.is_autocommit());
    }
    let mut max = vec![0; MAX_CUSTOM_IMAGE_BYTES];
    max[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    set_render_snapshot(&mut c, id, &encode(&max)).unwrap();
    assert_eq!(
        db::get_file(&c, id)
            .unwrap()
            .unwrap()
            .render_snapshot_png
            .unwrap(),
        max
    );
}
