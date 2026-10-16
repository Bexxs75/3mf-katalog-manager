import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { WEB_READ_COMMANDS } from './readOnly';
import { getSnapshot, readResponse } from './webSession';

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!(WEB_READ_COMMANDS as readonly string[]).includes(command)) {
    const language = typeof document === 'undefined' ? 'de' : document.documentElement.lang;
    const messages: Record<string, string> = { de: de.webUnavailable, en: en.webUnavailable, es: es.webUnavailable, fr: fr.webUnavailable };
    throw { message: messages[language] ?? de.webUnavailable, expected: true };
  }
  return readResponse<T>(await fetch('/api/invoke/' + encodeURIComponent(command), {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': getSnapshot().csrfToken ?? '' },
    body: JSON.stringify(args ?? {}),
  }));
}
// Keep identity, argument count and synchronous dispatch for existing desktop mocks.
export const invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> =
  import.meta.env.VITE_TARGET === 'web' ? webInvoke : tauriInvoke;
