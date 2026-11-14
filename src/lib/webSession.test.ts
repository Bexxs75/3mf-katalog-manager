import { afterEach, expect, it, vi } from 'vitest';
afterEach(() => { vi.resetModules(); vi.unstubAllGlobals(); });
it('logs in, rotates CSRF on password change and clears it on logout', async () => {
  const fetch = vi.fn().mockResolvedValueOnce(Response.json({ csrfToken: 'a', mustChangePassword: true }))
    .mockResolvedValueOnce(Response.json({ csrfToken: 'b' })).mockResolvedValueOnce(Response.json({ ok: true }));
  vi.stubGlobal('fetch', fetch);
  const api = await import('./webSession');
  await api.login('old');
  expect(api.getSnapshot()).toEqual({ authenticated: true, writable: false, csrfToken: 'a', mustChangePassword: true });
  await api.changePassword('old', 'new');
  expect(fetch).toHaveBeenLastCalledWith('/api/password', expect.objectContaining({ body: '{"current":"old","new":"new"}', headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': 'a' } }));
  expect(api.getSnapshot().csrfToken).toBe('b');
  await api.logout();
  expect(fetch).toHaveBeenLastCalledWith('/api/logout', expect.objectContaining({ headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': 'b' } }));
  expect(api.getSnapshot()).toEqual({ authenticated: false, writable: false, csrfToken: null, mustChangePassword: false });
});
