import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { StepPartsPanel } from './StepPartsPanel';
import type { StepPart } from '../lib/stepParts';

const parts: StepPart[] = [
  { index: 0, rest: false, size: [10, 5, 2] },
  { index: 1, rest: false, size: [20, 10, 4] },
  { index: 2, rest: true, size: [1, 1, 1] },
];

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function panel(extra: Partial<React.ComponentProps<typeof StepPartsPanel>> = {}) {
  const props = { parts, hidden: new Set<number>(), selected: null, onToggle: vi.fn(), onSelect: vi.fn(), onShowAll: vi.fn(),
    onHighlight: vi.fn(), onSave: vi.fn() };
  render(<LanguageProvider><StepPartsPanel {...props} {...extra} /></LanguageProvider>);
  return props;
}

describe('StepPartsPanel', () => {
  it('lists every part as a labelled checkbox with its position and state', () => {
    panel({ hidden: new Set([1]) });
    expect(screen.getByRole('group', { name: 'Teile' })).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Teil 1 von 3, sichtbar' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Teil 2 von 3, ausgeblendet' })).not.toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Weitere Teile, sichtbar' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.spShowAll })).toBeEnabled();
  });

  it('toggles visibility, selects by name and shows the size of the chosen part', () => {
    const props = panel({ selected: 1 });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Teil 1 von 3, sichtbar' }));
    expect(props.onToggle).toHaveBeenCalledWith(0);
    fireEvent.click(screen.getByRole('button', { name: 'Teil 1' }));
    expect(props.onSelect).toHaveBeenCalledWith(0);
    expect(screen.getByRole('status')).toHaveTextContent('Teil 2');
    expect(screen.getByRole('status')).toHaveTextContent('Maße: 20 × 10 × 4 mm');
  });

  it('moves between the checkboxes with the arrow keys and stays on the ends', () => {
    panel();
    const boxes = screen.getAllByRole('checkbox');
    boxes[0].focus();
    fireEvent.keyDown(boxes[0], { key: 'ArrowDown' });
    expect(boxes[1]).toHaveFocus();
    fireEvent.keyDown(boxes[1], { key: 'ArrowDown' });
    fireEvent.keyDown(boxes[2], { key: 'ArrowDown' });
    expect(boxes[2]).toHaveFocus();
    fireEvent.keyDown(boxes[2], { key: 'ArrowUp' });
    expect(boxes[1]).toHaveFocus();
  });

  it('highlights on hover and on focus and ends the highlight afterwards', () => {
    const props = panel();
    const row = screen.getAllByRole('checkbox')[1].closest('[data-part-row]')!;
    fireEvent.mouseEnter(row);
    expect(props.onHighlight).toHaveBeenLastCalledWith(1);
    fireEvent.mouseLeave(row);
    expect(props.onHighlight).toHaveBeenLastCalledWith(null);
    fireEvent.focus(screen.getAllByRole('checkbox')[2]);
    expect(props.onHighlight).toHaveBeenLastCalledWith(2);
  });

  it('offers the save actions only where they apply', () => {
    const props = panel({ selected: 0, hidden: new Set([2]) });
    fireEvent.click(screen.getByRole('button', { name: 'Teil 1 als STL speichern' }));
    fireEvent.click(screen.getByRole('button', { name: de.spSaveVisible }));
    fireEvent.click(screen.getByRole('button', { name: de.spSaveEach }));
    expect(props.onSave.mock.calls.map(call => call[0])).toEqual(['selected', 'visible', 'each']);
  });

  it('disables the single save without a chosen part, hides "visible" when nothing is hidden, and has no save without a catalog id', () => {
    panel();
    expect(screen.getByRole('button', { name: de.spSaveSelected })).toBeDisabled();
    expect(screen.queryByRole('button', { name: de.spSaveVisible })).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent(de.spNoneSelected);
  });

  it('shows no save buttons without onSave and "Show all" is off while nothing is hidden', () => {
    panel({ onSave: undefined });
    expect(screen.queryByRole('button', { name: de.spSaveEach })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.spShowAll })).toBeDisabled();
  });

  it('collapses the list', () => {
    panel();
    fireEvent.click(screen.getByRole('button', { name: 'Teile (3)' }));
    expect(screen.queryByRole('group', { name: 'Teile' })).not.toBeInTheDocument();
  });
});
