import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { PrintFilterContext } from '../hooks/PrintFilterContext';
import { PlateOverviewContext } from '../hooks/PlateOverviewContext';
import { makeModelFile, makePrintFacts } from '../test/factories';
import { EMPTY_FILTER, type PrintDataFilter } from '../lib/printDataFilter';
import { printFactsExtraHeight } from './PrintFactsExtra';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { de } from '../i18n/de';

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });

const props = { selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(), displayPreference: 'thumbnail' as const };

const full = makeModelFile({ id: 'full', name: 'Full', printFacts: makePrintFacts({
  printerModel: 'Bambu Lab X1 Carbon', layerHeightMm: 0.2, nozzleMm: 0.4, printSeconds: 8040, printSecondsComplete: true, colors: ['#ff0000', '#ffffff'], plateCount: 2 }) });
const partial = makeModelFile({ id: 'partial', name: 'Partial', printFacts: makePrintFacts({ printerModel: 'Prusa MK4S', printSeconds: 5400, plateCount: 3 }) });
const bare = makeModelFile({ id: 'bare', name: 'Bare' });
const models = [full, partial, bare];
const active: PrintDataFilter = { ...EMPTY_FILTER, layers: [0.2], nozzles: [0.4], colors: ['red'], includeUnknown: true };

function renderWith(Component: typeof ModelGrid | typeof ModelList, filter: PrintDataFilter | null) {
  return render(
    <LanguageProvider><UiDensityProvider><PrintFilterContext.Provider value={filter}>
      <Component {...props} models={models} />
    </PrintFilterContext.Provider></UiDensityProvider></LanguageProvider>,
  );
}
const card = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-model-id="${id}"]`)!;

describe.each(['compact', 'comfort'] as const)('card extra block (%s)', density => {
  beforeEach(() => localStorage.setItem('3mf-katalog-density', density));
  const baseHeight = density === 'comfort' ? 124 : 88;

  it('is not there without an active filter, and the card keeps its height', () => {
    const { container } = renderWith(ModelGrid, null);
    expect(container.querySelectorAll('[data-print-facts]')).toHaveLength(0);
    for (const node of container.querySelectorAll<HTMLElement>('[data-card-metadata]')) expect(node.style.height).toBe(`${baseHeight}px`);
  });

  it('gives every card the same, larger height while a filter is active (the grid measures one card)', () => {
    const { container } = renderWith(ModelGrid, active);
    const heights = [...container.querySelectorAll<HTMLElement>('[data-card-metadata]')].map(node => node.style.height);
    expect(heights).toHaveLength(3);
    expect(new Set(heights).size).toBe(1);
    expect(heights[0]).toBe(`${baseHeight + printFactsExtraHeight(density === 'comfort')}px`);
    expect(container.querySelectorAll('[data-print-facts]')).toHaveLength(3);
    // The block itself is as high for a card with all values as for one with none.
    const blocks = [...container.querySelectorAll<HTMLElement>('[data-print-facts]')].map(node => node.style.height);
    expect(new Set(blocks).size).toBe(1);
  });

  it('shows printer, time, layer height, nozzle and named color dots of a file that has them', () => {
    const { container } = renderWith(ModelGrid, active);
    const block = within(card(container, 'full'));
    expect(block.getByText('Bambu Lab X1 Carbon')).toBeInTheDocument();
    expect(block.getByText('2 h 14 min · 0,20 mm · Ø 0,4 mm')).toBeInTheDocument();
    // The grid is not laid out in jsdom and hides itself until it is measured, which also empties
    // computed names, so the label is read from the attribute.
    expect(block.getAllByRole('img', { hidden: true }).map(dot => dot.getAttribute('aria-label'))).toEqual([de.pfColorRed, de.pfColorWhite]);
    expect(block.queryByText(/Angabe fehlt/)).toBeNull();
  });

  it('names what is missing for the active filters, shows a partial time as "mind.", and never writes 0', () => {
    const { container } = renderWith(ModelGrid, active);
    const partialCard = within(card(container, 'partial'));
    expect(partialCard.getByText(/Angabe fehlt: Düse, Layerhöhe, Farbe/)).toBeInTheDocument();
    expect(partialCard.getByText('Prusa MK4S')).toBeInTheDocument();
    expect(partialCard.getByText('mind. 1 h 30 min')).toBeInTheDocument();
    const text = card(container, 'partial').querySelector('[data-print-facts]')!.textContent ?? '';
    expect(text).not.toMatch(/(^|[^\d.,])0(\s|$|,0)/);
    expect(text).not.toContain('0 mm');
  });

  it('marks a file without any print data for every active filter and only for those', () => {
    const { container } = renderWith(ModelGrid, { ...EMPTY_FILTER, printers: ['x'], times: ['lt1'] });
    const bareCard = within(card(container, 'bare'));
    expect(bareCard.getByText(/Angabe fehlt: Zieldrucker, Druckzeit/)).toBeInTheDocument();
    expect(card(container, 'bare').querySelector('[data-print-facts]')?.textContent).not.toContain('Düse');
  });

  it('shows at most six color dots and the rest as a number', () => {
    const many = makeModelFile({ id: 'many', printFacts: makePrintFacts({ colors: ['#ff0000', '#00ff00', '#0000ff', '#ffff00', '#000000', '#ffffff', '#808080', '#ff8000'] }) });
    const { container } = render(
      <LanguageProvider><UiDensityProvider><PrintFilterContext.Provider value={{ ...EMPTY_FILTER, colors: ['red'] }}>
        <ModelGrid {...props} models={[many]} />
      </PrintFilterContext.Provider></UiDensityProvider></LanguageProvider>,
    );
    expect(container.querySelectorAll('[data-print-facts] [role="img"]')).toHaveLength(6);
    expect(container.querySelector('[data-print-facts]')).toHaveTextContent('+2');
  });
});

describe('list rows', () => {
  it('say which value is missing while a filter is active, and are plain otherwise', () => {
    const { container, rerender } = renderWith(ModelList, active);
    expect(card(container, 'partial').querySelector('[data-print-facts-missing]')).toHaveTextContent('Angabe fehlt: Düse, Layerhöhe, Farbe');
    expect(card(container, 'full').querySelector('[data-print-facts-missing]')).toBeNull();
    rerender(<LanguageProvider><UiDensityProvider><PrintFilterContext.Provider value={null}><ModelList {...props} models={models} /></PrintFilterContext.Provider></UiDensityProvider></LanguageProvider>);
    expect(container.querySelectorAll('[data-print-facts-missing]')).toHaveLength(0);
  });
});

describe('plate overview entry on the card', () => {
  const renderOpener = (opener: ((id: string) => void) | null, onSelect = vi.fn(), list = models) => render(
    <LanguageProvider><UiDensityProvider><PlateOverviewContext.Provider value={opener}><PrintFilterContext.Provider value={active}>
      <ModelGrid {...props} onSelect={onSelect} models={list} />
    </PrintFilterContext.Provider></PlateOverviewContext.Provider></UiDensityProvider></LanguageProvider>,
  );
  // The grid hides itself until it is measured, which also empties computed names: read the text.
  const entry = (container: HTMLElement, id: string) => card(container, id).querySelector<HTMLButtonElement>('[data-print-facts] button');

  it('names the plate count, opens the overview of that card, and does not select the card', () => {
    const opener = vi.fn();
    const onSelect = vi.fn();
    const { container } = renderOpener(opener, onSelect);
    expect(entry(container, 'full')).toHaveTextContent('Plattenübersicht (2 Platten)');
    fireEvent.click(entry(container, 'full')!);
    expect(opener).toHaveBeenCalledWith('full');
    expect(onSelect).not.toHaveBeenCalled();
    expect(entry(container, 'partial')).toHaveTextContent('Plattenübersicht (3 Platten)');
  });

  it('is left out for a card without plates and where no dialog is available', () => {
    const { container, unmount } = renderOpener(vi.fn());
    expect(entry(container, 'bare')).toBeNull();
    unmount();
    const none = renderOpener(null);
    expect(none.container.querySelectorAll('[data-print-facts] button')).toHaveLength(0);
  });

  it('uses the singular for one plate', () => {
    const one = makeModelFile({ id: 'one', printFacts: makePrintFacts({ plateCount: 1, printerModel: 'X' }) });
    const { container } = renderOpener(vi.fn(), vi.fn(), [one]);
    expect(entry(container, 'one')).toHaveTextContent('Plattenübersicht (1 Platte)');
  });
});
