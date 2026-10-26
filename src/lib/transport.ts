import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './readOnly';
import { retryAfterSeconds } from './fileOpsErrors';
import { canEditMetadata, canManageFiles, canOpenInSlicerLink, getSnapshot, readResponse } from './webSession';

export const FILE_OPS_COMMANDS = ['rename_file', 'move_files_to_folder', 'delete_files', 'restore_file', 'delete_file_permanently', 'empty_trash_start', 'empty_trash_block', 'create_folder', 'rename_folder', 'move_folder', 'remove_folder_from_catalog'] as const;

const FILE_OPS_MAX_AUTO_RETRY_SECONDS = 5;

// Uploads and file operations track their own operation ids; only plain catalog writes are replay-protected by the server.
const OWN_OPERATION_COMMANDS: readonly string[] = [...FILE_OPS_COMMANDS, 'start_upload_import', 'discard_upload', 'cancel_import'];
// Waits before the automatic repeats after a lost or timed-out answer. The server may still be finishing the first attempt, which the shared request id makes safe.
// Mutable only so UI tests of failure paths do not have to wait through the backoff.
export const lostAnswerRetry = { delaysMs: [1000, 3000] };
// 408 is the server's own request timeout; 502/504/524 are what a reverse proxy or CDN answers when it gave up waiting, and 499 is nginx's code for a client it saw go away.
const LOST_ANSWER_STATUSES = [408, 499, 502, 504, 524];

// One key per invoke() call: two deliberate user actions (e.g. a double click) are two keys and both run, because the server cannot tell a repeat from a second intent.
// Plain-http home-network deployments are not a secure context, where crypto.randomUUID does not exist but getRandomValues does.
export function newRequestId(): string {
  const c = (globalThis as { crypto?: Crypto }).crypto;
  if (typeof c?.randomUUID === 'function') return c.randomUUID();
  const bytes = new Uint8Array(16);
  if (typeof c?.getRandomValues === 'function') c.getRandomValues(bytes);
  else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const fileOperation = (FILE_OPS_COMMANDS as readonly string[]).includes(command);
  if ((fileOperation && !canManageFiles()) || (!(command === 'slicer_link' && canOpenInSlicerLink()) && !(WEB_READ_COMMANDS as readonly string[]).includes(command) && !(canEditMetadata() && (WEB_WRITE_COMMANDS as readonly string[]).includes(command)))) {
    const language = typeof document === 'undefined' ? 'de' : document.documentElement.lang;
    const messages: Record<string, string> = { de: de.webUnavailable, en: en.webUnavailable, es: es.webUnavailable, fr: fr.webUnavailable };
    throw { message: messages[language] ?? de.webUnavailable, expected: true };
  }
  const initiatingToken = getSnapshot().csrfToken;
  const writing = (WEB_WRITE_COMMANDS as readonly string[]).includes(command);
  // One key per user action, reused by every automatic repeat of it.
  const requestId = writing && !OWN_OPERATION_COMMANDS.includes(command) ? newRequestId() : null;
  // Serialized once so every repeat carries byte-identical arguments even if the caller mutates the object meanwhile; a differing body would be rejected as a reused key.
  const payload = JSON.stringify(args ?? {});
  const send = () => fetch('/api/invoke/' + encodeURIComponent(command), {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': getSnapshot().csrfToken ?? '', ...(requestId ? { 'X-Request-Id': requestId } : {}) },
    body: payload,
  });
  // A lost or timed-out answer does not say whether the change was applied. Repeating with the same
  // key is safe because the server answers a finished request from its stored result.
  const sendRepeating = async (): Promise<Response> => {
    for (let attempt = 0; ; attempt++) {
      let outcome: { response: Response } | { error: unknown };
      try { outcome = { response: await send() }; } catch (error) { outcome = { error }; }
      if ('response' in outcome && !LOST_ANSWER_STATUSES.includes(outcome.response.status)) return outcome.response;
      if (attempt < lostAnswerRetry.delaysMs.length) {
        await new Promise(resolve => setTimeout(resolve, lostAnswerRetry.delaysMs[attempt]));
        if (getSnapshot().csrfToken === initiatingToken && canEditMetadata()) continue;
      }
      if ('error' in outcome) throw outcome.error;
      return outcome.response;
    }
  };
  let response = requestId === null ? await send() : await sendRepeating();
  // Only explicit rate/busy responses are safe to retry without a request id, never a lost response.
  const managerRead = ['list_filament_spools', 'get_filament_spool_image', 'list_printers', 'list_printer_history', 'get_last_printer_for_file', 'check_filament', 'companion_image_data', 'list_companions'].includes(command);
  if ((writing || managerRead || command === 'get_model_geometry') && [429, 503].includes(response.status)) {
    const body = fileOperation ? await response.clone().json().catch(() => ({})) : null;
    const safe = !fileOperation || body.kind === 'rate_limited' || (!body.kind && body.message === 'Schreibzugriff derzeit ausgelastet');
    const retry = safe ? response.headers.get('Retry-After') : null;
    if (retry !== null) {
      const waitSeconds = retryAfterSeconds(retry);
      // A confirmed destructive action must not fire minutes later unseen: long waits are surfaced to the user instead.
      if (waitSeconds !== null && (!fileOperation || waitSeconds <= FILE_OPS_MAX_AUTO_RETRY_SECONDS)) {
        await new Promise(resolve => setTimeout(resolve, waitSeconds * 1000));
        if (fileOperation ? canManageFiles() && getSnapshot().csrfToken === initiatingToken : !writing || canEditMetadata()) response = await send();
      }
    }
  }
  return readResponse<T>(response);
}
// Keep identity, argument count and synchronous dispatch for existing desktop mocks.
export const invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> =
  import.meta.env.VITE_TARGET === 'web' ? webInvoke : tauriInvoke;
