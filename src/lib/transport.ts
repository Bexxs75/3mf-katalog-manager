import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './readOnly';
import { canEditMetadata, getSnapshot, readResponse } from './webSession';

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!(WEB_READ_COMMANDS as readonly string[]).includes(command) && !(canEditMetadata() && (WEB_WRITE_COMMANDS as readonly string[]).includes(command))) {
    const language = typeof document === 'undefined' ? 'de' : document.documentElement.lang;
    const messages: Record<string, string> = { de: de.webUnavailable, en: en.webUnavailable, es: es.webUnavailable, fr: fr.webUnavailable };
    throw { message: messages[language] ?? de.webUnavailable, expected: true };
  }
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
    const retry = response.headers.get('Retry-After');
    if (retry !== null) {
      const seconds = Number(retry);
      const delay = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(retry) - Date.now();
      if (Number.isFinite(delay) && delay >= 0) {
        await new Promise(resolve => setTimeout(resolve, delay));
        if (!writing || canEditMetadata()) response = await send();
      }
    }
  }
  return readResponse<T>(response);
}
// Keep identity, argument count and synchronous dispatch for existing desktop mocks.
export const invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> =
  import.meta.env.VITE_TARGET === 'web' ? webInvoke : tauriInvoke;
