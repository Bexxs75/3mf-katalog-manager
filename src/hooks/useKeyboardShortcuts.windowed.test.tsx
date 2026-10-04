import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ModelLayoutContext, type ModelLayout } from './ModelLayoutContext';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';

it.each(['absent', 'mounted'])('moves by columns when the current tile is %s and the next row is offscreen', current => {
  const order = Array.from({ length: 100 }, (_, i) => String(i));
  const scrollToIndex = vi.fn();
  const layout: ModelLayout = { columns: 4, order, scrollToIndex };
  const selectModel = vi.fn();
  const registry = { layouts: new Map([[{}, layout]]) };
  const tile = document.createElement('div'); tile.dataset.modelId = '10';
  if (current === 'mounted') document.body.append(tile);
  const { unmount } = renderHook(() => useKeyboardShortcuts({ filteredIds: order, selectedId: '10', selectModel,
    navigationEnabled: true, hasBulkSelection: false, openBulkDeleteConfirm: vi.fn(), toggleBulkSelect: vi.fn() }), {
    wrapper: ({ children }) => <ModelLayoutContext.Provider value={registry}>{children}</ModelLayoutContext.Provider>,
  });
  act(() => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' })); });
  expect(selectModel).toHaveBeenCalledWith('14');
  expect(scrollToIndex).toHaveBeenCalledWith(14);
  act(() => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp' })); });
  expect(selectModel).toHaveBeenCalledWith('6');
  expect(scrollToIndex).toHaveBeenCalledWith(6);
  tile.remove(); unmount();
});
