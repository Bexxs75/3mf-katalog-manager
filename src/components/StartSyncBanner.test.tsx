import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { StartSyncBanner } from './StartSyncBanner';
import { startSyncNotice } from '../hooks/useStartSync';
import type { StartSyncResult } from '../lib/api/diskSync';

const result = (over: Partial<StartSyncResult> = {}): StartSyncResult => ({
  ran: true, busy: false, newFiles: 0, renamed: 0, missingFiles: 0, missingFolders: 0, duplicates: 0, unreachableRoots: 0, applied: null, ...over,
});
const show = (r: StartSyncResult, language = 'de') => {
  localStorage.setItem('3mf-katalog-language', language);
  const onView = vi.fn(), onClose = vi.fn();
  const { container } = render(<LanguageProvider><StartSyncBanner notice={startSyncNotice(r)} onView={onView} onClose={onClose} /></LanguageProvider>);
  return { onView, onClose, container };
};
beforeEach(() => localStorage.clear());

it('names new, renamed and missing files and leaves out zero parts', () => {
  show(result({ newFiles: 3, renamed: 1, missingFiles: 2 }));
  expect(screen.getByRole('status')).toHaveTextContent('Beim Start gefunden: 3 neue Dateien, 1 umbenannt/verschoben, 2 Dateien fehlen.');
});
it('leaves out parts that are zero', () => {
  show(result({ missingFiles: 1 }));
  expect(screen.getByRole('status')).toHaveTextContent(/^Beim Start gefunden: 1 Datei fehlt\.AnsehenSpäter$/);
});
it('shows nothing without changes, when nothing ran or when the catalog was busy', () => {
  for (const r of [result(), result({ ran: false }), result({ ran: false, busy: true }), result({ applied: { added: 0, renamed: 0, failed: 0 } })]) {
    const { container } = show(r);
    expect(container).toBeEmptyDOMElement();
    document.body.innerHTML = '';
  }
});
it('reports what was taken over and what stays open, without the zero parts', () => {
  show(result({ applied: { added: 3, renamed: 1, failed: 0 }, missingFiles: 2, duplicates: 1 }));
  expect(screen.getByRole('status')).toHaveTextContent('Beim Start aufgenommen: 3 neue Dateien, 1 Umbenennung. Weiter offen: 2 Dateien fehlen, 1 mit gleichem Inhalt.');
});
it('says nothing about the open part when nothing is open, and reports failures', () => {
  show(result({ applied: { added: 2, renamed: 0, failed: 3 } }));
  const text = screen.getByRole('status').textContent!;
  expect(text).toContain('Beim Start aufgenommen: 2 neue Dateien. 3 Dateien konnten nicht aufgenommen werden.');
  expect(text).not.toContain('Weiter offen');
});
it('shows nothing when everything was taken over without anything left', () => {
  const { container } = show(result({ applied: { added: 0, renamed: 0, failed: 0 } }));
  expect(container).toBeEmptyDOMElement();
});
it('reports unreachable folders instead of missing files and offers only to close', () => {
  show(result({ unreachableRoots: 2, missingFiles: 5, newFiles: 4 }));
  expect(screen.getByRole('status')).toHaveTextContent('2 Katalogordner nicht erreichbar');
  expect(screen.getByRole('status')).not.toHaveTextContent('fehl');
  expect(screen.queryByRole('button', { name: 'Ansehen' })).toBeNull();
  expect(screen.getByRole('button', { name: 'Schließen' })).toBeTruthy();
});
it('offers View and Later before a decision and View and Close after an automatic take-over', () => {
  const asking = show(result({ newFiles: 1 }));
  fireEvent.click(screen.getByRole('button', { name: 'Ansehen' }));
  fireEvent.click(screen.getByRole('button', { name: 'Später' }));
  expect(asking.onView).toHaveBeenCalledTimes(1);
  expect(asking.onClose).toHaveBeenCalledTimes(1);
  document.body.innerHTML = '';
  show(result({ applied: { added: 1, renamed: 0, failed: 0 }, missingFiles: 1 }));
  expect(screen.getByRole('button', { name: 'Ansehen' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Schließen' })).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Später' })).toBeNull();
});
it.each([
  ['en', 'Found at start: 3 new files, 1 renamed/moved, 2 missing files.', 'View'],
  ['es', 'Encontrado al iniciar: 3 archivos nuevos', 'Ver'],
  ['fr', 'Trouvé au démarrage : 3 nouveaux fichiers', 'Afficher'],
])('is translated in %s', (language, text, view) => {
  show(result({ newFiles: 3, renamed: 1, missingFiles: 2 }), language);
  expect(screen.getByRole('status').textContent).toContain(text);
  expect(screen.getByRole('button', { name: view })).toBeTruthy();
});
