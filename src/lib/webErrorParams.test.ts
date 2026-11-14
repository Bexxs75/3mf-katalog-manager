import { afterEach, expect, it, vi } from 'vitest';
import { de } from '../i18n/de';
afterEach(() => { vi.unstubAllEnvs(); vi.unstubAllGlobals(); });

// The web server answers with the JSON body `{ message, expected, params? }`; `webInvoke`
// rejects with that body unchanged, so the params of a coded error reach `formatAppError`.
it('hands the params of a server error body to formatAppError', async () => {
  vi.stubEnv('VITE_TARGET', 'web'); vi.resetModules();
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ message: 'folderNameForbiddenChar', params: [':'], expected: true }, { status: 422 })));
  const { invoke } = await import('./transport');
  const { toAppError } = await import('./errors');
  const { formatAppError } = await import('./errorMessage');
  const error = toAppError(await invoke('list_folders').catch((e: unknown) => e));
  expect(error).toMatchObject({ message: 'folderNameForbiddenChar', params: [':'], unexpected: false });
  expect(formatAppError(k => de[k], error)).toBe(de.errFolderNameForbiddenChar.replace('{0}', ':'));
});

it('shows server text without a code as it is and drops params that are not strings', async () => {
  vi.stubEnv('VITE_TARGET', 'web'); vi.resetModules();
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ message: 'Eintrag nicht gefunden', params: [1, { a: 2 }], expected: true }, { status: 404 })));
  const { invoke } = await import('./transport');
  const { toAppError } = await import('./errors');
  const { formatAppError } = await import('./errorMessage');
  const error = toAppError(await invoke('list_folders').catch((e: unknown) => e));
  expect(error.params).toBeUndefined();
  expect(formatAppError(k => de[k], error)).toBe('Eintrag nicht gefunden');
});
