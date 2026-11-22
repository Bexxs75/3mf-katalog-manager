import { fireEvent, screen, within } from '@testing-library/react';

/** Opens a selection action group. */
export function openBulkMore(group = 'Ablegen') {
  const trigger = within(screen.getByRole('toolbar')).getByRole('button', { name: group });
  if (trigger.getAttribute('aria-expanded') !== 'true') fireEvent.click(trigger);
  return screen.getByRole('menu', { name: group });
}

export function clickBulkMore(name: string) {
  const group = /Bündel|Bündeln/.test(name) ? 'Bündel' : /Favorit|Warteschlange|gedruckt/i.test(name) ? 'Markieren' : 'Ablegen';
  fireEvent.click(within(openBulkMore(group)).getByRole('menuitem', { name }));
}
