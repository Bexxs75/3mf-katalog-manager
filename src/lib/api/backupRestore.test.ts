import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import * as session from '../webSession';
import { BACKUP_MAX_BYTES, backupFileProblem, discardBackup, restoreBackup, uploadBackup, waitForServer } from './backupRestore';

class Xhr {
  static requests: Xhr[] = [];
  upload = { onprogress: null as ((event: { loaded: number; total: number; lengthComputable: boolean }) => void) | null };
  onload: (() => void) | null = null; onerror: (() => void) | null = null; onabort: (() => void) | null = null;
  status = 200; responseText = ''; headers: Record<string, string> = {}; body?: File; method = ''; url = '';
  open(method: string, url: string) { this.method = method; this.url = url; }
  setRequestHeader(name: string, value: string) { this.headers[name] = value; }
  send(body: File) { this.body = body; Xhr.requests.push(this); }
  abort() { this.onabort?.(); }
  respond(status: number, value: unknown) { this.status = status; this.responseText = typeof value === 'string' ? value : JSON.stringify(value); this.onload?.(); }
}
const fetchMock = vi.fn();
const sessionBody = { authenticated: true, writable: true, csrfToken: 'csrf', mustChangePassword: false };
beforeEach(async () => {
  Xhr.requests = [];
  fetchMock.mockReset();
  fetchMock.mockImplementation(async () => Response.json(sessionBody));
  vi.stubGlobal('XMLHttpRequest', Xhr);
  vi.stubGlobal('fetch', fetchMock);
  await session.getSession();
  fetchMock.mockReset();
});
afterEach(() => { session.authenticationFailed(401); vi.unstubAllGlobals(); vi.useRealTimers(); });

const file = (name = 'k.zip') => new File(['abc'], name);

it('judges the file before anything is sent', () => {
  expect(backupFileProblem(file('k.zip'))).toBeNull();
  expect(backupFileProblem(file('K.ZIP'))).toBeNull();
  expect(backupFileProblem(file('k.db'))).toBe('not_zip');
  expect(backupFileProblem(new File([], 'k.zip'))).toBe('empty');
  const big = file(); Object.defineProperty(big, 'size', { value: BACKUP_MAX_BYTES + 1, configurable: true });
  expect(backupFileProblem(big)).toBe('too_large');
  Object.defineProperty(big, 'size', { value: BACKUP_MAX_BYTES, configurable: true });
  expect(backupFileProblem(big)).toBeNull();
});

it('sends the ZIP raw, the password as base64 in a header only, and reports progress', async () => {
  const progress = vi.fn();
  const done = uploadBackup(file(), 'pässwörd €', progress);
  const xhr = Xhr.requests[0];
  expect([xhr.method, xhr.url]).toEqual(['POST', '/api/backup/upload']);
  expect(xhr.url).not.toContain('pässwörd');
  expect(xhr.body).toBeInstanceOf(File);
  expect(xhr.headers['X-CSRF-Token']).toBe('csrf');
  expect(new TextDecoder().decode(Uint8Array.from(atob(xhr.headers['X-Restore-Password']), (c) => c.charCodeAt(0)))).toBe('pässwörd €');
  expect(Object.values(xhr.headers).join(' ')).not.toContain('pässwörd');
  xhr.upload.onprogress?.({ loaded: 1, total: 4, lengthComputable: true });
  expect(progress).toHaveBeenCalledWith(25);
  xhr.respond(200, { token: 't', report: { schema_version: 1 }, settings_ignored: true });
  await expect(done).resolves.toMatchObject({ token: 't', settingsIgnored: true, report: { schema_version: 1 } });
});

it('never writes the password to the console', async () => {
  const spies = (['log', 'info', 'warn', 'error', 'debug'] as const).map((name) => vi.spyOn(console, name).mockImplementation(() => {}));
  const done = uploadBackup(file(), 'topsecret', () => {});
  Xhr.requests[0].respond(403, { message: 'password_invalid', expected: true });
  await done.catch(() => {});
  for (const spy of spies) expect(JSON.stringify(spy.mock.calls)).not.toContain('topsecret');
  spies.forEach((spy) => spy.mockRestore());
});

it.each([
  [403, { message: 'password_invalid', expected: true }, 'password_invalid'],
  [400, { message: 'backupInvalidDatabase', expected: true }, 'backupInvalidDatabase'],
  [400, { message: 'backupEntryTooLarge', expected: true, params: ['catalog.db', '300', '256'] }, 'backupEntryTooLarge'],
  [429, { message: 'webTooManyRequests', expected: true }, 'webTooManyRequests'],
  [503, { message: 'maintenance', expected: true }, 'maintenance'],
  [413, { message: 'webBodyTooLarge', expected: true }, 'backupUploadTooLarge'],
  [413, '<html>too big</html>', 'backupUploadTooLarge'],
  [503, '<html>gateway</html>', 'maintenance'],
  [500, '', 'webServerInternal'],
])('turns status %i into the code %s', async (status, body, message) => {
  const done = uploadBackup(file(), 'x', () => {});
  Xhr.requests[0].respond(status, body);
  await expect(done).rejects.toMatchObject({ message, expected: true, status });
});

it('passes the parameters of a coded answer on', async () => {
  const done = uploadBackup(file(), 'x', () => {});
  Xhr.requests[0].respond(400, { message: 'backupEntryTooLarge', expected: true, params: ['a', '1', '2'] });
  await expect(done).rejects.toMatchObject({ params: ['a', '1', '2'] });
});

it('ends the browser session on a 401', async () => {
  const done = uploadBackup(file(), 'x', () => {});
  Xhr.requests[0].respond(401, { message: 'session_expired', expected: true });
  await expect(done).rejects.toMatchObject({ message: 'session_expired' });
  expect(session.getSnapshot().authenticated).toBe(false);
});

it('reports a broken connection and an abort as a lost connection', async () => {
  const first = uploadBackup(file(), 'x', () => {});
  Xhr.requests[0].onerror?.();
  await expect(first).rejects.toMatchObject({ message: 'backupConnectionLost', status: 0 });
  const controller = new AbortController();
  const second = uploadBackup(file(), 'x', () => {}, controller.signal);
  controller.abort();
  await expect(second).rejects.toMatchObject({ message: 'backupConnectionLost' });
});

it('sends nothing without the write right', async () => {
  session.authenticationFailed(401);
  await expect(uploadBackup(file(), 'x', () => {})).rejects.toMatchObject({ message: 'session_required' });
  expect(Xhr.requests).toHaveLength(0);
  await expect(restoreBackup('t', 'x')).rejects.toMatchObject({ message: 'session_required' });
  await discardBackup('t');
  expect(fetchMock).not.toHaveBeenCalled();
});

it('discards by token with the CSRF header and swallows a failure', async () => {
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));
  await discardBackup('abc');
  expect(fetchMock).toHaveBeenCalledWith('/api/backup/staging/abc', expect.objectContaining({ method: 'DELETE', headers: { 'X-CSRF-Token': 'csrf' } }));
  fetchMock.mockRejectedValueOnce(new TypeError('offline'));
  await expect(discardBackup('abc')).resolves.toBeUndefined();
});

it('restores with token and password in the JSON body, not the URL', async () => {
  fetchMock.mockResolvedValueOnce(Response.json({ restored: true, restart: true }));
  await expect(restoreBackup('tok', 'pw')).resolves.toEqual({ restored: true, restart: true });
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe('/api/backup/restore');
  expect(url).not.toContain('pw');
  expect(init.method).toBe('POST');
  expect(init.headers['X-CSRF-Token']).toBe('csrf');
  expect(JSON.parse(init.body)).toEqual({ token: 'tok', password: 'pw' });
});

it('reports the coded refusals of the restore route', async () => {
  fetchMock.mockResolvedValueOnce(Response.json({ message: 'restoreUploadsPending', expected: true }, { status: 409 }));
  await expect(restoreBackup('t', 'p')).rejects.toMatchObject({ message: 'restoreUploadsPending', status: 409 });
  fetchMock.mockResolvedValueOnce(Response.json({ message: 'restoreDrainTimeout', expected: true }, { status: 503 }));
  await expect(restoreBackup('t', 'p')).rejects.toMatchObject({ message: 'restoreDrainTimeout', status: 503 });
});

it('treats a lost or odd answer to the restore as a lost connection, never as success', async () => {
  fetchMock.mockRejectedValueOnce(new TypeError('network'));
  await expect(restoreBackup('t', 'p')).rejects.toMatchObject({ message: 'backupConnectionLost', status: 0 });
  fetchMock.mockResolvedValueOnce(new Response('<html></html>', { status: 200 }));
  await expect(restoreBackup('t', 'p')).rejects.toMatchObject({ message: 'backupConnectionLost' });
});

it('polls the health route after a pause until the server answers', async () => {
  vi.useFakeTimers();
  fetchMock.mockRejectedValueOnce(new TypeError('down')).mockResolvedValueOnce(new Response('', { status: 502 })).mockResolvedValueOnce(Response.json({ ok: true }));
  const result = waitForServer({ initialDelayMs: 1000, intervalMs: 500, timeoutMs: 60_000 });
  await vi.advanceTimersByTimeAsync(900);
  expect(fetchMock).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(100);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1000);
  await expect(result).resolves.toBe(true);
  expect(fetchMock).toHaveBeenCalledTimes(3);
  expect(fetchMock.mock.calls[0][0]).toBe('/api/health');
});

it('stops waiting at the deadline instead of looping forever', async () => {
  vi.useFakeTimers();
  fetchMock.mockRejectedValue(new TypeError('down'));
  const result = waitForServer({ initialDelayMs: 0, intervalMs: 1000, timeoutMs: 5000 });
  await vi.advanceTimersByTimeAsync(7000);
  await expect(result).resolves.toBe(false);
  const calls = fetchMock.mock.calls.length;
  await vi.advanceTimersByTimeAsync(10_000);
  expect(fetchMock.mock.calls.length).toBe(calls);
});

it('stops waiting when asked to', async () => {
  vi.useFakeTimers();
  fetchMock.mockRejectedValue(new TypeError('down'));
  const controller = new AbortController();
  const result = waitForServer({ signal: controller.signal, initialDelayMs: 0, intervalMs: 1000, timeoutMs: 60_000 });
  await vi.advanceTimersByTimeAsync(2500);
  controller.abort();
  await expect(result).resolves.toBe(false);
});
