import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentTable } from './FilamentTable';
import type { FilamentSpool } from '../types';
import { groupFilaments } from '../lib/filamentGroups';

const S: FilamentSpool = {
  id: 'a', material: 'PETG', manufacturer: null, color: 'Rot', location: 'Regal 1',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 800, price: null, imagePng: null,
  colorHex: null, colorHexes: [], homeLocation: null, unitId: null, slotIndex: null, kind: 'filament',
};

function renderTable(props: Partial<Parameters<typeof FilamentTable>[0]> = {}) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onEdit = vi.fn();
  const onRestock = vi.fn();
  render(
    <LanguageProvider>
      <FilamentTable
        spools={[S]}
        confirmDeleteId={null}
        onEdit={onEdit}
        onRequestDelete={vi.fn()}
        onCancelDelete={vi.fn()}
        onConfirmDelete={vi.fn()}
        onRestock={onRestock}
        {...props}
      />
    </LanguageProvider>,
  );
  return { onEdit, onRestock };
}

describe('FilamentTable restock', () => {
  it('offers restock in the row', () => {
    const { onRestock } = renderTable();
    const button = screen.getByRole('button', { name: 'Nachkaufen' });
    fireEvent.click(button);
    expect(onRestock).toHaveBeenCalledWith(S, button);
  });

  it('highlights freshly created rows', () => {
    renderTable({ highlightIds: new Set(['a']) });
    expect(screen.getByTestId('spool-row-a')).toHaveClass('spool-new-row');
  });
});

describe('FilamentTable double-click', () => {
  it('opens the edit form on a double-click on the row', () => {
    const { onEdit } = renderTable();
    fireEvent.doubleClick(screen.getByText('PETG'));
    expect(onEdit).toHaveBeenCalledWith(S);
  });

  it('ignores double-clicks on the row buttons', () => {
    const { onEdit } = renderTable();
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Nachkaufen' }));
    expect(onEdit).not.toHaveBeenCalled();
  });

  it('ignores double-clicks while the delete confirmation is open', () => {
    const { onEdit } = renderTable({ confirmDeleteId: 'a' });
    fireEvent.doubleClick(screen.getByTestId('spool-row-a'));
    expect(onEdit).not.toHaveBeenCalled();
  });
});

describe('FilamentTable resin', () => {
  it('drops the diameter column and shows ml', () => {
    const R: FilamentSpool = { ...S, id: 'r', kind: 'resin', remainingWeightG: 90 };
    renderTable({ spools: [R], kind: 'resin' });
    expect(screen.queryByRole('columnheader', { name: /⌀/ })).toBeNull();
    expect(screen.getByTestId('spool-row-r')).toHaveTextContent('90 ml');
  });
});

describe('FilamentTable spool icon', () => {
  it('shows a spool icon in the spool color for filament without an image', () => {
    renderTable({ spools: [{ ...S, colorHex: '#c0392b' }] });
    const icon = screen.getByTestId('filament-spool-icon');
    expect(icon.querySelector('circle[fill="#c0392b"]')).not.toBeNull();
  });

  it('prefers the own image over the spool icon', () => {
    renderTable({ spools: [{ ...S, imagePng: 'AAAA' }] });
    expect(screen.queryByTestId('filament-spool-icon')).toBeNull();
  });
});

function mk(id: string, over: Partial<FilamentSpool> = {}): FilamentSpool {
  return {
    ...S, id, material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', colorHex: '#1a1a1a', location: 'Regal A',
    originalWeightG: 1000, remainingWeightG: 1000, ...over,
  };
}

describe('FilamentTable grouped rows', () => {
  const a = mk('a1', { remainingWeightG: 920 });
  const b = mk('a2', { remainingWeightG: 640 });
  const lone = mk('c1', { material: 'ABS', color: 'Grau', colorHex: '#8a8d91', location: 'Regal B' });

  function renderGroups(extra: Partial<Parameters<typeof FilamentTable>[0]> = {}) {
    const spools = [a, b, lone];
    return renderTable({ spools, groups: groupFilaments(spools), showCount: true, ...extra });
  }

  it('shows one row per group with count and total, and a count column header', () => {
    renderGroups();
    expect(screen.getByRole('columnheader', { name: 'Stück' })).toBeInTheDocument();
    const row = screen.getByTestId('group-row-a1');
    expect(row).toHaveTextContent('1.560,0 g');
    expect(within(row).getAllByRole('cell')[7]).toHaveTextContent('2');
    expect(screen.queryByTestId('spool-row-a1')).toBeNull();
    expect(within(screen.getByTestId('spool-row-c1')).getAllByRole('cell')[7]).toHaveTextContent('1');
  });

  it('expands the group to its spools and collapses it again', () => {
    renderGroups();
    const toggle = screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('spool-row-a1')).toHaveTextContent('920,0 g');
    expect(screen.getByTestId('spool-row-a2')).toHaveTextContent('640,0 g');
    fireEvent.click(toggle);
    expect(screen.queryByTestId('spool-row-a1')).toBeNull();
  });

  it('restocks from the group row but not from the rows inside', () => {
    const onRestock = vi.fn();
    renderGroups({ onRestock });
    const groupRestock = within(screen.getByTestId('group-row-a1')).getByRole('button', { name: /Nachkaufen/ });
    fireEvent.click(groupRestock);
    expect(onRestock).toHaveBeenCalledWith(a, groupRestock);
    fireEvent.click(screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' }));
    expect(within(screen.getByTestId('spool-row-a2')).queryByRole('button', { name: /Nachkaufen/ })).toBeNull();
  });

  it('edits a single spool inside the group', () => {
    const onEdit = vi.fn();
    renderGroups({ onEdit });
    fireEvent.click(screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' }));
    fireEvent.click(within(screen.getByTestId('spool-row-a2')).getByRole('button', { name: /Spule bearbeiten/ }));
    expect(onEdit).toHaveBeenCalledWith(b);
  });

  it('shows the unit, slots and booking hint of a loaded group and does not drag it', () => {
    const loaded = [
      mk('u1', { unitId: 'u1', slotIndex: 0, location: null, homeLocation: 'Regal A' }),
      mk('u2', { unitId: 'u1', slotIndex: 2, location: null, homeLocation: 'Regal A' }),
    ];
    const onSpoolMouseDown = vi.fn();
    renderTable({ spools: loaded, groups: groupFilaments(loaded, () => 'X1C · AMS 1'), showCount: true, onSpoolMouseDown });
    const row = screen.getByTestId('group-row-u1');
    expect(row).toHaveTextContent('X1C · AMS 1');
    expect(row).toHaveTextContent('Fach 1, Fach 3');
    expect(row).toHaveTextContent('Eingelegt');
    fireEvent.mouseDown(row);
    expect(onSpoolMouseDown).not.toHaveBeenCalled();
  });

  it('sorts groups by their totals', () => {
    const heavy = [mk('h1', { material: 'AAA', remainingWeightG: 1000 }), mk('h2', { material: 'AAA', remainingWeightG: 1000 })];
    const light = mk('l1', { material: 'ZZZ', remainingWeightG: 100 });
    const spools = [...heavy, light];
    renderTable({ spools, groups: groupFilaments(spools), showCount: true });
    fireEvent.click(screen.getByRole('columnheader', { name: /Bestand/ }));
    const rows = screen.getAllByTestId(/^(group|spool)-row-/);
    expect(rows.map((r) => r.getAttribute('data-testid'))).toEqual(['spool-row-l1', 'group-row-h1']);
  });

  it('has no count column in the single view', () => {
    renderTable({ spools: [a, b] });
    expect(screen.queryByRole('columnheader', { name: 'Stück' })).toBeNull();
    expect(screen.getByTestId('spool-row-a1')).toBeInTheDocument();
    expect(screen.getByTestId('spool-row-a2')).toBeInTheDocument();
  });
});
