import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useBulkSelection } from './useBulkSelection';
import { makeModelFile } from '../test/factories';
it('drops only the given IDs from the bulk selection and closes the delete confirmation when it empties', () => {
  const noop = vi.fn();
  const models = ['1', '2', '3'].map(id => makeModelFile({ id }));
  const { result } = renderHook(() => useBulkSelection({ models, setModels: noop, refreshFolders: noop, refreshTags: noop, refreshTrash: noop, refetchAfterPartialDelete: noop, bulkAddToCollection: noop, bulkRemoveFromCollection: noop }));
  act(() => { result.current.selectAllVisible(['1', '2', '3']); result.current.setConfirmBulkDelete(true); });
  act(() => result.current.deselectBulk(['1', '9']));
  expect([...result.current.selectedForBulk]).toEqual(['2', '3']); expect(result.current.confirmBulkDelete).toBe(true);
  act(() => result.current.deselectBulk(['2', '3']));
  expect(result.current.selectedForBulk.size).toBe(0); expect(result.current.confirmBulkDelete).toBe(false);
});
