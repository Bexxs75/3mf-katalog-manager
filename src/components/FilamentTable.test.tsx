import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentTable } from './FilamentTable';
import type { FilamentSpool } from '../types';

const S: FilamentSpool = {
  id: 'a', material: 'PETG', manufacturer: null, color: 'Rot', location: 'Regal 1',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 800, price: null, imagePng: null,
  colorHex: null, homeLocation: null, unitId: null, slotIndex: null, kind: 'filament',
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
