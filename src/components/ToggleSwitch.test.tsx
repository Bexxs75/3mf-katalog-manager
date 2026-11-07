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

  it('passes aria-describedby on to the switch', () => {
    render(<><ToggleSwitch checked label="Kürzel" onChange={vi.fn()} aria-describedby="hint" /><p id="hint">Erklärung</p></>);
    expect(screen.getByRole('switch', { name: 'Kürzel' })).toHaveAccessibleDescription('Erklärung');
  });

  it('has no description reference unless one is given', () => {
    render(<ToggleSwitch checked label="Kürzel" onChange={vi.fn()} />);
    expect(screen.getByRole('switch', { name: 'Kürzel' })).not.toHaveAttribute('aria-describedby');
  });
});
