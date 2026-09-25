import { useState, type KeyboardEvent } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { AutocompleteInput } from './AutocompleteInput';

// Simuliert die Situation in RestockPopover/SpoolPopoverShell: ein
// umgebendes Element behandelt Escape selbst (dort: schliesst das ganze
// Popover und verwirft die Eingaben).
function Wrapper({ onParentKeyDown }: { onParentKeyDown: (e: KeyboardEvent) => void }) {
  const [value, setValue] = useState('');
  return (
    <div onKeyDown={onParentKeyDown}>
      <AutocompleteInput value={value} onChange={setValue} options={['Regal 1', 'Regal 2']} />
    </div>
  );
}

describe('AutocompleteInput', () => {
  it('closes only the open suggestion list on Escape and does not propagate', () => {
    const onParentKeyDown = vi.fn();
    render(<Wrapper onParentKeyDown={onParentKeyDown} />);
    const input = screen.getByRole('textbox');
    fireEvent.focus(input);
    expect(screen.getByText('Regal 1')).toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });

    expect(screen.queryByText('Regal 1')).not.toBeInTheDocument();
    expect(onParentKeyDown).not.toHaveBeenCalled();
  });

  it('propagates Escape to the parent when the list is not open', () => {
    const onParentKeyDown = vi.fn();
    render(<Wrapper onParentKeyDown={onParentKeyDown} />);
    const input = screen.getByRole('textbox');
    expect(screen.queryByText('Regal 1')).not.toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });

    expect(onParentKeyDown).toHaveBeenCalledTimes(1);
  });
});
