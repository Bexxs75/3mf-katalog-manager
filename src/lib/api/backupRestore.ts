import type { BackupPreview, BackupRestoreReport } from '../../types';
import { authenticationFailed, canEditMetadata, getSnapshot } from '../webSession';

/** The server's limit for the whole ZIP body; checked here first so nothing is sent that it would turn away. */
export const BACKUP_MAX_BYTES = 258 * 1024 * 1024;

/** An error in the shape the interface already turns into text (`ErrorText`). `status` is 0 when no answer arrived. */
export interface BackupFailure { message: string; expected: true; params?: string[]; status: number }

/** What is wrong with the chosen file before a single byte is sent; null when it can go up. */
export function backupFileProblem(file: File): 'not_zip' | 'empty' | 'too_large' | null {
  // The server looks into the archive anyway; the extension only keeps an obviously wrong file from being sent.
  if (!/\.zip$/i.test(file.name)) return 'not_zip';
  if (file.size === 0) return 'empty';
  return file.size > BACKUP_MAX_BYTES ? 'too_large' : null;
}

// What the server's reverse proxies and its own limits answer without a JSON body.
const MESSAGE_BY_STATUS: Record<number, string> = { 401: 'session_required', 408: 'webRequestTimeout', 413: 'backupUploadTooLarge', 429: 'webTooManyRequests', 503: 'maintenance' };

/** Turns a failed answer into a failure. A body that carries a message (a registered code or a bare word) is passed on as the server sent it. */
function failure(status: number, text: string): BackupFailure {
  let body: { message?: unknown; params?: unknown } = {};
  try { body = JSON.parse(text); } catch { /* A proxy may answer with HTML. */ }
  const message = typeof body.message === 'string' ? body.message : MESSAGE_BY_STATUS[status] ?? 'webServerInternal';
  const params = Array.isArray(body.params) && body.params.every((p) => typeof p === 'string') && body.params.length > 0 ? body.params as string[] : undefined;
  // The server's own text for a too-large body talks about JSON; here the body is a ZIP.
  const fixed = status === 413 ? 'backupUploadTooLarge' : message;
  authenticationFailed(status, undefined);
  return { message: fixed, expected: true, status, ...(params ? { params } : {}) };
}
const connectionLost = (): BackupFailure => ({ message: 'backupConnectionLost', expected: true, status: 0 });
const notAllowed = (): BackupFailure => ({ message: 'session_required', expected: true, status: 401 });

/** Standard base64 of the UTF-8 password, as the upload header expects. It is encoded per call and never kept. */
function encodePassword(password: string): string {
  let binary = '';
  for (const byte of new TextEncoder().encode(password)) binary += String.fromCharCode(byte);
  return btoa(binary);
}

interface UploadAnswer { token?: unknown; report?: unknown; settings_ignored?: unknown }

/**
 * Sends the ZIP as the raw body of `POST /api/backup/upload` with progress. The password travels in a header
 * (never the URL) and is checked by the server in this same request. A retry makes a separate preview.
 */
export function uploadBackup(file: File, password: string, onProgress: (percent: number) => void, signal?: AbortSignal): Promise<BackupPreview> {
  return new Promise((resolve, reject) => {
    if (!canEditMetadata()) { reject(notAllowed()); return; }
    if (signal?.aborted) { reject(connectionLost()); return; }
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    const done = (settle: () => void) => { signal?.removeEventListener('abort', abort); settle(); };
    xhr.open('POST', '/api/backup/upload');
    xhr.setRequestHeader('Content-Type', 'application/zip');
    xhr.setRequestHeader('X-CSRF-Token', getSnapshot().csrfToken ?? '');
    xhr.setRequestHeader('X-Restore-Password', encodePassword(password));
    xhr.upload.onprogress = (event) => { if (event.lengthComputable && event.total > 0) onProgress(Math.round(event.loaded / event.total * 100)); };
    // No answer arrived: the connection broke, or the server turned the file away early.
    xhr.onerror = () => done(() => reject(connectionLost()));
    xhr.onabort = () => done(() => reject(connectionLost()));
    xhr.onload = () => {
      let body: UploadAnswer = {};
      try { body = JSON.parse(xhr.responseText); } catch { /* handled below */ }
      if (xhr.status >= 200 && xhr.status < 300 && typeof body.token === 'string' && body.report && typeof body.report === 'object') {
        const preview: BackupPreview = { token: body.token, report: body.report as BackupRestoreReport, settingsIgnored: body.settings_ignored !== false };
        done(() => resolve(preview));
        return;
      }
      done(() => reject(failure(xhr.status, xhr.responseText)));
    };
    signal?.addEventListener('abort', abort, { once: true });
    xhr.send(file);
  });
}

/** Removes a checked backup from the server. Best effort: an expired or foreign token needs no further action. */
export async function discardBackup(token: string): Promise<void> {
  if (!canEditMetadata()) return;
  try {
    await fetch('/api/backup/staging/' + encodeURIComponent(token), {
      method: 'DELETE', credentials: 'same-origin', headers: { 'X-CSRF-Token': getSnapshot().csrfToken ?? '' },
    });
  } catch { /* The token expires on its own after a while. */ }
}

/**
 * Replaces the catalog with a checked backup. The password is confirmed again in this request. On success the
 * server ends every session and restarts, so the answer is the last one this page gets before the sign-in.
 */
export async function restoreBackup(token: string, password: string): Promise<{ restored: boolean; restart: boolean }> {
  if (!canEditMetadata()) throw notAllowed();
  let response: Response;
  try {
    response = await fetch('/api/backup/restore', {
      method: 'POST', credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': getSnapshot().csrfToken ?? '' },
      body: JSON.stringify({ token, password }),
    });
  } catch { throw connectionLost(); }
  const text = await response.text().catch(() => '');
  if (!response.ok) throw failure(response.status, text);
  let body: { restored?: unknown; restart?: unknown } = {};
  try { body = JSON.parse(text); } catch { /* handled below */ }
  // An answer that is not the expected one cannot be told apart from a lost one.
  if (body.restored !== true) throw connectionLost();
  return { restored: true, restart: body.restart !== false };
}

/**
 * Polls the health route until the server answers, for the wait after a restore. The first look comes only
 * after a pause, because the old process may still answer for a moment before it stops.
 */
export async function waitForServer(options: { signal?: AbortSignal; initialDelayMs?: number; intervalMs?: number; timeoutMs?: number } = {}): Promise<boolean> {
  const { signal, initialDelayMs = 3000, intervalMs = 2000, timeoutMs = 90_000 } = options;
  const sleep = (ms: number) => new Promise<void>((resolve) => { const timer = setTimeout(resolve, ms); signal?.addEventListener('abort', () => { clearTimeout(timer); resolve(); }, { once: true }); });
  const deadline = Date.now() + timeoutMs;
  await sleep(initialDelayMs);
  while (!signal?.aborted && Date.now() < deadline) {
    try {
      const response = await fetch('/api/health', { cache: 'no-store', credentials: 'same-origin' });
      if (response.ok) return true;
    } catch { /* not up yet */ }
    await sleep(intervalMs);
  }
  return false;
}
