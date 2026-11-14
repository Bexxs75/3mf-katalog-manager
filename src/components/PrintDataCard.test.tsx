import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PlateOverviewContext } from '../hooks/PlateOverviewContext';
import { PrintDataCard } from './PrintDataCard';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { makeModelFile, makePrintInfo } from '../test/factories';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import type { ModelFile, PrintInfo, SliceInfo } from '../types';

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); });

const dot = (color: string, usedG: number) => ({ type: 'PLA', color, usedG, usedM: usedG / 3 });
const sliceInfo: SliceInfo = {
  totalWeightG: 113.8,
  plates: [
    { plateIndex: 1, weightG: 38.2, filaments: [dot('#2b2b2b', 38.2)] },
    { plateIndex: 2, weightG: 14.6, filaments: [dot('#f2f2f0', 14.6)] },
    { plateIndex: 3, weightG: 61, filaments: [dot('#2b2b2b', 40.2), dot('#e8590c', 20.8)] },
  ],
};
const plate = (plateIndex: number, printSeconds: number | null, name: string | null = null) => ({
  plateIndex, name, printSeconds, firstLayerSeconds: null, plateBedType: null,
});
const full: Partial<PrintInfo> = {
  printerModel: 'Bambu Lab X1 Carbon', bedType: 'Textured PEI Plate', layerHeightMm: 0.2,
  nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 60, mixed: false }, nozzleTempC: { value: 220, mixed: false },
  plates: [plate(1, 8040, 'Gehäuse unten'), plate(2, 3120, 'Deckel'), plate(3, 13260, 'Halterungen')],
};
const modelWith = (printInfo: PrintInfo | null, withUsage = true): Pick<ModelFile, 'sliceInfo' | 'printInfo'> =>
  makeModelFile({ sliceInfo: withUsage ? sliceInfo : null, printInfo });

const show = (model: Pick<ModelFile, 'sliceInfo' | 'printInfo'>, variant: 'compact' | 'comfort' | 'page') =>
  render(<LanguageProvider><PrintDataCard model={model} variant={variant} /></LanguageProvider>);

const variants = ['compact', 'comfort', 'page'] as const;

describe.each(variants)('PrintDataCard (%s)', variant => {
  it('shows a sliced file with all values and no chip', () => {
    show(modelWith(makePrintInfo(full)), variant);
    expect(screen.getByRole('heading', { name: de.printDataHeading })).toBeVisible();
    expect(screen.queryByText(de.printDataNoDataChip)).toBeNull();
    for (const text of ['Bambu Lab X1 Carbon', 'Textured PEI Plate', '0,20 mm', '0,4 mm', '60 °C', '220 °C']) {
      expect(screen.getByText(text)).toBeVisible();
    }
    expect(screen.queryByText('–')).toBeNull();
    expect(screen.queryByText(de.printDataNoDataHint)).toBeNull();
  });

  it('shows the chip, the hint and the project settings without temperatures and time for a file without print data', () => {
    show(modelWith(makePrintInfo({ sliceState: 'no_slice_data', ...full }), false), variant);
    expect(screen.getByText('Keine Druckdaten')).toBeVisible();
    expect(screen.getByText(de.printDataNoDataHint)).toBeVisible();
    expect(screen.queryByText('Geslict')).toBeNull();
    expect(screen.queryByText(/nicht geslict/i)).toBeNull();
    expect(screen.getByText(de.printDataProjectSettings)).toBeVisible();
    expect(screen.queryByText(de.printDataSharedHeadingPanel)).toBeNull();
    expect(screen.queryByText(de.printDataSharedHeadingPage)).toBeNull();
    for (const text of ['Bambu Lab X1 Carbon', 'Textured PEI Plate', '0,20 mm', '0,4 mm']) expect(screen.getByText(text)).toBeVisible();
    for (const text of [de.printDataBedTemp, de.printDataNozzleTemp, '60 °C', '220 °C']) expect(screen.queryByText(text)).toBeNull();
    expect(screen.queryByText(de.printDataTime)).toBeNull();
  });

  it('shows a PrusaSlicer project (printer, layer height, nozzle; bed type a dash) without temperatures or time', () => {
    const prusa = makePrintInfo({
      sliceState: 'no_slice_data', settingsState: 'read', printerModel: 'MK4S', bedType: null, layerHeightMm: 0.15,
      nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: null, mixed: false }, nozzleTempC: { value: null, mixed: false }, plates: [],
    });
    show(modelWith(prusa, false), variant);
    expect(screen.getByText('Keine Druckdaten')).toBeVisible();
    expect(screen.getByText(de.printDataProjectSettings)).toBeVisible();
    for (const text of ['MK4S', '0,15 mm', '0,4 mm']) expect(screen.getByText(text)).toBeVisible();
    expect(screen.getAllByTitle(de.printDataNotFound)).toHaveLength(1);
    expect(screen.getByText('–')).toBeVisible();
    for (const text of [de.printDataBedTemp, de.printDataNozzleTemp, de.printDataTime]) expect(screen.queryByText(text)).toBeNull();
    expect(screen.queryByText(/°C/)).toBeNull();
  });

  it('shows a missing project setting as a dash with a tooltip, never as 0', () => {
    show(modelWith(makePrintInfo({ sliceState: 'no_slice_data', ...full, bedType: null, layerHeightMm: null }), false), variant);
    expect(screen.getAllByTitle(de.printDataNotFound)).toHaveLength(2);
    expect(screen.queryByText(/^0 (mm|°C)$|^0$/)).toBeNull();
  });

  it.each(['missing', 'unreadable', 'not_attempted'] as const)('shows no project settings rows when the settings state is %s', settingsState => {
    show(modelWith(makePrintInfo({ sliceState: 'no_slice_data', ...full, settingsState }), false), variant);
    expect(screen.getByText(de.printDataNoDataHint)).toBeVisible();
    expect(screen.queryByText(de.printDataProjectSettings)).toBeNull();
    expect(screen.queryByText('Bambu Lab X1 Carbon')).toBeNull();
    expect(screen.queryByText(de.printDataPrinter)).toBeNull();
  });

  it.each([
    ['not_recognized', de.printDataNotRecognizedHint],
    ['unreadable', de.printDataUnreadableHint],
  ] as const)('shows only the hint and no chip for %s', (sliceState, hint) => {
    show(modelWith(makePrintInfo({ sliceState }), false), variant);
    expect(screen.getByText(hint)).toBeVisible();
    expect(screen.queryByText(de.printDataNoDataChip)).toBeNull();
    expect(screen.queryByText(de.printDataNoDataHint)).toBeNull();
    expect(screen.queryByText(/Quellmodell/)).toBeNull();
  });

  it('shows the sum with the "only k of n" note and dashes with a tooltip for a partly read file', () => {
    show(modelWith(makePrintInfo({ ...full, bedType: null, nozzleTempC: { value: null, mixed: false }, plates: [plate(1, 8040), plate(2, null), plate(3, null)] })), variant);
    expect(screen.getAllByText('nur 1 von 3 Platten bekannt').length).toBeGreaterThan(0);
    expect(screen.getAllByText('2 h 14 min').length).toBeGreaterThan(0);
    const dashes = screen.getAllByTitle(de.printDataNotFound);
    expect(dashes.length).toBeGreaterThanOrEqual(2);
    for (const dash of dashes) {
      expect(dash).toHaveTextContent('–');
      // The same words for keyboard and screen reader users.
      expect(within(dash).getByText(de.printDataNotFound)).toHaveClass('sr-only');
    }
  });

  it('shows a mixed value as "mehrere" without the not-found tooltip', () => {
    show(modelWith(makePrintInfo({ ...full, nozzleTempC: { value: null, mixed: true } })), variant);
    expect(screen.getByText(de.printDataMixed)).toBeVisible();
    expect(screen.queryByTitle(de.printDataNotFound)).toBeNull();
  });

  it('says so when the settings of the file could not be read', () => {
    show(modelWith(makePrintInfo({ ...full, settingsState: 'unreadable', bedType: null })), variant);
    expect(screen.getAllByTitle(de.printDataSettingsUnreadable).length).toBeGreaterThan(0);
    expect(screen.queryByTitle(de.printDataNotFound)).toBeNull();
  });

  it.each([
    ['no row', null],
    ['a row of another reader version', makePrintInfo({ ...full, readerCurrent: false })],
  ])('treats %s as not read: a note, no values from the row, no button', (_name, printInfo) => {
    show(modelWith(printInfo), variant);
    const note = variant === 'page' ? de.printDataUnreadKnown : de.printDataUnreadPanel;
    expect(screen.getByText(note)).toBeVisible();
    expect(screen.queryByRole('button', { name: /einlesen|nachlesen/i })).toBeNull();
    expect(screen.queryByText(de.printDataTime)).toBeNull();
    for (const text of ['Bambu Lab X1 Carbon', '0,20 mm', '220 °C', '2 h 14 min']) expect(screen.queryByText(text)).toBeNull();
    // Weight and material are known from the slicer usage and stay visible.
    expect(document.querySelectorAll('[data-filament-line]').length).toBeGreaterThan(0);
    expect(screen.getAllByText('113,8 g').length).toBeGreaterThan(0);
  });

  it('names the plain note for a file not read and without slicer usage (page) or the panel text', () => {
    show(modelWith(null, false), variant);
    expect(screen.getByText(variant === 'page' ? de.printDataUnreadPlain : de.printDataUnreadPanel)).toBeVisible();
  });
});

describe('PrintDataCard panel plate choice', () => {
  it('shows one plate with the total of all plates below', () => {
    show(modelWith(makePrintInfo(full)), 'compact');
    expect(screen.getByText(de.printDataTime).nextSibling).toHaveTextContent('6 h 47 min');
    fireEvent.click(screen.getByRole('button', { name: 'Platte 2 · Deckel' }));
    expect(screen.getByText('Platte 2 · Deckel', { selector: 'div' })).toBeVisible();
    expect(screen.getByText(de.printDataTime).nextSibling).toHaveTextContent('52 min');
    expect(screen.getByText(de.printDataTotal).nextSibling).toHaveTextContent('6 h 47 min');
    expect(screen.getByText(de.printDataWeight).nextSibling).toHaveTextContent('14,6 g');
  });

  it('shows a plate without a time as a dash and keeps the others', () => {
    show(modelWith(makePrintInfo({ ...full, plates: [plate(1, 8040), plate(2, null), plate(3, 13260)] })), 'compact');
    fireEvent.click(screen.getByRole('button', { name: 'Platte 2' }));
    expect(within(screen.getByText(de.printDataTime).parentElement as HTMLElement).getByTitle(de.printDataNotFound)).toBeVisible();
  });
});

describe('PrintDataCard page table', () => {
  it('is a real table with a caption, column and row headers, all plates and a total row', () => {
    show(modelWith(makePrintInfo(full)), 'page');
    const table = screen.getByRole('table', { name: de.printDataHeading });
    expect(within(table).getAllByRole('columnheader').map(h => h.textContent)).toEqual([de.printDataColPlate, de.printDataTime, de.printDataWeight, de.metaMaterial]);
    const rows = within(table).getAllByRole('row');
    expect(rows).toHaveLength(1 + 3 + 1);
    expect(within(rows[1]).getByRole('rowheader')).toHaveTextContent('Platte 1');
    expect(within(rows[1]).getByText('2 h 14 min')).toBeVisible();
    expect(within(rows[4]).getByRole('rowheader')).toHaveTextContent(de.printDataTotal);
    expect(within(rows[4]).getByText('6 h 47 min')).toBeVisible();
    expect(within(rows[4]).getByText('113,8 g')).toBeVisible();
    expect(screen.getByRole('heading', { name: de.printDataSharedHeadingPage })).toBeVisible();
  });

  it('marks the chosen plate in the table and the selector', () => {
    show(modelWith(makePrintInfo(full)), 'page');
    fireEvent.click(within(screen.getByRole('table')).getByRole('button', { name: /Platte 3/ }));
    expect(within(screen.getByRole('table')).getByRole('button', { name: /Platte 3/ })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Platte 3 · Halterungen' })).toHaveAttribute('aria-current', 'true');
    expect(screen.getByRole('button', { name: de.viewerAllPlates })).toHaveAttribute('aria-pressed', 'false');
  });

  it('adds up weights and shows the "only k of n" note once in the total row', () => {
    show(modelWith(makePrintInfo({ ...full, plates: [plate(1, 8040), plate(2, null), plate(3, null)] })), 'page');
    expect(screen.getAllByText('nur 1 von 3 Platten bekannt')).toHaveLength(1);
  });
});

describe('PrintDataCard keyboard and screen reader', () => {
  it('chooses the next and previous plate with the arrow keys and announces it', () => {
    show(modelWith(makePrintInfo(full)), 'compact');
    const group = screen.getByRole('group', { name: de.viewerPrintPlate });
    fireEvent.click(screen.getByRole('button', { name: 'Platte 1 · Gehäuse unten' }));
    fireEvent.keyDown(group, { key: 'ArrowRight' });
    expect(screen.getByRole('button', { name: 'Platte 2 · Deckel' })).toHaveAttribute('aria-current', 'true');
    expect(within(group).getByRole('status')).toHaveTextContent('Platte 2 · Deckel');
    fireEvent.keyDown(group, { key: 'ArrowLeft' });
    expect(screen.getByRole('button', { name: 'Platte 1 · Gehäuse unten' })).toHaveAttribute('aria-current', 'true');
  });

  it('does not pass arrow keys to the viewer shortcuts', () => {
    const onWindowKey = (event: KeyboardEvent) => { seen.push(event.key); };
    const seen: string[] = [];
    window.addEventListener('keydown', onWindowKey);
    show(modelWith(makePrintInfo(full)), 'compact');
    fireEvent.keyDown(screen.getByRole('group', { name: de.viewerPrintPlate }), { key: 'ArrowRight' });
    window.removeEventListener('keydown', onWindowKey);
    expect(seen).toEqual([]);
  });

  it('keeps the plate buttons reachable by keyboard and the rows themselves out of the tab order', () => {
    show(modelWith(makePrintInfo(full)), 'page');
    for (const row of within(screen.getByRole('table')).getAllByRole('row')) expect(row).not.toHaveAttribute('tabindex');
    for (const button of within(screen.getByRole('table')).getAllByRole('button')) expect(button.tagName).toBe('BUTTON');
  });

  it('keeps the same words in English', () => {
    localStorage.setItem('3mf-katalog-language', 'en');
    show(modelWith(makePrintInfo({ ...full, bedType: null })), 'compact');
    expect(screen.getByRole('heading', { name: en.printDataHeading })).toBeVisible();
    expect(screen.getAllByTitle(en.printDataNotFound).length).toBeGreaterThan(0);
    expect(screen.getByText('0.20 mm')).toBeVisible();
  });
});

describe('PrintDataCard look', () => {
  const lines = () => [...document.querySelectorAll<HTMLElement>('[data-filament-line]')];

  it('gives every filament dot a visible border from the muted ink token (dark colors on a dark theme)', () => {
    show(modelWith(makePrintInfo(full)), 'page');
    const dots = lines().map(line => line.firstElementChild as HTMLElement);
    expect(dots.length).toBe(4);
    for (const dot of dots) expect(dot.className).toContain('border-[var(--ink-3)]');
  });

  it('reserves the dot column on every line so several filaments line up, also without a color', () => {
    show(modelWith(makePrintInfo(full)), 'compact');
    const block = lines()[0].parentElement as HTMLElement;
    expect(block.className).toMatch(/items-start/);
    for (const line of lines()) expect(line.firstElementChild?.className).toContain('w-2.5');
  });

  it('keeps the total label on one line when a plate is chosen', () => {
    show(modelWith(makePrintInfo(full)), 'compact');
    fireEvent.click(screen.getByRole('button', { name: 'Platte 2 · Deckel' }));
    expect(screen.getByText(de.printDataTotal).className).toContain('whitespace-nowrap');
  });

  it('draws the dash for missing values in the stronger ink token (contrast above 4.5:1 in both themes)', () => {
    show(modelWith(makePrintInfo({ ...full, bedType: null })), 'page');
    expect(screen.getAllByTitle(de.printDataNotFound)[0].className).toContain('text-[var(--ink-2)]');
  });

  it('shows weights with one decimal: 61 g becomes 61,0 g', () => {
    const round = { ...sliceInfo, plates: [{ plateIndex: 1, weightG: 61, filaments: [dot('#2b2b2b', 61)] }, sliceInfo.plates[1]] };
    show(makeModelFile({ sliceInfo: round, printInfo: makePrintInfo(full) }), 'page');
    expect(screen.getAllByText('61,0 g').length).toBeGreaterThan(0);
    expect(screen.queryByText('61 g')).toBeNull();
  });
});

describe('PrintDataCard plate overview entry', () => {
  it.each(variants)('opens the overview of the model (%s)', variant => {
    const opener = vi.fn();
    render(<LanguageProvider><PlateOverviewContext.Provider value={opener}>
      <PrintDataCard model={makeModelFile({ id: 'abc', sliceInfo, printInfo: makePrintInfo(full) })} variant={variant} />
    </PlateOverviewContext.Provider></LanguageProvider>);
    fireEvent.click(screen.getByRole('button', { name: de.poOpen }));
    expect(opener).toHaveBeenCalledWith('abc');
  });

  it('is not offered without a dialog, without an id, or without plates', () => {
    const { unmount } = show(makeModelFile({ id: 'abc', sliceInfo, printInfo: makePrintInfo(full) }), 'page');
    expect(screen.queryByRole('button', { name: de.poOpen })).toBeNull();
    unmount();
    const noId = { sliceInfo, printInfo: makePrintInfo(full) };
    const { unmount: unmount2 } = render(<LanguageProvider><PlateOverviewContext.Provider value={vi.fn()}><PrintDataCard model={noId} variant="page" /></PlateOverviewContext.Provider></LanguageProvider>);
    expect(screen.queryByRole('button', { name: de.poOpen })).toBeNull();
    unmount2();
    render(<LanguageProvider><PlateOverviewContext.Provider value={vi.fn()}>
      <PrintDataCard model={makeModelFile({ id: 'abc', sliceInfo: null, printInfo: makePrintInfo({ sliceState: 'no_slice_data', plates: [] }) })} variant="page" />
    </PlateOverviewContext.Provider></LanguageProvider>);
    expect(screen.queryByRole('button', { name: de.poOpen })).toBeNull();
  });
});
