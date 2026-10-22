import { fireEvent, screen, within } from '@testing-library/react';

/** Opens the "More" menu of the selection bar and returns it. */
export function openBulkMore() {
  fireEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: /^Mehr/ }));
  return screen.getByRole('menu', { name: 'Mehr Aktionen' });
}

/** Picks an entry from the selection bar's "More" menu, as a user would. */
export function clickBulkMore(name: string) {
  const menu = openBulkMore();
  fireEvent.click(within(menu).getByRole('menuitem', { name }));
}
