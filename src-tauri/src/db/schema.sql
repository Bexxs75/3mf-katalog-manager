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
    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl', 'stp', 'obj')),
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
    created_at TEXT NOT NULL,
    unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL,
    slot_index INTEGER,
    home_location TEXT,
    color_hex TEXT,
    kind TEXT NOT NULL DEFAULT 'filament' CHECK (kind IN ('filament', 'resin'))
);

CREATE TABLE IF NOT EXISTS printers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL DEFAULT 'filament' CHECK (kind IN ('filament', 'resin'))
);

CREATE TABLE IF NOT EXISTS material_units (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('bambu_ams', 'bambu_ams_lite', 'bambu_ams_ht', 'creality_cfs',
                                       'prusa_mmu3', 'anycubic_ace', 'external', 'custom', 'resin_vat')),
    slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 16),
    bambu_ams_index INTEGER CHECK (bambu_ams_index BETWEEN 0 AND 3),
    position INTEGER NOT NULL DEFAULT 0
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

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS printer_connections (
    printer_id INTEGER PRIMARY KEY REFERENCES printers(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('moonraker')),
    address TEXT NOT NULL,
    base_url TEXT,
    remote_version TEXT,
    connected_since REAL NOT NULL,
    last_synced_at REAL,
    last_error TEXT,
    error_since REAL,
    paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1))
);

CREATE TABLE IF NOT EXISTS printer_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    printer_id INTEGER NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    remote_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('completed', 'partial')),
    raw_status TEXT NOT NULL,
    ended_at REAL NOT NULL,
    print_duration_s REAL NOT NULL,
    used_mm REAL NOT NULL,
    slicer_total_mm REAL,
    slicer_weight_g REAL,
    material TEXT,
    thumbnail_path TEXT,
    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'confirmed', 'ignored')),
    booked_spool_id INTEGER REFERENCES filament_spools(id) ON DELETE SET NULL,
    booked_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
    booked_g REAL,
    decided_at TEXT,
    UNIQUE (printer_id, remote_id)
);
