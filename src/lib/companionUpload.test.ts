import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import * as session from './webSession';
import { uploadCompanion, isAllowedCompanionName, companionSizeProblem } from './companionUpload';

class Xhr {
  static requests: Xhr[] = [];
  upload = { onprogress: null as ((event: { loaded: number; total: number; lengthComputable: boolean }) => void) | null };
  onload: (() => void) | null = null; onerror: (() => void) | null = null; onabort: (() => void) | null = null;
  status = 200; responseText = ''; headers: Record<string, string> = {}; retry: string | null = null; body?: File; method = ''; url = '';
  open(method: string, url: string) { this.method = method; this.url = url; }
  setRequestHeader(name: string, value: string) { this.headers[name] = value; }
  getResponseHeader() { return this.retry; }
  send(body: File) { this.body = body; Xhr.requests.push(this); }
  abort() { this.onabort?.(); }
  respond(status: number, value: unknown, retry: string | null = null) { this.status = status; this.responseText = JSON.stringify(value); this.retry = retry; this.onload?.(); }
}
beforeEach(async () => {
  Xhr.requests = [];
  vi.stubGlobal('XMLHttpRequest', Xhr);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'csrf', mustChangePassword: false })));
  await session.getSession();
});
afterEach(() => { session.authenticationFailed(401); vi.unstubAllGlobals(); });

it('sends the File itself with the headers the server expects and reports progress', async () => {
  const progress = vi.fn();
  const done = uploadCompanion('12', new File(['abc'], 'ä #.png'), progress);
  const xhr = Xhr.requests[0];
  expect([xhr.method, xhr.url]).toEqual(['POST', '/api/companion-upload']);
  expect(xhr.body).toBeInstanceOf(File);
  expect(xhr.headers).toMatchObject({ 'Content-Type': 'application/octet-stream', 'X-CSRF-Token': 'csrf', 'X-File-Id': '12', 'X-Upload-Name': '%C3%A4%20%23.png' });
  expect(xhr.headers['X-Request-Id']).toMatch(/^[0-9a-f-]{36}$/);
  xhr.upload.onprogress?.({ loaded: 1, total: 4, lengthComputable: true });
  expect(progress).toHaveBeenCalledWith(25);
  xhr.respond(200, { id: '5', name: 'ä #.png' });
  await expect(done).resolves.toMatchObject({ id: '5' });
});

it('uses a new request id for every upload', () => {
  void uploadCompanion('1', new File(['a'], 'a.png'), () => {});
  void uploadCompanion('1', new File(['a'], 'a.png'), () => {});
  expect(Xhr.requests[0].headers['X-Request-Id']).not.toBe(Xhr.requests[1].headers['X-Request-Id']);
});

it.each([
  [413, { kind: 'companion_too_large' }, /größer als die Grenze/],
  [415, { kind: 'companion_type' }, /nicht erlaubt/],
  [422, { kind: 'companion_content' }, /passt nicht/],
  [503, { kind: 'upload_busy' }, /anderen Begleitdatei/],
  [507, { kind: 'insufficient_storage' }, /Speicherplatz/],
  [413, {}, /größer als die Grenze/],
])('turns status %i into a plain message', async (status, body, text) => {
  document.documentElement.lang = 'de';
  const done = uploadCompanion('1', new File(['a'], 'a.png'), () => {});
  Xhr.requests[0].respond(status, body);
  await expect(done).rejects.toMatchObject({ expected: true, message: expect.stringMatching(text) });
});

it('refuses without the file-management right and ends on abort', async () => {
  const controller = new AbortController();
  const done = uploadCompanion('1', new File(['a'], 'a.png'), () => {}, controller.signal);
  controller.abort();
  await expect(done).rejects.toMatchObject({ expected: true });
  session.authenticationFailed(401);
  await expect(uploadCompanion('1', new File(['a'], 'a.png'), () => {})).rejects.toMatchObject({ kind: 'revoked' });
});

it('knows the allowed extensions', () => {
  for (const name of ['a.PNG', 'b.jpeg', 'c.pdf', 'd.md', 'e.TXT', 'f.webp', 'g.gif']) expect(isAllowedCompanionName(name)).toBe(true);
  for (const name of ['a.svg', 'b.exe', 'c', 'd.png.exe', 'e.html']) expect(isAllowedCompanionName(name)).toBe(false);
});

it('refuses an empty or too big file before sending anything and says why', async () => {
  document.documentElement.lang = 'de';
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'csrf', mustChangePassword: false, companionMaxBytes: 10 })));
  await session.getSession();
  expect(companionSizeProblem(new File([], 'a.png'))).toBe('empty');
  expect(companionSizeProblem(new File(['x'.repeat(11)], 'a.png'))).toBe('too_large');
  expect(companionSizeProblem(new File(['x'.repeat(10)], 'a.png'))).toBeNull();
  await expect(uploadCompanion('1', new File(['x'.repeat(11)], 'a.png'), () => {})).rejects.toMatchObject({ kind: 'companion_too_large' });
  await expect(uploadCompanion('1', new File([], 'a.png'), () => {})).rejects.toMatchObject({ kind: 'companion_content' });
  expect(Xhr.requests).toHaveLength(0);
});

it('does not call a broken connection busy', async () => {
  document.documentElement.lang = 'de';
  const done = uploadCompanion('1', new File(['a'], 'a.png'), () => {});
  Xhr.requests[0].onerror?.();
  await expect(done).rejects.toMatchObject({ kind: 'upload_interrupted', message: expect.stringMatching(/Verbindung zum Server/) });
});
