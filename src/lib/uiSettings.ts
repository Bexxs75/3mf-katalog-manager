/** Browser preferences must remain device-local, including before initialization. */
export const isDesktopUiSettings = () => import.meta.env.VITE_TARGET !== 'web' && '__TAURI_INTERNALS__' in window;

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const transport = await import('./transport');
  return transport.invoke<T>(command, args);
}

interface Settings {
  schema_version: number;
  migrated_from_webview: boolean;
  values: Record<string, string>;
}

let values: Record<string, string> | null = null;
let initialization: Promise<void> | null = null;
let writes = Promise.resolve();
let ready = false;
let backendWritable = true;
const pending = new Map<string, string | null>();
type SettingsError = 'uiSettingsInvalid' | 'uiSettingsUnavailable';
let error: SettingsError | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
const queued = new Map<string, string | null>();
let detachClose: (() => void) | undefined;
let failureEpisode = false;
const bootKeys = new Set(['3mf-katalog-theme', '3mf-katalog-language', '3mf-katalog-density']);
function cache(key: string, value: string | null) {
  try { if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value); } catch { /* Memory remains usable when storage is blocked. */ }
}
function cacheBootValues(authoritative = false) {
  for (const key of bootKeys) {
    if (values?.[key] !== undefined) cache(key, values[key]);
    else if (authoritative) cache(key, null);
  }
}
function acceptSettings(settings: Settings, authoritative = settings.migrated_from_webview) {
  backendWritable = settings.schema_version === 1;
  if (!backendWritable) {
    values = legacyValues();
    failed({ message: 'uiSettingsInvalid' });
    return;
  }
  values = { ...settings.values };
  failureEpisode = false;
  cacheBootValues(authoritative);
}
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(listener => listener());
export const subscribeUiSettings = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export const getUiSettingsError = () => isDesktopUiSettings() ? error : null;
export function clearUiSettingsError() { error = null; notify(); }
function failed(cause?: unknown) {
  if (!failureEpisode) {
    failureEpisode = true;
    error = typeof cause === 'object' && cause !== null && 'message' in cause && cause.message === 'uiSettingsInvalid' ? 'uiSettingsInvalid' : 'uiSettingsUnavailable';
    notify();
  }
}
const RESTORE_ERROR_KEY = '3mf-ui-settings-restore-error';
export function reportUiSettingsRestoreError(cause?: unknown) {
  if (!isDesktopUiSettings()) return;
  failed(cause);
  // Catalog replacement requires a reload; carry its settings warning across that reload only.
  try { sessionStorage.setItem(RESTORE_ERROR_KEY, error ?? 'uiSettingsUnavailable'); } catch { /* The current session still shows the error. */ }
}

function legacyValues(): Record<string, string> {
  const result: Record<string, string> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith('3mf-katalog-') && !key.startsWith('3mf-katalog-preview-attempt:') && key !== '3mf-katalog-slicers') {
        const value = localStorage.getItem(key);
        if (value !== null) result[key] = value;
      }
    }
  } catch { /* A blocked WebView store must not prevent startup. */ }
  return result;
}

/** Called before mounting the catalog so hook initializers see the backend snapshot. */
export function initializeUiSettings(): Promise<void> {
  if (!isDesktopUiSettings()) return Promise.resolve();
  if (initialization) return initialization;
  try {
    const restoredError = sessionStorage.getItem(RESTORE_ERROR_KEY);
    if (restoredError) error = restoredError === 'uiSettingsInvalid' ? 'uiSettingsInvalid' : 'uiSettingsUnavailable';
    sessionStorage.removeItem(RESTORE_ERROR_KEY);
  } catch { /* A blocked session store must not prevent startup. */ }
  const fallback = legacyValues();
  initialization = (async () => {
    const startedAt = Date.now();
    try {
      let settings = await withDeadline(invoke<Settings>('get_ui_settings'), 3000);
      if (settings.schema_version === 1 && !settings.migrated_from_webview) settings = await withDeadline(invoke<Settings>('migrate_ui_settings', { values: migrationValues(fallback) }), Math.max(0, 3000 - (Date.now() - startedAt)));
      acceptSettings(settings);
    } catch (cause) {
      values = fallback;
      failed(cause);
    }
    values ??= fallback;
    cacheBootValues();
    ready = true;
    for (const [key, value] of pending) {
      if (value === null) delete values[key]; else values[key] = value;
      if (!backendWritable || bootKeys.has(key)) cache(key, value);
      if (backendWritable) queued.set(key, value);
    }
    schedule();
    pending.clear();
    attachLifecycle();
    notify();
  })();
  return initialization;
}

export const uiSettings = {
  getItem(key: string): string | null {
    if (isDesktopUiSettings() && values !== null) return values[key] ?? null;
    try { return localStorage.getItem(key); } catch { return null; }
  },
  setItem(key: string, value: string) { change(key, value); },
  removeItem(key: string) { change(key, null); },
};
function change(key: string, value: string | null) {
  if (!isDesktopUiSettings()) { cache(key, value); notify(); return; }
  values ??= legacyValues();
  if ((values[key] ?? null) === value) return;
  if (value === null) delete values[key]; else values[key] = value;
  if (!backendWritable || bootKeys.has(key)) cache(key, value);
  notify();
  if (!backendWritable) return;
  // Serial writes preserve rapid changes to the same key; a failed write never blocks later ones.
  if (!ready) { pending.set(key, value); return; }
  queued.set(key, value);
  schedule();
}
function schedule() {
  clearTimeout(timer);
  if (ready && queued.size) timer = setTimeout(() => { void flushUiSettings(); }, 300);
}
export async function flushUiSettings() {
  if (!isDesktopUiSettings() || !backendWritable) return;
  clearTimeout(timer);
  const drain = async () => {
    while (queued.size) {
      const [key, value] = queued.entries().next().value!;
      queued.delete(key);
      try {
        await invoke<void>('set_ui_setting', { key, value });
        failureEpisode = false;
      } catch (cause) { failed(cause); }
    }
  };
  writes = writes.then(drain);
  await writes;
}
async function withDeadline<T>(operation: Promise<T>, milliseconds = 500): Promise<T> {
  let deadline: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([operation, new Promise<never>((_, reject) => {
      deadline = setTimeout(() => reject({ message: 'uiSettingsUnavailable' }), milliseconds);
    })]);
  } finally { clearTimeout(deadline); }
}
/** Restart must not strand the debounce queue or wait indefinitely for stalled storage. */
export async function flushUiSettingsBeforeRestart() {
  try { await withDeadline(flushUiSettings(), 1500); }
  catch (cause) { failed(cause); }
}
function leaving() { void flushUiSettings(); }
function visibilityChanged() { if (document.visibilityState === 'hidden') leaving(); }
function attachLifecycle() {
  window.addEventListener('pagehide', leaving);
  document.addEventListener('visibilitychange', visibilityChanged);
  if (import.meta.env.VITE_TARGET !== 'web' && '__TAURI_INTERNALS__' in window) {
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const win = getCurrentWindow();
      detachClose = await win.onCloseRequested(async () => {
        // Best-effort persistence must not veto the native close decision.
        try { await withDeadline(flushUiSettings(), 1000); }
        catch (cause) { failed(cause); }
      });
    }).catch(failed);
  }
}
// Bound the one-shot IPC migration; the backend also accounts for existing values.
function migrationValues(legacy: Record<string, string>): Record<string, string> {
  const result: Record<string, string> = {};
  const bytes = (v: unknown) => new TextEncoder().encode(JSON.stringify(v)).length;
  for (const [key, value] of Object.entries(legacy).sort((a, b) => bytes(a) - bytes(b))) {
    if (new TextEncoder().encode(value).length > (['3mf-katalog-sidebar-expanded', '3mf-katalog-collapsed-folders'].includes(key) ? 262144 : 65536)) continue;
    result[key] = value;
    if (bytes({ values: result }) > 1024 * 1024) delete result[key];
  }
  return result;
}
export async function reloadUiSettings() {
  if (!isDesktopUiSettings()) { notify(); return; }
  const settings = await withDeadline(invoke<Settings>('get_ui_settings'), 3000);
  acceptSettings(settings, true);
  notify();
}

/** Test isolation also exercises the synchronous fallback used before backend startup. */
export function resetUiSettings() {
  values = null;
  initialization = null;
  writes = Promise.resolve();
  error = null;
  failureEpisode = false;
  ready = false;
  backendWritable = true;
  pending.clear();
  queued.clear();
  clearTimeout(timer);
  window.removeEventListener('pagehide', leaving);
  document.removeEventListener('visibilitychange', visibilityChanged);
  detachClose?.();
  detachClose = undefined;
}
