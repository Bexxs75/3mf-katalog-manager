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

it.each(['busy', 'disabled'] as const)('ignores click, Space and Enter when %s', state => {
  const onChange = vi.fn();
  render(<ToggleSwitch checked={false} onChange={onChange} label="Setting" {...{ [state]: true }} />);
  const toggle = screen.getByRole('switch');
  if (state === 'busy') {
    expect(toggle).toBeEnabled();
    expect(toggle).toHaveAttribute('aria-busy', 'true');
    expect(toggle).toHaveAttribute('aria-disabled', 'true');
  } else {
    expect(toggle).toBeDisabled();
  }
  expect(toggle).toHaveClass('opacity-50');
  expect(toggle).not.toHaveClass('hover:outline');
  fireEvent.click(toggle);
  fireEvent.keyDown(toggle, { key: ' ' });
  fireEvent.keyDown(toggle, { key: 'Enter' });
  expect(onChange).not.toHaveBeenCalled();
});

it('keeps focus when busy starts and finishes, then accepts input again', () => {
  const onChange = vi.fn();
  const { rerender } = render(<ToggleSwitch checked={false} onChange={onChange} label="Setting" />);
  const toggle = screen.getByRole('switch');
  toggle.focus();
  for (const busy of [true, false]) {
    rerender(<ToggleSwitch checked={false} onChange={onChange} label="Setting" busy={busy} />);
    expect(toggle).toBeEnabled();
    expect(document.activeElement).toBe(toggle);
  }
  expect(toggle).not.toHaveAttribute('aria-disabled');
  fireEvent.keyDown(toggle, { key: 'Enter' });
  expect(onChange).toHaveBeenCalledExactlyOnceWith(true);
});
