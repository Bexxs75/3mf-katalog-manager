import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelPicker } from './ModelPicker';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

describe('ModelPicker', () => {
  it('filters by name and allows "no model"', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <ModelPicker models={[{ id: '1', name: 'Rakete.3mf' }, { id: '2', name: 'Kabelclip.stl' }]} onChange={onChange} onClose={vi.fn()} />
      </LanguageProvider>,
    );
    fireEvent.change(screen.getByPlaceholderText('Modell suchen …'), { target: { value: 'kabel' } });
    expect(screen.queryByText('Rakete.3mf')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('Kabelclip.stl'));
    expect(onChange).toHaveBeenCalledWith('2');
    fireEvent.click(screen.getByText('Kein Modell'));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('navigates with the keyboard from the search field and selects with Enter', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <ModelPicker models={[{ id: '1', name: 'Rakete.3mf' }, { id: '2', name: 'Kabelclip.stl' }]} onChange={onChange} onClose={vi.fn()} />
      </LanguageProvider>,
    );
    const input = screen.getByPlaceholderText('Modell suchen …');
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // von "Kein Modell" zu "Rakete.3mf"
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // zu "Kabelclip.stl"
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('2');
  });

  it('reports the active option via aria-activedescendant on the search field', () => {
    render(
      <LanguageProvider>
        <ModelPicker models={[{ id: '1', name: 'Rakete.3mf' }]} onChange={vi.fn()} onClose={vi.fn()} />
      </LanguageProvider>,
    );
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
    render(
      <div onKeyDown={outerKeyDown}>
        <LanguageProvider>
          <ModelPicker models={[{ id: '1', name: 'Rakete.3mf' }]} onChange={vi.fn()} onClose={onClose} />
        </LanguageProvider>
      </div>,
    );
    fireEvent.keyDown(screen.getByPlaceholderText('Modell suchen …'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(outerKeyDown).not.toHaveBeenCalled();
  });
});
