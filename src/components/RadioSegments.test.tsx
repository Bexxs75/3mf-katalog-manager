import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RadioSegments } from './RadioSegments';

const OPTIONS = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Hell' },
  { value: 'dark', label: 'Dunkel' },
] as const;

describe('RadioSegments', () => {
  it('is a named radio group with exactly the current value checked', () => {
    render(<RadioSegments legend="Farbschema" options={[...OPTIONS]} value="light" onChange={vi.fn()} />);
    const group = screen.getByRole('group', { name: 'Farbschema' });
    expect(group).toBeVisible();
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: 'Hell' })).toBeChecked();
    expect(screen.getByRole('radio', { name: 'System' })).not.toBeChecked();
  });

  it('reports a newly chosen value once and ignores the already checked one', () => {
    const onChange = vi.fn();
    render(<RadioSegments legend="Farbschema" options={[...OPTIONS]} value="light" onChange={onChange} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Hell' }));
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('radio', { name: 'Dunkel' }));
    expect(onChange).toHaveBeenCalledExactlyOnceWith('dark');
  });

  it('marks the selection with a check mark in addition to the colour', () => {
    render(<RadioSegments legend="Farbschema" options={[...OPTIONS]} value="dark" onChange={vi.fn()} />);
    expect(screen.getByRole('radio', { name: 'Dunkel' }).closest('label')?.querySelector('svg')).not.toBeNull();
    expect(screen.getByRole('radio', { name: 'Hell' }).closest('label')?.querySelector('svg')).toBeNull();
  });

  it('keeps the group name for assistive technology when the legend is hidden', () => {
    render(<RadioSegments legend="Sprache" hideLegend options={[...OPTIONS]} value="system" onChange={vi.fn()} />);
    expect(screen.getByRole('group', { name: 'Sprache' })).toBeInTheDocument();
    expect(screen.getByText('Sprache')).toHaveClass('sr-only');
  });

  it('disables every option together', () => {
    render(<RadioSegments legend="Farbschema" options={[...OPTIONS]} value="system" onChange={vi.fn()} disabled />);
    for (const radio of screen.getAllByRole('radio')) expect(radio).toBeDisabled();
  });
});
