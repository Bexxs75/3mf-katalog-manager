import { afterEach, beforeEach, expect, it, vi } from 'vitest';

class FakeSource {
  static instances: FakeSource[] = [];
  static CLOSED = 2;
  readyState = 1;
  onerror: (() => void) | null = null;
  closed = false;
  private listeners = new Map<string, Array<(m: MessageEvent) => void>>();
  constructor(public url: string) { FakeSource.instances.push(this); }
  addEventListener(name: string, fn: (m: MessageEvent) => void) { this.listeners.set(name, [...(this.listeners.get(name) ?? []), fn]); }
  close() { this.closed = true; this.readyState = 2; }
  emit(name: string, data: string) { this.listeners.get(name)?.forEach(fn => fn({ data } as MessageEvent)); }
}

async function load(authenticated = true) {
  vi.stubEnv('VITE_TARGET', 'web');
  vi.resetModules();
  vi.stubGlobal('EventSource', FakeSource);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated, mustChangePassword: false, csrfToken: 'token' })));
  const session = await import('./webSession');
  await session.getSession();
  return { ...(await import('./events')), session };
}

beforeEach(() => { FakeSource.instances = []; });
afterEach(() => { vi.unstubAllEnvs(); vi.unstubAllGlobals(); vi.useRealTimers(); });

it('delivers a server printer event to listeners over one shared stream', async () => {
  const { listen } = await load();
  const first = vi.fn(); const second = vi.fn();
  const off = await listen('printer-jobs-changed', first);
  await listen('printer-jobs-changed', second);
  expect(FakeSource.instances).toHaveLength(1);
  expect(FakeSource.instances[0].url).toBe('/api/events');
  FakeSource.instances[0].emit('printer-jobs-changed', 'null');
  expect(first).toHaveBeenCalledWith({ event: 'printer-jobs-changed', id: 0, payload: null });
  expect(second).toHaveBeenCalledTimes(1);
  off();
  FakeSource.instances[0].emit('printer-jobs-changed', 'null');
  expect(first).toHaveBeenCalledTimes(1);
  expect(second).toHaveBeenCalledTimes(2);
});

it('closes the stream with the last listener', async () => {
  const { listen } = await load();
  const off = await listen('printer-jobs-changed', vi.fn());
  off();
  expect(FakeSource.instances[0].closed).toBe(true);
});

it('keeps desktop-only event names silent in the browser', async () => {
  const { listen } = await load();
  await listen('tauri://drag-drop', vi.fn());
  expect(FakeSource.instances).toHaveLength(0);
});

it('opens no stream without a signed-in session and starts one after sign-in', async () => {
  const { listen, session } = await load(false);
  const handler = vi.fn();
  await listen('printer-jobs-changed', handler);
  expect(FakeSource.instances).toHaveLength(0);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, mustChangePassword: false, csrfToken: 'token' })));
  await session.login('secret');
  expect(FakeSource.instances).toHaveLength(1);
});

it('reopens the stream after the server ended it', async () => {
  vi.useFakeTimers();
  const { listen } = await load();
  await listen('printer-jobs-changed', vi.fn());
  const [stream] = FakeSource.instances;
  stream.readyState = 2;
  stream.onerror?.();
  await vi.advanceTimersByTimeAsync(3100);
  expect(FakeSource.instances).toHaveLength(2);
});
