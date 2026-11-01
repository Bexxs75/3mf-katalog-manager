import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile, makePrintInfo } from '../test/factories';
import { de } from '../i18n/de';
import type { FilamentCheck, ModelFile } from '../types';

const check: { value: Record<string, unknown> } = { value: {} };
vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => check.value }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); check.value = {}; });

const props = {
  model: makeModelFile(), allTags: [], onClose: vi.fn(), onAddTag: vi.fn(), onRemoveTag: vi.fn(),
  onDelete: vi.fn(), onTogglePrintStatus: vi.fn(), onToggleFavorite: vi.fn(), onToggleQueue: vi.fn(),
  onUploadImage: vi.fn(), onSnapshotCaptured: vi.fn(), onSetSourceUrl: vi.fn(), onOpenInSlicer: vi.fn(),
  onRescanMetadata: vi.fn(), onAddToCollection: vi.fn(), collections: [], slicers: [], slicerError: null,
  rescanError: null, rescanSuccess: false, displayPreference: 'thumbnail' as const, hasPrevious: false, hasNext: false,
};
const renderPage = (model: ModelFile, extra: Partial<React.ComponentProps<typeof ModelDetailPage>> = {}) => render(
  <LanguageProvider><ModelDetailPage {...props} model={model} {...extra} /></LanguageProvider>);
const slice = {
  totalWeightG: 52.8,
  plates: [
    { plateIndex: 1, weightG: 38.2, filaments: [{ type: 'PLA', color: '#2b2b2b', usedG: 38.2, usedM: 12.8 }] },
    { plateIndex: 2, weightG: 14.6, filaments: [{ type: 'PETG', color: '#ffffff', usedG: 14.6, usedM: 4.9 }] },
  ],
};
const info = makePrintInfo({
  printerModel: 'Bambu Lab X1 Carbon', bedType: 'Textured PEI Plate', layerHeightMm: 0.2,
  nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 60, mixed: false }, nozzleTempC: { value: 220, mixed: false },
  plates: [{ plateIndex: 1, name: 'Boden', printSeconds: 8040, firstLayerSeconds: null, plateBedType: null },
    { plateIndex: 2, name: null, printSeconds: 3120, firstLayerSeconds: null, plateBedType: null }],
});
const rescanButtons = () => screen.queryAllByRole('button', { name: de.rescanMetadataButton });

it('shows a sliced file as a table with shared values and one rescan button', () => {
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: slice, printInfo: info }));
  const table = screen.getByRole('table', { name: de.printDataHeading });
  expect(within(table).getByText('PLA · 38,2 g · 12,8 m')).toBeVisible();
  expect(within(table).getByText('3 h 6 min')).toBeVisible();
  const shared = screen.getByRole('region', { name: de.printDataSharedHeadingPage });
  expect(within(shared).getByText('Bambu Lab X1 Carbon')).toBeVisible();
  expect(within(shared).getByText('Textured PEI Plate')).toBeVisible();
  expect(within(shared).getByText('0,4 mm')).toBeVisible();
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
  expect(rescanButtons()).toHaveLength(1);
});

it('keeps the filament usage per plate, the filament check and the cost line (usage block still works)', () => {
  const filamentCheck: FilamentCheck = { fileId: 'model-1', status: 'ok', needs: [{
    filamentType: 'PLA', color: '#2b2b2b', neededG: 38.2, status: 'ok', missingG: 0, spools: [], possible: [] }] };
  check.value = { checks: new Map([['model-1', filamentCheck]]), error: false };
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: slice, printInfo: info, costEstimate: { totalCost: 4.5, hasUnpricedFilaments: true } }));
  expect(screen.getByText('PLA · 38,2 g · 12,8 m')).toBeVisible();
  expect(screen.getByText('PETG · 14,6 g · 4,9 m')).toBeVisible();
  expect(screen.getByText(de.filamentCheckHeading)).toBeVisible();
  expect(screen.getByText(de.metaCostEstimate)).toBeVisible();
  expect(screen.getByTitle(de.costEstimateUnpricedHint)).toBeVisible();
  expect(screen.getByText('4,50')).toBeVisible();
});

it('shows the filament usage even when the print data row is missing (not read yet)', () => {
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: slice, printInfo: null }));
  expect(screen.getByText('PLA · 38,2 g · 12,8 m')).toBeVisible();
  expect(screen.getByText(de.printDataUnreadKnown)).toBeVisible();
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
  expect(rescanButtons()).toHaveLength(1);
  expect(screen.queryByRole('table')).not.toBeNull();
  expect(within(screen.getByRole('table')).queryByText(de.printDataTime)).toBeNull();
});

it('keeps the old missing-usage hint for a file not read and without slicer usage, with the plain note and one button', () => {
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: null, printInfo: null }));
  expect(screen.getByText(de.filamentUsageMissingTitle)).toBeVisible();
  expect(screen.getByText(de.printDataUnreadPlain)).toBeVisible();
  expect(rescanButtons()).toHaveLength(1);
});

it('shows the neutral chip and hint for a file without print data and no second usage hint', () => {
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: null, printInfo: makePrintInfo({ sliceState: 'no_slice_data', bedType: 'Textured PEI Plate' }) }));
  expect(screen.getByText('Keine Druckdaten')).toBeVisible();
  expect(screen.getByText(de.printDataNoDataHint)).toBeVisible();
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
  const settings = screen.getByRole('region', { name: de.printDataProjectSettings });
  expect(within(settings).getByText('Textured PEI Plate')).toBeVisible();
  expect(within(settings).queryByText(de.printDataBedTemp)).toBeNull();
  expect(screen.queryByRole('region', { name: de.printDataSharedHeadingPage })).toBeNull();
  expect(screen.queryByText('Geslict')).toBeNull();
});

it.each([
  ['not_recognized', de.printDataNotRecognizedHint],
  ['unreadable', de.printDataUnreadableHint],
] as const)('shows only the hint for %s, no chip and no "Quellmodell"', (sliceState, hint) => {
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: null, printInfo: makePrintInfo({ sliceState }) }));
  expect(screen.getByText(hint)).toBeVisible();
  expect(screen.queryByText(de.printDataNoDataChip)).toBeNull();
  expect(screen.queryByText(/Quellmodell/)).toBeNull();
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
});

it('shows a partly read file with the sum note, dashes with a tooltip and "mehrere" for a mixed value', () => {
  renderPage(makeModelFile({
    path: '/c/a.3mf', sliceInfo: slice,
    printInfo: makePrintInfo({ ...info, bedType: null, nozzleTempC: { value: null, mixed: true },
      plates: [{ plateIndex: 1, name: null, printSeconds: 8040, firstLayerSeconds: null, plateBedType: null },
        { plateIndex: 2, name: null, printSeconds: null, firstLayerSeconds: 99, plateBedType: null }] }),
  }));
  expect(screen.getByText('nur 1 von 2 Platten bekannt')).toBeVisible();
  const table = screen.getByRole('table');
  expect(within(table).getAllByTitle(de.printDataNotFound)).toHaveLength(1);
  const shared = screen.getByRole('region', { name: de.printDataSharedHeadingPage });
  expect(within(shared).getByTitle(de.printDataNotFound)).toHaveTextContent('–');
  expect(within(shared).getByText(de.printDataMixed)).toBeVisible();
  // The first-layer time of plate 2 is never shown as its print time.
  expect(screen.queryByText('2 min')).toBeNull();
});

it('triggers the one existing rescan button and nothing else', () => {
  const onRescanMetadata = vi.fn();
  renderPage(makeModelFile({ path: '/c/a.3mf', sliceInfo: slice, printInfo: null }), { onRescanMetadata });
  act(() => { fireEvent.click(rescanButtons()[0]); });
  expect(onRescanMetadata).toHaveBeenCalledTimes(1);
});
