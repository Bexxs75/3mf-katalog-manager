import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile, makePrintInfo } from '../test/factories';
import { de } from '../i18n/de';
import type { ModelFile } from '../types';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); });

const sliceInfo = { totalWeightG: 52.8, plates: [
  { plateIndex: 1, weightG: 38.2, filaments: [{ type: 'PLA', color: '#2b2b2b', usedG: 38.2, usedM: 12.8 }] },
  { plateIndex: 2, weightG: 14.6, filaments: [{ type: 'PETG', color: null, usedG: 14.6, usedM: 4.9 }] },
] };
const info = makePrintInfo({
  printerModel: 'Bambu Lab X1 Carbon', layerHeightMm: 0.2,
  plates: [{ plateIndex: 1, name: null, printSeconds: 8040, firstLayerSeconds: null, plateBedType: null },
    { plateIndex: 2, name: null, printSeconds: 3120, firstLayerSeconds: null, plateBedType: null }],
});

function panel(density: string, model: ModelFile, extra: Partial<React.ComponentProps<typeof DetailPanel>> = {}) {
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn();
  return render(<LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]} printers={[]}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} {...extra} /></UiDensityProvider></LanguageProvider>);
}
const card = () => screen.getByRole('region', { name: de.printDataHeading });

it.each(['compact', 'comfort'])('shows the print data card with its values in the %s panel', density => {
  panel(density, makeModelFile({ path: '/c/a.3mf', sliceInfo, printInfo: info }));
  expect(within(card()).getByText('Bambu Lab X1 Carbon')).toBeVisible();
  expect(within(card()).getByText('0,20 mm')).toBeVisible();
  expect(within(card()).getByText(de.printDataTime).nextSibling).toHaveTextContent('3 h 6 min');
  fireEvent.click(within(card()).getByRole('button', { name: 'Platte 2' }));
  expect(within(card()).getByText(de.printDataTime).nextSibling).toHaveTextContent('52 min');
});

it.each(['compact', 'comfort'])('shows the chip and no "sliced" wording for a file without print data in the %s panel', density => {
  panel(density, makeModelFile({ path: '/c/a.3mf', printInfo: makePrintInfo({ sliceState: 'no_slice_data' }) }));
  expect(within(card()).getByText('Keine Druckdaten')).toBeVisible();
  expect(within(card()).queryByText('Geslict')).toBeNull();
});

it.each(['compact', 'comfort'])('says a file was not read yet, without a button and without the detail page hint, in the %s panel', density => {
  panel(density, makeModelFile({ path: '/c/a.3mf', sliceInfo, printInfo: null }));
  expect(within(card()).getByText(de.printDataUnreadPanel)).toBeVisible();
  expect(within(card()).queryByRole('button', { name: /neu einlesen/i })).toBeNull();
  expect(screen.queryByText(/Metadaten neu einlesen/)).toBeNull();
});

it.each(['compact', 'comfort'])('shows no print data card for an STL, a list summary or the trash (%s)', density => {
  const { unmount } = panel(density, makeModelFile({ path: '/c/a.stl' }));
  expect(screen.queryByRole('region', { name: de.printDataHeading })).toBeNull();
  unmount();
  const summary = panel(density, makeModelFile({ path: '/c/a.3mf', summaryOnly: true }));
  expect(screen.queryByRole('region', { name: de.printDataHeading })).toBeNull();
  summary.unmount();
  panel(density, makeModelFile({ path: '/c/a.3mf', deletedAt: '2026-01-01T00:00:00Z' }), { trashMode: true });
  expect(screen.queryByRole('region', { name: de.printDataHeading })).toBeNull();
});

it.each(['compact', 'comfort'])('preserves the model panel width in %s density', density => {
  const { container } = panel(density, makeModelFile());
  const aside = container.querySelector('aside');
  if (density === 'compact') expect(aside).toHaveClass('w-[336px]');
  else expect(aside).toHaveStyle({ width: '380px' });
});

it.each(['compact', 'comfort'])('wraps long unbroken model names without horizontal panel scrolling (%s)', density => {
  const name = 'VeryLongModelName'.repeat(20);
  const path = `/catalog/${name}.3mf`;
  const { container } = panel(density, makeModelFile({ name, path }));
  expect(container.querySelector('aside')).toHaveClass('min-w-0', 'overflow-x-hidden');
  if (density === 'compact') {
    expect(screen.getByText(path)).toHaveClass('[overflow-wrap:anywhere]');
    expect(screen.getByText(name)).toHaveClass('break-words');
  } else {
    expect(screen.getByText(name)).toHaveClass('min-w-0', '[overflow-wrap:anywhere]');
    expect(container.querySelector('aside')).toHaveClass('model-panel-comfort');
    expect(container.querySelector('.model-panel-content')).toHaveClass('min-h-0', 'overflow-x-hidden');
  }
});
