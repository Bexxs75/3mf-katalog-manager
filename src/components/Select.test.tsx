import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { Select } from './Select';
import { chooseSelect } from '../test/select';

afterEach(() => vi.restoreAllMocks());
function setup(disabled = false) {
  const changed = vi.fn();
  function Host() {
    const [value, setValue] = useState('a');
    return <><label htmlFor="choice">Material</label><Select id="choice" value={value} disabled={disabled}
      onValueChange={next => { changed(next); setValue(next); }}>
      <option value="a">Alpha</option><option value="b" disabled>Beta</option>
      <option value="c">Charlie</option><option value="d">Delta</option><option value="e">Denim</option>
    </Select><button>Outside</button></>;
  }
  render(<Host />);
  return { trigger: screen.getByRole('combobox', { name: 'Material' }), changed };
}
it('exposes roles and selection, traverses enabled options and restores focus on Escape', () => {
  const { trigger, changed } = setup();
  fireEvent.keyDown(trigger, { key: 'ArrowDown' });
  expect(trigger).toHaveFocus();
  expect(trigger).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByRole('listbox')).toHaveAccessibleName('Material');
  expect(screen.getByRole('option', { name: 'Alpha' })).toHaveAttribute('aria-selected', 'true');
  fireEvent.keyDown(trigger, { key: 'ArrowDown' });
  expect(trigger).toHaveAttribute('aria-activedescendant', screen.getByRole('option', { name: 'Charlie' }).id);
  fireEvent.keyDown(trigger, { key: 'End' });
  fireEvent.keyDown(trigger, { key: 'ArrowUp' });
  fireEvent.keyDown(trigger, { key: 'Enter' });
  expect(changed).toHaveBeenLastCalledWith('d');
  expect(trigger).toHaveFocus();
  fireEvent.click(trigger);
  fireEvent.keyDown(trigger, { key: 'Home' });
  expect(trigger).toHaveAttribute('aria-activedescendant', screen.getByRole('option', { name: 'Alpha' }).id);
  fireEvent.keyDown(trigger, { key: 'Escape' });
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(trigger).toHaveFocus();
  expect(changed).toHaveBeenCalledTimes(1);
});
it('supports buffered typeahead and repeated-letter cycling', () => {
  const { trigger, changed } = setup();
  for (const key of ['d', 'e', 'n', 'Enter']) fireEvent.keyDown(trigger, { key });
  expect(changed).toHaveBeenLastCalledWith('e');
  fireEvent.click(trigger);
  fireEvent.keyDown(trigger, { key: 'd' });
  expect(trigger).toHaveAttribute('aria-activedescendant', screen.getByRole('option', { name: 'Delta' }).id);
  fireEvent.keyDown(trigger, { key: 'd' });
  fireEvent.keyDown(trigger, { key: 'Enter' });
  expect(changed).toHaveBeenLastCalledWith('e');
});
it('selects by mouse, refuses disabled options and closes outside without stealing focus', () => {
  const { trigger, changed } = setup();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole('option', { name: 'Beta' }));
  expect(changed).not.toHaveBeenCalled();
  chooseSelect(trigger, 'c');
  expect(changed).toHaveBeenCalledWith('c');
  fireEvent.click(trigger);
  const outside = screen.getByRole('button', { name: 'Outside' });
  fireEvent.mouseDown(outside);
  outside.focus();
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(outside).toHaveFocus();
});
it('does not open a disabled control', () => {
  const { trigger, changed } = setup(true);
  fireEvent.click(trigger); fireEvent.keyDown(trigger, { key: 'ArrowDown' });
  expect(trigger).toBeDisabled();
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(changed).not.toHaveBeenCalled();
});
it('uses theme variables in either theme and fits a long popup above a low anchor', () => {
  const { trigger } = setup();
  vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({ left: 1000, right: 1100, top: 700, bottom: 730, width: 100, height: 30, x: 1000, y: 700, toJSON() {} });
  document.documentElement.classList.add('dark');
  try {
    fireEvent.click(trigger);
    const list = screen.getByRole('listbox');
    expect(list).toHaveClass('bg-[var(--panel)]', 'text-[var(--ink)]');
    expect(list.className).not.toMatch(/dark:|bg-white|bg-black|text-white|text-black/);
    expect(Number.parseFloat(list.style.left) + Number.parseFloat(list.style.width)).toBeLessThanOrEqual(window.innerWidth - 8);
    expect(list.style.bottom).toBe('72px');
    expect(Number.parseFloat(list.style.maxHeight)).toBeLessThanOrEqual(256);
  } finally { document.documentElement.classList.remove('dark'); }
});
it('keeps an empty trigger from collapsing', () => {
  render(<Select aria-label="Empty" value="zzz" onValueChange={() => {}}><option value="a">Alpha</option></Select>);
  const trigger = screen.getByRole('combobox', { name: 'Empty' });
  expect(trigger).toHaveClass('min-w-[4rem]', 'min-h-[1.5em]');
  expect(trigger.textContent).toContain('\u00a0');
});
it('forwards onBlur and still closes', () => {
  const onBlur = vi.fn();
  render(<Select aria-label="Blur" value="a" onBlur={onBlur} onValueChange={() => {}}><option value="a">Alpha</option></Select>);
  const trigger = screen.getByRole('combobox', { name: 'Blur' });
  fireEvent.click(trigger);
  fireEvent.blur(trigger);
  expect(onBlur).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole('listbox')).toBeNull();
});
