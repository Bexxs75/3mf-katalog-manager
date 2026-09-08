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
    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'local'
        CHECK (origin IN ('local', 'gdrive', 'onedrive', 'dropbox', 'proton')),
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
    file_modified_at TEXT
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
