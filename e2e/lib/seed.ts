// Builds the starting state of every test file: a catalog database plus real
// model files in a throw-away home. The database comes from the app itself
// (a template created by launching it once), so the schema is never
// duplicated here and cannot drift from the app's migrations.
import { spawn, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { basename, dirname, join } from 'node:path';
import { buildFixtures, CATALOG, stripedPng } from '../fixtures/make-fixtures.mjs';
import { appStateDirs, appUnderTest, assertDisposableWindowsProfile, dataRoot, findAppDataDir, isolatedEnv, makeScratchDir, OUT_DIR } from './run-env.js';

export interface Scratch {
  home: string;
  catalogDir: string;
  extrasDir: string;
  appDataDir: string;
}

const TEMPLATE = join(OUT_DIR, 'template', 'catalog.db');
// Folder name the app uses below the data root (its bundle identifier); differs between preview and debug builds.
const TEMPLATE_ID = join(OUT_DIR, 'template', 'identifier.txt');

async function sleep(ms: number) { await new Promise((r) => setTimeout(r, ms)); }

function schemaReady(dbPath: string): boolean {
  try {
    const db = new DatabaseSync(dbPath, { readOnly: true });
    try {
      const v = (db.prepare('PRAGMA user_version').get() as { user_version: number }).user_version;
      const has = db.prepare("SELECT 1 AS x FROM sqlite_master WHERE name = 'printer_jobs'").get();
      return v > 0 && !!has;
    } finally { db.close(); }
  } catch { return false; }
}

/** Waits until a launched app process has exited (single-instance lock released). */
export async function waitForExit(child: ChildProcess, ms = 15000) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await Promise.race([new Promise((r) => child.once('exit', r)), sleep(ms)]);
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
}

/**
 * Launches the app once in an empty home so it creates and migrates its own
 * database, then stores that file as the template for all later seeds.
 */
export async function ensureTemplate(): Promise<void> {
  assertDisposableWindowsProfile();
  if (existsSync(TEMPLATE)) rmSync(TEMPLATE);
  const home = makeScratchDir('template');
  const child = spawn(appUnderTest(), [], { env: { ...process.env, ...isolatedEnv(home) }, stdio: 'ignore' });
  try {
    const deadline = Date.now() + 60000;
    let dir: string | null = null;
    while (Date.now() < deadline) {
      dir = findAppDataDir(home);
      if (dir && schemaReady(join(dir, 'catalog.db'))) break;
      await sleep(500);
    }
    if (!dir || !schemaReady(join(dir, 'catalog.db'))) throw new Error('the app did not create its catalog database within 60 s');
    // Let the startup tasks (hash backfill etc.) finish before the copy.
    await sleep(1500);
  } finally {
    child.kill('SIGTERM');
    await waitForExit(child);
  }
  const dir = findAppDataDir(home)!;
  const db = new DatabaseSync(join(dir, 'catalog.db'));
  db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  db.close();
  mkdirSync(dirname(TEMPLATE), { recursive: true });
  copyFileSync(join(dir, 'catalog.db'), TEMPLATE);
  writeFileSync(TEMPLATE_ID, basename(dir));
  // Windows shares the real profile, so leave nothing of the template run behind.
  for (const state of appStateDirs(home, basename(dir))) rmSync(state, { recursive: true, force: true });
  rmSync(home, { recursive: true, force: true });
}

/** Creates a fresh home with fixture files and a seeded database. */
export function seedScratch(label: string): Scratch {
  if (!existsSync(TEMPLATE)) throw new Error('template database missing: ensureTemplate() must run first');
  const home = makeScratchDir(label);
  const catalogDir = join(home, 'Katalog');
  const extrasDir = join(home, 'Extras');
  buildFixtures(catalogDir, extrasDir);

  const identifier = readFileSync(TEMPLATE_ID, 'utf8').trim();
  // On Windows the state lives in the real profile: start every file from nothing.
  for (const state of appStateDirs(home, identifier)) rmSync(state, { recursive: true, force: true });
  const appDataDir = join(dataRoot(home), identifier);
  mkdirSync(appDataDir, { recursive: true });
  copyFileSync(TEMPLATE, join(appDataDir, 'catalog.db'));

  const db = new DatabaseSync(join(appDataDir, 'catalog.db'));
  try {
    db.exec('PRAGMA foreign_keys = ON');
    const insertFolder = db.prepare('INSERT INTO folders (name, path, parent_id) VALUES (?, ?, ?)');
    const root = Number(insertFolder.run(basename(catalogDir), catalogDir, null).lastInsertRowid);
    const folderIds = new Map<string, number>();
    for (const name of CATALOG.folders) folderIds.set(name, Number(insertFolder.run(name, join(catalogDir, name), root).lastInsertRowid));

    const insertFile = db.prepare(`INSERT INTO files
      (name, path, file_type, folder_id, file_size_bytes, dimension_x_mm, dimension_y_mm, dimension_z_mm,
       object_count, imported_at, file_modified_at, plate_count)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z', ?)`);
    for (const f of CATALOG.files) {
      const [folder, name] = f.path.split('/');
      const full = join(catalogDir, folder, name);
      const d = (f as { dims?: number[] }).dims ?? [null, null, null];
      insertFile.run(name, full, f.type, folderIds.get(folder)!, statSync(full).size, d[0], d[1], d[2], (f as { plates?: number }).plates ?? null);
    }

    // An embedded thumbnail makes the detail page offer the "Bild"/"3D-Ansicht" toggle for this model.
    db.prepare("UPDATE files SET thumbnail_png = ? WHERE name = 'Dreiplatten.3mf'").run(stripedPng());
    db.prepare("INSERT INTO app_settings (key, value) VALUES ('catalog_base_dir', ?)").run(catalogDir);
    db.prepare("INSERT INTO collections (name, created_at) VALUES ('Testsammlung', '2026-09-01T10:00:00Z')").run();
    db.prepare("INSERT INTO printers (name, kind, position) VALUES ('Testdrucker Alpha', 'filament', 0)").run();
    const spool = db.prepare(`INSERT INTO filament_spools
      (material, manufacturer, color, diameter_mm, original_weight_g, remaining_weight_g, created_at, color_hex, kind)
      VALUES (?, ?, ?, 1.75, 1000, ?, '2026-09-01T10:00:00Z', ?, 'filament')`);
    spool.run('PLA', 'Testhersteller', 'Orange', 800, '#FF8000');
    spool.run('PETG', 'Testhersteller', 'Blau', 450, '#2244CC');
  } finally { db.close(); }
  return { home, catalogDir, extrasDir, appDataDir };
}
