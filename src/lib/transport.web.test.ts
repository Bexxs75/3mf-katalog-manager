import { afterEach, beforeEach, expect, it, vi } from 'vitest';

beforeEach(() => { vi.resetModules(); vi.stubEnv('VITE_TARGET', 'web'); });
afterEach(() => { vi.unstubAllEnvs(); vi.unstubAllGlobals(); });

it('sends JSON with the session CSRF token and same-origin credentials', async () => {
  const fetch = vi.fn().mockResolvedValueOnce(Response.json({ authenticated: true, csrfToken: 'csrf', mustChangePassword: false }))
    .mockResolvedValueOnce(Response.json(['model']));
  vi.stubGlobal('fetch', fetch);
  await (await import('./webSession')).getSession();
  const { invoke } = await import('./transport');
  expect(await invoke('list_files_by_ids', { ids: ['1'] })).toEqual(['model']);
  expect(fetch).toHaveBeenLastCalledWith('/api/invoke/list_files_by_ids', expect.objectContaining({
    method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': 'csrf' }, body: '{"ids":["1"]}',
  }));
});
it('returns geometry as an ArrayBuffer', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new Uint8Array([1, 2, 3]), { headers: { 'Content-Type': 'application/octet-stream' } })));
  const result = await (await import('./transport')).invoke<ArrayBuffer>('get_model_geometry');
  expect([...new Uint8Array(result)]).toEqual([1, 2, 3]);
});
it.each([401, 403])('publishes authentication state for status %s and preserves the error object', async status => {
  const error = { message: 'denied', expected: true, code: status === 403 ? 'password_change_required' : 'unauthenticated' };
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json(error, { status })));
  const session = await import('./webSession');
  const listener = vi.fn();
  const off = session.subscribe(listener);
  await expect((await import('./transport')).invoke('list_folders')).rejects.toEqual(error);
  expect(listener).toHaveBeenCalled();
  expect(session.getSnapshot().mustChangePassword).toBe(status === 403);
  off();
});
it.each([429, 503])('retains Retry-After on ordinary errors (%s)', async status => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ message: 'busy', expected: true }, { status, headers: { 'Retry-After': '60' } })));
  await expect((await import('./transport')).invoke('list_folders')).rejects.toMatchObject({ message: 'busy', expected: true, retryAfter: '60' });
});

it.each([429, 503])('retries geometry once for %s with Retry-After', async status => {
  vi.useFakeTimers();
  try {
    const fetch = vi.fn().mockImplementation(async () => Response.json({ message: 'busy', expected: true }, { status, headers: { 'Retry-After': '2' } }));
    vi.stubGlobal('fetch', fetch);
    const pending = (await import('./transport')).invoke('get_model_geometry', { fileId: '1' });
    const rejected = expect(pending).rejects.toMatchObject({ message: 'busy' });
    await vi.advanceTimersByTimeAsync(1999); expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1); await rejected;
    expect(fetch).toHaveBeenCalledTimes(2);
  } finally { vi.useRealTimers(); }
});

const KEY_PATTERN = /^[A-Za-z0-9_-]{16,64}$/;
async function writable() {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(Response.json({ authenticated: true, csrfToken: 'csrf', mustChangePassword: false, writable: true })));
  await (await import('./webSession')).getSession();
}
const requestIds = (fetch: ReturnType<typeof vi.fn>) => fetch.mock.calls.map(call => (call[1].headers as Record<string, string>)['X-Request-Id']);

it('sends one request id per write action and none for reads', async () => {
  await writable();
  const fetch = vi.fn().mockImplementation(async () => Response.json({}));
  vi.stubGlobal('fetch', fetch);
  const { invoke } = await import('./transport');
  await invoke('create_collection', { name: 'a' });
  await invoke('create_collection', { name: 'a' });
  await invoke('list_folders');
  const [first, second] = requestIds(fetch);
  expect(first).toMatch(KEY_PATTERN);
  expect(second).toMatch(KEY_PATTERN);
  expect(first).not.toBe(second);
  expect((fetch.mock.calls[2][1].headers as Record<string, string>)['X-Request-Id']).toBeUndefined();
});
it.each(['start_upload_import', 'cancel_import'])('keeps %s free of a request id', async command => {
  await writable();
  const fetch = vi.fn().mockImplementation(async () => Response.json({}));
  vi.stubGlobal('fetch', fetch);
  await (await import('./transport')).invoke(command, {});
  expect((fetch.mock.calls[0][1].headers as Record<string, string>)['X-Request-Id']).toBeUndefined();
});
it.each([
  ['network error', () => Promise.reject(new TypeError('Failed to fetch'))],
  ['server timeout', async () => Response.json({ message: 'late', expected: true }, { status: 408 })],
  ['proxy 502', async () => Response.json({ message: 'gateway', expected: false }, { status: 502 })],
  ['proxy 504', async () => Response.json({ message: 'gateway', expected: false }, { status: 504 })],
  ['proxy 524', async () => Response.json({ message: 'gateway', expected: false }, { status: 524 })],
  ['closed client 499', async () => Response.json({ message: 'gone', expected: false }, { status: 499 })],
])('repeats a write after a %s with the same key and backoff 1 s then 3 s', async (_name, failing) => {
  vi.useFakeTimers();
  try {
    await writable();
    const fetch = vi.fn().mockImplementationOnce(failing).mockImplementationOnce(failing).mockImplementation(async () => Response.json({ id: '7' }));
    vi.stubGlobal('fetch', fetch);
    const pending = (await import('./transport')).invoke('create_collection', { name: 'a' });
    await vi.advanceTimersByTimeAsync(999); expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1); expect(fetch).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2999); expect(fetch).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(await pending).toEqual({ id: '7' });
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(new Set(requestIds(fetch)).size).toBe(1);
  } finally { vi.useRealTimers(); }
});
it('gives up after two repeats and surfaces the last failure', async () => {
  vi.useFakeTimers();
  try {
    await writable();
    const fetch = vi.fn().mockImplementation(() => Promise.reject(new TypeError('Failed to fetch')));
    vi.stubGlobal('fetch', fetch);
    const rejected = expect((await import('./transport')).invoke('create_collection', { name: 'a' })).rejects.toThrow('Failed to fetch');
    await vi.advanceTimersByTimeAsync(4000); await rejected;
    expect(fetch).toHaveBeenCalledTimes(3);
  } finally { vi.useRealTimers(); }
});
it('does not repeat reads after a network error', async () => {
  const fetch = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
  vi.stubGlobal('fetch', fetch);
  await expect((await import('./transport')).invoke('list_folders')).rejects.toThrow('Failed to fetch');
  expect(fetch).toHaveBeenCalledTimes(1);
});
it('builds a valid key without crypto.randomUUID (insecure context)', async () => {
  vi.stubGlobal('crypto', { getRandomValues: (a: Uint8Array) => { a.fill(0xab); return a; } });
  const { newRequestId } = await import('./transport');
  const id = newRequestId();
  expect(id).toMatch(KEY_PATTERN);
  expect(id).toBe('abababab-abab-4bab-abab-abababababab');
  vi.stubGlobal('crypto', undefined);
  expect(newRequestId()).toMatch(KEY_PATTERN);
});

it('sends byte-identical arguments on every repeat even if the caller changes the object', async () => {
  vi.useFakeTimers();
  try {
    await writable();
    const fetch = vi.fn().mockImplementationOnce(() => Promise.reject(new TypeError('Failed to fetch'))).mockImplementation(async () => Response.json({}));
    vi.stubGlobal('fetch', fetch);
    const args = { name: 'before' };
    const pending = (await import('./transport')).invoke('create_collection', args);
    await vi.advanceTimersByTimeAsync(0);
    args.name = 'after';
    await vi.advanceTimersByTimeAsync(1000); await pending;
    expect(fetch.mock.calls.map(call => call[1].body)).toEqual(['{"name":"before"}', '{"name":"before"}']);
  } finally { vi.useRealTimers(); }
});
it('stops repeating when the session changed during the backoff', async () => {
  vi.useFakeTimers();
  try {
    await writable();
    const fetch = vi.fn().mockImplementation(async () => Response.json({ message: 'gateway', expected: false }, { status: 502 }));
    vi.stubGlobal('fetch', fetch);
    const rejected = expect((await import('./transport')).invoke('create_collection', { name: 'a' })).rejects.toMatchObject({ message: 'gateway' });
    await vi.advanceTimersByTimeAsync(500);
    (await import('./webSession')).authenticationFailed(401);
    await vi.advanceTimersByTimeAsync(1000); await rejected;
    expect(fetch).toHaveBeenCalledTimes(1);
  } finally { vi.useRealTimers(); }
});
it('explains a lost, unreplayable answer in every language', async () => {
  const { formatAppError } = await import('../lib/errorMessage');
  const { de } = await import('../i18n/de'); const { en } = await import('../i18n/en');
  const { es } = await import('../i18n/es'); const { fr } = await import('../i18n/fr');
  for (const dictionary of [de, en, es, fr]) {
    const text = formatAppError(key => dictionary[key], { message: 'request_id_result_unavailable' });
    expect(text).toBe(dictionary.requestResultLost);
    expect(text).not.toContain('request_id');
  }
});
