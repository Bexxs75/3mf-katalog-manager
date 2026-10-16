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
