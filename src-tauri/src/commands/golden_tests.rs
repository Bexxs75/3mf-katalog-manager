//! Golden scenarios of today's desktop behaviour (P0). Later packages add their own here.
use super::golden_harness::run_golden;
use super::*;

fn add_file(w: &World, rel: &str, folder: i64) -> i64 {
    let p = w.file(rel, &format!("content of {rel}"));
    db::test_insert_minimal_file(&w.conn, &p.to_string_lossy(), Some(folder)).unwrap()
}
use super::golden_harness::World;

#[test]
fn golden_rename_file() {
    run_golden("rename_file", |w| {
        let dir = w.dir("cat");
        let f = db::insert_folder_with_parent(&w.conn, "cat", None, &dir.to_string_lossy()).unwrap();
        let a = add_file(w, "cat/a.3mf", f);
        add_file(w, "cat/taken.3mf", f);
        let r = rename_file_with_conn(&w.conn, a, "b.3mf".into(), &[]);
        w.step("rename a -> b", r);
        let r = rename_file_with_conn(&w.conn, a, "taken.3mf".into(), &[]);
        w.step("rename b -> taken (collision)", r);
        let r = rename_file_with_conn(&w.conn, a, "B.3mf".into(), &[]);
        w.step("rename b -> B (case only)", r);
        let r = rename_file_with_conn(&w.conn, a, "bad/name.3mf".into(), &[]);
        w.step("rename with slash", r);
        let r = rename_file_with_conn(&w.conn, 999, "x.3mf".into(), &[]);
        w.step("rename unknown id", r);
    });
}

#[test]
fn golden_move_file() {
    run_golden("move_file", |w| {
        let src = db::insert_folder_with_parent(&w.conn, "src", None, &w.dir("src").to_string_lossy()).unwrap();
        let dst = db::insert_folder_with_parent(&w.conn, "dst", None, &w.dir("dst").to_string_lossy()).unwrap();
        let a = add_file(w, "src/a.3mf", src);
        let b = add_file(w, "src/b.3mf", src);
        add_file(w, "dst/b.3mf", dst);
        let r = move_file_to_folder_with_conn(&w.conn, a, Some(dst), &[]);
        w.step("move a to dst", r);
        let r = move_file_to_folder_with_conn(&w.conn, b, Some(dst), &[]);
        w.step("move b to dst (name taken)", r);
        let r = move_file_to_folder_with_conn(&w.conn, a, Some(12345), &[]);
        w.step("move to unknown folder", r);
    });
}

#[test]
fn golden_create_folder() {
    run_golden("create_folder", |w| {
        let base = w.dir("base");
        let root = db::insert_folder_with_parent(&w.conn, "base", None, &base.to_string_lossy()).unwrap();
        let r = create_folder_with_conn(&w.conn, Some(root), "child".into(), &base.join("child"), &[]);
        w.step("create child", r);
        let r = create_folder_with_conn(&w.conn, Some(root), "child".into(), &base.join("child"), &[]);
        w.step("create child again (exists)", r);
        let r = create_folder_with_conn(&w.conn, None, "viaparent".into(), &base.join("viaparent"), &[]);
        w.step("create without parent id (parent derived)", r);
    });
}

#[test]
fn golden_create_folder_rules() {
    run_golden("create_folder_rules", |w| {
        let base = w.dir("base");
        let root = db::insert_folder_with_parent(&w.conn, "base", None, &base.to_string_lossy()).unwrap();
        for (label, name) in [
            ("empty", ""),
            ("blank", "   "),
            ("slash", "a/b"),
            ("colon", "D:Neu"),
            ("dot", "."),
            ("dotdot", ".."),
            ("trailing dot", "a."),
            ("trailing space", "a "),
            ("windows reserved CON", "CON"),
            ("windows reserved with extension", "lpt9.log"),
            ("control char", "a\u{7}b"),
        ] {
            let r = validate_folder_name(name);
            w.step(&format!("name rule: {label}"), r);
        }
        let long = "a".repeat(256);
        let r = validate_folder_name(&long);
        w.step("name rule: 256 bytes", r);
        let r = new_dir_below_parent(&w.conn, root + 99, "n").map(|p| p.to_string_lossy().into_owned());
        w.step("unknown parent id", r);
        let r = new_dir_below_parent(&w.conn, root, "n").map(|p| p.to_string_lossy().into_owned());
        w.step("target below parent", r);
        w.file("base/taken", "x");
        let r = create_folder_with_conn(&w.conn, Some(root), "taken".into(), &base.join("taken"), &[]);
        w.step("target is an existing file", r);
        let r = create_folder_with_conn(&w.conn, Some(root), "Dup".into(), &base.join("Dup"), &[]);
        w.step("create Dup", r);
        let r = create_folder_with_conn(&w.conn, Some(root), "Dup".into(), &base.join("Dup"), &[]);
        w.step("create Dup again", r);
        let sub = base.join("Dup");
        let r = create_folder_with_conn(&w.conn, None, "deep".into(), &sub.join("deep"), &[]);
        w.step("nested, parent derived from the closest catalog folder", r);
    });
}

#[test]
fn golden_remove_folder_from_catalog() {
    run_golden("remove_folder_catalog", |w| {
        let top = db::insert_folder_with_parent(&w.conn, "top", None, &w.dir("top").to_string_lossy()).unwrap();
        let sub = db::insert_folder_with_parent(&w.conn, "sub", Some(top), &w.dir("top/sub").to_string_lossy()).unwrap();
        let other = db::insert_folder_with_parent(&w.conn, "other", None, &w.dir("other").to_string_lossy()).unwrap();
        let a = add_file(w, "top/a.3mf", top);
        let b = add_file(w, "top/sub/b.3mf", sub);
        // Control row in an unrelated folder: must survive the removal untouched.
        let c = add_file(w, "other/c.3mf", other);
        w.conn.execute_batch("INSERT INTO tags (id, name, color_hue) VALUES (1, 'red', 10); INSERT INTO collections (id, name, created_at) VALUES (1, 'set', '2026-01-01T00:00:00Z'); INSERT INTO printers (id, name) VALUES (1, 'P1');").unwrap();
        for (n, id) in [(1, a), (2, b), (3, c)] {
            w.conn
                .execute_batch(&format!(
                    "INSERT INTO file_tags (file_id, tag_id) VALUES ({id}, 1);
                     INSERT INTO file_metadata (file_id, label, value) VALUES ({id}, 'k', 'v{n}');
                     INSERT INTO file_materials (file_id, name, display_color) VALUES ({id}, 'PLA{n}', '#ff0000');
                     INSERT INTO collection_files (collection_id, file_id, position) VALUES (1, {id}, {n});
                     INSERT INTO print_log (file_id, printed_at, note, created_at) VALUES ({id}, '2026-01-01', 'n{n}', '2026-01-01T00:00:00Z');
                     INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state) VALUES ({id}, 1, '2026-01-01T00:00:00Z', 'sliced', 'read');
                     INSERT INTO file_print_plates (file_id, plate_index, plate_name) VALUES ({id}, 1, 'plate{n}');
                     INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, booked_file_id) VALUES (1, 'job{n}', 'f{n}.3mf', 'completed', 'ok', 1.0, 1.0, 1.0, {id});
                     INSERT INTO slicer_reports (reported_at, reported_path, file_name, file_size_bytes, summary_json, suggested_file_id, model_file_id) VALUES ('2026-01-01T00:00:00Z', 'r{n}', 'f{n}.3mf', 1, '{{}}', {id}, {id});"
                ))
                .unwrap();
        }
        let r = remove_folder_with_conn(&w.conn, "abc").map(|c| serde_json::to_value(&c).unwrap());
        w.step("invalid id", r);
        let r = remove_folder_with_conn(&w.conn, &top.to_string()).map(|c| serde_json::to_value(&c).unwrap());
        w.step("remove top (files stay on disk)", r);
    });
}

#[test]
fn golden_trash_file_with_companion() {
    run_golden("trash_file_companion", |w| {
        let f = db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap();
        let id = add_file(w, "cat/m.3mf", f);
        let companion = {
            let model = w.root.join(crate::commands::golden_harness::nat("cat/m.3mf"));
            let extras = katalog_core::companions::extras_dir_for(&model).unwrap();
            let rel = extras.strip_prefix(&w.root).unwrap().join("photo.png");
            w.file(&rel.to_string_lossy(), "png")
        };
        db::companions::insert_companion(&w.conn, id, "photo.png", &companion.to_string_lossy(), "image", Some(3), "2026-01-01T00:00:00Z").unwrap();
        let trash = w.dir("trash");
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete model with companion", r);
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete again (already trashed)", r);
    });
}

/// Documents the refusal: a companion that is not in an `_extras` folder is not taken along.
#[test]
fn golden_trash_file_invalid_companion() {
    run_golden("trash_file_invalid_companion", |w| {
        let f = db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap();
        let id = add_file(w, "cat/m.3mf", f);
        let companion = w.file("cat/m/photo.png", "png");
        db::companions::insert_companion(&w.conn, id, "photo.png", &companion.to_string_lossy(), "image", Some(3), "2026-01-01T00:00:00Z").unwrap();
        let trash = w.dir("trash");
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete model with companion outside _extras (rejected)", r);
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete again (already trashed)", r);
    });
}

/// Related rows for every file id: they must follow the model (CASCADE / SET NULL) in removals.
fn seed_related(w: &World, ids: &[i64]) {
    w.conn.execute_batch("INSERT INTO tags (id, name, color_hue) VALUES (1, 'red', 10); INSERT INTO collections (id, name, created_at) VALUES (1, 'set', '2026-01-01T00:00:00Z'); INSERT INTO printers (id, name) VALUES (1, 'P1');").unwrap();
    for (n, id) in ids.iter().enumerate() {
        let n = n + 1;
        w.conn
            .execute_batch(&format!(
                "INSERT INTO file_tags (file_id, tag_id) VALUES ({id}, 1);
                 INSERT INTO file_metadata (file_id, label, value) VALUES ({id}, 'k', 'v{n}');
                 INSERT INTO file_materials (file_id, name, display_color) VALUES ({id}, 'PLA{n}', '#ff0000');
                 INSERT INTO collection_files (collection_id, file_id, position) VALUES (1, {id}, {n});
                 INSERT INTO print_log (file_id, printed_at, note, created_at) VALUES ({id}, '2026-01-01', 'n{n}', '2026-01-01T00:00:00Z');
                 INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state) VALUES ({id}, 1, '2026-01-01T00:00:00Z', 'sliced', 'read');
                 INSERT INTO file_print_plates (file_id, plate_index, plate_name) VALUES ({id}, 1, 'plate{n}');
                 INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, booked_file_id) VALUES (1, 'job{n}', 'f{n}.3mf', 'completed', 'ok', 1.0, 1.0, 1.0, {id});"
            ))
            .unwrap();
    }
}

fn removal_world(w: &mut World) -> (i64, i64, [i64; 5]) {
    let top = db::insert_folder_with_parent(&w.conn, "top", None, &w.dir("top").to_string_lossy()).unwrap();
    let sub = db::insert_folder_with_parent(&w.conn, "sub", Some(top), &w.dir("top/sub").to_string_lossy()).unwrap();
    let other = db::insert_folder_with_parent(&w.conn, "other", None, &w.dir("other").to_string_lossy()).unwrap();
    let a = add_file(w, "top/a.3mf", top);
    let b = add_file(w, "top/sub/b.3mf", sub);
    let c = add_file(w, "other/c.3mf", other);
    let d = add_file(w, "other/d.3mf", other);
    let t = add_file(w, "top/sub/t.3mf", sub);
    db::soft_delete_file(&w.conn, t, Some(&w.root.join(crate::commands::golden_harness::nat("trash/t.3mf")).to_string_lossy()), "2026-01-01T00:00:00Z").unwrap();
    seed_related(w, &[a, b, c, d, t]);
    db::bundles::create_bundle(&w.conn, "Pair", db::bundles::BundleKind::Parts, &[a, c], "2026-01-01T00:00:00Z").unwrap();
    db::bundles::create_bundle(&w.conn, "Trio", db::bundles::BundleKind::Variants, &[b, c, d], "2026-01-01T00:00:00Z").unwrap();
    (top, sub, [a, b, c, d, t])
}

#[test]
fn golden_remove_files_from_catalog() {
    run_golden("remove_files_catalog", |w| {
        let (_top, _sub, [a, b, c, _d, t]) = removal_world(w);
        for (label, ids) in [
            ("zero id", vec!["0".to_string()]),
            ("not a number", vec!["abc".to_string()]),
            ("negative id", vec!["-3".to_string()]),
            ("unknown id", vec!["9999".to_string()]),
            ("trashed id", vec![t.to_string()]),
            ("valid and unknown (nothing removed)", vec![a.to_string(), "9999".to_string()]),
            ("empty list", vec![]),
        ] {
            let r = remove_files_with_conn(&w.conn, ids);
            w.step(label, r);
        }
        let r = remove_files_with_conn(&w.conn, vec![a.to_string(), a.to_string(), c.to_string()]);
        w.step("batch with duplicate (a, a, c)", r);
        let r = remove_files_with_conn(&w.conn, vec![b.to_string()]);
        w.step("single (b), bundle Trio keeps two", r);
    });
}

#[test]
fn golden_remove_folder_refusals_and_summary() {
    run_golden("remove_folder_refusals", |w| {
        let (top, sub, _ids) = removal_world(w);
        let step_count = |w: &mut World, label: &str, id: &str| {
            let r = remove_folder_with_conn(&w.conn, id).map(|c| serde_json::to_value(&c).unwrap());
            w.step(label, r);
        };
        for (label, id) in [("summary top", top.to_string()), ("summary sub", sub.to_string()), ("summary unknown", "9999".to_string()), ("summary bad id", "x".to_string())] {
            let r = folder_summary_with_conn(&w.conn, &id);
            w.step(label, r);
        }
        step_count(w, "remove unknown folder", "9999");
        step_count(w, "remove zero id", "0");
        let sub_path = w.root.join(crate::commands::golden_harness::nat("top/sub")).to_string_lossy().to_string();
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &sub_path).unwrap();
        step_count(w, "remove top (subtree holds the base dir)", &top.to_string());
        step_count(w, "remove sub (is the base dir)", &sub.to_string());
        let r = db::printer_link::get_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR).map_err(CmdError::from);
        w.step("base dir setting after refusals", r.map(|v| v.map(|s| s.replace(&w.root.to_string_lossy().to_string(), "<ROOT>"))));
        let other = w.root.join("other").to_string_lossy().to_string();
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &other).unwrap();
        step_count(w, "remove sub (base dir elsewhere; trashed file stays)", &sub.to_string());
    });
}

#[test]
fn golden_reset_catalog() {
    run_golden("reset_catalog", |w| {
        let (_top, _sub, _ids) = removal_world(w);
        db::printer_link::set_setting(&w.conn, folders::SETTING_CATALOG_BASE_DIR, &w.root.join("top").to_string_lossy()).unwrap();
        db::printer_link::set_setting(&w.conn, "keep_me", "yes").unwrap();
        let r = reset_with_conn(&w.conn).map(|c| serde_json::to_value(&c).unwrap());
        w.step("reset", r);
        for key in [folders::SETTING_CATALOG_BASE_DIR, "keep_me"] {
            let r = db::printer_link::get_setting(&w.conn, key).map_err(CmdError::from);
            w.step(&format!("setting {key}"), r);
        }
        let r = reset_with_conn(&w.conn).map(|c| serde_json::to_value(&c).unwrap());
        w.step("reset again (empty)", r);
    });
}
