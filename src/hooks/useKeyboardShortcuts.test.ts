import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';

const SEARCH_INPUT_ID = 'catalog-search-input';

function fireKey(key: string, target: EventTarget = window) {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

function setup(overrides: Partial<Parameters<typeof useKeyboardShortcuts>[0]> = {}) {
  const selectModel = vi.fn();
  const openBulkDeleteConfirm = vi.fn();
  const args = {
    filteredIds: ['a', 'b', 'c'],
    selectedId: null as string | null,
    selectModel,
    hasBulkSelection: false,
    openBulkDeleteConfirm,
    navigationEnabled: true,
    ...overrides,
  };
  const hook = renderHook(() => useKeyboardShortcuts(args));
  return { ...hook, selectModel, openBulkDeleteConfirm };
}

describe('useKeyboardShortcuts', () => {
  let searchInput: HTMLInputElement;

  beforeEach(() => {
    searchInput = document.createElement('input');
    searchInput.id = SEARCH_INPUT_ID;
    document.body.appendChild(searchInput);
  });

  afterEach(() => {
    document.body.removeChild(searchInput);
  });

  it('"/" focuses the search input when nothing else has focus', () => {
    setup();
    fireKey('/');
    expect(document.activeElement).toBe(searchInput);
  });

  it('"/" does not steal focus while already typing in an input', () => {
    const otherInput = document.createElement('input');
    document.body.appendChild(otherInput);
    otherInput.focus();
    setup();
    fireKey('/', otherInput);
    expect(document.activeElement).toBe(otherInput);
    document.body.removeChild(otherInput);
  });

  it('ArrowRight selects the next model in the filtered order', () => {
    const { selectModel } = setup({ selectedId: 'a' });
    fireKey('ArrowRight');
    expect(selectModel).toHaveBeenCalledWith('b');
  });

  it('ArrowLeft selects the previous model in the filtered order', () => {
    const { selectModel } = setup({ selectedId: 'b' });
    fireKey('ArrowLeft');
    expect(selectModel).toHaveBeenCalledWith('a');
  });

  it('ArrowRight selects the first model when nothing is selected yet', () => {
    const { selectModel } = setup({ selectedId: null });
    fireKey('ArrowRight');
    expect(selectModel).toHaveBeenCalledWith('a');
  });

  it('ArrowRight on the last model does not wrap or call selectModel', () => {
    const { selectModel } = setup({ selectedId: 'c' });
    fireKey('ArrowRight');
    expect(selectModel).not.toHaveBeenCalled();
  });

  it('arrow navigation is ignored while typing in an input', () => {
    const otherInput = document.createElement('input');
    document.body.appendChild(otherInput);
    otherInput.focus();
    const { selectModel } = setup({ selectedId: 'a' });
    fireKey('ArrowRight', otherInput);
    expect(selectModel).not.toHaveBeenCalled();
    document.body.removeChild(otherInput);
  });

  it('arrow navigation is ignored when navigationEnabled is false (e.g. detail view open)', () => {
    const { selectModel } = setup({ selectedId: 'a', navigationEnabled: false });
    fireKey('ArrowRight');
    expect(selectModel).not.toHaveBeenCalled();
  });

  it('Delete opens the bulk-delete confirmation when a bulk selection exists', () => {
    const { openBulkDeleteConfirm } = setup({ hasBulkSelection: true });
    fireKey('Delete');
    expect(openBulkDeleteConfirm).toHaveBeenCalled();
  });

  it('Delete does nothing without an active bulk selection', () => {
    const { openBulkDeleteConfirm } = setup({ hasBulkSelection: false });
    fireKey('Delete');
    expect(openBulkDeleteConfirm).not.toHaveBeenCalled();
  });

  it('Delete is ignored while typing in an input', () => {
    const otherInput = document.createElement('input');
    document.body.appendChild(otherInput);
    otherInput.focus();
    const { openBulkDeleteConfirm } = setup({ hasBulkSelection: true });
    fireKey('Delete', otherInput);
    expect(openBulkDeleteConfirm).not.toHaveBeenCalled();
    document.body.removeChild(otherInput);
  });
});
