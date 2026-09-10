pub mod error;
pub mod models;
mod repository;

pub use repository::{
    add_tag_to_file, connect, delete_file, delete_filament_spool, delete_unused_tags,
    file_exists_by_hash, file_exists_by_path, get_file, insert_file, insert_filament_spool, insert_folder,
    list_cloud_accounts, list_creator_counts, list_filament_spools, list_files, list_folders, list_tag_counts,
    mark_file_viewed, remove_tag_from_file, set_cloud_account_status, set_file_cloud_link, set_file_modified_at,
    set_file_sync_status, set_print_status, update_filament_spool, upsert_cloud_account,
};

#[cfg(test)]
pub use repository::connect_in_memory;

#[cfg(test)]
mod tests {
    use super::*;
    use models::{FileType, MaterialRecord, NewFile, NewFilamentSpool};
    use std::collections::BTreeMap;

    fn sample_file() -> NewFile {
        let mut metadata = BTreeMap::new();
        metadata.insert("Designer".to_string(), "Jane".to_string());

        NewFile {
            name: "cube.3mf".to_string(),
            path: "/tmp/cube.3mf".to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 1024,
            dimensions_mm: Some([10.0, 10.0, 10.0]),
            volume_cm3: Some(1.0),
            object_count: Some(1),
            thumbnail_png: None,
            imported_at: "2026-09-08T12:00:00Z".to_string(),
            file_modified_at: None,
            materials: vec![MaterialRecord {
                name: "PLA".to_string(),
                display_color: Some("#ff0000".to_string()),
            }],
            metadata,
            tags: vec!["cube".to_string(), "test".to_string()],
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
        }
    }

    #[test]
    fn stores_and_lists_the_new_file_columns() {
        let mut conn = connect_in_memory().expect("connect");
        let mut file = sample_file();
        file.print_status = "printed".to_string();
        file.creator = Some("Jane".to_string());
        file.content_hash = Some("abc123".to_string());
        let id = insert_file(&mut conn, &file).expect("insert");

        let stored = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(stored.print_status, "printed");
        assert_eq!(stored.last_viewed_at, None);
        assert_eq!(stored.creator, Some("Jane".to_string()));
        assert_eq!(stored.content_hash, Some("abc123".to_string()));
    }

    #[test]
    fn set_print_status_updates_the_status() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_print_status(&conn, id, "printed").expect("update");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.print_status, "printed");
    }

    #[test]
    fn mark_file_viewed_sets_a_timestamp() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");
        let before = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(before.last_viewed_at, None);

        mark_file_viewed(&conn, id).expect("mark viewed");
        let after = get_file(&conn, id).expect("query").expect("present");
        assert!(after.last_viewed_at.is_some());
    }

    #[test]
    fn list_creator_counts_groups_by_creator_and_excludes_missing() {
        let mut conn = connect_in_memory().expect("connect");

        let mut a = sample_file();
        a.creator = Some("Jane".to_string());
        insert_file(&mut conn, &a).expect("insert 1");

        let mut b = sample_file();
        b.name = "second.3mf".to_string();
        b.path = "/tmp/second.3mf".to_string();
        b.creator = Some("Jane".to_string());
        insert_file(&mut conn, &b).expect("insert 2");

        let mut c = sample_file();
        c.name = "third.stl".to_string();
        c.path = "/tmp/third.stl".to_string();
        c.creator = None;
        insert_file(&mut conn, &c).expect("insert 3");

        let counts = list_creator_counts(&conn).expect("list");
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].name, "Jane");
        assert_eq!(counts[0].count, 2);
    }

    #[test]
    fn file_exists_by_hash_finds_only_matching_hash() {
        let mut conn = connect_in_memory().expect("connect");
        let mut file = sample_file();
        file.content_hash = Some("hash-a".to_string());
        insert_file(&mut conn, &file).expect("insert");

        assert!(file_exists_by_hash(&conn, "hash-a").expect("query"));
        assert!(!file_exists_by_hash(&conn, "hash-b").expect("query"));
    }

    #[test]
    fn inserts_and_reads_back_a_file() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.name, "cube.3mf");
        assert_eq!(file.file_type, FileType::ThreeMf);
        assert_eq!(file.origin, "local");
        assert_eq!(file.sync_status, "local-only");
        assert_eq!(file.dimensions_mm, Some([10.0, 10.0, 10.0]));
        assert_eq!(file.materials.len(), 1);
        assert_eq!(file.materials[0].name, "PLA");
        assert_eq!(file.metadata.get("Designer"), Some(&"Jane".to_string()));
        assert_eq!(file.tags, vec!["cube".to_string(), "test".to_string()]);
    }

    #[test]
    fn list_files_returns_all_inserted_files() {
        let mut conn = connect_in_memory().expect("connect");
        insert_file(&mut conn, &sample_file()).expect("insert 1");

        let mut second = sample_file();
        second.name = "bracket.stl".to_string();
        second.path = "/tmp/bracket.stl".to_string();
        second.file_type = FileType::Stl;
        second.tags = vec![];
        insert_file(&mut conn, &second).expect("insert 2");

        let files = list_files(&conn).expect("list");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "bracket.stl");
        assert_eq!(files[1].name, "cube.3mf");
    }

    #[test]
    fn get_file_returns_none_for_missing_id() {
        let conn = connect_in_memory().expect("connect");
        assert!(get_file(&conn, 999).expect("query").is_none());
    }

    #[test]
    fn folders_round_trip() {
        let conn = connect_in_memory().expect("connect");
        insert_folder(&conn, "Miniatures").expect("insert folder");
        insert_folder(&conn, "Vases").expect("insert folder");

        let folders = list_folders(&conn).expect("list folders");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].name, "Miniatures");
        assert_eq!(folders[1].name, "Vases");
    }

    #[test]
    fn tag_counts_and_deterministic_hue() {
        let mut conn = connect_in_memory().expect("connect");
        insert_file(&mut conn, &sample_file()).expect("insert");

        let counts = list_tag_counts(&conn).expect("tag counts");
        assert_eq!(counts.len(), 2);
        let cube_tag = counts.iter().find(|t| t.name == "cube").expect("cube tag");
        assert_eq!(cube_tag.count, 1);

        // Re-inserting a file with the same tag must reuse the tag row and
        // keep its color_hue stable rather than assigning a new one.
        let mut second = sample_file();
        second.name = "cube2.3mf".to_string();
        second.path = "/tmp/cube2.3mf".to_string();
        insert_file(&mut conn, &second).expect("insert 2");

        let counts_after = list_tag_counts(&conn).expect("tag counts after");
        let cube_tag_after = counts_after
            .iter()
            .find(|t| t.name == "cube")
            .expect("cube tag after");
        assert_eq!(cube_tag_after.count, 2);
        assert_eq!(cube_tag_after.color_hue, cube_tag.color_hue);
    }

    #[test]
    fn file_exists_by_path_reflects_current_db_state() {
        let mut conn = connect_in_memory().expect("connect");
        assert!(!file_exists_by_path(&conn, "/tmp/cube.3mf").expect("check"));

        insert_file(&mut conn, &sample_file()).expect("insert");
        assert!(file_exists_by_path(&conn, "/tmp/cube.3mf").expect("check"));
        assert!(!file_exists_by_path(&conn, "/tmp/other.3mf").expect("check"));
    }

    #[test]
    fn deletes_a_file_and_cascades_tags() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        delete_file(&conn, id).expect("delete");

        assert!(get_file(&conn, id).expect("query").is_none());
        let counts = list_tag_counts(&conn).expect("tag counts");
        assert!(counts.iter().all(|t| t.count == 0));
    }

    #[test]
    fn adds_and_removes_tags_on_a_file() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        add_tag_to_file(&conn, id, "neu-hinzugefuegt").expect("add tag");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert!(file.tags.contains(&"neu-hinzugefuegt".to_string()));

        remove_tag_from_file(&conn, id, "cube").expect("remove tag");
        let file = get_file(&conn, id).expect("query").expect("present");
        assert!(!file.tags.contains(&"cube".to_string()));
        assert!(file.tags.contains(&"neu-hinzugefuegt".to_string()));
    }

    #[test]
    fn removing_the_last_use_of_a_tag_deletes_it() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        // "cube" haengt nur an dieser einen Datei (siehe sample_file()).
        remove_tag_from_file(&conn, id, "cube").expect("remove tag");

        let tags = list_tag_counts(&conn).expect("list tags");
        assert!(!tags.iter().any(|t| t.name == "cube"));
    }

    #[test]
    fn deleting_a_file_deletes_its_now_unused_tags() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        delete_file(&conn, id).expect("delete file");

        let tags = list_tag_counts(&conn).expect("list tags");
        assert!(!tags.iter().any(|t| t.name == "cube" || t.name == "test"));
    }

    #[test]
    fn deleting_a_file_keeps_tags_still_used_elsewhere() {
        let mut conn = connect_in_memory().expect("connect");
        let id_a = insert_file(&mut conn, &sample_file()).expect("insert a");
        let mut other = sample_file();
        other.name = "other.3mf".to_string();
        other.path = "/tmp/other.3mf".to_string();
        let _id_b = insert_file(&mut conn, &other).expect("insert b");

        delete_file(&conn, id_a).expect("delete file a");

        // "cube"/"test" haengen noch an der zweiten Datei und duerfen nicht
        // mitgeloescht werden.
        let tags = list_tag_counts(&conn).expect("list tags");
        assert!(tags.iter().any(|t| t.name == "cube"));
        assert!(tags.iter().any(|t| t.name == "test"));
    }

    #[test]
    fn delete_unused_tags_removes_orphans_but_keeps_used_tags() {
        let mut conn = connect_in_memory().expect("connect");
        insert_file(&mut conn, &sample_file()).expect("insert");
        // Verwaisten Tag simulieren, wie er vor dieser Aufraeum-Logik
        // entstehen konnte (z.B. durch das direkte DELETE FROM files vor
        // dem Fix, das file_tags per Cascade mitloeschte, den Tag selbst
        // aber stehen liess).
        conn.execute(
            "INSERT INTO tags (name, color_hue) VALUES ('verwaist', 10)",
            [],
        )
        .expect("insert orphan tag");

        let removed = delete_unused_tags(&conn).expect("cleanup");
        assert_eq!(removed, 1);

        let tags = list_tag_counts(&conn).expect("list tags");
        assert!(!tags.iter().any(|t| t.name == "verwaist"));
        assert!(tags.iter().any(|t| t.name == "cube"));
    }

    #[test]
    fn upserts_and_lists_cloud_accounts() {
        let conn = connect_in_memory().expect("connect");
        let id = upsert_cloud_account(&conn, "gdrive", "user@example.com", "2026-09-09T12:00:00Z")
            .expect("upsert");
        assert!(id > 0);

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].provider, "gdrive");
        assert_eq!(accounts[0].account_label, "user@example.com");
        assert_eq!(accounts[0].status, "connected");
    }

    #[test]
    fn upsert_cloud_account_updates_existing_row_for_same_provider() {
        let conn = connect_in_memory().expect("connect");
        upsert_cloud_account(&conn, "gdrive", "first@example.com", "2026-09-09T12:00:00Z")
            .expect("first upsert");
        upsert_cloud_account(&conn, "gdrive", "second@example.com", "2026-09-09T13:00:00Z")
            .expect("second upsert");

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account_label, "second@example.com");
    }

    #[test]
    fn set_file_cloud_link_updates_origin_cloud_id_sync_status_and_modified_at() {
        let mut conn = connect_in_memory().expect("connect");
        let id = insert_file(&mut conn, &sample_file()).expect("insert");

        set_file_cloud_link(&conn, id, "gdrive", "drive-file-1", "synced", "2026-09-10T08:00:00Z")
            .expect("set cloud link");

        let file = get_file(&conn, id).expect("query").expect("present");
        assert_eq!(file.origin, "gdrive");
        assert_eq!(file.sync_status, "synced");
        assert_eq!(file.cloud_id, Some("drive-file-1".to_string()));
        assert_eq!(file.file_modified_at, Some("2026-09-10T08:00:00Z".to_string()));
    }

    #[test]
    fn sets_cloud_account_status() {
        let conn = connect_in_memory().expect("connect");
        upsert_cloud_account(&conn, "gdrive", "user@example.com", "2026-09-09T12:00:00Z")
            .expect("upsert");
        set_cloud_account_status(&conn, "gdrive", "disconnected").expect("set status");

        let accounts = list_cloud_accounts(&conn).expect("list");
        assert_eq!(accounts[0].status, "disconnected");
    }

    fn sample_filament_spool() -> NewFilamentSpool {
        NewFilamentSpool {
            material: "PLA".to_string(),
            manufacturer: Some("Bambu Lab".to_string()),
            color: Some("Schwarz".to_string()),
            diameter_mm: 1.75,
            original_weight_g: 1000,
            remaining_weight_g: 620,
            price: Some(19.99),
            image_png: None,
        }
    }

    #[test]
    fn inserts_and_lists_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].material, "PLA");
        assert_eq!(spools[0].manufacturer, Some("Bambu Lab".to_string()));
        assert_eq!(spools[0].color, Some("Schwarz".to_string()));
        assert_eq!(spools[0].diameter_mm, 1.75);
        assert_eq!(spools[0].original_weight_g, 1000);
        assert_eq!(spools[0].remaining_weight_g, 620);
        assert_eq!(spools[0].price, Some(19.99));
    }

    #[test]
    fn updates_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        let mut updated = sample_filament_spool();
        updated.remaining_weight_g = 450;
        updated.color = None;
        update_filament_spool(&conn, id, &updated).expect("update");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].remaining_weight_g, 450);
        assert_eq!(spools[0].color, None);
    }

    #[test]
    fn deletes_a_filament_spool() {
        let conn = connect_in_memory().expect("connect");
        let id = insert_filament_spool(&conn, &sample_filament_spool()).expect("insert");

        delete_filament_spool(&conn, id).expect("delete");

        let spools = list_filament_spools(&conn).expect("list");
        assert!(spools.is_empty());
    }

    #[test]
    fn stores_and_lists_a_filament_spool_image() {
        let conn = connect_in_memory().expect("connect");
        let mut with_image = sample_filament_spool();
        with_image.image_png = Some(vec![137, 80, 78, 71]); // PNG-Magic-Bytes als Platzhalter-Daten
        insert_filament_spool(&conn, &with_image).expect("insert");

        let spools = list_filament_spools(&conn).expect("list");
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].image_png, Some(vec![137, 80, 78, 71]));
    }
}
