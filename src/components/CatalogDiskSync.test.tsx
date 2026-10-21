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
const empty: DiskScan = { newFiles: [], newFolders: [], renamedFiles: [], duplicateFiles: [], missingFiles: [], missingFolders: [], unreachableRoots: [], skippedPaths: [] };
const changes: DiskScan = {
  ...empty,
  newFiles: [{ path: '/models/new.obj', relativePath: 'new.obj', root: '/models', id: null }],
  missingFiles: [{ path: '/models/gone.stl', relativePath: 'gone.stl', root: '/models', id: '42' }],
};
const renderSync = (onApplied = vi.fn().mockResolvedValue(undefined)) => {
  render(<LanguageProvider><CatalogDiskSync onApplied={onApplied} /></LanguageProvider>);
  return onApplied;
};
const removeBox = (dictionary: { diskSyncRemove: string } = de) => screen.getByRole('checkbox', { name: dictionary.diskSyncRemove });
beforeEach(() => { vi.resetAllMocks(); localStorage.clear(); });

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
      selection: { newPaths: [path], missingIds: [], removeMissing: false, renames: [] },
    }));
    await screen.findByRole('status');
  });
  it('defaults removal to OFF and Later never writes', async () => {
    vi.mocked(invoke).mockResolvedValue(changes);
    const reload = renderSync();
    fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    expect(await screen.findByRole('dialog')).toHaveAccessibleName('Abgleich mit der Festplatte');
    expect(removeBox()).not.toBeChecked();
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
    if (remove) fireEvent.click(removeBox());
    fireEvent.click(screen.getByRole('button', { name: 'Änderungen übernehmen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', { selection: { newPaths: ['/models/new.obj'], missingIds: ['42'], removeMissing: remove, renames: [] } }));
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
    fireEvent.click(removeBox());
    expect(screen.getByRole('button', { name: de.diskSyncRemoveAction })).toBeEnabled();
    fireEvent.click(removeBox());
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
    fireEvent.click(removeBox(dictionary));
    const remove = screen.getByRole('button', { name: dictionary.diskSyncRemoveAction });
    expect(remove).toBeEnabled();
    fireEvent.click(remove);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', {
      selection: { newPaths: [], missingIds: ['42'], removeMissing: true, renames: [] },
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
    fireEvent.keyDown(document, { key: 'Tab' }); expect(screen.getByRole('checkbox', { name: 'new.obj' })).toHaveFocus();
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
  describe('renamed files and copies of existing models', () => {
    const grouped: DiskScan = {
      ...empty,
      renamedFiles: [
        { id: '1', oldPath: '/models/Drucker-Teile/Halter.3mf', oldRelativePath: 'Drucker-Teile/Halter.3mf', oldRoot: '/models', newPath: '/models/Druckerteile/Halter.3mf', newRelativePath: 'Druckerteile/Halter.3mf', root: '/models' },
        { id: '2', oldPath: '/models/Deko/Vase.stl', oldRelativePath: 'Deko/Vase.stl', oldRoot: '/models', newPath: '/models/Deko/Vase_v2.stl', newRelativePath: 'Deko/Vase_v2.stl', root: '/models' },
      ],
      newFiles: [{ path: '/models/Deko/Schale.3mf', relativePath: 'Deko/Schale.3mf', root: '/models', id: null }],
      duplicateFiles: [{ path: '/models/Deko/Vase - Kopie.3mf', relativePath: 'Deko/Vase - Kopie.3mf', root: '/models', id: null, duplicateOfName: 'Vase.3mf', duplicateOfPath: '/models/Deko/Vase.3mf' }],
    };
    const open = async (scan: DiskScan = grouped) => {
      vi.mocked(invoke).mockResolvedValueOnce(scan);
      const reload = renderSync();
      fireEvent.click(screen.getByRole('button', { name: de.diskSyncRefresh }));
      return { reload, dialog: await screen.findByRole('dialog') };
    };
    const box = (name: string) => screen.getByRole('checkbox', { name });
    const applied = { fileIds: [], folderIds: [], renamedIds: ['1', '2'], renamedCount: 2, removedCount: 0, failedPaths: [] };

    it('shows the groups, the old and new path and the copy hint', async () => {
      const { dialog } = await open();
      for (const heading of [de.diskSyncRenamedHeading, de.diskSyncNewHeading, de.diskSyncDuplicateHeading]) expect(within(dialog).getByRole('heading', { name: new RegExp(heading) })).toBeVisible();
      expect(dialog).toHaveTextContent('2 umbenannt/verschoben');
      expect(dialog).toHaveTextContent('1 mit gleichem Inhalt');
      const halter = box('Drucker-Teile/Halter.3mf → Druckerteile/Halter.3mf').closest('li')!;
      expect(halter.querySelector('s')).toHaveTextContent('Drucker-Teile/Halter.3mf');
      expect(halter).toHaveTextContent('Druckerteile/Halter.3mf');
      expect(halter).toHaveTextContent('Ordner „Drucker-Teile“ → „Druckerteile“ · Tags und Favorit bleiben erhalten');
      expect(box('Deko/Vase.stl → Deko/Vase_v2.stl').closest('li')).toHaveTextContent('Datei umbenannt · Tags und Favorit bleiben erhalten');
      expect(box('Deko/Vase - Kopie.3mf').closest('li')).toHaveTextContent('Gleicher Inhalt wie „/models/Deko/Vase.3mf“ (schon im Katalog)');
      expect(dialog).toHaveTextContent(de.diskSyncRenamedHint);
      expect(dialog).toHaveTextContent(de.diskSyncDuplicateHint);
    });
    it('preselects renamed and new files but not copies', async () => {
      await open();
      expect(box('Drucker-Teile/Halter.3mf → Druckerteile/Halter.3mf')).toBeChecked();
      expect(box('Deko/Vase.stl → Deko/Vase_v2.stl')).toBeChecked();
      expect(box('Deko/Schale.3mf')).toBeChecked();
      expect(box('Deko/Vase - Kopie.3mf')).not.toBeChecked();
    });
    it('sends only what is ticked', async () => {
      const { reload } = await open();
      vi.mocked(invoke).mockResolvedValueOnce(applied);
      fireEvent.click(box('Deko/Vase.stl → Deko/Vase_v2.stl'));
      fireEvent.click(box('Deko/Schale.3mf'));
      fireEvent.click(box('Deko/Vase - Kopie.3mf'));
      fireEvent.click(screen.getByRole('button', { name: de.diskSyncApply }));
      await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', { selection: {
        newPaths: ['/models/Deko/Vase - Kopie.3mf'], missingIds: [], removeMissing: false,
        renames: [{ id: '1', newPath: '/models/Druckerteile/Halter.3mf' }],
      } }));
      await waitFor(() => expect(reload).toHaveBeenCalledOnce());
      expect(await screen.findByRole('status')).toHaveTextContent('2 umbenannt/verschoben');
    });
    it('sends the preselection unchanged when nothing is touched', async () => {
      await open();
      vi.mocked(invoke).mockResolvedValueOnce(applied);
      fireEvent.click(screen.getByRole('button', { name: de.diskSyncApply }));
      await waitFor(() => expect(invoke).toHaveBeenCalledWith('apply_catalog_disk_sync', { selection: {
        newPaths: ['/models/Deko/Schale.3mf'], missingIds: [], removeMissing: false,
        renames: [{ id: '1', newPath: '/models/Druckerteile/Halter.3mf' }, { id: '2', newPath: '/models/Deko/Vase_v2.stl' }],
      } }));
    });
    it('can be operated with the keyboard and keeps focus on the checkbox', async () => {
      await open();
      const copy = box('Deko/Vase - Kopie.3mf');
      expect(copy.tagName).toBe('INPUT'); expect(copy).toHaveAttribute('type', 'checkbox');
      copy.focus();
      // Space on a focused checkbox is a click.
      fireEvent.click(copy);
      expect(copy).toBeChecked(); expect(copy).toHaveFocus();
      fireEvent.click(copy);
      expect(copy).not.toBeChecked(); expect(copy).toHaveFocus();
    });
    it('disables the apply button when everything is unticked', async () => {
      await open();
      for (const name of ['Drucker-Teile/Halter.3mf → Druckerteile/Halter.3mf', 'Deko/Vase.stl → Deko/Vase_v2.stl', 'Deko/Schale.3mf']) fireEvent.click(box(name));
      expect(screen.getByRole('button', { name: de.diskSyncApply })).toBeDisabled();
      fireEvent.click(box('Deko/Vase - Kopie.3mf'));
      expect(screen.getByRole('button', { name: de.diskSyncApply })).toBeEnabled();
    });
    it('offers a scan that only contains renames', async () => {
      await open({ ...empty, renamedFiles: grouped.renamedFiles });
      expect(screen.queryByRole('heading', { name: de.diskSyncNewHeading })).toBeNull();
      expect(screen.getByRole('button', { name: de.diskSyncApply })).toBeEnabled();
    });
    it.each(Object.entries({ de, en, es, fr }))('has the texts for the new groups in %s', (_language, dictionary) => {
      const keys = ['diskSyncRenamedHeading', 'diskSyncDuplicateHeading', 'diskSyncPreselected', 'diskSyncNotPreselected', 'diskSyncRenamedHint', 'diskSyncDuplicateHint', 'diskSyncDuplicateOf', 'diskSyncRenamedFolderSub', 'diskSyncRenamedFileSub', 'diskSyncKeepsData', 'diskSyncTopLevel'] as const;
      for (const key of keys) expect(dictionary[key].trim()).not.toBe('');
      expect(dictionary.diskSyncDuplicateOf).toContain('{name}');
      expect(dictionary.diskSyncRenamedFolderSub).toContain('{from}');
      expect(dictionary.diskSyncRenamedFolderSub).toContain('{to}');
      for (const key of ['diskSyncRenamed', 'diskSyncDuplicates'] as const) { expect(dictionary[key].one).toContain('{count}'); expect(dictionary[key].other).toContain('{count}'); }
    });
  });

});

describe('request from outside (start notice)', () => {
  it('opens the same scan and dialog as the button, once, and not under another dialog', async () => {
    const { requestDiskSync } = await import('../lib/api/diskSync');
    vi.mocked(invoke).mockResolvedValue(changes);
    renderSync();
    act(() => requestDiskSync());
    expect(await screen.findByRole('dialog')).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith('scan_catalog_disk');
    act(() => requestDiskSync());
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'scan_catalog_disk')).toHaveLength(1);
  });
  it('stops listening when the component is gone', async () => {
    const { requestDiskSync } = await import('../lib/api/diskSync');
    vi.mocked(invoke).mockResolvedValue(changes);
    const view = render(<LanguageProvider><CatalogDiskSync /></LanguageProvider>);
    view.unmount();
    act(() => requestDiskSync());
    expect(invoke).not.toHaveBeenCalled();
  });
});
