import { useState } from 'react';
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { PrintDataFilterBar } from './PrintDataFilterBar';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { EMPTY_FILTER, printDataFacets, type FilterKey, type PrintDataFilter } from '../lib/printDataFilter';
import { makeModelFile, makePrintFacts } from '../test/factories';
import { de } from '../i18n/de';

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); });

const models = [
  makeModelFile({ id: '1', printFacts: makePrintFacts({ printerModel: 'Bambu Lab X1 Carbon', nozzleMm: 0.4, layerHeightMm: 0.2, printSeconds: 7200, printSecondsComplete: true, colors: ['#ff0000', '#ffffff'] }) }),
  makeModelFile({ id: '2', printFacts: makePrintFacts({ printerModel: 'Prusa MK4S', nozzleMm: 0.6, layerHeightMm: 0.28, colors: ['#0000ff'] }) }),
  makeModelFile({ id: '3' }),
];

let latest: PrintDataFilter = EMPTY_FILTER;
function Harness({ initial = EMPTY_FILTER }: { initial?: PrintDataFilter }) {
  const [filter, setFilter] = useState(initial);
  const [open, setOpen] = useState<FilterKey | null>(null);
  latest = filter;
  return (
    <LanguageProvider>
      <button type="button">outside</button>
      <PrintDataFilterBar filter={filter} onChange={setFilter} facets={printDataFacets(models)} open={open} onOpenChange={setOpen} hits={7} />
    </LanguageProvider>
  );
}

describe('PrintDataFilterBar', () => {
  it('has one button per filter, closed, announcing a popover', () => {
    render(<Harness />);
    for (const name of [de.pfFilterPrinter, de.pfFilterNozzle, de.pfFilterLayer, de.pfFilterTime, de.pfFilterColor]) {
      const button = screen.getByRole('button', { name });
      expect(button).toHaveAttribute('aria-haspopup', 'true');
      expect(button).toHaveAttribute('aria-expanded', 'false');
    }
    expect(screen.queryByRole('group')).toBeNull();
  });

  it('opens a labelled popover with the values that occur, their counts and the files without the value', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    const popover = screen.getByRole('group', { name: de.pfFilterPrinter });
    expect(screen.getByRole('button', { name: de.pfFilterPrinter })).toHaveAttribute('aria-expanded', 'true');
    expect(within(popover).getByRole('checkbox', { name: /Bambu Lab X1 Carbon/ })).not.toBeChecked();
    expect(within(popover).getByRole('checkbox', { name: /Prusa MK4S/ })).toBeInTheDocument();
    expect(within(popover).getByText('1 Datei ohne diese Angabe')).toBeInTheDocument();
    expect(within(popover).getByText('Treffer: 7')).toBeInTheDocument();
  });

  it('toggles a choice with a native checkbox and shows the count on the button', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterNozzle }));
    const box = within(screen.getByRole('group', { name: de.pfFilterNozzle })).getByRole('checkbox', { name: /0,6 mm/ });
    // A native checkbox: Space on it is the click the browser sends, so the keyboard path is the same.
    expect(box.tagName).toBe('INPUT');
    expect(box).toHaveAttribute('type', 'checkbox');
    fireEvent.click(box);
    expect(latest.nozzles).toEqual([0.6]);
    expect(screen.getByRole('button', { name: new RegExp(`${de.pfFilterNozzle}\\s*1`) })).toBeInTheDocument();
    fireEvent.click(box);
    expect(latest.nozzles).toEqual([]);
  });

  it('closes with Escape and gives the focus back to its button', () => {
    render(<Harness />);
    const button = screen.getByRole('button', { name: de.pfFilterLayer });
    fireEvent.click(button);
    within(screen.getByRole('group', { name: de.pfFilterLayer })).getAllByRole('checkbox')[0].focus();
    fireEvent.keyDown(document.activeElement ?? window, { key: 'Escape' });
    expect(screen.queryByRole('group')).toBeNull();
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute('aria-expanded', 'false');
  });

  it('closes on a click outside, and the button closes its own popover', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    fireEvent.mouseDown(screen.getByRole('button', { name: 'outside' }));
    expect(screen.queryByRole('group')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    expect(screen.queryByRole('group')).toBeNull();
  });

  it('opens one popover at a time', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    // A real press sends mousedown first; that is what closes the popover that was open.
    fireEvent.mouseDown(screen.getByRole('button', { name: de.pfFilterTime }));
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterTime }));
    expect(screen.getAllByRole('group')).toHaveLength(1);
    expect(screen.getByRole('group', { name: de.pfFilterTime })).toBeInTheDocument();
  });

  it('offers the time steps with counts and only colors that occur, as toggle buttons with a name', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterTime }));
    const times = within(screen.getByRole('group', { name: de.pfFilterTime }));
    expect(times.getByRole('checkbox', { name: /1–4 h/ })).toBeInTheDocument();
    fireEvent.click(times.getByRole('checkbox', { name: /1–4 h/ }));
    expect(latest.times).toEqual(['1to4']);
    fireEvent.mouseDown(screen.getByRole('button', { name: new RegExp(de.pfFilterColor) }));
    fireEvent.click(screen.getByRole('button', { name: new RegExp(de.pfFilterColor) }));
    const colors = within(screen.getByRole('group', { name: de.pfFilterColor }));
    expect(colors.getAllByRole('button', { pressed: false }).map(b => b.textContent)).toEqual(['Weiß1', 'Rot1', 'Blau1']);
    expect(colors.queryByRole('button', { name: /Grün/ })).toBeNull();
    fireEvent.click(colors.getByRole('button', { name: /Rot/ }));
    expect(latest.colors).toEqual(['red']);
    expect(colors.getByRole('button', { name: /Rot/ })).toHaveAttribute('aria-pressed', 'true');
  });

  it('combines colors with OR or AND through a labelled option group and explains the choice', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterColor }));
    const popover = within(screen.getByRole('group', { name: de.pfFilterColor }));
    const mode = popover.getByRole('group', { name: de.pfColorModeLegend });
    expect(within(mode).getByRole('radio', { name: de.pfModeOr })).toBeChecked();
    expect(popover.getByText(de.pfOrDesc)).toBeInTheDocument();
    fireEvent.click(within(mode).getByRole('radio', { name: de.pfModeAnd }));
    expect(latest.colorMode).toBe('and');
    expect(popover.getByText(de.pfAndDesc)).toBeInTheDocument();
  });

  it('resets only its own filter and closes with its Close button', () => {
    render(<Harness initial={{ ...EMPTY_FILTER, nozzles: [0.4], layers: [0.2] }} />);
    fireEvent.click(screen.getByRole('button', { name: new RegExp(de.pfFilterNozzle) }));
    fireEvent.click(screen.getByRole('button', { name: de.pfReset }));
    expect(latest.nozzles).toEqual([]);
    expect(latest.layers).toEqual([0.2]);
    expect(screen.getByRole('button', { name: de.pfReset })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: de.pfClose }));
    expect(screen.queryByRole('group')).toBeNull();
    expect(screen.getByRole('button', { name: new RegExp(de.pfFilterNozzle) })).toHaveFocus();
  });

  it('keeps the hit count live', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: de.pfFilterPrinter }));
    const hits = screen.getByText('Treffer: 7');
    expect(hits).toHaveAttribute('aria-live', 'polite');
  });
});
