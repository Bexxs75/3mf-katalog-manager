// Everything that differs between "this laptop in Docker", "GitHub Linux" and
// "GitHub Windows" is read here, from environment variables, so specs never
// look at the platform themselves.
import { existsSync, mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export const IS_WINDOWS = process.platform === 'win32';
export const E2E_ROOT = join(import.meta.dirname, '..');
export const OUT_DIR = join(E2E_ROOT, 'out');

export function appUnderTest(): string {
  const app = process.env.MFK_E2E_APP;
  if (!app) throw new Error('MFK_E2E_APP must point to the app executable (AppRun of an extracted AppImage, or the debug binary).');
  if (!existsSync(app)) throw new Error(`MFK_E2E_APP does not exist: ${app}`);
  return app;
}

/** Fresh throw-away directory for one test file. Never the real HOME. */
export function makeScratchDir(prefix: string): string {
  return mkdtempSync(join(process.env.MFK_E2E_WORK || tmpdir(), `mfk-e2e-${prefix}-`));
}

/**
 * Windows takes %APPDATA% from the registry (known-folder API), not from the
 * environment, so a throw-away HOME cannot isolate the app there. The suite
 * therefore only runs on Windows on a disposable CI machine.
 */
export function assertDisposableWindowsProfile() {
  if (IS_WINDOWS && process.env.CI !== 'true' && !process.env.MFK_E2E_ALLOW_REAL_PROFILE) {
    throw new Error('Refusing to run on Windows outside CI: the app data folder cannot be redirected there and the tests would overwrite the real catalog. Set MFK_E2E_ALLOW_REAL_PROFILE=1 only on a throw-away machine.');
  }
}

/** Environment that points every place the app writes to into `home` (Linux). On Windows only HOME-like variables, see above. */
export function isolatedEnv(home: string): Record<string, string> {
  if (IS_WINDOWS) return { USERPROFILE: home, HOME: home };
  return {
    HOME: home,
    XDG_DATA_HOME: join(home, 'data'),
    XDG_CONFIG_HOME: join(home, 'config'),
    XDG_CACHE_HOME: join(home, 'cache'),
    XDG_STATE_HOME: join(home, 'state'),
  };
}

/** Directory the app calls `app_data_dir()` for its identifier (the first subfolder with a catalog.db). */
export function dataRoot(home: string): string {
  return IS_WINDOWS ? process.env.APPDATA! : join(home, 'data');
}

/** Folders the app keeps its state in for `identifier` (Windows: roaming data plus the WebView2 profile with localStorage). */
export function appStateDirs(home: string, identifier: string): string[] {
  return IS_WINDOWS
    ? [join(process.env.APPDATA!, identifier), join(process.env.LOCALAPPDATA!, identifier)]
    : [join(dataRoot(home), identifier)];
}

export function findAppDataDir(home: string): string | null {
  const root = dataRoot(home);
  if (!existsSync(root)) return null;
  for (const entry of readdirSync(root)) {
    // On Windows the roaming folder is shared with other apps, so only look at ours.
    if (entry.startsWith('com.thebexxs.mfkatalogmanager') && existsSync(join(root, entry, 'catalog.db'))) return join(root, entry);
  }
  return null;
}
