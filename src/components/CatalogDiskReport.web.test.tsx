import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import type { DiskReport } from '../lib/api/diskSync';

const api = vi.hoisted(() => ({ report: vi.fn() }));
vi.mock('../lib/api/diskSync', () => ({ diskSyncReport: api.report }));

import { CatalogDiskReport } from './CatalogDiskReport';

const clean: DiskReport = {
  roots: [{ name: 'models', status: 'ok' }], missingModels: [], newFiles: [], danglingCompanions: [],
  modelsOutsideRoots: 0, companionsOutsideRoots: 0, scannedEntries: 3, truncated: false, truncatedBy: [],
};
const findings: DiskReport = {
  ...clean,
  roots: [{ name: 'models', status: 'ok' }, { name: 'archive', status: 'ok' }],
  missingModels: [{ id: '2', root: 0, relativePath: 'a/gone.3mf' }],
  newFiles: [{ root: 1, relativePath: 'b/new.stl' }],
  danglingCompanions: [
    { id: '5', fileId: '1', name: 'lost.pdf', root: 0, relativePath: 'a/x_extras/lost.pdf', reason: 'missing' },
    { id: '6', fileId: '1', name: 'dir', root: 0, relativePath: 'a/x_extras/dir', reason: 'notFile' },
  ],
};
const show = () => render(<LanguageProvider><CatalogDiskReport /></LanguageProvider>);
const open = async () => { fireEvent.click(screen.getByRole('button', { name: de.diskReportOpen })); return screen.findByRole('dialog'); };

beforeEach(() => { api.report.mockReset(); localStorage.clear(); });
afterEach(() => cleanup());

describe('CatalogDiskReport', () => {
  it('checks nothing until the button is pressed, then lists the three findings with their root', async () => {
    api.report.mockResolvedValue(findings);
    show();
    expect(api.report).not.toHaveBeenCalled();
    const dialog = await open();
    expect(await within(dialog).findByText('a/gone.3mf')).toBeTruthy();
    expect(within(dialog).getByText('b/new.stl')).toBeTruthy();
    expect(within(dialog).getByText('a/x_extras/lost.pdf')).toBeTruthy();
    expect(within(dialog).getByText('archive')).toBeTruthy();
    expect(within(dialog).getByText(de.diskReportNotFile)).toBeTruthy();
    expect(within(dialog).getByText(de.diskReportReadOnly)).toBeTruthy();
  });

  it('offers nothing that changes the catalog or the disk', async () => {
    api.report.mockResolvedValue(findings);
    show();
    const dialog = await open();
    await within(dialog).findByText('a/gone.3mf');
    expect(within(dialog).queryAllByRole('checkbox')).toHaveLength(0);
    expect(within(dialog).getAllByRole('button').map(b => b.textContent)).toEqual([de.diskSyncClose]);
  });

  it('says so when everything matches', async () => {
    api.report.mockResolvedValue(clean);
    show();
    const dialog = await open();
    expect(await within(dialog).findByText(de.diskReportAllGood)).toBeTruthy();
  });

  it('warns that a truncated report is incomplete and does not claim a match', async () => {
    api.report.mockResolvedValue({ ...clean, truncated: true, truncatedBy: ['time'] });
    show();
    const dialog = await open();
    expect(await within(dialog).findByText(de.diskReportTruncatedTime)).toBeTruthy();
    expect(within(dialog).queryByText(de.diskReportAllGood)).toBeNull();
  });

  it('names unreachable and empty folders and counts rows outside the folders', async () => {
    api.report.mockResolvedValue({ ...clean, roots: [{ name: 'usb', status: 'unreachable' }, { name: 'nas', status: 'empty' }], modelsOutsideRoots: 2, companionsOutsideRoots: 1 });
    show();
    const dialog = await open();
    expect(await within(dialog).findByText(de.diskReportRootUnreachable.replace('{name}', 'usb'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskReportRootEmpty.replace('{name}', 'nas'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskReportOutside.other.replace('{count}', '3'))).toBeTruthy();
    expect(within(dialog).queryByText(de.diskReportAllGood)).toBeNull();
  });

  it('shows a failure with a retry that asks again', async () => {
    api.report.mockRejectedValueOnce({ message: 'Abgleich läuft bereits', expected: true });
    show();
    const dialog = await open();
    expect(await within(dialog).findByRole('alert')).toBeTruthy();
    api.report.mockResolvedValueOnce(clean);
    fireEvent.click(within(dialog).getByRole('button', { name: de.diskSyncRetry }));
    await waitFor(() => expect(api.report).toHaveBeenCalledTimes(2));
    expect(await within(dialog).findByText(de.diskReportAllGood)).toBeTruthy();
  });

  it('closes the dialog', async () => {
    api.report.mockResolvedValue(clean);
    show();
    const dialog = await open();
    await within(dialog).findByText(de.diskReportAllGood);
    fireEvent.click(within(dialog).getByRole('button', { name: de.diskSyncClose }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it.each([['en', en], ['es', es], ['fr', fr]] as const)('is translated in %s', async (language, dictionary) => {
    localStorage.setItem('3mf-katalog-language', language);
    api.report.mockResolvedValue(findings);
    show();
    fireEvent.click(screen.getByRole('button', { name: dictionary.diskReportOpen }));
    const dialog = await screen.findByRole('dialog', { name: dictionary.diskReportTitle });
    expect(await within(dialog).findByText(dictionary.diskReportMissingHeading)).toBeTruthy();
    expect(within(dialog).getByText(dictionary.diskReportReadOnly)).toBeTruthy();
  });
});
