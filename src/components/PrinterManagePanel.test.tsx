import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterManagePanel, suggestUnitName } from './PrinterManagePanel';
import type { PrinterActions } from './PrinterManagePanel';
import type { FilamentSpool, Printer } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';

const X1C: Printer = {
  id: 'p1',
  name: 'X1C',
  kind: 'filament',
  units: [
    { id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 },
    { id: 'u2', printerId: 'p1', name: 'AMS B', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 1 },
    { id: 'u3', printerId: 'p1', name: 'Box', kind: 'custom', slotCount: 6, bambuAmsIndex: null },
  ],
};

const LOADED = {
  id: 's1', material: 'PLA', manufacturer: null, color: null, location: null, diameterMm: 1.75,
  originalWeightG: 1000, remainingWeightG: 500, price: null, imagePng: null, colorHex: null,
  homeLocation: 'Regal 1', unitId: 'u1', slotIndex: 0, kind: 'filament',
} satisfies FilamentSpool;

function actions(): PrinterActions {
  return {
    addPrinter: vi.fn().mockResolvedValue({}),
    renamePrinter: vi.fn().mockResolvedValue(undefined),
    deletePrinter: vi.fn().mockResolvedValue(1),
    addUnit: vi.fn().mockResolvedValue({}),
    updateUnit: vi.fn().mockResolvedValue(0),
    deleteUnit: vi.fn().mockResolvedValue(1),
    reorderUnits: vi.fn().mockResolvedValue(undefined),
  };
}

const SATURN: Printer = {
  id: 'p9',
  name: 'Saturn 4',
  kind: 'resin',
  units: [{ id: 'vat', printerId: 'p9', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null }],
};

function renderPanel(printers: Printer[] = [X1C], printerLink?: PrinterLinkState) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const a = actions();
  const onSpoolsChanged = vi.fn();
  render(
    <LanguageProvider>
      <PrinterManagePanel
        open
        printers={printers}
        spools={[LOADED]}
        error={null}
        actions={a}
        onClose={vi.fn()}
        onSpoolsChanged={onSpoolsChanged}
        printerLink={printerLink}
      />
    </LanguageProvider>,
  );
  return { a, onSpoolsChanged };
}

describe('suggestUnitName', () => {
  it('letters AMS units and numbers repeated other units', () => {
    expect(suggestUnitName(X1C, 'bambu_ams', 'AMS')).toBe('AMS C');
    expect(suggestUnitName(X1C, 'external', 'Spulenhalter')).toBe('Spulenhalter');
    expect(suggestUnitName({ ...X1C, units: [...X1C.units, { ...X1C.units[0], id: 'e', kind: 'external' }] }, 'external', 'Spulenhalter')).toBe('Spulenhalter 2');
  });
});

describe('PrinterManagePanel', () => {
  it('adds a printer', async () => {
    const { a } = renderPanel([]);
    fireEvent.change(screen.getByPlaceholderText('Name, z. B. X1C'), { target: { value: 'X1C' } });
    fireEvent.click(screen.getByRole('button', { name: 'Drucker hinzufügen' }));
    await waitFor(() => expect(a.addPrinter).toHaveBeenCalledWith('X1C', 'Spulenhalter', 'filament'));
  });

  it('adds a unit from a template with a suggested name', () => {
    const { a } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: /Einheit hinzufügen/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Bambu AMS\s*4/ }));
    expect(a.addUnit).toHaveBeenCalledWith('p1', 'bambu_ams', 'AMS C', null);
  });

  it('names a spool holder added from the template menu in the UI language', () => {
    const { a } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: /Einheit hinzufügen/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Spulenhalter/ }));
    expect(a.addUnit).toHaveBeenCalledWith('p1', 'external', 'Spulenhalter', null);
  });

  it('adds a custom unit with its own slot count', async () => {
    const { a } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: /Einheit hinzufügen/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Eigene/ }));
    fireEvent.change(screen.getByPlaceholderText('Name der Einheit'), { target: { value: 'Box Turtle' } });
    const steppers = screen.getAllByRole('button', { name: '+' });
    fireEvent.click(steppers[steppers.length - 1]);
    fireEvent.click(screen.getByRole('button', { name: 'Einheit hinzufügen' }));
    await waitFor(() => expect(a.addUnit).toHaveBeenCalledWith('p1', 'custom', 'Box Turtle', 5));
  });

  it('confirms deleting a unit with the number of spools that go home', async () => {
    const { a, onSpoolsChanged } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'Löschen AMS A' }));
    const question = screen.getByText('„AMS A“ entfernen? 1 Spule kehrt an ihren Stammplatz zurück.');
    fireEvent.click(within(question.parentElement as HTMLElement).getByRole('button', { name: 'Löschen' }));
    await waitFor(() => expect(a.deleteUnit).toHaveBeenCalledWith('u1'));
    await waitFor(() => expect(onSpoolsChanged).toHaveBeenCalled());
  });

  it('confirms deleting a unit without any spools without the return-home sentence', async () => {
    const { a } = renderPanel();
    // "Box" (u3) hat keine geladenen Spulen in diesem Test-Setup.
    fireEvent.click(screen.getByRole('button', { name: 'Löschen Box' }));
    const question = screen.getByText('„Box“ entfernen?');
    fireEvent.click(within(question.parentElement as HTMLElement).getByRole('button', { name: 'Löschen' }));
    await waitFor(() => expect(a.deleteUnit).toHaveBeenCalledWith('u3'));
  });

  it('renames a printer', async () => {
    const { a } = renderPanel();
    fireEvent.click(screen.getAllByRole('button', { name: 'Umbenennen' })[0]);
    const input = screen.getByDisplayValue('X1C');
    fireEvent.change(input, { target: { value: 'X1 Carbon' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(a.renamePrinter).toHaveBeenCalledWith('p1', 'X1 Carbon'));
  });

  it('changes the slot count of a custom unit', async () => {
    const { a } = renderPanel();
    fireEvent.click(screen.getAllByRole('button', { name: '−' })[0]);
    await waitFor(() => expect(a.updateUnit).toHaveBeenCalledWith('u3', 'Box', 5));
  });

  it('reorders units by dragging the handle', async () => {
    const { a } = renderPanel();
    fireEvent.mouseDown(screen.getAllByRole('button', { name: 'Zum Sortieren am Griff ziehen' })[2]);
    fireEvent.mouseEnter(screen.getByTestId('unit-row-u1'));
    fireEvent.mouseUp(document);
    await waitFor(() => expect(a.reorderUnits).toHaveBeenCalledWith('p1', ['u3', 'u1', 'u2']));
  });

  it('adds a resin printer with its resin vat when Resin is chosen', async () => {
    const { a } = renderPanel([]);
    fireEvent.change(screen.getByPlaceholderText('Name, z. B. X1C'), { target: { value: 'Saturn 4' } });
    const kind = screen.getByRole('group', { name: 'Art' });
    expect(within(kind).getByRole('button', { name: 'Filament' })).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(within(kind).getByRole('button', { name: 'Resin' }));
    fireEvent.click(screen.getByRole('button', { name: 'Drucker hinzufügen' }));
    await waitFor(() => expect(a.addPrinter).toHaveBeenCalledWith('Saturn 4', 'Harzwanne', 'resin'));
  });

  it('shows a resin printer with its fixed vat: no rename, delete, reorder or add unit', () => {
    renderPanel([SATURN]);
    const row = screen.getByTestId('unit-row-vat');
    expect(row).toHaveTextContent('Harzwanne');
    expect(row).toHaveTextContent('1 Flasche');
    expect(within(row).queryByRole('button')).toBeNull();
    expect(screen.queryByRole('button', { name: /Einheit hinzufügen/ })).toBeNull();
    expect(screen.getByText('Resin-Drucker haben nur ihre Harzwanne und keine Druckeranbindung.')).toBeInTheDocument();
    // Der Drucker selbst bleibt umbenenn- und loeschbar.
    expect(screen.getByRole('button', { name: 'Umbenennen' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Löschen' })).toBeInTheDocument();
  });

  it('shows the printer kind next to each printer', () => {
    renderPanel([X1C, SATURN]);
    expect(screen.getByTestId('printer-kind-p1')).toHaveTextContent('Filament');
    expect(screen.getByTestId('printer-kind-p9')).toHaveTextContent('Resin');
  });

  it('shows the connection section only for filament printers', () => {
    const link = {
      enabled: true, connections: [], jobs: [], error: null,
      refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
      removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
    } as unknown as PrinterLinkState;
    renderPanel([X1C, SATURN], link);
    const cards = screen.getAllByTestId(/^printer-card-/);
    expect(cards).toHaveLength(2);
    const x1c = screen.getByTestId('printer-card-p1');
    const saturn = screen.getByTestId('printer-card-p9');
    expect(within(x1c).getByPlaceholderText('192.168.1.60')).toBeInTheDocument();
    expect(within(saturn).queryByPlaceholderText('192.168.1.60')).toBeNull();
  });
});
