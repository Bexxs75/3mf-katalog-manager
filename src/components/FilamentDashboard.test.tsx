import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentDashboard } from './FilamentDashboard';
import type { FilamentSpool } from '../types';
import { groupFilaments } from '../lib/filamentGroups';

const S: FilamentSpool = {
  id: 'a', material: 'PETG', manufacturer: null, color: 'Rot', location: 'Regal 1',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 800, price: null, imagePng: null,
  colorHex: null, colorHexes: [], homeLocation: null, unitId: null, slotIndex: null, kind: 'filament',
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

describe('FilamentDashboard consume', () => {
  it('offers "Verbrauch" only on resin bottles', () => {
    const onConsume = vi.fn();
    renderDashboard({ spools: [S, R], onConsume });
    expect(within(screen.getByTestId('spool-card-a')).queryByRole('button', { name: 'Verbrauch' })).toBeNull();
    const button = within(screen.getByTestId('spool-card-r')).getByRole('button', { name: 'Verbrauch' });
    fireEvent.click(button);
    expect(onConsume).toHaveBeenCalledWith(R, button);
  });
});

function mk(id: string, over: Partial<FilamentSpool> = {}): FilamentSpool {
  return {
    ...S, id, material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', colorHex: '#1a1a1a', location: 'Regal A',
    originalWeightG: 1000, remainingWeightG: 1000, ...over,
  };
}

describe('FilamentDashboard grouped cards', () => {
  const a = mk('a1', { remainingWeightG: 920 });
  const b = mk('a2', { remainingWeightG: 640 });
  const lone = mk('c1', { material: 'ABS', color: 'Grau', colorHex: '#8a8d91', remainingWeightG: 0, location: 'Regal B' });

  function renderGroups(extra: Partial<Parameters<typeof FilamentDashboard>[0]> = {}) {
    const spools = [a, b, lone];
    return renderDashboard({ spools, groups: groupFilaments(spools), ...extra });
  }

  it('shows one card for equal spools with count, total rest and total target', () => {
    renderGroups();
    const card = screen.getByTestId('group-card-a1');
    expect(card).toHaveTextContent('2 Spulen');
    expect(card).toHaveTextContent('1.560,0 g');
    expect(card).toHaveTextContent('von 2.000,0 g');
    expect(card).toHaveTextContent('Vorrätig');
    expect(screen.queryByTestId('spool-card-a1')).toBeNull();
  });

  it('keeps a spool without a twin as a normal card', () => {
    renderGroups();
    expect(screen.getByTestId('spool-card-c1')).toBeInTheDocument();
    expect(screen.getByTestId('spool-card-c1')).toHaveTextContent('Leer');
  });

  it('shows no count, total or group toggle on a single card', () => {
    renderGroups();
    const card = screen.getByTestId('spool-card-c1');
    expect(within(card).queryByRole('button', { name: /anzeigen/ })).toBeNull();
    expect(card).not.toHaveTextContent('Spulen');
  });

  it('judges the group status by its total', () => {
    const low = [mk('l1', { remainingWeightG: 150 }), mk('l2', { remainingWeightG: 150 })];
    renderDashboard({ spools: low, groups: groupFilaments(low) });
    expect(screen.getByTestId('group-card-l1')).toHaveTextContent('Niedrig');
    expect(screen.getByTestId('group-card-l1')).toHaveTextContent('300,0 g');
  });

  it('names the empty spools of a group', () => {
    const list = [mk('e1', { remainingWeightG: 0 }), mk('e2', { remainingWeightG: 300 })];
    renderDashboard({ spools: list, groups: groupFilaments(list) });
    expect(screen.getByTestId('group-card-e1')).toHaveTextContent('1 Spule leer');
  });

  it('expands to the single spools with aria-expanded and collapses again', () => {
    renderGroups();
    const toggle = screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('spool-row-a1')).toBeNull();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(toggle).toHaveAttribute('aria-controls', screen.getByTestId('spool-row-a1').parentElement!.id);
    expect(screen.getByTestId('spool-row-a1')).toHaveTextContent('920,0 g');
    expect(screen.getByTestId('spool-row-a2')).toHaveTextContent('640,0 g');
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('spool-row-a1')).toBeNull();
  });

  it('edits and deletes a single spool inside the group', () => {
    const onEdit = vi.fn();
    const onRequestDelete = vi.fn();
    renderGroups({ onEdit, onRequestDelete });
    fireEvent.click(screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' }));
    const row = screen.getByTestId('spool-row-a2');
    fireEvent.click(within(row).getByRole('button', { name: /Spule bearbeiten/ }));
    expect(onEdit).toHaveBeenCalledWith(b);
    fireEvent.click(within(row).getByRole('button', { name: /^Eintrag löschen/ }));
    expect(onRequestDelete).toHaveBeenCalledWith('a2');
  });

  it('asks for confirmation inside the group before deleting', () => {
    const onConfirmDelete = vi.fn();
    renderGroups({ confirmDeleteId: 'a2', onConfirmDelete });
    fireEvent.click(screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' }));
    fireEvent.click(within(screen.getByTestId('spool-row-a2')).getByRole('button', { name: 'Löschen' }));
    expect(onConfirmDelete).toHaveBeenCalledWith('a2');
  });

  it('restocks from the group card using its first spool as the template', () => {
    const onRestock = vi.fn();
    renderGroups({ onRestock });
    const button = within(screen.getByTestId('group-card-a1')).getByRole('button', { name: 'Nachkaufen' });
    fireEvent.click(button);
    expect(onRestock).toHaveBeenCalledWith(a, button);
  });

  it('starts a drag with the spool of the group that has the smallest rest', () => {
    const onSpoolMouseDown = vi.fn();
    renderGroups({ onSpoolMouseDown });
    fireEvent.mouseDown(screen.getByTestId('group-card-a1'));
    expect(onSpoolMouseDown).toHaveBeenCalledWith('a2', expect.anything());
  });

  it('drags the exact spool when it is taken from an expanded row', () => {
    const onSpoolMouseDown = vi.fn();
    renderGroups({ onSpoolMouseDown });
    fireEvent.click(screen.getByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' }));
    fireEvent.mouseDown(screen.getByTestId('spool-row-a1'));
    expect(onSpoolMouseDown).toHaveBeenCalledTimes(1);
    expect(onSpoolMouseDown).toHaveBeenCalledWith('a1', expect.anything());
  });

  it('shows a group of loaded spools with its unit, slots and booking hint, without dragging', () => {
    const loaded = [
      mk('u1a', { unitId: 'u1', slotIndex: 0, location: null, homeLocation: 'Regal A', remainingWeightG: 700 }),
      mk('u1b', { unitId: 'u1', slotIndex: 2, location: null, homeLocation: 'Regal A', remainingWeightG: 650 }),
    ];
    const onSpoolMouseDown = vi.fn();
    renderDashboard({
      spools: loaded, groups: groupFilaments(loaded, () => 'X1C · AMS 1'), onSpoolMouseDown,
    });
    const card = screen.getByTestId('group-card-u1a');
    expect(card).toHaveTextContent('X1C · AMS 1');
    expect(card).toHaveTextContent('Fach 1, Fach 3');
    expect(card).toHaveTextContent('Drucke buchen hier ab');
    expect(card).toHaveTextContent('Eingelegt');
    expect(card).toHaveTextContent('1.350,0 g');
    fireEvent.mouseDown(card);
    expect(onSpoolMouseDown).not.toHaveBeenCalled();
  });

  it('shows a lone loaded spool with its unit and slot', () => {
    const loaded = [mk('u9', { unitId: 'u1', slotIndex: 1, location: null })];
    renderDashboard({ spools: loaded, groups: groupFilaments(loaded, () => 'X1C · AMS 1') });
    expect(screen.getByTestId('spool-card-u9')).toHaveTextContent('X1C · AMS 1 · Fach 2');
  });

  it('opens "Verbrauch" for a resin group with all bottles to choose from', () => {
    const onConsume = vi.fn();
    const bottles = [
      mk('r1', { kind: 'resin', material: 'Standard', color: 'Grau', colorHex: '#8a8f98', remainingWeightG: 800 }),
      mk('r2', { kind: 'resin', material: 'Standard', color: 'Grau', colorHex: '#8a8f98', remainingWeightG: 200 }),
    ];
    renderDashboard({ spools: bottles, groups: groupFilaments(bottles), onConsume });
    expect(screen.getByTestId('group-card-r1')).toHaveTextContent('2 Flaschen');
    const button = within(screen.getByTestId('group-card-r1')).getByRole('button', { name: 'Verbrauch' });
    fireEvent.click(button);
    expect(onConsume).toHaveBeenCalledWith(bottles[1], button, bottles);
  });

  it('shows a split color dot for multi-color groups and names the color', () => {
    const rainbow = ['#dd3333', '#33aa66', '#3366cc'];
    const list = [
      mk('m1', { color: 'Regenbogen', colorHex: rainbow[0], colorHexes: rainbow }),
      mk('m2', { color: 'Regenbogen', colorHex: rainbow[0], colorHexes: rainbow }),
    ];
    renderDashboard({ spools: list, groups: groupFilaments(list) });
    const dot = within(screen.getByTestId('group-card-m1')).getByRole('img', { name: 'Regenbogen, mehrfarbig' });
    expect(dot).toHaveAttribute('data-multicolor', 'true');
  });
});
