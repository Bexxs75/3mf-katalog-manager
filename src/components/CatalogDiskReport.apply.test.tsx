import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import type { DiskBlock, DiskReport, DiskSyncPreview } from '../lib/api/diskSync';

const api = vi.hoisted(() => ({ report: vi.fn(), preview: vi.fn(), start: vi.fn(), block: vi.fn() }));
vi.mock('../lib/api/diskSync', async importOriginal => ({
  ...await importOriginal<typeof import('../lib/api/diskSync')>(),
  diskSyncReport: api.report, diskSyncPreview: api.preview, diskSyncApplyStart: api.start, diskSyncApplyBlock: api.block,
}));
vi.mock('../lib/webSession', async importOriginal => ({ ...await importOriginal<typeof import('../lib/webSession')>(), useCanManageFiles: () => true }));

import { CatalogDiskReport } from './CatalogDiskReport';
import { diskSyncRetry, subscribeFileOpsRefresh } from '../lib/fileOps';

const report: DiskReport = {
  roots: [{ name: 'models', status: 'ok', mostlyMissing: false }],
  missingModels: [{ id: '12', root: 0, relativePath: 'a/gone.3mf' }],
  newFiles: [{ root: 0, relativePath: 'b/new.3mf' }, { root: 0, relativePath: 'b/other.stl' }],
  danglingCompanions: [
    { id: '5', fileId: '1', name: 'lost.pdf', root: 0, relativePath: 'a/x_extras/lost.pdf', reason: 'missing' },
    { id: '6', fileId: '1', name: 'dir', root: 0, relativePath: 'a/x_extras/dir', reason: 'notFile' },
  ],
  modelsOutsideRoots: 0, companionsOutsideRoots: 0, scannedEntries: 9, truncated: false, truncatedBy: [],
};
const preview = (over: Partial<DiskSyncPreview> = {}): DiskSyncPreview => ({
  token: 'tok', expiresIn: 3600, counts: { import: 2, removeModels: 0, removeCompanions: 1 }, skipped: [], mostlyMissingRoots: [], removes: true, ...over,
});
const block = (over: Partial<DiskBlock> & Pick<DiskBlock, 'seq' | 'remaining'>): DiskBlock => ({
  results: [], done: 0, total: 3, counts: { imported: 0, removedModels: 0, removedCompanions: 0, skipped: 0 }, stopped: null, ...over,
});
const ok = (kind: 'import' | 'removeModel' | 'removeCompanion', key: string, fileId?: string) => ({ kind, key, ok: true, ...(fileId ? { fileId } : {}) });

const show = () => render(<LanguageProvider><CatalogDiskReport /></LanguageProvider>);
const open = async () => { fireEvent.click(screen.getByRole('button', { name: de.diskReportOpen })); return screen.findByRole('dialog'); };
const step = () => screen.getByRole('dialog').getAttribute('data-step');
const press = (name: string | RegExp) => fireEvent.click(screen.getByRole('button', { name }));
const toPreview = async () => { show(); await open(); await screen.findByText('b/new.3mf'); press(/^Weiter/); await waitFor(() => expect(step()).toBe('preview')); };

beforeEach(() => {
  Object.values(api).forEach(fn => fn.mockReset());
  api.report.mockResolvedValue(report);
  diskSyncRetry.delaysMs = [0, 0];
  localStorage.clear();
});
afterEach(() => cleanup());

describe('select step', () => {
  it('preselects new files and companion entries but never a model whose file is gone', async () => {
    show(); const dialog = await open();
    await within(dialog).findByText('b/new.3mf');
    const box = (path: string) => within(within(dialog).getByText(path).closest('label')!).getByRole('checkbox') as HTMLInputElement;
    expect(box('b/new.3mf').checked).toBe(true);
    expect(box('b/other.stl').checked).toBe(true);
    expect(box('a/x_extras/lost.pdf').checked).toBe(true);
    expect(box('a/gone.3mf').checked).toBe(false);
    // a companion row that is not a file is shown but cannot be ticked
    expect(box('a/x_extras/dir').disabled).toBe(true);
    expect(box('a/x_extras/dir').checked).toBe(false);
    expect(within(dialog).getByRole('button', { name: 'Weiter (3 Änderungen)' })).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplyMissingHint)).toBeTruthy();
  });

  it('blocks Weiter at zero and says why', async () => {
    show(); const dialog = await open();
    await within(dialog).findByText('b/new.3mf');
    for (const section of [de.diskSyncNewHeading, de.diskReportCompanionHeading]) fireEvent.click(within(dialog).getByRole('button', { name: de.diskSyncSelectNoneIn.replace('{section}', section) }));
    const next = within(dialog).getByRole('button', { name: de.diskApplyNextPlain }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
    expect(document.getElementById(next.getAttribute('aria-describedby')!)!.textContent).toBe(de.diskApplyBlockedHint);
  });

  it('sends exactly the ticked rows to the preview', async () => {
    api.preview.mockResolvedValue(preview());
    show(); const dialog = await open();
    await within(dialog).findByText('b/new.3mf');
    fireEvent.click(within(within(dialog).getByText('b/other.stl').closest('label')!).getByRole('checkbox'));
    fireEvent.click(within(within(dialog).getByText('a/gone.3mf').closest('label')!).getByRole('checkbox'));
    press(/^Weiter/);
    await waitFor(() => expect(api.preview).toHaveBeenCalledWith({ newFiles: [{ root: 0, relativePath: 'b/new.3mf' }], missingModels: ['12'], danglingCompanions: ['5'] }));
  });

  it('groups models of one missing folder as a folder row that is not preselected', async () => {
    api.report.mockResolvedValue({ ...report, missingModels: [
      { id: '20', root: 0, relativePath: 'Spielzeug/Alt/a.3mf' }, { id: '21', root: 0, relativePath: 'Spielzeug/Alt/b.3mf' }, { id: '12', root: 0, relativePath: 'a/gone.3mf' }] });
    api.preview.mockResolvedValue(preview());
    show(); const dialog = await open();
    const folder = await within(dialog).findByText(de.diskFolderLabel);
    const box = within(folder.closest('label')!).getByRole('checkbox') as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(within(dialog).getByText(de.diskFolderModels.other.replace('{count}', '2'))).toBeTruthy();
    fireEvent.click(box);
    press(/^Weiter/);
    await waitFor(() => expect(api.preview.mock.calls[0][0].missingModels).toEqual(['20', '21']));
  });

  it('shows unclear entries collapsed, read-only, with a chip, and never sends them', async () => {
    api.report.mockResolvedValue({ ...report, unclear: [{ root: 0, relativePath: 'nas/x.3mf', reason: 'unreachable' }, { relativePath: 'y.3mf', reason: 'link' }, { relativePath: 'z.stl', reason: 'read' }] });
    api.preview.mockResolvedValue(preview());
    show(); const dialog = await open();
    const details = (await within(dialog).findByTestId('disk-unclear')) as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(within(details).queryAllByRole('checkbox')).toHaveLength(0);
    expect(within(details).getByText(de.diskUnclearSummary.other.replace('{count}', '3'))).toBeTruthy();
    expect(within(details).getByText(de.diskUnclearTag)).toBeTruthy();
    expect(within(details).getByText(de.diskUnclearWhy_unreachable)).toBeTruthy();
    expect(within(details).getByText(de.diskUnclearWhy_link)).toBeTruthy();
    expect(within(details).getByText(de.diskUnclearWhy_read)).toBeTruthy();
    expect(within(dialog).getByText(de.diskUnclearChip.other.replace('{count}', '3'))).toBeTruthy();
    press(/^Weiter/);
    await waitFor(() => expect(api.preview).toHaveBeenCalled());
    expect(JSON.stringify(api.preview.mock.calls[0][0])).not.toMatch(/nas|y\.3mf|z\.stl/);
  });
});

describe('unclear entries with several folders', () => {
  it('names the folder of an unclear entry', async () => {
    api.report.mockResolvedValue({ ...report, roots: [{ name: 'models', status: 'ok', mostlyMissing: false }, { name: 'nas', status: 'ok', mostlyMissing: false }], unclear: [{ root: 1, relativePath: 'x/y.3mf', reason: 'link' }] });
    show(); const dialog = await open();
    const details = await within(dialog).findByTestId('disk-unclear');
    expect(within(details).getByText('nas: x/y.3mf')).toBeTruthy();
  });
});

describe('preview and apply', () => {
  it('shows the summary with the trash note and no confirmation checkbox', async () => {
    api.preview.mockResolvedValue(preview({ counts: { import: 2, removeModels: 1, removeCompanions: 1 }, skipped: [{ kind: 'new', key: '0:b/new.3mf', reason: 'already_in_catalog' }] as never, mostlyMissingRoots: ['models'] }));
    await toPreview();
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(de.diskApplySummaryImport.other.replace('{count}', '2'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplySummaryRemove.one.replace('{count}', '1'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplyRemoveTrashNote)).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplyBackupNote)).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplyRecheckNote)).toBeTruthy();
    expect(within(dialog).getByText(de.diskReportRootMostlyMissing.replace('{name}', 'models'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskSkip_already_in_catalog)).toBeTruthy();
    expect(within(dialog).queryAllByRole('checkbox')).toHaveLength(0);
    expect((within(dialog).getByRole('button', { name: de.diskSyncApply }) as HTMLButtonElement).disabled).toBe(false);
    press(de.diskApplyBack);
    expect(step()).toBe('select');
  });

  it('blocks applying when the server left nothing in the plan', async () => {
    api.preview.mockResolvedValue(preview({ token: '', counts: { import: 0, removeModels: 0, removeCompanions: 0 }, skipped: [{ kind: 'import', key: '0:b/new.3mf', reason: 'changed' }, { kind: 'import', key: '0:b/other.stl', reason: 'outside' }], removes: false }));
    await toPreview();
    expect((screen.getByRole('button', { name: de.diskSyncApply }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(de.diskSkip_outside)).toBeTruthy();
  });

  it('runs block by block, shows progress, ends with the result and reloads the lists', async () => {
    api.preview.mockResolvedValue(preview({ removes: false, counts: { import: 2, removeModels: 0, removeCompanions: 1 } }));
    api.start.mockResolvedValue({ total: 3, remaining: 3 });
    let release!: () => void;
    api.block.mockImplementationOnce(async () => block({ seq: 0, remaining: 1, done: 2, results: [ok('import', '0:b/new.3mf', '431'), ok('import', '0:b/other.stl', '432')] }));
    api.block.mockImplementationOnce(() => new Promise(resolve => { release = () => resolve(block({ seq: 1, remaining: 0, done: 3, results: [{ kind: 'removeCompanion', key: '5', ok: false, reason: 'exists_again' }] })); }));
    const refresh = vi.fn(); const off = subscribeFileOpsRefresh(refresh);
    await toPreview();
    press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('running'));
    const bar = screen.getByRole('progressbar');
    expect(bar.getAttribute('aria-valuemax')).toBe('3');
    expect(bar.getAttribute('aria-valuenow')).toBe('2');
    expect(screen.getByRole('status').textContent).toBe(de.diskApplyRunning.replace('{done}', '2').replace('{total}', '3'));
    expect(api.start).toHaveBeenCalledWith('tok', false);
    release();
    await waitFor(() => expect(step()).toBe('done'));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(de.diskApplyResult.replace('{added}', '2').replace('{removed}', '0').replace('{companions}', '0'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskApplySkippedHeading.one.replace('{count}', '1'))).toBeTruthy();
    expect(within(dialog).getByText(de.diskSkip_exists_again)).toBeTruthy();
    expect(api.block.mock.calls.map(c => c[1])).toEqual([0, 1]);
    expect(document.activeElement).toBe(within(dialog).getByText(de.diskApplyDone));
    expect(refresh).toHaveBeenCalled(); off();
    press(de.diskApplyRecheck);
    await waitFor(() => expect(api.report).toHaveBeenCalledTimes(2));
  });

  it('cannot be closed while running, and cancel stops after the running block', async () => {
    api.preview.mockResolvedValue(preview({ removes: false }));
    api.start.mockResolvedValue({ total: 30, remaining: 30 });
    let release!: () => void;
    api.block.mockImplementation(() => new Promise(resolve => { release = () => resolve(block({ seq: 0, total: 30, done: 10, remaining: 20, results: [ok('import', '0:b/new.3mf', '1')] })); }));
    await toPreview();
    press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('running'));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeTruthy();
    expect(screen.queryByRole('button', { name: de.diskSyncClose })).toBeNull();
    press(de.diskApplyCancel);
    expect(step()).toBe('cancelling');
    expect(screen.getByRole('status').textContent).toBe(de.diskApplyCancelling);
    release();
    await waitFor(() => expect(step()).toBe('cancelled'));
    expect(api.block).toHaveBeenCalledTimes(1);
    expect(screen.getByText(de.diskApplyCancelled.replace('{done}', '10').replace('{total}', '30'))).toBeTruthy();
  });

  it('repeats a block whose answer was lost with the same seq', async () => {
    api.preview.mockResolvedValue(preview({ removes: false }));
    api.start.mockResolvedValue({ total: 1, remaining: 1 });
    api.block.mockRejectedValueOnce(new TypeError('network'));
    api.block.mockResolvedValueOnce(block({ seq: 0, total: 1, done: 1, remaining: 0, results: [ok('import', '0:b/new.3mf', '1')] }));
    await toPreview(); press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('done'));
    expect(api.block.mock.calls.map(c => c[1])).toEqual([0, 0]);
  });

  it('keeps the count and says so when a backup is restored in the middle of the run', async () => {
    api.preview.mockResolvedValue(preview({ removes: false }));
    api.start.mockResolvedValue({ total: 30, remaining: 30 });
    api.block.mockResolvedValueOnce(block({ seq: 0, total: 30, done: 10, remaining: 20, results: [ok('import', '0:b/new.3mf', '1')] }));
    api.block.mockRejectedValueOnce({ message: 'maintenance' });
    await toPreview(); press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('error'));
    const alert = screen.getByRole('alert');
    expect(within(alert).getByText(de.diskApplyMaintenanceRun)).toBeTruthy();
    expect(alert.textContent).toContain(de.diskApplyAppliedSoFar.replace('{done}', '10').replace('{total}', '30'));
    expect(screen.getByRole('button', { name: de.diskApplyRecheck })).toBeTruthy();
  });

  it('stops on a server-side maintenance stop as well', async () => {
    api.preview.mockResolvedValue(preview({ removes: false }));
    api.start.mockResolvedValue({ total: 30, remaining: 30 });
    api.block.mockResolvedValueOnce(block({ seq: 0, total: 30, done: 10, remaining: 20, stopped: 'maintenance', results: [ok('import', '0:b/new.3mf', '1')] }));
    await toPreview(); press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('error'));
    expect(screen.getByText(de.diskApplyMaintenanceRun)).toBeTruthy();
    expect(api.block).toHaveBeenCalledTimes(1);
  });

  it('shows what was applied when a block fails for another reason', async () => {
    api.preview.mockResolvedValue(preview({ removes: false }));
    api.start.mockResolvedValue({ total: 30, remaining: 30 });
    api.block.mockResolvedValueOnce(block({ seq: 0, total: 30, done: 10, remaining: 20, results: [ok('import', '0:b/new.3mf', '1')] }));
    api.block.mockRejectedValueOnce({ kind: 'snapshot_mismatch', message: 'x' });
    await toPreview(); press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('error'));
    expect(screen.getByText(de.diskApplyStateChanged)).toBeTruthy();
    expect(screen.getByRole('alert').textContent).toContain(de.diskApplyAppliedSoFar.replace('{done}', '10').replace('{total}', '30'));
  });
});

describe('refusals', () => {
  const failPreview = async (reason: unknown) => {
    api.preview.mockRejectedValue(reason);
    show(); await open(); await screen.findByText('b/new.3mf'); press(/^Weiter/);
    await waitFor(() => expect(step()).toBe('error'));
  };
  it('snapshot_not_found: preview expired, check again', async () => {
    await failPreview({ kind: 'snapshot_not_found', message: 'x' });
    expect(screen.getByText(de.diskApplyExpired)).toBeTruthy();
    api.report.mockClear();
    press(de.diskApplyRecheck);
    await waitFor(() => expect(api.report).toHaveBeenCalledTimes(1));
  });
  it('limit_exceeded: daily limit, nothing changed', async () => {
    await failPreview({ kind: 'limit_exceeded', message: 'x' });
    expect(screen.getByText(de.diskApplyLimit)).toBeTruthy();
  });
  it('rate_limited with Retry-After names the wait', async () => {
    await failPreview({ kind: 'rate_limited', message: 'x', retryAfter: '120' });
    expect(screen.getByText(`${de.diskApplyLimit} ${de.diskApplyRetryIn.replace('{wait}', '2 Minuten')}`)).toBeTruthy();
  });
  it('maintenance before the start: nothing changed', async () => {
    await failPreview({ message: 'maintenance' });
    expect(screen.getByText(de.diskApplyMaintenanceStart)).toBeTruthy();
  });
  it('backup_failed at the start: nothing changed', async () => {
    api.preview.mockResolvedValue(preview());
    api.start.mockRejectedValue({ kind: 'backup_failed', message: 'x' });
    await toPreview(); press(de.diskSyncApply);
    await waitFor(() => expect(step()).toBe('error'));
    expect(screen.getByText(de.diskApplyBackupFailed)).toBeTruthy();
    expect(api.block).not.toHaveBeenCalled();
  });
  it('restart_required offers no retry', async () => {
    await failPreview({ kind: 'restart_required', message: 'x' });
    expect(screen.queryByRole('button', { name: de.diskApplyRecheck })).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('Dateiverwaltung gesperrt');
  });
  it('snapshot_capacity and busy show the server-busy text', async () => {
    await failPreview({ kind: 'snapshot_capacity', message: 'x' });
    expect(screen.getByRole('alert').textContent).toContain('ausgelastet');
  });
});

describe('accessibility', () => {
  it('names the dialog, moves the focus to the new title on a step change and marks the scan busy', async () => {
    api.preview.mockResolvedValue(preview());
    await toPreview();
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.getAttribute('aria-label')).toBe(de.diskApplyPreviewTitle);
    expect(document.activeElement).toBe(within(dialog).getByRole('heading', { level: 2 }));
  });
  it('labels each section and each select-all with its section', async () => {
    show(); const dialog = await open(); await within(dialog).findByText('b/new.3mf');
    expect(within(dialog).getByRole('region', { name: de.diskSyncNewHeading })).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: de.diskSyncSelectAllIn.replace('{section}', de.diskSyncMissingHeading) })).toBeTruthy();
  });
});
