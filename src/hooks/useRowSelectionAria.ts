import { useState } from 'react';

/**
 * ARIA state for the rows of the model grid/list. `aria-selected` means "part of the batch
 * selection" (the checkboxes), because that is what the bulk actions and the live
 * announcement ("N selected" / "selection cleared") talk about. The model open in the
 * preview is marked `aria-current` instead. Only while no batch selection exists, and none
 * was just cleared, the open model also counts as selected, so someone who only navigates
 * with the arrow keys still hears a selection.
 */
export function useRowSelectionAria(selectedId: string | null, selectedForBulk: ReadonlySet<string>) {
  const size = selectedForBulk.size;
  const [seen, setSeen] = useState({ size, selectedId });
  // True after a batch selection was cleared while the same model stayed open: that model must
  // not turn into "selected" again right under the "selection cleared" announcement.
  const [justCleared, setJustCleared] = useState(false);
  if (seen.size !== size || seen.selectedId !== selectedId) {
    setSeen({ size, selectedId });
    if (seen.selectedId !== selectedId || size > 0) setJustCleared(false);
    else if (seen.size > 0) setJustCleared(true);
  }
  const detailCounts = size === 0 && !justCleared;
  return (id: string) => ({
    'aria-selected': size > 0 ? selectedForBulk.has(id) : detailCounts && id === selectedId,
    'aria-current': id === selectedId ? ('true' as const) : undefined,
  });
}
