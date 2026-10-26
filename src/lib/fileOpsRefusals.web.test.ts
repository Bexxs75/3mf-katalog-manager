import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, login } from './webSession';
import { fileOpsBatch } from './fileOps';
import { clearMetadataError, getMetadataError, isMetadataErrorPlain } from './metadataErrors';
let results: unknown;
beforeEach(async () => {
  results = []; clearMetadataError();
  vi.stubGlobal('fetch', vi.fn(async (url: string) => url.includes('/invoke/')
    ? Response.json(results)
    : Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'token' })));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.unstubAllGlobals(); });

it('names the file instead of its ID and marks a known refusal as self-explanatory', async () => {
  results = [{ id: '7', ok: false, kind: 'has_companions' }, { id: '8', ok: true }];
  await expect(fileOpsBatch('move_files_to_folder', ['7', '8'], {}, () => false, { '7': 'Deckel.3mf', '8': 'Boden.3mf' })).rejects.toBeTruthy();
  expect(getMetadataError()).toContain('Deckel.3mf: Ein betroffenes Modell hat Begleitdateien');
  expect(getMetadataError()).not.toMatch(/(^|\n)7: /);
  expect(isMetadataErrorPlain()).toBe(true);
});
it('drops the meaningless ID when a single result has no known name', async () => {
  results = [{ id: '4', ok: false, kind: 'protected' }];
  await expect(fileOpsBatch('move_files_to_folder', ['4'], {})).rejects.toBeTruthy();
  expect(getMetadataError()).toBe('Dieser Speicherort ist geschützt.');
  expect(isMetadataErrorPlain()).toBe(true);
});
it('keeps the generic framing for unknown or technical failures', async () => {
  results = [{ id: '4', ok: false, kind: 'something_odd' }];
  await expect(fileOpsBatch('move_files_to_folder', ['4'], {})).rejects.toBeTruthy();
  expect(isMetadataErrorPlain()).toBe(false);
  clearMetadataError();
  results = [{ id: '1', ok: false, kind: 'protected' }, { id: '2', ok: false, kind: 'generic' }];
  await expect(fileOpsBatch('move_files_to_folder', ['1', '2'], {})).rejects.toBeTruthy();
  expect(isMetadataErrorPlain()).toBe(false);
});
