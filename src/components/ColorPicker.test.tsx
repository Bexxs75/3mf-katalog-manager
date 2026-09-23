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
});
