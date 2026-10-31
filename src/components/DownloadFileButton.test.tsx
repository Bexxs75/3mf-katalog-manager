import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { DownloadFileButton } from './DownloadFileButton';
import { LanguageProvider } from '../i18n/LanguageContext';
import { getSnapshot } from '../lib/webSession';
import { WebAuthGate } from './WebAuthGate';
let links: string[];
beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.setItem('3mf-katalog-language', 'de'); links = [];
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { links.push(this.getAttribute('href')!); expect(this.hasAttribute('download')).toBe(true); });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
const response = (authenticated = true, mustChangePassword = false) => Response.json({ authenticated, mustChangePassword, csrfToken: null });
function show() { return render(<LanguageProvider><DownloadFileButton fileId="42" /></LanguageProvider>); }
it('checks session then starts a native download at the file URL without fetching bytes', async () => {
  const fetcher = vi.fn().mockResolvedValue(response()); vi.stubGlobal('fetch', fetcher); show();
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Herunterladen' })));
  expect(links).toEqual(['/api/files/42/download']);
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual(['/api/session']);
  expect(fetcher.mock.calls[0][1].credentials).toBe('same-origin');
});
it.each([false, true])('shows the existing auth gate instead of navigating when password change is %s', async forced => {
  const fetcher = vi.fn().mockResolvedValueOnce(response()).mockResolvedValueOnce(response(forced, forced));
  vi.stubGlobal('fetch', fetcher);
  await act(async () => render(<LanguageProvider><WebAuthGate><DownloadFileButton fileId="42" /></WebAuthGate></LanguageProvider>));
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Herunterladen' })));
  expect(links).toEqual([]);
  expect(getSnapshot()).toMatchObject({ authenticated: forced, mustChangePassword: forced });
  expect(screen.getByLabelText(forced ? 'Aktuelles Passwort' : 'Passwort')).toBeVisible();
});
it.each([401, 403])('displays session check failure %s without navigation', async status => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ message: 'Anmeldung erforderlich', expected: true }, { status })));
  show(); await act(async () => fireEvent.click(screen.getByRole('button')));
  expect(links).toEqual([]); expect(screen.getByRole('alert')).toHaveTextContent('Anmeldung erforderlich');
});
it('displays network failure and permits retry', async () => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Offline'))); show();
  await act(async () => fireEvent.click(screen.getByRole('button')));
  expect(screen.getByRole('alert')).toHaveTextContent('Offline'); expect(screen.getByRole('button')).toBeEnabled(); expect(links).toEqual([]);
});
it('does not navigate if unmounted during session check', async () => {
  let resolve!: (r: Response) => void;
  vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(r => { resolve = r; })));
  const view = show(); fireEvent.click(screen.getByRole('button')); view.unmount();
  await act(async () => resolve(response())); expect(links).toEqual([]);
});
it('has no desktop download control', () => {
  vi.stubEnv('VITE_TARGET', 'desktop'); show(); expect(screen.queryByRole('button')).toBeNull();
});
