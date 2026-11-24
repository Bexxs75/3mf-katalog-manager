import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { WebCatalogBackup } from './WebCatalogBackup';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
afterEach(cleanup);
it('lets the download button and the 30 second note wrap instead of clipping', () => {
  render(<LanguageProvider><WebCatalogBackup /></LanguageProvider>);
  const link = screen.getByRole('link', { name: de.exportCatalogWebButton });
  expect(link.className).not.toMatch(/(^|\s)h-7(\s|$)/);
  expect(link.className).toMatch(/min-h-7/);
  fireEvent.click(link);
  const note = screen.getByText(de.catalogBackupWebPause);
  expect(note.className).toMatch(/overflow-wrap:anywhere/);
});
