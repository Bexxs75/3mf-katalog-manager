import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentDashboard } from './FilamentDashboard';
import type { FilamentSpool } from '../types';

const S: FilamentSpool = {
  id: 'a', material: 'PETG', manufacturer: null, color: 'Rot', location: 'Regal 1',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 800, price: null, imagePng: null,
  colorHex: null, homeLocation: null, unitId: null, slotIndex: null, kind: 'filament',
};

function renderDashboard(props: Partial<Parameters<typeof FilamentDashboard>[0]> = {}) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onEdit = vi.fn();
  const onRestock = vi.fn();
  render(
    <LanguageProvider>
      <FilamentDashboard
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

describe('FilamentDashboard restock', () => {
  it('passes the spool and the clicked button as anchor', () => {
    const { onRestock } = renderDashboard();
    const button = within(screen.getByTestId('spool-card-a')).getByRole('button', { name: 'Nachkaufen' });
    fireEvent.click(button);
    expect(onRestock).toHaveBeenCalledWith(S, button);
  });

  it('marks the open popover on its button', () => {
    renderDashboard({ restockOpenId: 'a' });
    expect(screen.getByRole('button', { name: 'Nachkaufen' })).toHaveAttribute('aria-expanded', 'true');
  });

  it('highlights freshly created spools', () => {
    renderDashboard({ highlightIds: new Set(['a']) });
    expect(screen.getByTestId('spool-card-a')).toHaveClass('spool-new');
  });
});

describe('FilamentDashboard double-click', () => {
  it('opens the edit form on a double-click on the card', () => {
    const { onEdit } = renderDashboard();
    fireEvent.doubleClick(screen.getByText('PETG'));
    expect(onEdit).toHaveBeenCalledWith(S);
  });

  it('ignores double-clicks on the card buttons', () => {
    const { onEdit } = renderDashboard();
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Nachkaufen' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Spule bearbeiten' }));
    expect(onEdit).not.toHaveBeenCalled();
  });

  it('ignores double-clicks while the delete confirmation is open', () => {
    const { onEdit } = renderDashboard({ confirmDeleteId: 'a' });
    fireEvent.doubleClick(screen.getByTestId('spool-card-a'));
    expect(onEdit).not.toHaveBeenCalled();
  });
});

const R: FilamentSpool = {
  ...S, id: 'r', kind: 'resin', material: 'Standard', color: 'Grau', colorHex: '#8a8f98',
  originalWeightG: 1000, remainingWeightG: 640.5, location: 'Resin-Schrank',
};

describe('FilamentDashboard resin', () => {
  it('shows a bottle, ml and the bottle size instead of the diameter', () => {
    renderDashboard({ spools: [R] });
    const card = screen.getByTestId('spool-card-r');
    expect(within(card).getByTestId('resin-bottle-icon')).toBeInTheDocument();
    expect(card).toHaveTextContent('640,5 ml');
    expect(card).toHaveTextContent('1.000 ml Flasche');
    expect(card).not.toHaveTextContent('1,75 mm');
  });

  it('keeps grams and the diameter for filament', () => {
    renderDashboard();
    const card = screen.getByTestId('spool-card-a');
    expect(card).toHaveTextContent('800,0 g');
    expect(card).toHaveTextContent('1,75 mm');
  });
});
