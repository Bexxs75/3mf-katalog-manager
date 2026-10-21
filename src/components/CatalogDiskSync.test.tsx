import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { CatalogDiskSync } from './CatalogDiskSync';
import { LanguageProvider } from '../i18n/LanguageContext';
import { diskSyncErrorCodes, type DiskScan } from '../lib/api/diskSync';

import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const empty: DiskScan = { newFiles: [], newFolders: [], missingFiles: [], unreachableRoots: [], skippedPaths: [] };
const changes: DiskScan = {
  ...empty,
  newFiles: [{ path: '/models/new.obj', relativePath: 'new.obj', root: '/models', id: null }],
  missingFiles: [{ path: '/models/gone.stl', relativePath: 'gone.stl', root: '/models', id: '42' }],
};
const renderSync = (onApplied = vi.fn().mockResolvedValue(undefined)) => {
  render(<LanguageProvider><CatalogDiskSync onApplied={onApplied} /></LanguageProvider>);
  return onApplied;
};
beforeEach(() => { vi.clearAllMocks(); localStorage.clear(); });

describe('catalog disk sync', () => {
  it.each([['de', 'Datei', 'Katalogordner'], ['en', 'File', 'Catalog folder'], ['es', 'Archivo', 'Carpeta del catálogo'], ['fr', 'Fichier', 'Dossier du catalogue']])('separates file and catalog folder in %s', async (language, fileLabel, rootLabel) => {
    localStorage.setItem('3mf-katalog-language', language);
    vi.mocked(invoke).mockResolvedValue(changes);
    renderSync();
    fireEvent.keyDown(document, { key: 'F5' });
    const dialog = await screen.findByRole('dialog');
    for (const item of within(dialog).getAllByRole('listitem')) {
      const fileRow = within(item).getByText(fileLabel).parentElement!;
      const rootRow = within(item).getByText(rootLabel).parentElement!;
      expect(fileRow).not.toBe(rootRow);
      expect(fileRow).toHaveTextContent(/new.obj|gone.stl/);
      expect(fileRow).not.toHaveTextContent('/models');
      expect(rootRow).toHaveTextContent('/models');
      expect(item).toHaveAttribute('title');
    }
  });
  it('runs via F5, prevents duplicate scans and reports unchanged catalogs', async () => {
    let resolve!: (scan: DiskScan) => void;
    vi.mocked(invoke).mockReturnValue(new Promise<DiskScan>(r => { resolve = r; }));
    renderSync();
    fireEvent.keyDown(document, { key: 'F5' });
    expect(screen.getByRole('button', { name: 'Gleiche ab…' })).toBeDisabled();
    fireEvent.keyDown(document, { key: 'F5' });
    expect(invoke).toHaveBeenCalledTimes(1);
    await act(async () => resolve(empty));
    expect(screen.getByRole('status')).toHaveTextContent('Alles aktuell');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('button', { name: 'Aktualisieren' })).not.toBeDisabled();
    expect(screen.getByRole('button', { name: 'Aktualisieren' }).title).not.toContain('Noch nie');
  });
  it('exposes bidi controls in disk names while submitting their original paths', async () => {
    const path = '/models/foto\u202Egpj.stl';
    vi.mocked(invoke).mockResolvedValueOnce({ ...empty,
      newFiles: [{ path, relativePath: 'foto\u202Egpj.stl', root: '/models', id: null }],
    }).mockResolvedValueOnce({ fileIds: ['7'], folderIds: [], failedPaths: [], removedCount: 0 });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
    await screen.findByRole('dialog');
    expect(screen.getByText('foto[U+202E]gpj.stl').closest('li')).toHaveAttribute('title', '/models/foto[U+202E]gpj.stl');
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncApply }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', {
      selection: { newPaths: [path], missingIds: [], removeMissing: false },
    }));
    await screen.findByRole('status');
  });
  it('defaults removal to OFF and Later never writes', async () => {
    vi.mocked(invoke).mockResolvedValue(changes);
    const reload = renderSync();
    fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    expect(await screen.findByRole('dialog')).toHaveAccessibleName('Abgleich mit der Festplatte');
    expect(screen.getByRole('checkbox')).not.toBeChecked();
    expect(screen.getByText('gone.stl')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Später' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(1); expect(reload).not.toHaveBeenCalled();
  });
  it.each([false, true])('only applies after confirmation with removeMissing=%s', async remove => {
    vi.mocked(invoke).mockResolvedValueOnce(changes).mockResolvedValueOnce({ fileIds: ['7'], folderIds: ['8'], failedPaths: [], removedCount: remove ? 1 : 0 });
    const reload = renderSync();
    fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    await screen.findByRole('dialog');
    if (remove) fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(screen.getByRole('button', { name: 'Änderungen übernehmen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', { selection: { newPaths: ['/models/new.obj'], missingIds: ['42'], removeMissing: remove } }));
    await waitFor(() => expect(reload).toHaveBeenCalledOnce());
    expect(await screen.findByRole('status')).toHaveTextContent('1 neue Datei');
  });
  it('keeps the success toast when refreshing after apply fails', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(changes).mockResolvedValueOnce({ fileIds: ['7'], folderIds: [], failedPaths: [], removedCount: 0 });
    renderSync(vi.fn().mockRejectedValue(new Error('refresh failed')));
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncApply }));
    expect(await screen.findByRole('status')).toHaveTextContent('1 neue Datei');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });
  it('does not ask to check paths when the error has no paths', async () => {
    vi.mocked(invoke).mockRejectedValue({ code: 'diskSyncFailed', paths: [] });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
    expect(await screen.findByRole('alert')).not.toHaveTextContent('angegebenen Pfade');
  });
  it.each(Object.entries({ de, en, es, fr }))('shows skipped folders even without changes in %s', async (language, dictionary) => {
    localStorage.setItem('3mf-katalog-language', language);
    vi.mocked(invoke).mockResolvedValue({ ...empty, skippedPaths: ['/models/private', '/models/restricted'] });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncRefresh }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(dictionary.diskSyncSkipped.other.replace('{count}', '2'));
    expect(screen.getByText('/models/private')).toBeVisible();
    expect(screen.getByText('/models/restricted')).toBeVisible();
    expect(screen.queryByText(dictionary.diskSyncCurrent)).toBeNull();
    expect(screen.queryByRole('button', { name: dictionary.diskSyncApply })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncClose }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it('shows the missing-only action only while removal is checked', async () => {
    vi.mocked(invoke).mockResolvedValue({ ...empty, missingFiles: changes.missingFiles });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
    await screen.findByRole('dialog');
    expect(screen.queryByRole('button', { name: de.diskSyncRemoveAction })).toBeNull();
    expect(screen.getByRole('button', { name: de.diskSyncClose })).toBeVisible();
    fireEvent.click(screen.getByRole('checkbox'));
    expect(screen.getByRole('button', { name: de.diskSyncRemoveAction })).toBeEnabled();
    fireEvent.click(screen.getByRole('checkbox'));
    expect(screen.queryByRole('button', { name: de.diskSyncRemoveAction })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncClose }));
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it.each(Object.entries({ de, en, es, fr }))('removes missing files without importing anything in %s', async (language, dictionary) => {
    localStorage.setItem('3mf-katalog-language', language);
    vi.mocked(invoke).mockResolvedValueOnce({ ...empty, missingFiles: changes.missingFiles })
      .mockResolvedValueOnce({ fileIds: [], folderIds: [], failedPaths: [], removedCount: 1 });
    const reload = renderSync();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncRefresh }));
    await screen.findByRole('dialog');
    expect(screen.queryByRole('button', { name: dictionary.diskSyncRemoveAction })).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('checkbox'));
    const remove = screen.getByRole('button', { name: dictionary.diskSyncRemoveAction });
    expect(remove).toBeEnabled();
    fireEvent.click(remove);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', {
      selection: { newPaths: [], missingIds: ['42'], removeMissing: true },
    }));
    await waitFor(() => expect(reload).toHaveBeenCalledOnce());
    expect(await screen.findByRole('status')).toHaveTextContent(dictionary.diskSyncRemoved.one.replace('{count}', '1'));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it.each(Object.entries({ de, en, es, fr }).flatMap(([language, dictionary]) =>
    [false, true].map(allFailed => ({ language, dictionary, allFailed }))))('reports unreadable files in $language (all failed: $allFailed)', async ({ language, dictionary, allFailed }) => {
    localStorage.setItem('3mf-katalog-language', language);
    vi.mocked(invoke).mockResolvedValueOnce(changes).mockResolvedValueOnce({
      fileIds: allFailed ? [] : ['7'], folderIds: [], removedCount: 0,
      failedPaths: [{ path: '/models/broken.3mf', reason: 'Invalid archive' }],
    });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncRefresh }));
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncApply }));
    await waitFor(() => expect(screen.queryByRole('button', { name: dictionary.diskSyncApply })).toBeNull());
    const result = screen.getByRole(allFailed ? 'dialog' : 'status');
    expect(result).toHaveTextContent(dictionary.diskSyncUnreadable.one.replace('{count}', '1'));
    expect(result).toHaveTextContent('/models/broken.3mf');
    if (allFailed) {
      expect(screen.queryByRole('status')).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncClose }));
      expect(screen.queryByRole('dialog')).toBeNull();
    } else expect(screen.queryByRole('dialog')).toBeNull();
  });
  it('shows unreachable roots and retries without applying', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...empty, unreachableRoots: ['/models/Eingang'] }).mockResolvedValueOnce(empty);
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('/models/Eingang');
    expect(screen.queryByRole('checkbox')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Erneut versuchen' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Alles aktuell');
    expect(vi.mocked(invoke).mock.calls.every(([command]) => command === 'scan_catalog_disk')).toBe(true);
  });
  it('shows backend errors and Escape restores focus without applying', async () => {
    vi.mocked(invoke).mockRejectedValue({ code: 'diskSyncFailed', paths: [] });
    renderSync();
    const button = screen.getByRole('button', { name: 'Aktualisieren' }); button.focus();
    fireEvent.click(button);
    expect(await screen.findByRole('alert')).toHaveTextContent('Der Abgleich ist fehlgeschlagen.');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull(); expect(button).toHaveFocus();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it.each(Object.entries({ de, en, es, fr }).flatMap(([language, dictionary]) =>
    diskSyncErrorCodes.map(code => ({ language, dictionary, code }))))('translates $code in $language on apply', async ({ language, dictionary, code }) => {
    localStorage.setItem('3mf-katalog-language', language);
    vi.mocked(invoke).mockResolvedValueOnce(changes).mockRejectedValueOnce({ code, paths: ['/models/problem.obj'] });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncRefresh }));
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskSyncApply }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(dictionary[code]);
    expect(screen.getByRole('listitem')).toHaveTextContent('/models/problem.obj');
    expect(alert).not.toHaveTextContent(code);
  });
  it('translates scan errors and hides untranslated internal errors', async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ code: 'diskSyncStale', paths: [] }).mockRejectedValueOnce({ message: 'Interner deutscher Fehler', expected: false });
    renderSync();
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.diskSyncStale);
    fireEvent.click(screen.getByRole('button', { name: de.diskSyncRetry }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(de.diskSyncFailed));
    expect(screen.getByRole('alert')).not.toHaveTextContent('Interner deutscher Fehler');
  });
  it('traps keyboard focus in the result dialog', async () => {
    vi.mocked(invoke).mockResolvedValue(changes); renderSync();
    fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    await screen.findByRole('dialog');
    const apply = screen.getByRole('button', { name: 'Änderungen übernehmen' }); apply.focus();
    fireEvent.keyDown(document, { key: 'Tab' }); expect(screen.getByRole('checkbox')).toHaveFocus();
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true }); expect(apply).toHaveFocus();
  });
  it('does not scan during an import or outside the catalog area', () => {
    const { rerender } = render(<LanguageProvider><ImportLockContext.Provider value={true}><CatalogDiskSync /></ImportLockContext.Provider></LanguageProvider>);
    expect(screen.getByRole('button', { name: 'Aktualisieren' })).toBeDisabled();
    fireEvent.keyDown(document, { key: 'F5' });
    expect(invoke).not.toHaveBeenCalled();
    rerender(<LanguageProvider><CatalogDiskSync active={false} /></LanguageProvider>);
    expect(screen.queryByRole('button', { name: 'Aktualisieren' })).toBeNull();
    fireEvent.keyDown(document, { key: 'F5' });
    expect(invoke).not.toHaveBeenCalled();
  });

});
