-- parent_id/path wurden nachtraeglich zur bereits bestehenden Tabelle
-- hinzugefuegt - CREATE TABLE IF NOT EXISTS aendert eine schon vorhandene
-- Tabelle nicht. Die beiden Spalten kommen additiv ueber die versionierten
-- Migrationen in db/migrations.rs (siehe MIGRATIONS/CURRENT_SCHEMA_VERSION
-- dort), gleiches Muster wie bei filament_spools/files. Frueher liefen diese
-- ALTER-TABLE-Schritte unversioniert in repository.rs::init() - Task 2 hat
-- sie in ein richtiges Migrations-Framework ueberfuehrt.
CREATE TABLE IF NOT EXISTS folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    color_hue INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl', 'stp')),
    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'local'
        CHECK (origin IN ('local')),
    -- sync_status/cloud_id sind Relikte der entfernten Cloud-Anbindung
    -- (Google Drive u.a. - zu instabil, siehe CHANGELOG). Absichtlich nicht
    -- per Migration entfernt (kein DROP-COLUMN-Muster in diesem Projekt,
    -- Risiko fuer Bestands-DBs), bleiben bis zu einer sauberen Neukonzeption
    -- inert (immer 'local-only'/NULL, kein Code liest/schreibt sie mehr).
    sync_status TEXT NOT NULL DEFAULT 'local-only'
        CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
    cloud_id TEXT,
    file_size_bytes INTEGER NOT NULL,
    dimension_x_mm REAL,
    dimension_y_mm REAL,
    dimension_z_mm REAL,
    volume_cm3 REAL,
    object_count INTEGER,
    thumbnail_png BLOB,
    imported_at TEXT NOT NULL,
    file_modified_at TEXT,
    print_status TEXT NOT NULL DEFAULT 'not_printed'
        CHECK (print_status IN ('not_printed', 'printed')),
    last_viewed_at TEXT,
    creator TEXT,
    content_hash TEXT,
    render_snapshot_png BLOB,
    custom_image_png BLOB,
    source_url TEXT,
    queue_position INTEGER,
    favorite INTEGER NOT NULL DEFAULT 0,
    plate_count INTEGER,
    slice_info_json TEXT,
    deleted_at TEXT,
    trash_path TEXT
);

CREATE INDEX IF NOT EXISTS idx_files_folder_id ON files (folder_id);
CREATE INDEX IF NOT EXISTS idx_files_file_type ON files (file_type);

CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (file_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_file_tags_tag_id ON file_tags (tag_id);

CREATE TABLE IF NOT EXISTS file_metadata (
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (file_id, label)
);

CREATE TABLE IF NOT EXISTS file_materials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    display_color TEXT
);

CREATE INDEX IF NOT EXISTS idx_file_materials_file_id ON file_materials (file_id);

CREATE TABLE IF NOT EXISTS filament_spools (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    material TEXT NOT NULL,
    manufacturer TEXT,
    color TEXT,
    location TEXT,
    diameter_mm REAL NOT NULL,
    original_weight_g INTEGER NOT NULL,
    remaining_weight_g INTEGER NOT NULL,
    price REAL,
    image_png BLOB,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS saved_filters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    folder_id INTEGER,
    tag TEXT,
    creator TEXT,
    query TEXT,
    sort TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collection_files (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    UNIQUE (collection_id, file_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_files_collection_id
    ON collection_files (collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_files_file_id
    ON collection_files (file_id);

CREATE TABLE IF NOT EXISTS print_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    printed_at TEXT NOT NULL,
    note TEXT,
    photo_png BLOB,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_print_log_file_id ON print_log (file_id);
