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

it('scrolls keyboard selections into view in a long suggestion list', () => {
  const scrollIntoView = vi.fn();
  render(<AutocompleteInput value="" onChange={vi.fn()} onSubmit={vi.fn()}
    options={Array.from({ length: 20 }, (_, i) => `Tag ${i}`)} maxSuggestions={20} />);
  const input = screen.getByRole('combobox');
  fireEvent.focus(input);
  const last = screen.getAllByRole('option')[19];
  last.scrollIntoView = scrollIntoView;
  fireEvent.keyDown(input, { key: 'ArrowUp' });
  expect(last).toHaveAttribute('aria-selected', 'true');
  expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });
});

it('does not scroll the list when the pointer only hovers an option', () => {
  const scrollIntoView = vi.fn();
  render(<AutocompleteInput value="" onChange={vi.fn()} onSubmit={vi.fn()}
    options={Array.from({ length: 20 }, (_, i) => `Tag ${i}`)} maxSuggestions={20} />);
  fireEvent.focus(screen.getByRole('combobox'));
  const option = screen.getAllByRole('option')[15];
  option.scrollIntoView = scrollIntoView;
  fireEvent.mouseEnter(option);
  expect(option).toHaveAttribute('aria-selected', 'true');
  expect(scrollIntoView).not.toHaveBeenCalled();
});

describe('AutocompleteInput list pointer handling', () => {
  it('keeps focus in the input when the list itself is pressed, e.g. on its scrollbar', () => {
    render(<AutocompleteInput value="" onChange={vi.fn()} onSubmit={vi.fn()} options={['Tag 1', 'Tag 2']} />);
    const input = screen.getByRole('combobox');
    fireEvent.focus(input);
    const notPrevented = fireEvent.mouseDown(screen.getByRole('listbox'));
    expect(notPrevented).toBe(false);
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  it('still selects an option on press', () => {
    const onSubmit = vi.fn();
    render(<AutocompleteInput value="" onChange={vi.fn()} onSubmit={onSubmit} options={['Tag 1', 'Tag 2']} />);
    fireEvent.focus(screen.getByRole('combobox'));
    fireEvent.mouseDown(screen.getByText('Tag 2'));
    expect(onSubmit).toHaveBeenCalledWith('Tag 2');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('still closes the list on a press outside', () => {
    render(<div><span>outside</span><AutocompleteInput value="" onChange={vi.fn()} options={['Tag 1']} /></div>);
    fireEvent.focus(screen.getByRole('textbox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByText('outside'));
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
});
