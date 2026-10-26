// The transport picks web or desktop once, at import time.
import { vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { WebSlicerMenu } from './WebSlicerMenu';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { getSession } from '../lib/webSession';

let launched: string[];
type Reply = Response | (() => Response);
function server(session: Record<string, unknown>, link: Reply = () => Response.json({ path: '/dl/42/1.abc.sig/Wurfel.stl', expiresAt: 1, name: 'Würfel.stl' })) {
  const fetcher = vi.fn(async (url: string) => {
    if (url === '/api/session') return Response.json({ authenticated: true, mustChangePassword: false, csrfToken: 'csrf-1', ...session });
    return typeof link === 'function' ? link() : link;
  });
  vi.stubGlobal('fetch', fetcher);
  return fetcher;
}
async function show(name = 'Würfel.stl', fileType?: string) {
  await act(async () => { await getSession(); });
  return render(<LanguageProvider><WebSlicerMenu fileId="42" fileName={name} fileType={fileType} /></LanguageProvider>);
}
const openMenu = () => fireEvent.click(screen.getByRole('button', { name: /In Slicer öffnen/ }));
const names = () => screen.getAllByRole('menuitem').map(item => item.textContent);

beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.setItem('3mf-katalog-language', 'de'); launched = [];
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { launched.push(this.getAttribute('href')!); });
});
afterEach(async () => {
  cleanup(); vi.restoreAllMocks();
  server({ authenticated: false, csrfToken: null });
  await act(async () => { await getSession(); });
  vi.unstubAllGlobals(); vi.stubEnv('VITE_TARGET', 'web');
});

it('shows nothing while the server has not enabled slicer links', async () => {
  server({ slicerLinks: false }); await show();
  expect(screen.queryByRole('button')).toBeNull();
});

it('shows nothing before sign-in or while a password change is pending', async () => {
  server({ authenticated: false, csrfToken: null, slicerLinks: true }); await show();
  expect(screen.queryByRole('button')).toBeNull();
  cleanup();
  server({ mustChangePassword: true, slicerLinks: true }); await show();
  expect(screen.queryByRole('button')).toBeNull();
});

it('has no web slicer menu in the desktop build', async () => {
  server({ slicerLinks: true }); vi.stubEnv('VITE_TARGET', 'desktop'); await show();
  expect(screen.queryByRole('button')).toBeNull();
});

it('offers every slicer for 3MF and leaves Bambu Studio out for other formats', async () => {
  server({ slicerLinks: true }); await show('a.3mf'); openMenu();
  expect(names()).toEqual(['OrcaSlicer', 'Bambu Studio', 'Creality Print', 'ElegooSlicer', 'Cura', 'Lychee', 'SuperSlicer']);
  cleanup(); await show('a.stl'); openMenu();
  expect(names()).not.toContain('Bambu Studio');
  expect(names()).toContain('Cura');
});

it('does not offer STEP files to slicers that cannot open them', async () => {
  server({ slicerLinks: true }); await show('a.STEP'); openMenu();
  expect(names()).toEqual(['OrcaSlicer', 'Creality Print', 'ElegooSlicer', 'SuperSlicer']);
  cleanup(); await show('readme.txt');
  expect(screen.queryByRole('button')).toBeNull();
});

it('explains the network and certificate requirements', async () => {
  server({ slicerLinks: true }); await show(); openMenu();
  expect(screen.getByText(`${de.webSlicerHint} ${de.webSlicerNotInstalled}`)).toBeVisible();
  expect(de.webSlicerHint).toContain('gleiches Netzwerk');
  expect(de.webSlicerHint).toContain('selbstsigniertem Zertifikat');
});

it('requests a link with the session token and builds the scheme URL from the page origin', async () => {
  const fetcher = server({ slicerLinks: true }); await show(); openMenu();
  await act(async () => fireEvent.click(screen.getByRole('menuitem', { name: 'OrcaSlicer' })));
  const [url, init] = fetcher.mock.calls.find(([u]) => u !== '/api/session')! as unknown as [string, RequestInit];
  expect(url).toBe('/api/invoke/slicer_link');
  expect(init.method).toBe('POST');
  expect(JSON.parse(init.body as string)).toEqual({ fileId: '42' });
  expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-1');
  expect(launched).toEqual(['orcaslicer://open?file=' + encodeURIComponent(window.location.origin + '/dl/42/1.abc.sig/Wurfel.stl')]);
  expect(screen.queryByRole('menu')).toBeNull();
});

it('uses each slicer scheme', async () => {
  server({ slicerLinks: true }); await show('a.3mf');
  const target = encodeURIComponent(window.location.origin + '/dl/42/1.abc.sig/Wurfel.stl');
  for (const [name, prefix] of [['Bambu Studio', 'bambustudio://open?file='], ['Lychee', 'lycheeslicer://open/'], ['SuperSlicer', 'prusaslicer://open?file=']]) {
    openMenu();
    await act(async () => fireEvent.click(screen.getByRole('menuitem', { name })));
    expect(launched[launched.length - 1]).toBe(prefix + target);
  }
});

it('shows the server message when the link cannot be created and starts nothing', async () => {
  server({ slicerLinks: true }, () => Response.json({ message: 'Datei nicht gefunden', expected: true }, { status: 404 }));
  await show(); openMenu();
  await act(async () => fireEvent.click(screen.getByRole('menuitem', { name: 'OrcaSlicer' })));
  expect(launched).toEqual([]);
  expect(screen.getByRole('alert')).toHaveTextContent(`${de.webSlicerError} Datei nicht gefunden`);
  expect(screen.getByRole('button', { name: /In Slicer öffnen/ })).toBeEnabled();
});

it('shows a network failure as an error', async () => {
  const fetcher = server({ slicerLinks: true }); await show(); openMenu();
  fetcher.mockImplementation(async () => { throw new Error('Offline'); });
  await act(async () => fireEvent.click(screen.getByRole('menuitem', { name: 'OrcaSlicer' })));
  expect(screen.getByRole('alert')).toHaveTextContent('Offline');
  expect(launched).toEqual([]);
});

it('starts nothing when the model changes while the link is being created', async () => {
  let release!: (r: Response) => void;
  server({ slicerLinks: true }, () => { throw new Error('unused'); });
  await show();
  (fetch as ReturnType<typeof vi.fn>).mockImplementation(() => new Promise<Response>(r => { release = r; }));
  openMenu(); fireEvent.click(screen.getByRole('menuitem', { name: 'OrcaSlicer' }));
  cleanup();
  await act(async () => release(Response.json({ path: '/dl/42/x/y.stl', expiresAt: 1, name: 'a' })));
  expect(launched).toEqual([]);
});

it('takes the type from the catalog and uses the name ending only when the type is missing', async () => {
  server({ slicerLinks: true });
  await show('renamed-without-extension', '3mf'); openMenu();
  expect(names()).toContain('Bambu Studio');
  cleanup(); await show('part.3mf', 'stl'); openMenu();
  expect(names()).not.toContain('Bambu Studio');
  cleanup(); await show('part.3mf'); openMenu();
  expect(names()).toContain('Bambu Studio');
  cleanup(); await show('no-extension');
  expect(screen.queryByRole('button')).toBeNull();
});

it('moves through the entries with the arrow keys, Home and End', async () => {
  server({ slicerLinks: true }); await show('a.3mf'); openMenu();
  const items = screen.getAllByRole('menuitem');
  expect(items[0]).toHaveFocus();
  const menu = screen.getByRole('menu');
  fireEvent.keyDown(menu, { key: 'ArrowDown' }); expect(items[1]).toHaveFocus();
  fireEvent.keyDown(menu, { key: 'End' }); expect(items[items.length - 1]).toHaveFocus();
  fireEvent.keyDown(menu, { key: 'ArrowDown' }); expect(items[0]).toHaveFocus();
  fireEvent.keyDown(menu, { key: 'ArrowUp' }); expect(items[items.length - 1]).toHaveFocus();
  fireEvent.keyDown(menu, { key: 'Home' }); expect(items[0]).toHaveFocus();
});

it('closes with Escape, returns focus to the button and exposes its state', async () => {
  server({ slicerLinks: true }); await show();
  const button = screen.getByRole('button', { name: /In Slicer öffnen/ });
  expect(button).toHaveAttribute('aria-haspopup', 'menu');
  expect(button).toHaveAttribute('aria-expanded', 'false');
  fireEvent.click(button);
  expect(button).toHaveAttribute('aria-expanded', 'true');
  expect(button).toHaveAttribute('aria-controls', screen.getByRole('menu').id);
  expect(screen.getByRole('menu')).toHaveAccessibleDescription(`${de.webSlicerHint} ${de.webSlicerNotInstalled}`);
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(screen.queryByRole('menu')).toBeNull();
  expect(button).toHaveFocus();
  expect(button).toHaveAttribute('aria-expanded', 'false');
});

it('says that nothing happens when the slicer is not installed', async () => {
  server({ slicerLinks: true }); await show(); openMenu();
  expect(screen.getByText(/nicht installiert, passiert nichts/)).toBeVisible();
});

it('handles Escape itself: the key stays unhandled for window shortcuts and the focus returns to the button', async () => {
  server({ slicerLinks: true }); await show(); openMenu();
  const reached: boolean[] = [];
  const probe = (event: KeyboardEvent) => reached.push(event.defaultPrevented);
  window.addEventListener('keydown', probe);
  fireEvent.keyDown(screen.getAllByRole('menuitem')[0], { key: 'Escape' });
  window.removeEventListener('keydown', probe);
  expect(screen.queryByRole('menu')).toBeNull();
  expect(reached).toEqual([true]);
  expect(screen.getByRole('button', { name: /In Slicer öffnen/ })).toHaveFocus();
});
