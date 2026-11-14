import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { PrintTechnologyProvider, usePrintTechnology } from './PrintTechnologyContext';
import { useCatalogFilters } from './useCatalogFilters';
import { makeModelFile, makePrinter } from '../test/factories';
import type { PrintTechnology } from '../lib/printTechnology';

let stored: PrintTechnology = 'both';
vi.mock('../lib/api/printTechnology', () => ({
  getPrintTechnology: async () => ({ value: stored, asked: true }),
  setPrintTechnology: async (value: PrintTechnology) => { stored = value; return { value, asked: true }; },
}));

const wrapper = ({ children }: { children: ReactNode }) => <PrintTechnologyProvider>{children}</PrintTechnologyProvider>;
const models = [makeModelFile({ id: 'small', dimensionsMm: [50, 50, 50] }), makeModelFile({ id: 'wide', dimensionsMm: [300, 50, 50] })];
const saturn = makePrinter({ id: 'saturn', name: 'Saturn', kind: 'resin', bedXMm: 100, bedYMm: 100, bedZMm: 100 });
const qidi = makePrinter({ id: 'qidi', name: 'Qidi', kind: 'filament', bedXMm: 100, bedYMm: 100, bedZMm: 100 });

beforeEach(() => { stored = 'both'; });

function mount(printers = [saturn, qidi]) {
  return renderHook(() => ({ filters: useCatalogFilters(models, [], 'de', printers), tech: usePrintTechnology() }), { wrapper });
}

describe('the fit filter follows the print technology', () => {
  it('lets a resin printer filter while both technologies are shown', async () => {
    const { result } = mount();
    act(() => result.current.filters.setFitPrinterId('saturn'));
    expect(result.current.filters.fitPrinterId).toBe('saturn');
    expect(result.current.filters.filtered.map(m => m.id)).toEqual(['small']);
  });

  it('drops a chosen resin printer once the choice is filament and brings it back with "both"', async () => {
    const { result } = mount();
    act(() => result.current.filters.setFitPrinterId('saturn'));
    await act(async () => { await result.current.tech.save('filament'); });
    // Not hanging: no chip, no hidden narrowing of the list.
    expect(result.current.filters.fitPrinterId).toBeNull();
    expect(result.current.filters.fitBed).toBeNull();
    expect(result.current.filters.filtered).toHaveLength(2);
    // The printer is still the chosen one underneath and returns with the technology.
    await act(async () => { await result.current.tech.save('both'); });
    expect(result.current.filters.fitPrinterId).toBe('saturn');
    expect(result.current.filters.filtered.map(m => m.id)).toEqual(['small']);
  });

  it('keeps a filament printer for the filament choice and refuses it for resin', async () => {
    const { result } = mount();
    await act(async () => { await result.current.tech.save('filament'); });
    act(() => result.current.filters.setFitPrinterId('qidi'));
    expect(result.current.filters.fitPrinterId).toBe('qidi');
    act(() => result.current.filters.setFitPrinterId('saturn'));
    expect(result.current.filters.fitPrinterId).toBeNull();
    await act(async () => { await result.current.tech.save('resin'); });
    act(() => result.current.filters.setFitPrinterId('qidi'));
    expect(result.current.filters.fitPrinterId).toBeNull();
    act(() => result.current.filters.setFitPrinterId('saturn'));
    expect(result.current.filters.fitPrinterId).toBe('saturn');
    await waitFor(() => expect(stored).toBe('resin'));
  });
});
