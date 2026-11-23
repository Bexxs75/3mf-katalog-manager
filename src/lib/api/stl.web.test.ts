import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { downloadModelAsStl } from './stl';
import { authenticationFailed } from '../webSession';

let clicked: { href: string; download: string }[];
beforeEach(() => {
  clicked = [];
  vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn(() => 'blob:stl'), revokeObjectURL: vi.fn() }));
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { clicked.push({ href: this.href, download: this.download }); });
});
afterEach(() => { authenticationFailed(401); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const stl = (triangles: number) => new Uint8Array(84 + 50 * triangles).buffer;

it('fetches the converted file and hands it to the browser under the model name', async () => {
  const fetchMock = vi.fn(async () => new Response(stl(2), { headers: { 'Content-Type': 'application/octet-stream' } }));
  vi.stubGlobal('fetch', fetchMock);
  const result = await downloadModelAsStl('7', 'Box.3mf', { kind: 'all' });
  expect(fetchMock).toHaveBeenCalledWith('/api/files/7/stl', { credentials: 'same-origin' });
  expect(clicked).toEqual([{ href: 'blob:stl', download: 'Box.stl' }]);
  expect(result).toMatchObject({ name: 'Box.stl', triangles: 2, fileCount: 1, folder: '' });
});

it('asks for one plate and names the file after it', async () => {
  const fetchMock = vi.fn(async () => new Response(stl(1), { headers: { 'Content-Type': 'application/octet-stream' } }));
  vi.stubGlobal('fetch', fetchMock);
  const result = await downloadModelAsStl('7', 'Box.3mf', { kind: 'plate', number: 3 });
  expect(fetchMock).toHaveBeenCalledWith('/api/files/7/stl?plate=3', { credentials: 'same-origin' });
  expect(result.name).toBe('Box_Platte3.stl');
});

it('passes a coded refusal on and saves nothing', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ message: 'stlTooLarge', expected: true, params: ['2000000', '1000000'] }, { status: 413 })));
  await expect(downloadModelAsStl('7', 'Box.3mf', { kind: 'all' })).rejects.toMatchObject({ message: 'stlTooLarge' });
  expect(clicked).toEqual([]);
});
