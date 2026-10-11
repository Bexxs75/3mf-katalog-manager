import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { DownloadSlicerView } from './DownloadSlicerView';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const url = 'http://nas:3000/katalog/';
const show = (container = true) => render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>
  <DownloadSlicerView />
</RuntimeEnvironmentProvider></LanguageProvider>);
beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(invoke).mockReset().mockResolvedValue({ url, path: '/katalog/' });
});
it.each([true, false])('shows selectable list link and clipboard result success=%s', async ok => {
  const writeText = ok ? vi.fn().mockResolvedValue(undefined) : vi.fn().mockRejectedValue(new Error('denied'));
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  show();
  expect(await screen.findByText(url)).toHaveStyle({ userSelect: 'text' });
  expect(invoke).toHaveBeenCalledWith('get_catalog_web_link', { fileId: null });
  fireEvent.click(screen.getByRole('button', { name: /Link kopieren/ }));
  expect(writeText).toHaveBeenCalledWith(url);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(ok ? de.externalLinkCopied : de.externalLinkCopyFailed));
});
it('shows the relative path and existing origin instructions', async () => {
  vi.mocked(invoke).mockResolvedValue({ url: null, path: '/katalog/' });
  show();
  expect(await screen.findByText('/katalog/')).toHaveStyle({ userSelect: 'text' });
  expect(screen.getByText(de.catalogWebUnknown)).toBeVisible();
  expect(screen.getByText(de.catalogWebConfigure)).toBeVisible();
  expect(screen.queryByRole('button', { name: /Link kopieren/ })).toBeNull();
});
it('reports loading failure', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('unavailable'));
  show();
  expect(await screen.findByRole('alert')).toHaveTextContent(de.catalogWebFailed);
});
it('cannot expose the view or request a link on desktop', () => {
  show(false);
  expect(screen.queryByRole('main')).toBeNull();
  expect(invoke).not.toHaveBeenCalled();
});
it.each(['Enter', ' '])('uses a native disclosure button for %s activation', async key => {
  show();
  await screen.findByText(url);
  const button = screen.getByRole('button', { name: 'Voraussetzungen' });
  expect(button).toHaveAttribute('aria-expanded', 'false');
  const content = document.getElementById(button.getAttribute('aria-controls')!)!;
  expect(content).not.toBeVisible();
  button.focus();
  fireEvent.keyDown(button, { key });
  // jsdom does not synthesize native keyboard clicks.
  fireEvent.click(button, { detail: 0 });
  expect(button).toHaveFocus();
  expect(button).toHaveAttribute('aria-expanded', 'true');
  expect(content).toBeVisible();
  expect(content).toHaveTextContent('PrusaSlicer');
  fireEvent.click(button, { detail: 0 });
  expect(content).not.toBeVisible();
});
it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('renders complete translated content in %s', async (language, dictionary) => {
  localStorage.setItem('3mf-katalog-language', language);
  show();
  await screen.findByText(url);
  expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(dictionary.railDownloads);
  fireEvent.click(screen.getByRole('button', { name: dictionary.downloadRequirements }));
  const keys = ['downloadIntro', 'downloadSteps', 'downloadStepOpen', 'downloadStepFind', 'downloadStepAction',
    'downloadRequirementInstalled', 'downloadRequirementReachable', 'downloadRequirementCertificate',
    'downloadRequirementBambu', 'downloadRequirementPrusa'] as const;
  for (const key of keys) {
    expect(dictionary[key].trim().length).toBeGreaterThan(0);
    expect(screen.getByText(dictionary[key])).toBeVisible();
  }
  expect(screen.getAllByRole('listitem')).toHaveLength(8);
});
