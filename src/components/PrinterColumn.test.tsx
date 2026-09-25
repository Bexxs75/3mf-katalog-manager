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

const X1C: Printer = {
  id: 'p1',
  name: 'X1C',
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
    const handlers = renderColumn({ printers: [{ id: 'p2', name: 'A1 mini', units: [] }] });
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
});
