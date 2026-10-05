import { render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it } from 'vitest';
import { StatusBadges } from './StatusBadges';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile } from '../test/factories';
import type { FilamentCheck } from '../types';
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
function show(extra: Partial<Parameters<typeof StatusBadges>[0]> = {}) {
  return render(<LanguageProvider><StatusBadges model={makeModelFile({ materials: [], queuePosition: null, favorite: false })} {...extra} /></LanguageProvider>);
}
it.each(['printed', 'not_printed'] as const)('shows %s and list semantics', printStatus => {
  show({ model: makeModelFile({ printStatus, materials: [], queuePosition: null, favorite: false }) });
  expect(within(screen.getByRole('list', { name: 'Status' })).getAllByRole('listitem')).toHaveLength(1);
  expect(screen.getByText(printStatus === 'printed' ? 'Gedruckt' : 'Nicht gedruckt')).toBeVisible();
});
it.each([null, '#ff0000'])('shows material names with color %s', displayColor => {
  const { container } = show({ model: makeModelFile({ materials: [{ name: 'PLA', displayColor }, { name: 'PETG', displayColor: null }] }) });
  expect(screen.getByText('PLA, PETG')).toBeVisible();
  expect(container.querySelector('.status-material-dot') !== null).toBe(displayColor !== null);
});
it.each([
  ['ok', 'Filament reicht'], ['swap', 'Reicht mit Spulenwechsel'],
  ['short', 'Filament reicht nicht · es fehlen 3,3 g'], ['unknown', 'Filament unklar'],
] as const)('shows filament %s', (status, label) => {
  show({ filament: { fileId: '1', status, needs: [{ missingG: 1.11 }, { missingG: 2.22 }] } as FilamentCheck });
  expect(screen.getByText(label)).toBeVisible();
});
it.each([null, { fileId: '1', status: 'no_data', needs: [] } as FilamentCheck])('omits filament without data', filament => {
  show({ filament });
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
});
it('shows optional badges in the specified order', () => {
  show({ model: makeModelFile({ printStatus: 'printed', favorite: true, queuePosition: 0, materials: [{ name: 'PLA', displayColor: null }] }),
    filament: { fileId: '1', status: 'ok', needs: [] } as FilamentCheck,
    lastPrinter: { printerName: 'Printer A', endedAt: Date.now() / 1000 - 3 * 86400 } });
  expect(screen.getAllByRole('listitem').map(node => node.textContent)).toEqual(['Gedruckt', 'PLA', '✓Filament reicht', 'Favorit', 'In Warteschlange', 'Printer A · vor 3 Tagen']);
});
it('omits optional badges when absent', () => {
  show();
  expect(screen.queryByText('Favorit')).toBeNull();
  expect(screen.queryByText('In Warteschlange')).toBeNull();
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
});
