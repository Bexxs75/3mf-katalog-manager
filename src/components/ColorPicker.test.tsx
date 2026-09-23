import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ColorPicker } from './ColorPicker';

function renderPicker(value: string | null, onChange = vi.fn()) {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(
    <LanguageProvider>
      <ColorPicker value={value} onChange={onChange} />
    </LanguageProvider>,
  );
  return onChange;
}

describe('ColorPicker', () => {
  it('uses no native color input and marks the selected swatch', () => {
    const { container } = render(<></>);
    renderPicker('#c0392b');
    expect(container.ownerDocument.querySelector('input[type="color"]')).toBeNull();
    expect(screen.getByRole('radio', { name: '#c0392b' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: '#1a1a1a' })).toHaveAttribute('aria-checked', 'false');
  });

  it('picks a palette color', () => {
    const onChange = renderPicker(null);
    fireEvent.click(screen.getByRole('radio', { name: '#27ae60' }));
    expect(onChange).toHaveBeenCalledWith('#27ae60');
  });

  it('accepts a typed hex value with or without # and ignores incomplete input', () => {
    const onChange = renderPicker(null);
    const input = screen.getByRole('textbox', { name: 'Farbwert' });
    fireEvent.change(input, { target: { value: '#12AB' } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: '12AB34' } });
    expect(onChange).toHaveBeenLastCalledWith('#12ab34');
  });

  it('clears the value', () => {
    const onChange = renderPicker('#1a1a1a');
    fireEvent.click(screen.getByRole('button', { name: 'Kein Farbwert' }));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('exposes only one swatch in the tab order', () => {
    renderPicker('#27ae60');
    const swatches = screen.getAllByRole('radio');
    const tabindexes = swatches.map((s) => s.getAttribute('tabindex'));
    expect(tabindexes.filter((t) => t === '0')).toHaveLength(1);
    expect(tabindexes.filter((t) => t === '-1')).toHaveLength(swatches.length - 1);
  });

  it('puts the first swatch in tab order when no color is selected', () => {
    renderPicker(null);
    const swatches = screen.getAllByRole('radio');
    expect(swatches[0]).toHaveAttribute('tabindex', '0');
  });

  it('navigates right with ArrowRight and selects', () => {
    const onChange = renderPicker('#c0392b');
    const current = screen.getByRole('radio', { name: '#c0392b' });
    fireEvent.keyDown(current, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith('#e67e22');
  });

  it('navigates left with ArrowLeft and selects', () => {
    const onChange = renderPicker('#e67e22');
    const current = screen.getByRole('radio', { name: '#e67e22' });
    fireEvent.keyDown(current, { key: 'ArrowLeft' });
    expect(onChange).toHaveBeenCalledWith('#c0392b');
  });

  it('wraps around on ArrowRight at the end', () => {
    const onChange = renderPicker('#e8eef0');
    const current = screen.getByRole('radio', { name: '#e8eef0' });
    fireEvent.keyDown(current, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith('#1a1a1a');
  });

  it('wraps around on ArrowLeft at the start', () => {
    const onChange = renderPicker('#1a1a1a');
    const current = screen.getByRole('radio', { name: '#1a1a1a' });
    fireEvent.keyDown(current, { key: 'ArrowLeft' });
    expect(onChange).toHaveBeenCalledWith('#e8eef0');
  });
});
