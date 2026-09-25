import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { SegmentedControl } from './SegmentedControl';

const OPTIONS = [
  { value: 'filament' as const, label: 'Filament' },
  { value: 'resin' as const, label: 'Resin' },
];

describe('SegmentedControl', () => {
  it('marks the active option and reports a change', () => {
    const onChange = vi.fn();
    render(<SegmentedControl label="Art" options={OPTIONS} value="filament" onChange={onChange} />);
    expect(screen.getByRole('group', { name: 'Art' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Filament' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Resin' })).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));
    expect(onChange).toHaveBeenCalledWith('resin');
    fireEvent.click(screen.getByRole('button', { name: 'Filament' }));
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('can be disabled', () => {
    const onChange = vi.fn();
    render(<SegmentedControl label="Art" options={OPTIONS} value="filament" onChange={onChange} disabled />);
    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));
    expect(onChange).not.toHaveBeenCalled();
  });
});
