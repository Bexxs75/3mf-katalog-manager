import { listen as tauriListen } from '@tauri-apps/api/event';
import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event';
import { getSnapshot, subscribe } from './webSession';
export type { Event, UnlistenFn } from '@tauri-apps/api/event';

// The web build has no Tauri event bus. The server pushes the same named events over one shared
// SSE stream, so every listener of a tab reuses a single connection (the server caps connections per session).
const RECONNECT_MS = 3000;
// Only events the server really sends are bridged; every other name keeps the silent no-op it had in the web build,
// so desktop-only listeners (drag, open requests, archive import) never open a stream.
const SERVER_EVENTS: ReadonlySet<string> = new Set(['printer-jobs-changed']);
// A stream without a usable session would only be answered with an error and retried forever.
const signedIn = () => { const session = getSnapshot(); return session.authenticated && !session.mustChangePassword; };
type Handler = EventCallback<unknown>;
const handlers = new Map<string, Set<Handler>>();
const attached = new Set<string>();
let source: EventSource | undefined;
let reconnect: ReturnType<typeof setTimeout> | undefined;

function deliver(name: string, message: MessageEvent) {
  let payload: unknown = null;
  try { payload = message.data ? JSON.parse(message.data) : null; } catch { /* A non-JSON frame still signals the event. */ }
  for (const handler of [...(handlers.get(name) ?? [])]) handler({ event: name, id: 0, payload });
}

function attach(stream: EventSource) {
  for (const name of handlers.keys()) {
    if (attached.has(name)) continue;
    attached.add(name);
    stream.addEventListener(name, (message) => deliver(name, message as MessageEvent));
  }
}

function open() {
  if (source || handlers.size === 0 || !signedIn()) return;
  const stream = new EventSource('/api/events');
  source = stream;
  attached.clear();
  attach(stream);
  stream.onerror = () => {
    // EventSource retries by itself unless the server ended the stream (e.g. an expired session); then retry later ourselves.
    if (stream.readyState !== EventSource.CLOSED) return;
    stream.close();
    if (source === stream) source = undefined;
    clearTimeout(reconnect);
    reconnect = setTimeout(open, RECONNECT_MS);
  };
}

function close() {
  clearTimeout(reconnect);
  source?.close();
  source = undefined;
  attached.clear();
}

let watchingSession = false;
const webListen = (async (name: string, handler: Handler): Promise<UnlistenFn> => {
  if (!SERVER_EVENTS.has(name)) return () => {};
  if (!watchingSession) {
    watchingSession = true;
    subscribe(() => { if (signedIn()) open(); else close(); });
  }
  const set = handlers.get(name) ?? new Set<Handler>();
  handlers.set(name, set);
  set.add(handler);
  if (source) attach(source); else open();
  return () => {
    set.delete(handler);
    if (set.size === 0) handlers.delete(name);
    if (handlers.size === 0) close();
  };
}) as unknown as typeof tauriListen;

export const listen: typeof tauriListen = import.meta.env.VITE_TARGET === 'web' ? webListen : tauriListen;
