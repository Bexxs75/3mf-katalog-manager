import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ToggleSwitch } from './ToggleSwitch';

describe('ToggleSwitch', () => {
  it('is a switch with aria-checked and toggles on click and Space', () => {
    const onChange = vi.fn();
    render(<ToggleSwitch checked={false} onChange={onChange} label="Druckeranbindung" />);
    const sw = screen.getByRole('switch', { name: 'Druckeranbindung' });
    expect(sw).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
    fireEvent.keyDown(sw, { key: ' ' });
    expect(onChange).toHaveBeenCalledTimes(2);
  });
});
