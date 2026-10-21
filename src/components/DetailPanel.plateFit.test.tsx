import { render, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile, makePrinter } from '../test/factories';
import { de } from '../i18n/de';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const printers = [makePrinter({ id: 'a', name: 'X1C' }),
  makePrinter({ id: 'b', name: 'Mini', bedXMm: 180, bedYMm: 180, bedZMm: 180 }), makePrinter({ id: 'c', name: 'NoBed', bedZMm: null })];

function panel(density: string, extra: Partial<React.ComponentProps<typeof DetailPanel>> = {}, model = makeModelFile({ dimensionsMm: [220, 180, 140] })) {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn();
  return render(<LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]} printers={printers}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} {...extra} /></UiDensityProvider></LanguageProvider>);
}

it.each(['compact', 'comfort'])('lists the print bed verdict per printer with a build volume in the %s panel', density => {
  panel(density);
  const section = screen.getByRole('region', { name: de.plateFitHeading });
  expect(section).toHaveTextContent('Modell 220 × 180 × 140 mm');
  const rows = within(section).getAllByRole('listitem');
  expect(rows.map(row => row.textContent)).toEqual([
    `${de.plateFitFits}X1C (256 × 256 × 256)`,
    `${de.plateFitTooLarge}Mini (180 × 180 × 180): 220 mm zu breit`,
  ]);
  expect(section).toHaveTextContent(de.plateFitHint);
});

it.each(['compact', 'comfort'])('says the size is unknown instead of judging in the %s panel', density => {
  panel(density, {}, makeModelFile({ dimensionsMm: null }));
  const rows = within(screen.getByRole('region', { name: de.plateFitHeading })).getAllByRole('listitem');
  expect(rows).toHaveLength(2);
  rows.forEach(row => expect(row).toHaveTextContent(de.plateFitUnknown));
  rows.forEach(row => expect(row).not.toHaveTextContent(de.plateFitFits));
});

it.each(['compact', 'comfort'])('shows no print bed section in the trash panel or without printers (%s)', density => {
  const { unmount } = panel(density, { trashMode: true });
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
  unmount();
  panel(density, { printers: [] });
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
});
