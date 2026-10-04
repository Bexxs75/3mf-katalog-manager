import { useState, type KeyboardEvent } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { AutocompleteInput } from './AutocompleteInput';

// Simulates the situation in RestockPopover/SpoolPopoverShell: a
// surrounding element handles Escape itself (there: closes the whole
// popover and discards the input).
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

it('honors a custom suggestion limit', () => {
  render(<AutocompleteInput value="" onChange={vi.fn()} options={['a', 'b', 'c']} maxSuggestions={2} />);
  fireEvent.focus(screen.getByRole('textbox'));
  expect(screen.getAllByRole('option')).toHaveLength(2);
});

it('retains initial selection and unlimited lists when requested by existing callers', () => {
  const onChange = vi.fn();
  render(<AutocompleteInput value="" onChange={onChange} options={Array.from({ length: 12 }, (_, i) => `Regal ${i}`)} maxSuggestions={Infinity} />);
  const input = screen.getByRole('textbox');
  fireEvent.focus(input);
  expect(screen.getAllByRole('option')).toHaveLength(12);
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onChange).toHaveBeenCalledWith('Regal 0');
});
