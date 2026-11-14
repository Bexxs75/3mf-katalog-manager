import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile, makePrintFacts, makePrintInfo, makePrinter } from '../test/factories';
import { PlateOverviewContext } from '../hooks/PlateOverviewContext';
import { de } from '../i18n/de';
import type { FilamentCheck, ModelFile } from '../types';

const check: { value: Record<string, unknown> } = { value: {} };
vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Viewer</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => check.value }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); check.value = {}; });

const sliceInfo = { totalWeightG: 48, plates: [{ plateIndex: 1, weightG: 48, filaments: [{ type: 'PETG', color: '#3d6786', usedG: 48, usedM: 16 }] }] };
const sliced = makePrintInfo({
  printerModel: 'Example Printer 0.4', layerHeightMm: 0.2, nozzleMm: { value: 0.4, mixed: false },
  bedTempC: { value: 80, mixed: false }, nozzleTempC: { value: 240, mixed: false },
  plates: [{ plateIndex: 1, name: null, printSeconds: 8040, firstLayerSeconds: null, plateBedType: null }],
});
// Dimensions on purpose: a G-code entry never has any, and the fit section must stay away even if one did.
const gcode = (extra: Partial<ModelFile> = {}) => makeModelFile({ id: 'g1', name: 'Fan duct', path: '/c/fan-duct.gcode', dimensionsMm: [100, 100, 100], sliceInfo, printInfo: sliced, ...extra });
const notRecognized = () => gcode({ sliceInfo: null, printInfo: makePrintInfo({ sliceState: 'not_recognized', settingsState: 'missing' }) });

function panel(density: string, model: ModelFile, extra: Partial<React.ComponentProps<typeof DetailPanel>> = {}) {
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn();
  return render(<LanguageProvider><PlateOverviewContext.Provider value={vi.fn()}><UiDensityProvider><DetailPanel model={model} allTags={[]} printers={[makePrinter({ id: 'a', name: 'X1C' })]}
    hasSlicer onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} {...extra} /></UiDensityProvider></PlateOverviewContext.Provider></LanguageProvider>);
}

it.each(['compact', 'comfort'])('shows a note instead of the 3D viewer, the print data and no slicer or fit parts for G-code in the %s panel', density => {
  panel(density, gcode());
  expect(screen.queryByText('Viewer')).toBeNull();
  expect(screen.getByRole('heading', { name: de.gcodeNoPreviewTitle })).toBeVisible();
  expect(screen.getByText(de.gcodeNoPreviewBody)).toBeVisible();
  expect(screen.queryByText(de.dragToRotate)).toBeNull();
  expect(screen.queryByRole('button', { name: new RegExp(de.openInSlicer) })).toBeNull();
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
  const card = screen.getByRole('region', { name: de.printDataHeading });
  expect(within(card).getByText('Example Printer 0.4')).toBeVisible();
  expect(within(card).getByText(de.printDataTime).nextSibling).toHaveTextContent('2 h 14 min');
  // One plate: no plate selector, and no 3MF plate overview.
  expect(card.querySelector('[data-plate-selector]')).toBeNull();
  expect(within(card).queryByRole('button', { name: de.poOpen })).toBeNull();
});

it.each(['compact', 'comfort'])('keeps the viewer, the slicer button, the fit section and the plate overview for a 3MF (%s)', density => {
  panel(density, makeModelFile({ path: '/c/a.3mf', dimensionsMm: [100, 100, 100], sliceInfo, printInfo: sliced }));
  expect(within(screen.getByRole('region', { name: de.printDataHeading })).getByRole('button', { name: de.poOpen })).toBeVisible();
  expect(screen.getByText('Viewer')).toBeVisible();
  expect(screen.getByRole('button', { name: new RegExp(de.openInSlicer) })).toBeVisible();
  expect(screen.getByRole('region', { name: de.plateFitHeading })).toBeVisible();
});

it('shows the embedded picture instead of the note when the file has one', () => {
  panel('compact', gcode({ customImage: 'data:image/png;base64,AAA' }));
  expect(screen.getByRole('img', { name: 'Fan duct' })).toHaveAttribute('src', 'data:image/png;base64,AAA');
  expect(screen.queryByRole('heading', { name: de.gcodeNoPreviewTitle })).toBeNull();
});

it.each(['compact', 'comfort'])('says "no print data recognised" with the neutral chip, never "not sliced" (%s)', density => {
  panel(density, notRecognized());
  const card = screen.getByRole('region', { name: de.printDataHeading });
  expect(within(card).getByText(de.printDataNotRecognizedChip)).toBeVisible();
  expect(within(card).getByText(de.printDataGcodeNotRecognizedHint)).toBeVisible();
  expect(within(card).getByText(de.printDataGcodeNothingKnown)).toBeVisible();
  expect(card).not.toHaveTextContent(/nicht geslict/i);
});

it('does not show the G-code wording for a 3MF that was not recognised', () => {
  panel('compact', makeModelFile({ path: '/c/a.3mf', printInfo: makePrintInfo({ sliceState: 'not_recognized' }) }));
  const card = screen.getByRole('region', { name: de.printDataHeading });
  expect(within(card).queryByText(de.printDataNotRecognizedChip)).toBeNull();
  expect(within(card).queryByText(de.printDataGcodeNothingKnown)).toBeNull();
});

const pageProps = {
  model: makeModelFile(), allTags: [], onClose: vi.fn(), onAddTag: vi.fn(), onRemoveTag: vi.fn(),
  onDelete: vi.fn(), onTogglePrintStatus: vi.fn(), onToggleFavorite: vi.fn(), onToggleQueue: vi.fn(),
  onUploadImage: vi.fn(), onSnapshotCaptured: vi.fn(), onSetSourceUrl: vi.fn(), onOpenInSlicer: vi.fn(),
  onRescanMetadata: vi.fn(), onAddToCollection: vi.fn(), collections: [], slicers: [{ id: 'prusa', name: 'Prusa', path: '/x' }] as never,
  slicerError: null, printers: [makePrinter({ id: 'a', name: 'X1C' })], rescanError: null, rescanSuccess: false, displayPreference: 'thumbnail' as const, hasPrevious: false, hasNext: false,
};
const page = (model: ModelFile) => render(<LanguageProvider><ModelDetailPage {...pageProps} model={model} /></LanguageProvider>);

it('shows the detail page of G-code with the note, the table, the filament check and a rescan button, without model-only parts', () => {
  const filamentCheck: FilamentCheck = { fileId: 'g1', status: 'ok', needs: [{
    filamentType: 'PETG', color: '#3d6786', neededG: 48, status: 'ok', missingG: 0, spools: [], possible: [] }] };
  check.value = { checks: new Map([['g1', filamentCheck]]), error: false };
  const onRescanMetadata = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...pageProps} model={gcode()} onRescanMetadata={onRescanMetadata} /></LanguageProvider>);
  expect(screen.queryByText('Viewer')).toBeNull();
  expect(screen.getByRole('heading', { name: de.gcodeNoPreviewTitle })).toBeVisible();
  expect(screen.queryByRole('button', { name: new RegExp(`^${de.openInSlicer}`) })).toBeNull();
  expect(screen.queryByText(de.detailViewer3d)).toBeNull();
  expect(screen.queryByText(de.metaDimensions)).toBeNull();
  expect(screen.queryByText(de.metaVolume)).toBeNull();
  expect(screen.queryByText(de.metaObjectCount)).toBeNull();
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
  const table = screen.getByRole('table', { name: de.printDataHeading });
  expect(within(table).getByText('2 h 14 min')).toBeVisible();
  expect(document.body).toHaveTextContent(de.filamentCheckHeading);
  fireEvent.click(screen.getByRole('button', { name: de.rescanMetadataButton }));
  expect(onRescanMetadata).toHaveBeenCalledTimes(1);
  // The "export it as sliced 3MF" explanation is for 3MF files only.
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
});

it('shows the detail page of a 3MF unchanged: viewer, dimensions, slicer button', () => {
  page(makeModelFile({ path: '/c/a.3mf', dimensionsMm: [100, 100, 100] }));
  expect(screen.getByText('Viewer')).toBeVisible();
  expect(screen.getByRole('region', { name: de.plateFitHeading })).toBeVisible();
  expect(screen.getByText(de.metaDimensions)).toBeVisible();
  expect(screen.getByRole('button', { name: new RegExp(`^${de.openInSlicer}`) })).toBeVisible();
});

const grid = (models: ModelFile[], density: string) => {
  localStorage.setItem('3mf-katalog-density', density);
  const props = { models, selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(), displayPreference: 'thumbnail' as const };
  return render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} /></UiDensityProvider></LanguageProvider>);
};

it.each(['compact', 'comfort'])('marks a G-code card with its type, print time and a name that says the type (%s)', density => {
  const model = gcode({ estimatedWeightG: 48, weightSource: 'slicer', materials: [{ name: 'PETG', displayColor: '#3d6786' }],
    printFacts: makePrintFacts({ printSeconds: 8040, printSecondsComplete: true, plateCount: 1 }), printInfo: null, sliceInfo: null });
  const { container } = grid([model, makeModelFile({ id: 'm', path: '/c/m.3mf', name: 'Plain model' })], density);
  const cards = [...container.querySelectorAll<HTMLElement>('[data-model-id]')];
  expect(cards[0].querySelector('[data-kind-badge]')).toHaveTextContent('G-Code');
  expect(cards[0].querySelector('[data-card-print-time]')).toHaveTextContent('2 h 14 min');
  expect(cards[0]).toHaveAttribute('aria-label', expect.stringContaining('G-Code'));
  expect(cards[1].querySelector('[data-kind-badge]')).toBeNull();
  expect(cards[1].querySelector('[data-card-print-time]')).toBeNull();
});

it('shows no print time on a card when the slicer gave none, and "bgcode" for the binary format', () => {
  const { container } = grid([gcode({ path: '/c/a.bgcode', printInfo: null, sliceInfo: null })], 'compact');
  expect(container.querySelector('[data-kind-badge]')).toHaveTextContent('bgcode');
  expect(container.querySelector('[data-card-print-time]')).toBeNull();
});

it('puts the "new" badge below the type badge instead of on top of it', () => {
  const { container } = grid([gcode({ importedAt: new Date().toISOString() })], 'compact');
  expect(container.querySelector('[data-kind-badge]')).toBeInTheDocument();
  expect(screen.getByText(de.newBadge).className).toContain('top-[30px]');
});

it('labels a G-code row of the list view with its type', () => {
  const props = { models: [gcode()], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn() };
  const { container } = render(<LanguageProvider><ModelList {...props} /></LanguageProvider>);
  expect(container.querySelector('[data-kind-badge]')).toHaveTextContent('G-Code');
  expect(container.querySelector('[data-model-id]')).toHaveAttribute('aria-label', expect.stringContaining('G-Code'));
});

const lengthOnly = () => gcode({
  sliceInfo: { totalWeightG: 0, plates: [{ plateIndex: 1, weightG: 0, filaments: [{ type: '', color: null, usedG: 0, usedM: 1.5 }] }] },
});

it.each(['compact', 'comfort'])('says "length known, weight unknown" instead of 0 g for a length-only usage in the %s panel', density => {
  panel(density, lengthOnly());
  const card = screen.getByRole('region', { name: de.printDataHeading });
  expect(within(card).getByText(de.printDataWeight).nextSibling).toHaveTextContent(de.printDataLengthOnly);
  expect(card).not.toHaveTextContent(/\b0 g\b/);
  expect(card.querySelector('[data-filament-line]')).toHaveTextContent('1,5 m');
});

it('says it on the detail page too, offers no filament check and shows no cost', () => {
  check.value = { checks: new Map(), error: false };
  const model = { ...lengthOnly(), costEstimate: { totalCost: null, hasUnpricedFilaments: true } };
  page(model);
  expect(screen.getAllByText(de.printDataLengthOnly).length).toBeGreaterThan(0);
  expect(document.body).not.toHaveTextContent(/\b0 g\b/);
  expect(screen.queryByText(de.filamentCheckHeading)).toBeNull();
  expect(screen.queryByText(de.metaCostEstimate)).toBeNull();
});

// A resin file is only catalogued: no content is read, so nothing but the placeholder, the file facts and the note.
const resin = (extra: Partial<ModelFile> = {}) => makeModelFile({ id: 'r1', name: 'Layers', path: '/c/layers.ctb', fileType: 'resin', dimensionsMm: null, volumeCm3: null, objectCount: null, sliceInfo: null, printInfo: null, ...extra });

it.each(['compact', 'comfort'])('shows the resin note, its type and no viewer, slicer, fit or print data in the %s panel', density => {
  panel(density, resin());
  expect(screen.queryByText('Viewer')).toBeNull();
  expect(screen.getByRole('heading', { name: de.resinNoPreviewTitle })).toBeVisible();
  expect(screen.getByText(de.resinNoPreviewBody)).toBeVisible();
  expect(document.body).toHaveTextContent('Resin-Dateien werden bisher nur katalogisiert');
  expect(screen.queryByRole('heading', { name: de.gcodeNoPreviewTitle })).toBeNull();
  expect(document.querySelector('[data-sliced-preview] [data-kind-badge]')).toHaveTextContent('CTB');
  expect(screen.queryByRole('button', { name: new RegExp(de.openInSlicer) })).toBeNull();
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
  expect(screen.queryByRole('region', { name: de.printDataHeading })).toBeNull();
  expect(screen.queryByText(de.metaDimensions)).toBeNull();
  expect(screen.queryByText(de.metaObjectCount)).toBeNull();
});

it('shows the detail page of a resin file with the note, the file facts and without model-only parts', () => {
  check.value = { checks: new Map(), error: false };
  page(resin());
  expect(screen.queryByText('Viewer')).toBeNull();
  expect(screen.getByRole('heading', { name: de.resinNoPreviewTitle })).toBeVisible();
  expect(screen.queryByRole('button', { name: new RegExp(`^${de.openInSlicer}`) })).toBeNull();
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
  expect(screen.queryByRole('table', { name: de.printDataHeading })).toBeNull();
  expect(screen.queryByText(de.metaDimensions)).toBeNull();
  expect(screen.queryByText(de.metaVolume)).toBeNull();
  expect(screen.queryByText(de.metaWeight)).toBeNull();
  expect(screen.getByText(de.metaFileSize)).toBeVisible();
});

it.each(['compact', 'comfort'])('marks a resin card with the ending as its type and a name that says it (%s)', density => {
  const { container } = grid([resin(), resin({ id: 'r2', name: 'Goo', path: '/c/goo.GOO' }), makeModelFile({ id: 'm', path: '/c/m.stl', name: 'Plain model' })], density);
  const cards = [...container.querySelectorAll<HTMLElement>('[data-model-id]')];
  expect(cards[0].querySelector('[data-kind-badge]')).toHaveTextContent('CTB');
  expect(cards[0]).toHaveAttribute('aria-label', expect.stringContaining('CTB'));
  expect(cards[0].querySelector('[data-card-print-time]')).toBeNull();
  expect(cards[1].querySelector('[data-kind-badge]')).toHaveTextContent('GOO');
  expect(cards[2].querySelector('[data-kind-badge]')).toBeNull();
  // The compact placeholder says there is no preview instead of promising a 3D one; the comfortable one has no label.
  if (density === 'compact') {
    expect(within(cards[0]).getByText(de.previewLabelNone)).toBeInTheDocument();
    expect(within(cards[2]).getByText(de.previewLabel3d)).toBeInTheDocument();
  }
});

it('labels a resin row of the list view with its ending', () => {
  const props = { models: [resin()], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn() };
  const { container } = render(<LanguageProvider><ModelList {...props} /></LanguageProvider>);
  expect(container.querySelector('[data-kind-badge]')).toHaveTextContent('CTB');
  expect(container.querySelector('[data-model-id]')).toHaveAttribute('aria-label', expect.stringContaining('CTB'));
});
