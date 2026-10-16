import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { getSnapshot, readResponse } from './webSession';

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return readResponse<T>(await fetch('/api/invoke/' + encodeURIComponent(command), {
    method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': getSnapshot().csrfToken ?? '' },
    body: JSON.stringify(args ?? {}),
  }));
}
// Keep identity, argument count and synchronous dispatch for existing desktop mocks.
export const invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> =
  import.meta.env.VITE_TARGET === 'web' ? webInvoke : tauriInvoke;
