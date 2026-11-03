import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import type { ModelFile } from '../types';
import { de } from '../i18n/de';

const { rescan, invalidate } = vi.hoisted(() => ({ rescan: vi.fn(), invalidate: vi.fn() }));
vi.mock('../lib/api/files', () => ({ rescanFileMetadata: rescan, listFilesByIds: vi.fn() }));
vi.mock('../hooks/useModelImages', () => ({ invalidateModelImages: invalidate }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(async () => {}), warn: vi.fn(async () => {}), info: vi.fn(async () => {}), debug: vi.fn(async () => {}), trace: vi.fn(async () => {}), attachConsole: vi.fn(async () => () => {}) }));

import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { PrintDataRereadProvider } from '../hooks/usePrintDataReread';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { PrintDataRereadSettings } from './PrintDataRereadSettings';
import { BulkActionToolbar } from './BulkActionToolbar';
import { openBulkMore } from '../test/bulkBar';

function model(id: string, over: Partial<ModelFile> = {}): ModelFile {
  return {
    id, name: id, path: `/m/${id}.3mf`, weightSource: 'slicer', deletedAt: null, printInfo: null, printDataState: 'unread', summaryOnly: true,
    materials: [], tags: [],
    ...over,
  } as unknown as ModelFile;
}
const answer = (id: string, over: Partial<ModelFile> = {}) => model(id, {
  printDataState: undefined, summaryOnly: undefined,
  printInfo: { sliceState: 'sliced', settingsState: 'read', readerCurrent: true, readAt: 'x', printerModel: 'P', bedType: 'B', layerHeightMm: 0.2,
    nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 55, mixed: false }, nozzleTempC: { value: 220, mixed: false }, plates: [], filamentColors: [] },
  ...over,
});

/** One pending read per call, finished from the test. */
function pendingReads() {
  const waiting: { id: string; finish: (error?: unknown) => void }[] = [];
  rescan.mockImplementation((id: string) => new Promise<ModelFile>((resolve, reject) => { waiting.push({ id, finish: error => error ? reject(error) : resolve(answer(id)) }); }));
  const next = async () => { await waitFor(() => expect(waiting.length).toBeGreaterThan(0)); return waiting.shift()!; };
  return { next, finishNext: async (error?: unknown) => { const read = await next(); await act(async () => { read.finish(error); }); } };
}

function Harness({ initial, locked = false, bar = false }: { initial: ModelFile[]; locked?: boolean; bar?: boolean }) {
  const [models, setModels] = useState(initial);
  return (
    <LanguageProvider>
      <ImportLockContext.Provider value={locked}>
        <PrintDataRereadProvider models={models} setModels={setModels}>
          <PrintDataRereadSettings />
          {bar && <BulkActionToolbar {...barProps(['a', 'b'])} />}
          <output data-testid="states">{models.map(m => m.printDataState).join(',')}</output>
        </PrintDataRereadProvider>
      </ImportLockContext.Provider>
    </LanguageProvider>
  );
}
function barProps(selectedIds: string[]) {
  return {
    selectedCount: selectedIds.length, selectedIds, confirmBulkDelete: false, onConfirmBulkDeleteChange: vi.fn(), onSelectAllVisible: vi.fn(), onClearSelection: vi.fn(),
    onBulkAddToQueue: vi.fn(), addToCollectionMenuOpen: false, onAddToCollectionMenuOpenChange: vi.fn(), collections: [], onBulkAddToCollection: vi.fn(),
    activeCollection: null, onBulkRemoveFromCollection: vi.fn(), onBulkSetPrintStatus: vi.fn(), onBulkDelete: vi.fn(), addTagMenuOpen: false,
    onAddTagMenuOpenChange: vi.fn(), allTags: [], tagsInAllSelection: [], tagDraft: '', onTagDraftChange: vi.fn(), onSubmitBulkAddTag: vi.fn(),
    removeTagMenuOpen: false, onRemoveTagMenuOpenChange: vi.fn(), tagsInSelection: [], onBulkRemoveTag: vi.fn(),
  };
}

const startButton = () => screen.getByRole('button', { name: de.printDataRereadAll });
const files = (n: number) => Array.from({ length: n }, (_, i) => model(`f${i}`));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); rescan.mockReset(); invalidate.mockReset(); });
afterEach(cleanup);

describe('settings block', () => {
  it('shows the title, the explanation and the start button', () => {
    render(<Harness initial={files(2)} />);
    expect(screen.getByText(de.printDataRereadIntro)).toBeInTheDocument();
    expect(startButton()).toBeEnabled();
    expect(screen.queryByRole('progressbar')).toBeNull();
  });

  it('runs: progress bar with values, "File n of total", cancel button focused, list state updated', async () => {
    const reads = pendingReads();
    render(<Harness initial={files(3)} />);
    fireEvent.click(startButton());
    const bar = await screen.findByRole('progressbar', { name: de.printDataRereadTitle });
    expect(bar).toHaveAttribute('aria-valuemin', '0');
    expect(bar).toHaveAttribute('aria-valuemax', '3');
    expect(bar).toHaveAttribute('aria-valuenow', '0');
    expect(screen.getByText('Datei 1 von 3')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.cancel })).toHaveFocus();
    expect(startButton()).toBeDisabled();
    expect(startButton()).toHaveAttribute('title', de.printDataRereadBusy);
    await reads.finishNext();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '1');
    expect(screen.getByText('Datei 2 von 3')).toBeInTheDocument();
    await reads.finishNext(); await reads.finishNext();
    expect(await screen.findByText('3 Dateien gelesen.')).toBeInTheDocument();
    expect(screen.getByTestId('states')).toHaveTextContent('sliced,sliced,sliced');
    expect(screen.queryByRole('progressbar')).toBeNull();
  });

  it('reads every unread 3MF, also one without a slicer weight, and leaves the read ones alone', async () => {
    const reads = pendingReads();
    render(<Harness initial={[model('a'), model('b', { weightSource: 'estimated' }), model('c', { printDataState: 'sliced' })]} />);
    fireEvent.click(startButton());
    expect(await screen.findByText('Datei 1 von 2')).toBeInTheDocument();
    await reads.finishNext(); await reads.finishNext();
    expect(await screen.findByText('2 Dateien gelesen.')).toBeInTheDocument();
    expect(rescan.mock.calls.map(call => call[0])).toEqual(['a', 'b']);
  });

  it('puts the focus back on the start button after the end, with a status text', async () => {
    const reads = pendingReads();
    render(<Harness initial={files(1)} />);
    fireEvent.click(startButton());
    await reads.finishNext();
    const status = await screen.findByText('1 Datei gelesen.');
    expect(status.closest('[role="status"]')).not.toBeNull();
    await waitFor(() => expect(startButton()).toHaveFocus());
  });

  it('announces the progress only every 10 files and does not announce the bar itself', async () => {
    const reads = pendingReads();
    const { container } = render(<Harness initial={files(12)} />);
    fireEvent.click(startButton());
    const live = () => container.querySelector('[aria-live="polite"]')!;
    expect(live()).toHaveTextContent('');
    for (let i = 0; i < 9; i++) await reads.finishNext();
    expect(live().textContent).toBe('');
    await reads.finishNext();
    await waitFor(() => expect(live().textContent).toBe('Datei 10 von 12'));
    await reads.finishNext();
    expect(live().textContent).toBe('Datei 10 von 12');
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-live');
  });

  it('cancel: says it ends after the current file, then reports the files read so far', async () => {
    const reads = pendingReads();
    render(<Harness initial={files(3)} />);
    fireEvent.click(startButton());
    await reads.next().then(read => act(async () => { read.finish(); }));
    fireEvent.click(await screen.findByRole('button', { name: de.cancel }));
    expect(screen.getAllByText(de.printDataRereadCancelling).length).toBeGreaterThan(0);
    await reads.finishNext();
    expect(await screen.findByText('Abgebrochen nach 2 von 3 Dateien. Bereits gelesene Werte bleiben erhalten.')).toBeInTheDocument();
    expect(rescan).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('states')).toHaveTextContent('sliced,sliced,unread');
    await waitFor(() => expect(startButton()).toHaveFocus());
  });

  it('shows "paused" while an import blocks the write and goes on afterwards', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      let blocked = true;
      rescan.mockImplementation(async (id: string) => { if (blocked) throw { message: 'importActive', expected: true }; return answer(id); });
      render(<Harness initial={files(1)} />);
      fireEvent.click(startButton());
      expect((await screen.findAllByText(de.printDataRereadStoppedImport)).length).toBeGreaterThan(0);
      blocked = false;
      await act(async () => { await vi.advanceTimersByTimeAsync(2100); });
      expect(await screen.findByText('1 Datei gelesen.')).toBeInTheDocument();
    } finally { vi.useRealTimers(); }
  });

  it('reports failures at the end with the translated reason', async () => {
    const reads = pendingReads();
    render(<Harness initial={files(2)} />);
    fireEvent.click(startButton());
    await reads.finishNext({ message: 'rescan_file_invalid', expected: true });
    await reads.finishNext();
    expect(await screen.findByText(/1 Datei konnte nicht gelesen werden\./)).toHaveTextContent(de.rescanFileInvalid);
    expect(screen.getByText('1 Datei gelesen.')).toBeInTheDocument();
  });

  it('says "no files" when nothing is due', () => {
    render(<Harness initial={[model('x', { printDataState: 'sliced' })]} />);
    fireEvent.click(startButton());
    expect(screen.getByText(de.printDataRereadNothing)).toBeInTheDocument();
    expect(rescan).not.toHaveBeenCalled();
  });

  it('blocks the start while an import runs and names the reason', () => {
    render(<Harness initial={files(1)} locked />);
    expect(startButton()).toBeDisabled();
    expect(startButton()).toHaveAttribute('title', de.importLockedHint);
    expect(startButton()).toHaveAccessibleDescription(de.importLockedHint);
  });
});

describe('selection bar', () => {
  it('has the menu entry, reads only the chosen files and shows the progress in the bar', async () => {
    const reads = pendingReads();
    render(<Harness initial={[...files(3).map((m, i) => ({ ...m, id: ['a', 'b', 'c'][i] })), model('x', { printDataState: 'sliced' })]} bar />);
    const menu = openBulkMore();
    fireEvent.click(within(menu).getByRole('menuitem', { name: de.printDataRereadSelected }));
    const toolbar = screen.getByRole('toolbar');
    expect(await within(toolbar).findByRole('progressbar')).toHaveAttribute('aria-valuemax', '2');
    expect(within(toolbar).getByText('Datei 1 von 2')).toBeInTheDocument();
    // The settings block did not start: its own area shows no bar.
    expect(screen.getAllByRole('progressbar')).toHaveLength(1);
    expect(screen.getByRole('button', { name: de.cancel })).toHaveFocus();
    await reads.finishNext(); await reads.finishNext();
    expect(await within(toolbar).findByText('2 Dateien gelesen.')).toBeInTheDocument();
    expect(rescan.mock.calls.map(call => call[0])).toEqual(['a', 'b']);
    await waitFor(() => expect(within(toolbar).getByRole('button', { name: /^Mehr/ })).toHaveFocus());
  });

  it('disables the entry while a run is going and shares the lock with the settings block', async () => {
    pendingReads();
    render(<Harness initial={files(2).map((m, i) => ({ ...m, id: ['a', 'b'][i] }))} bar />);
    fireEvent.click(startButton());
    await screen.findByRole('progressbar');
    const entry = within(openBulkMore()).getByRole('menuitem', { name: de.printDataRereadSelected });
    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute('title', de.printDataRereadBusy);
  });

  it('blocks the entry while an import runs', () => {
    render(<Harness initial={files(2)} locked bar />);
    expect(within(openBulkMore()).getByRole('menuitem', { name: de.printDataRereadSelected })).toBeDisabled();
  });
});
