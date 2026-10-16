import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './readOnly';
import { canEditMetadata, canManageFiles, getSnapshot, readResponse } from './webSession';

export const FILE_OPS_COMMANDS = ['rename_file', 'move_files_to_folder', 'delete_files', 'restore_file', 'delete_file_permanently', 'empty_trash_start', 'empty_trash_block', 'create_folder', 'rename_folder', 'move_folder', 'remove_folder_from_catalog'] as const;

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const fileOperation = (FILE_OPS_COMMANDS as readonly string[]).includes(command);
  if ((fileOperation && !canManageFiles()) || (!(WEB_READ_COMMANDS as readonly string[]).includes(command) && !(canEditMetadata() && (WEB_WRITE_COMMANDS as readonly string[]).includes(command)))) {
    const language = typeof document === 'undefined' ? 'de' : document.documentElement.lang;
    const messages: Record<string, string> = { de: de.webUnavailable, en: en.webUnavailable, es: es.webUnavailable, fr: fr.webUnavailable };
    throw { message: messages[language] ?? de.webUnavailable, expected: true };
  }
  const initiatingToken = getSnapshot().csrfToken;
  const send = () => fetch('/api/invoke/' + encodeURIComponent(command), {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': getSnapshot().csrfToken ?? '' },
    body: JSON.stringify(args ?? {}),
  });
  let response = await send();
  // Only explicit rate/busy responses are safe to retry, never a lost response.
  const writing = (WEB_WRITE_COMMANDS as readonly string[]).includes(command);
  const managerRead = ['list_filament_spools', 'get_filament_spool_image', 'list_printers', 'list_printer_history', 'get_last_printer_for_file', 'check_filament'].includes(command);
  if ((writing || managerRead || command === 'get_model_geometry') && [429, 503].includes(response.status)) {
    const body = fileOperation ? await response.clone().json().catch(() => ({})) : null;
    const safe = !fileOperation || body.kind === 'rate_limited' || (!body.kind && body.message === 'Schreibzugriff derzeit ausgelastet');
    const retry = safe ? response.headers.get('Retry-After') : null;
    if (retry !== null) {
      const seconds = Number(retry);
      const delay = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(retry) - Date.now();
      if (Number.isFinite(delay) && delay >= 0) {
        await new Promise(resolve => setTimeout(resolve, delay));
        if (fileOperation ? canManageFiles() && getSnapshot().csrfToken === initiatingToken : !writing || canEditMetadata()) response = await send();
      }
    }
  }
  return readResponse<T>(response);
}
// Keep identity, argument count and synchronous dispatch for existing desktop mocks.
export const invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> =
  import.meta.env.VITE_TARGET === 'web' ? webInvoke : tauriInvoke;
