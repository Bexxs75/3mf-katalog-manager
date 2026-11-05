import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { PrinterStatistics } from './PrinterStatistics';
import { getPrintStatistics } from '../lib/api/printers';
import type { Printer, PrintStatRow, PrintStatistics } from '../types';

vi.mock('../lib/api/printers', () => ({ getPrintStatistics: vi.fn() }));

const DAY = 86_400;
const nowS = () => Math.floor(Date.now() / 1000);
const printers = [{ id: 'a', name: 'Voron' }, { id: 'b', name: 'Ender' }] as Printer[];
const row = (over: Partial<PrintStatRow> = {}): PrintStatRow => ({
  printerId: 'a', endedAt: nowS() - DAY, outcome: 'completed', rawStatus: 'completed', printDurationS: 3600,
  grams: null, usedMm: 0, material: null, state: 'confirmed', autoClosed: false, failureReason: null, ...over,
});
const result = (rows: PrintStatRow[], over: Partial<PrintStatistics> = {}): PrintStatistics => ({ rows, truncated: false, firstConnectedAt: null, ...over });
const show = () => render(<LanguageProviderWithDiagnostics><PrinterStatistics printers={printers} printerId="a" /></LanguageProviderWithDiagnostics>);

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'en'); vi.mocked(getPrintStatistics).mockReset(); });
afterEach(() => { cleanup(); vi.unstubAllEnvs(); });

describe('PrinterStatistics', () => {
  it('shows the empty state with a pointer to the connection', async () => {
    vi.mocked(getPrintStatistics).mockResolvedValue(result([]));
    show();
    expect(await screen.findByText('No prints from the printer connection yet')).toBeInTheDocument();
    expect(screen.getByText(/Set up the connection/)).toBeInTheDocument();
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('shows key figures, reasons and material from the rows', async () => {
    vi.mocked(getPrintStatistics).mockResolvedValue(result([
      row({ grams: 10, material: 'PLA' }), row({ grams: 5, material: 'PLA' }),
      row({ rawStatus: 'cancelled', failureReason: 'clog' }), row({ rawStatus: 'error' }),
    ], { firstConnectedAt: 1_700_000_000 }));
    show();
    expect(await screen.findByText('2 succeeded · 0 cancelled · 2 failed')).toBeInTheDocument();
    expect(screen.getAllByText('50 %').length).toBeGreaterThan(0);
    expect(screen.getAllByText('15 g').length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText('2 prints without grams').length).toBeGreaterThan(0);
    expect(screen.getByText(/Counted since .*2023/)).toBeInTheDocument();
    expect(screen.getByText('Nozzle clogged / under-extrusion')).toBeInTheDocument();
    expect(screen.getByText('No reason given')).toBeInTheDocument();
    expect(screen.getAllByText('PLA').length).toBeGreaterThan(0);
    expect(screen.getByRole('img', { name: /Bar chart\./ })).toBeInTheDocument();
  });

  it('offers the chart as a table as well, and a stopped note when rows were cut off', async () => {
    vi.mocked(getPrintStatistics).mockResolvedValue(result([row(), row({ rawStatus: 'cancelled' })], { truncated: true }));
    show();
    expect(await screen.findByText('Only the newest 5000 prints')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Table' }));
    const table = screen.getByRole('table', { name: 'Prints per month' });
    const tableRows = within(table).getAllByRole('row');
    const last = tableRows[tableRows.length - 1];
    expect(within(last).getAllByRole('cell').map(c => c.textContent)).toEqual(['1', '1', '0', '2']);
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('filters by period and says so when a period has no prints', async () => {
    vi.mocked(getPrintStatistics).mockResolvedValue(result([row({ endedAt: nowS() - 100 * DAY })]));
    show();
    expect(await screen.findByText('1 of 1 prints succeeded')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '30 days' }));
    expect(screen.getByText('There were no prints in this period.')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Statistics for All printers, 30 days: 0 prints');
  });

  it('asks for one printer when chosen and follows the printer picked in the list', async () => {
    vi.mocked(getPrintStatistics).mockResolvedValue(result([row(), row({ printerId: 'b' })]));
    const { rerender } = show();
    await screen.findByText('Per printer');
    expect(getPrintStatistics).toHaveBeenLastCalledWith(undefined);
    fireEvent.change(screen.getByLabelText('Printer'), { target: { value: 'b' } });
    expect(getPrintStatistics).toHaveBeenLastCalledWith('b');
    rerender(<LanguageProviderWithDiagnostics><PrinterStatistics printers={printers} printerId="a" /></LanguageProviderWithDiagnostics>);
    rerender(<LanguageProviderWithDiagnostics><PrinterStatistics printers={printers} printerId="b" /></LanguageProviderWithDiagnostics>);
    expect(screen.queryByText('Per printer')).toBeNull();
  });

  it('says in the web version that no printers are queried', async () => {
    vi.stubEnv('VITE_TARGET', 'web');
    vi.mocked(getPrintStatistics).mockResolvedValue(result([]));
    show();
    expect(await screen.findByText('This server does not query printers; the data from the database is shown.')).toBeInTheDocument();
  });

  it('reports a failed load', async () => {
    vi.mocked(getPrintStatistics).mockRejectedValue('boom');
    show();
    expect(await screen.findByRole('alert')).toHaveTextContent('The statistics could not be loaded.');
  });
});
