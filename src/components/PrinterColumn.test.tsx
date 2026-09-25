import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterColumn } from './PrinterColumn';
import type { FilamentSpool, Printer } from '../types';

function spool(overrides: Partial<FilamentSpool>): FilamentSpool {
  return {
    id: 's1', material: 'PLA', manufacturer: null, color: 'Schwarz', location: null,
    diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 620, price: null, imagePng: null,
    colorHex: '#1a1a1a', homeLocation: null, unitId: null, slotIndex: null, kind: 'filament', ...overrides,
  };
}

const SATURN: Printer = {
  id: 'p9',
  name: 'Saturn 4',
  kind: 'resin',
  units: [{ id: 'vat', printerId: 'p9', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null }],
};

const BOTTLE = spool({
  id: 'bottle', kind: 'resin', material: 'Standard', color: 'Grau', location: 'Resin-Schrank',
  colorHex: '#8a8f98', remainingWeightG: 620,
});

const X1C: Printer = {
  id: 'p1',
  name: 'X1C',
  kind: 'filament',
  units: [{ id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 }],
};

function renderColumn(props: Partial<Parameters<typeof PrinterColumn>[0]> = {}) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const handlers = {
    onSlotMouseDown: vi.fn(),
    onEnterSlot: vi.fn(),
    onLeaveSlot: vi.fn(),
    onLoad: vi.fn(),
    onUnload: vi.fn(),
    onEditSpool: vi.fn(),
    onManage: vi.fn(),
  };
  render(
    <LanguageProvider>
      <PrinterColumn
        printers={[X1C]}
        spools={[
          spool({ id: 'in', unitId: 'u1', slotIndex: 0, homeLocation: 'Regal 2' }),
          spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1', colorHex: '#c0392b' }),
        ]}
        draggingSpoolId={null}
        dropTarget={null}
        {...handlers}
        {...props}
      />
    </LanguageProvider>,
  );
  return handlers;
}

describe('PrinterColumn', () => {
  it('shows an empty state with a button to add a printer', () => {
    const { onManage } = renderColumn({ printers: [] });
    fireEvent.click(screen.getByRole('button', { name: 'Drucker anlegen' }));
    expect(onManage).toHaveBeenCalled();
  });

  it('tells how to add slots to a printer without units and opens the management on click', () => {
    const handlers = renderColumn({ printers: [{ id: 'p2', name: 'A1 mini', kind: 'filament', units: [] }] });
    const hint = screen.getByRole('button', { name: 'Keine Fächer – über „Drucker verwalten“ hinzufügen' });
    fireEvent.click(hint);
    expect(handlers.onManage).toHaveBeenCalled();
  });

  it('shows every slot with its loaded spool and the occupancy', () => {
    renderColumn();
    expect(screen.getByText('X1C')).toBeInTheDocument();
    expect(screen.getByText('1/4')).toBeInTheDocument();
    expect(screen.getByTestId('slot-u1-0')).toHaveTextContent('PLA · Schwarz');
    expect(screen.getAllByText('leer')).toHaveLength(3);
  });

  it('loads a storage spool through the slot menu', () => {
    const { onLoad } = renderColumn();
    fireEvent.click(screen.getByTestId('slot-u1-2'));
    fireEvent.click(screen.getByRole('button', { name: /PETG · Rot/ }));
    expect(onLoad).toHaveBeenCalledWith('store', 'u1', 2);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('unloads and edits a loaded spool through the slot menu', () => {
    const { onUnload, onEditSpool } = renderColumn();
    fireEvent.click(screen.getByTestId('slot-u1-0'));
    fireEvent.click(screen.getByRole('button', { name: 'Herausnehmen' }));
    expect(onUnload).toHaveBeenCalledWith('in');
    fireEvent.click(screen.getByTestId('slot-u1-0'));
    fireEvent.click(screen.getByRole('button', { name: 'Spule bearbeiten' }));
    expect(onEditSpool).toHaveBeenCalledWith(expect.objectContaining({ id: 'in' }));
  });

  it('filters the spools offered in the menu', () => {
    renderColumn();
    fireEvent.click(screen.getByTestId('slot-u1-1'));
    fireEvent.change(screen.getByPlaceholderText('Spule suchen…'), { target: { value: 'abs' } });
    expect(screen.getByText('Keine passenden Spulen im Lager.')).toBeInTheDocument();
  });

  it('highlights the slot under a dragged spool', () => {
    renderColumn({ draggingSpoolId: 'store', dropTarget: { kind: 'slot', unitId: 'u1', slotIndex: 3 } });
    expect(screen.getByTestId('slot-u1-3')).toHaveTextContent('hier ablegen');
  });

  it('starts a drag only from occupied slots', () => {
    const { onSlotMouseDown } = renderColumn();
    fireEvent.mouseDown(screen.getByTestId('slot-u1-1'));
    expect(onSlotMouseDown).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByTestId('slot-u1-0'));
    expect(onSlotMouseDown).toHaveBeenCalledWith('in', 'u1', 0, expect.anything());
  });

  it('shows only filament printers in the filament view', () => {
    renderColumn({ printers: [X1C, SATURN] });
    expect(screen.getByText('X1C')).toBeInTheDocument();
    expect(screen.queryByText('Saturn 4')).toBeNull();
  });

  it('shows only resin printers in the resin view, with their own empty hint', () => {
    renderColumn({ printers: [X1C, SATURN], kind: 'resin' });
    expect(screen.getByText('Saturn 4')).toBeInTheDocument();
    expect(screen.queryByText('X1C')).toBeNull();
    screen.getByText('Harzwanne');
    expect(screen.getByText('0/1')).toBeInTheDocument();
  });

  it('shows the resin empty hint when there is no resin printer yet', () => {
    const { onManage } = renderColumn({ printers: [X1C], kind: 'resin' });
    expect(screen.getByText(/welches Harz gerade in der Wanne ist/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Drucker anlegen' }));
    expect(onManage).toHaveBeenCalled();
  });

  it('shows the bottle in the vat with its icon and the rest in ml', () => {
    renderColumn({
      printers: [SATURN],
      kind: 'resin',
      spools: [{ ...BOTTLE, location: null, homeLocation: 'Resin-Schrank', unitId: 'vat', slotIndex: 0 }],
    });
    const vat = screen.getByTestId('slot-vat-0');
    expect(vat).toHaveTextContent('Standard · Grau');
    expect(vat).toHaveTextContent('620 ml');
    expect(vat.querySelector('[data-testid="resin-bottle-icon"]')).not.toBeNull();
    expect(screen.getByText('1/1')).toBeInTheDocument();
  });

  it('offers only resin bottles in the vat menu and never filament', () => {
    const { onLoad } = renderColumn({
      printers: [SATURN],
      kind: 'resin',
      spools: [BOTTLE, spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1' })],
    });
    fireEvent.click(screen.getByTestId('slot-vat-0'));
    const menu = screen.getByRole('menu');
    expect(menu).toHaveTextContent('Flasche einsetzen');
    expect(menu).not.toHaveTextContent('PETG');
    fireEvent.click(screen.getByRole('button', { name: /Standard · Grau/ }));
    expect(onLoad).toHaveBeenCalledWith('bottle', 'vat', 0);
  });

  it('never offers resin bottles in the menu of a filament slot', () => {
    renderColumn({
      spools: [BOTTLE, spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1' })],
    });
    fireEvent.click(screen.getByTestId('slot-u1-1'));
    const menu = screen.getByRole('menu');
    expect(menu).toHaveTextContent('PETG · Rot');
    expect(menu).not.toHaveTextContent('Standard');
  });

  it('does not accept a dragged resin bottle on a filament slot', () => {
    const { onEnterSlot } = renderColumn({
      spools: [BOTTLE],
      draggingSpoolId: 'bottle',
      dropTarget: { kind: 'slot', unitId: 'u1', slotIndex: 1 },
    });
    expect(screen.getByTestId('slot-u1-1')).not.toHaveTextContent('hier ablegen');
    fireEvent.mouseEnter(screen.getByTestId('slot-u1-2'));
    expect(onEnterSlot).not.toHaveBeenCalled();
  });

  it('does not accept a dragged filament spool on the vat', () => {
    const { onEnterSlot } = renderColumn({
      printers: [SATURN],
      kind: 'resin',
      spools: [spool({ id: 'store', location: 'Regal 1' })],
      draggingSpoolId: 'store',
      dropTarget: { kind: 'slot', unitId: 'vat', slotIndex: 0 },
    });
    expect(screen.getByTestId('slot-vat-0')).not.toHaveTextContent('hier ablegen');
    fireEvent.mouseEnter(screen.getByTestId('slot-vat-0'));
    expect(onEnterSlot).not.toHaveBeenCalled();
  });

  it('accepts a dragged resin bottle on the vat', () => {
    const { onEnterSlot } = renderColumn({
      printers: [SATURN],
      kind: 'resin',
      spools: [BOTTLE],
      draggingSpoolId: 'bottle',
      dropTarget: { kind: 'slot', unitId: 'vat', slotIndex: 0 },
    });
    expect(screen.getByTestId('slot-vat-0')).toHaveTextContent('hier ablegen');
    fireEvent.mouseEnter(screen.getByTestId('slot-vat-0'));
    expect(onEnterSlot).toHaveBeenCalledWith('vat', 0);
  });

  it('keeps "− Verbrauch" at a bottle that sits in the vat', () => {
    const onConsume = vi.fn();
    renderColumn({
      printers: [SATURN],
      kind: 'resin',
      spools: [{ ...BOTTLE, location: null, homeLocation: 'Resin-Schrank', unitId: 'vat', slotIndex: 0 }],
      onConsume,
    });
    fireEvent.click(screen.getByTestId('slot-vat-0'));
    fireEvent.click(screen.getByRole('button', { name: '− Verbrauch' }));
    expect(onConsume).toHaveBeenCalledWith(expect.objectContaining({ id: 'bottle' }), screen.getByTestId('slot-vat-0'));
  });

  it('offers no "− Verbrauch" in the menu of a filament slot', () => {
    renderColumn({ onConsume: vi.fn() });
    fireEvent.click(screen.getByTestId('slot-u1-0'));
    expect(screen.queryByRole('button', { name: '− Verbrauch' })).toBeNull();
  });
});
