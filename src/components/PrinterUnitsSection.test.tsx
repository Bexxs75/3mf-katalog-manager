import { expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import type { PrintersState } from '../hooks/usePrinters';
import type { Printer } from '../types';
import { PrinterUnitsSection } from './PrinterUnitsSection';

const printer: Printer = {
  id: 'p1', name: 'H3C', kind: 'filament', manufacturer: null, model: null,
  nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null,
  units: [{ id: 'u1', printerId: 'p1', name: 'AMS 3H', kind: 'bambu_ams_3h', slotCount: 9, bambuAmsIndex: null }],
};

function setup(language: string) {
  localStorage.setItem('3mf-katalog-language', language);
  const actions: PrintersState = {
    printers: [printer], error: null, refresh: vi.fn(), addPrinter: vi.fn(),
    renamePrinter: vi.fn(), updateDetails: vi.fn(), reorderPrinters: vi.fn(), deletePrinter: vi.fn(),
    addUnit: vi.fn().mockResolvedValue(undefined), updateUnit: vi.fn(), deleteUnit: vi.fn(), reorderUnits: vi.fn(),
  };
  render(<LanguageProvider><PrinterUnitsSection printer={printer} spools={[]} actions={actions}
    onChanged={vi.fn()} onMaterial={vi.fn()} /></LanguageProvider>);
  return actions;
}

it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)(
  'labels AMS 3H in %s and renders nine wrapping slots', (language, translations) => {
    setup(language);
    expect(screen.getByText('AMS 3H').parentElement).toHaveClass('min-w-[8rem]');
    expect(translations.printersKindBambuAms3h).toBe('Bambu AMS 3H');
    expect(screen.getByText(/^Bambu AMS 3H ·/)).toBeInTheDocument();
    const slots = screen.getAllByTitle(/^\d+ ·/);
    expect(slots).toHaveLength(9);
    expect(slots[8]).toHaveAttribute('aria-label', `9 · ${translations.printersSlotEmpty}`);
    expect(slots[8].parentElement).toHaveClass('flex-wrap');
    fireEvent.click(screen.getByRole('button', { name: translations.pmEdit }));
    expect(within(screen.getByRole('dialog')).queryByRole('combobox')).toBeNull();
  },
);

it('adds the nine-slot template with the default name and no custom slot count', async () => {
  const actions = setup('de');
  fireEvent.click(screen.getByRole('button', { name: de.printersAddUnitButton }));
  const dialog = screen.getByRole('dialog');
  fireEvent.click(within(dialog).getByRole('button', { name: 'Bambu AMS 3H · 9' }));
  expect(within(dialog).getByLabelText(de.pmName)).toHaveValue('AMS 3H 2');
  fireEvent.submit(dialog.querySelector('form')!);
  await waitFor(() => expect(actions.addUnit).toHaveBeenCalledWith('p1', 'bambu_ams_3h', 'AMS 3H 2', null));
});
