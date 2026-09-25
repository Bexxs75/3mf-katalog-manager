import { createRef } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelPicker, type ModelOption } from './ModelPicker';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const defaultModels: ModelOption[] = [{ id: '1', name: 'Rakete.3mf' }, { id: '2', name: 'Kabelclip.stl' }];

// Echter Auswahl-Knopf als Anker, wie in PrinterJobsDialog.
function renderPicker(overrides: { models?: ModelOption[]; onChange?: (id: string | null) => void; onClose?: () => void } = {}) {
  const anchorRef = createRef<HTMLButtonElement>();
  const onChange = overrides.onChange ?? vi.fn();
  const onClose = overrides.onClose ?? vi.fn();
  const models = overrides.models ?? defaultModels;
  const utils = render(
    <LanguageProvider>
      <button ref={anchorRef}>Modell wählen</button>
      <ModelPicker models={models} anchorRef={anchorRef} onChange={onChange} onClose={onClose} />
    </LanguageProvider>,
  );
  return { ...utils, onChange, onClose, anchorRef };
}

describe('ModelPicker', () => {
  it('filters by name and allows "no model"', () => {
    const { onChange } = renderPicker();
    fireEvent.change(screen.getByPlaceholderText('Modell suchen …'), { target: { value: 'kabel' } });
    expect(screen.queryByText('Rakete.3mf')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('Kabelclip.stl'));
    expect(onChange).toHaveBeenCalledWith('2');
    fireEvent.click(screen.getByText('Kein Modell'));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('navigates with the keyboard from the search field and selects with Enter', () => {
    const { onChange } = renderPicker();
    const input = screen.getByPlaceholderText('Modell suchen …');
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // von "Kein Modell" zu "Rakete.3mf"
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // zu "Kabelclip.stl"
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('2');
  });

  it('reports the active option via aria-activedescendant on the search field', () => {
    renderPicker({ models: [{ id: '1', name: 'Rakete.3mf' }] });
    const input = screen.getByPlaceholderText('Modell suchen …');
    const none = screen.getByText('Kein Modell');
    expect(input).toHaveAttribute('aria-activedescendant', none.id);
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    const rakete = screen.getByText('Rakete.3mf');
    expect(input).toHaveAttribute('aria-activedescendant', rakete.id);
  });

  it('does not bubble Escape to the surrounding dialog', () => {
    const onClose = vi.fn();
    const outerKeyDown = vi.fn();
    const anchorRef = createRef<HTMLButtonElement>();
    render(
      <div onKeyDown={outerKeyDown}>
        <LanguageProvider>
          <button ref={anchorRef}>Modell wählen</button>
          <ModelPicker models={[{ id: '1', name: 'Rakete.3mf' }]} anchorRef={anchorRef} onChange={vi.fn()} onClose={onClose} />
        </LanguageProvider>
      </div>,
    );
    fireEvent.keyDown(screen.getByPlaceholderText('Modell suchen …'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(outerKeyDown).not.toHaveBeenCalled();
  });

  it('renders its popup in a portal attached to document.body, not the local render tree', () => {
    // Das Popup haengt per Portal an document.body, sonst schneidet es der
    // Scrollcontainer des Dialogs ab.
    const { container } = renderPicker();
    const listbox = screen.getByRole('listbox');
    expect(container.contains(listbox)).toBe(false);
    expect(document.body.contains(listbox)).toBe(true);
  });

  it('opens on mount and stays open (no self-close via mount side effects)', () => {
    const { onClose } = renderPicker();
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not close when a scroll event originates from inside the popup list itself', () => {
    const { onClose } = renderPicker();
    fireEvent.scroll(screen.getByRole('listbox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes on click outside the popup and its anchor', () => {
    const { onClose } = renderPicker();
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalled();
  });

  it('closes on a real scroll event from outside the popup (e.g. the page or an outer scroll container)', () => {
    const { onClose } = renderPicker();
    fireEvent.scroll(window);
    expect(onClose).toHaveBeenCalled();
  });

  it('closes on window resize', () => {
    const { onClose } = renderPicker();
    fireEvent(window, new Event('resize'));
    expect(onClose).toHaveBeenCalled();
  });
});
