import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { CatalogWebButton } from './CatalogWebButton';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const url = 'http://nas.lan:3000/katalog/?file=42';
function show(container = true) {
  return render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>
    <CatalogWebButton fileId="42" />
  </RuntimeEnvironmentProvider></LanguageProvider>);
}
beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(invoke).mockReset().mockResolvedValue({ url, path: '/katalog/?file=42' });
});
it.each([true, false])('copies and announces clipboard success=%s with keyboard focus', async ok => {
  const writeText = ok ? vi.fn().mockResolvedValue(undefined) : vi.fn().mockRejectedValue(new Error('denied'));
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  show();
  const opener = screen.getByRole('button', { name: /Im Browser öffnen/ });
  opener.focus(); fireEvent.click(opener);
  const dialog = screen.getByRole('dialog', { name: /Download und Slicer/ });
  expect(dialog).toHaveAttribute('aria-modal', 'true');
  expect(dialog.contains(document.activeElement)).toBe(true);
  expect(await screen.findByText(url)).toHaveStyle({ userSelect: 'text' });
  expect(invoke).toHaveBeenCalledWith('get_catalog_web_link', { fileId: 42 });
  const copy = screen.getByRole('button', { name: /Link kopieren/ });
  fireEvent.click(copy);
  expect(writeText).toHaveBeenCalledWith(url);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(ok ? 'Link kopiert' : /nicht|fehl/i));
  const close = screen.getByRole('button', { name: 'Schließen' });
  close.focus(); fireEvent.keyDown(close, { key: 'Tab' }); expect(copy).toHaveFocus();
  fireEvent.keyDown(copy, { key: 'Tab', shiftKey: true }); expect(close).toHaveFocus();
  fireEvent.keyDown(close, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(opener).toHaveFocus();
});
it('shows only a path and configuration instructions when origin is unknown', async () => {
  vi.mocked(invoke).mockResolvedValue({ url: null, path: '/katalog/?file=42' });
  show(); fireEvent.click(screen.getByRole('button', { name: /Im Browser öffnen/ }));
  expect(await screen.findByText('/katalog/?file=42')).toBeVisible();
  expect(screen.getByText(/http:\/\/dein-server:3000/)).toBeVisible();
  expect(screen.getByText(/KATALOG_PUBLIC_URL/)).toBeVisible();
  expect(screen.queryByRole('button', { name: /Link kopieren/ })).not.toBeInTheDocument();
  expect(screen.queryByText(/Strg\+V/)).not.toBeInTheDocument();
});
it('handles backend failure', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('unavailable'));
  show(); fireEvent.click(screen.getByRole('button', { name: /Im Browser öffnen/ }));
  expect(await screen.findByRole('alert')).toBeVisible();
});
it('does not expose container action on desktop', () => {
  show(false); expect(screen.queryByRole('button')).not.toBeInTheDocument(); expect(invoke).not.toHaveBeenCalled();
});
it.each(['de', 'en', 'es', 'fr'])('has localized dialog text in %s', async language => {
  localStorage.setItem('3mf-katalog-language', language);
  show(); fireEvent.click(screen.getByRole('button'));
  expect(await screen.findByText(url)).toBeVisible();
  expect(screen.getByRole('dialog')).not.toHaveTextContent('catalogWeb');
});
it('tells the user to paste the copied link into a browser tab only when a full link exists', async () => {
  show(); fireEvent.click(screen.getByRole('button', { name: /Im Browser öffnen/ }));
  expect(await screen.findByText(/Strg\+V/)).toBeVisible();
});

it.each([url, null])('shows a clipboard-independent search hint for %s', async address => {
  vi.mocked(invoke).mockResolvedValue({ url: address, path: '/katalog/?file=42' });
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}>
    <CatalogWebButton fileId="42" fileName={'<Würfel>.3mf'} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: /Im Browser öffnen/ }));
  const hint = await screen.findByText(/Klappt das Einfügen nicht/);
  expect(hint).toHaveTextContent(address ? 'http://nas.lan:3000/katalog/' : '/katalog/');
  expect(hint).toHaveTextContent('„<Würfel>.3mf“');
  expect(hint).not.toHaveTextContent('?file=');
});

it.each(['de', 'en', 'es', 'fr'])('translates the search fallback in %s and preserves literal filenames', async language => {
  localStorage.setItem('3mf-katalog-language', language);
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}>
    <CatalogWebButton fileId="42" fileName={'cube $& <test>.3mf'} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
  fireEvent.click(screen.getByRole('button'));
  const hint = await screen.findByText(/cube \$& <test>.3mf/);
  expect(hint).toHaveTextContent('http://nas.lan:3000/katalog/');
  expect(hint).not.toHaveTextContent('catalogWebSearchHint');
});
