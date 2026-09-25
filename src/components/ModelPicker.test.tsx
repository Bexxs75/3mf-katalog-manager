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
});
