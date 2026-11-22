import { fireEvent, screen, within } from '@testing-library/react';

export function selectOptions(trigger: HTMLElement) {
  if (trigger.getAttribute('aria-expanded') !== 'true') fireEvent.click(trigger);
  return within(screen.getByRole('listbox')).getAllByRole('option');
}

/** Exercise the public interactions, including keyboard traversal past disabled rows. */
export function chooseSelect(trigger: HTMLElement, value: string, mode: 'mouse' | 'keyboard' = 'mouse') {
  trigger.focus();
  if (trigger.getAttribute('aria-expanded') !== 'true') {
    if (mode === 'keyboard') fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    else fireEvent.click(trigger);
  }
  const options = within(screen.getByRole('listbox')).getAllByRole('option');
  const target = options.find(option => option.getAttribute('data-value') === value);
  if (!target) throw new Error(`Missing option ${value}`);
  if (mode === 'mouse') { fireEvent.mouseDown(target); fireEvent.mouseUp(target); fireEvent.click(target); }
  else {
    fireEvent.keyDown(trigger, { key: 'Home' });
    for (let i = 0; i < options.length && trigger.getAttribute('aria-activedescendant') !== target.id; i++) {
      fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    }
    if (trigger.getAttribute('aria-activedescendant') !== target.id) throw new Error(`Unreachable option ${value}`);
    fireEvent.keyDown(trigger, { key: 'Enter' });
  }
}
