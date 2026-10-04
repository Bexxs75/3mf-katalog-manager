import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useKeyboardShortcuts, findSpatialNeighbor, MODEL_TILE_ATTR } from './useKeyboardShortcuts';

const SEARCH_INPUT_ID = 'catalog-search-input';

function fireKey(key: string, target: EventTarget = window) {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

// Tile with a mocked position: jsdom doesn't do layout.
function placeTile(id: string, rect: { left: number; top: number; width?: number; height?: number }) {
  const el = document.createElement('div');
  el.setAttribute(MODEL_TILE_ATTR, id);
  const width = rect.width ?? 100;
  const height = rect.height ?? 100;
  el.getBoundingClientRect = () =>
    ({ left: rect.left, top: rect.top, width, height, right: rect.left + width, bottom: rect.top + height, x: rect.left, y: rect.top, toJSON() {} }) as DOMRect;
  // jsdom doesn't implement scrollIntoView - for tests that want to check
  // whether/with what it was called, the tile needs its own spy.
  el.scrollIntoView = vi.fn();
  document.body.appendChild(el);
  return el;
}

describe('findSpatialNeighbor', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('finds the tile directly below in a 3-column grid', () => {
    // Row 1: a b c / row 2: d e f - selection on "b", "down" must hit "e".
    placeTile('a', { left: 0, top: 0 });
    placeTile('b', { left: 100, top: 0 });
    placeTile('c', { left: 200, top: 0 });
    placeTile('d', { left: 0, top: 100 });
    placeTile('e', { left: 100, top: 100 });
    placeTile('f', { left: 200, top: 100 });

    expect(findSpatialNeighbor(document, 'b', 'down')).toBe('e');
    expect(findSpatialNeighbor(document, 'e', 'up')).toBe('b');
  });

  it('picks the horizontally closest tile in the next row when columns are uneven', () => {
    // Row 2 has only 2 tiles: "down" from "b" (x center 150) must hit the
    // closer of "d" (50) and "e" (150), i.e. "e".
    placeTile('a', { left: 0, top: 0 });
    placeTile('b', { left: 100, top: 0 });
    placeTile('c', { left: 200, top: 0 });
    placeTile('d', { left: 0, top: 100 });
    placeTile('e', { left: 100, top: 100 });

    expect(findSpatialNeighbor(document, 'b', 'down')).toBe('e');
  });

  it('returns null when there is no tile in the requested direction', () => {
    placeTile('a', { left: 0, top: 0 });
    placeTile('b', { left: 100, top: 0 });
    expect(findSpatialNeighbor(document, 'a', 'up')).toBeNull();
    expect(findSpatialNeighbor(document, 'a', 'down')).toBeNull();
  });

  it('returns null when the current tile is not in the DOM (e.g. collapsed folder section)', () => {
    placeTile('a', { left: 0, top: 0 });
    expect(findSpatialNeighbor(document, 'not-rendered', 'down')).toBeNull();
  });

  it('treats a single-column list (one tile per row) as straightforward up/down', () => {
    placeTile('row1', { left: 0, top: 0, width: 400, height: 40 });
    placeTile('row2', { left: 0, top: 40, width: 400, height: 40 });
    placeTile('row3', { left: 0, top: 80, width: 400, height: 40 });

    expect(findSpatialNeighbor(document, 'row2', 'down')).toBe('row3');
    expect(findSpatialNeighbor(document, 'row2', 'up')).toBe('row1');
  });
});

function setup(overrides: Partial<Parameters<typeof useKeyboardShortcuts>[0]> = {}) {
  const selectModel = vi.fn();
  const openBulkDeleteConfirm = vi.fn();
  const toggleBulkSelect = vi.fn();
  const args = {
    filteredIds: ['a', 'b', 'c'],
    selectedId: null as string | null,
    selectModel,
    hasBulkSelection: false,
    openBulkDeleteConfirm,
    navigationEnabled: true,
    toggleBulkSelect,
    ...overrides,
  };
  const hook = renderHook(() => useKeyboardShortcuts(args));
  return { ...hook, selectModel, openBulkDeleteConfirm, toggleBulkSelect };
}

describe('useKeyboardShortcuts', () => {
  it('blocks search, selection, bulk delete and space while a modal exists', () => {
    const input = document.createElement('input'); input.id = SEARCH_INPUT_ID;
    const modal = document.createElement('div'); modal.setAttribute('role', 'dialog'); modal.setAttribute('aria-modal', 'true');
    document.body.append(input, modal);
    const args = setup({ hasBulkSelection: true, selectedId: 'a' });
    for (const key of ['/', 'ArrowRight', 'ArrowDown', ' ', 'Delete']) fireKey(key);
    expect(document.activeElement).not.toBe(input);
    expect(args.selectModel).not.toHaveBeenCalled();
    expect(args.toggleBulkSelect).not.toHaveBeenCalled();
    expect(args.openBulkDeleteConfirm).not.toHaveBeenCalled();
    modal.remove(); input.remove();
  });

  let searchInput: HTMLInputElement;

  beforeEach(() => {
    searchInput = document.createElement('input');
    searchInput.id = SEARCH_INPUT_ID;
    document.body.appendChild(searchInput);
  });

  afterEach(() => {
    document.body.innerHTML = '';
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

  it('ArrowDown uses spatial position (next row), not just list order', () => {
    placeTile('a', { left: 0, top: 0 });
    placeTile('b', { left: 100, top: 0 });
    const target = placeTile('c', { left: 0, top: 100 });
    const { selectModel } = setup({ filteredIds: ['a', 'b', 'c'], selectedId: 'a' });
    fireKey('ArrowDown');
    expect(selectModel).toHaveBeenCalledWith('c');
    expect(target.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });
  });

  it('ArrowRight scrolls the newly selected tile into view (flat-order path)', () => {
    placeTile('a', { left: 0, top: 0 });
    const target = placeTile('b', { left: 100, top: 0 });
    const { selectModel } = setup({ filteredIds: ['a', 'b', 'c'], selectedId: 'a' });
    fireKey('ArrowRight');
    expect(selectModel).toHaveBeenCalledWith('b');
    expect(target.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });
  });

  it('ArrowUp falls back to flat list order when the selection has no rendered tile', () => {
    // No tile created in the DOM - findSpatialNeighbor returns null, the hook
    // must fall back to the plain list order instead of doing nothing.
    const { selectModel } = setup({ filteredIds: ['a', 'b', 'c'], selectedId: 'b' });
    fireKey('ArrowUp');
    expect(selectModel).toHaveBeenCalledWith('a');
  });

  it('arrow navigation is ignored while typing in an input', () => {
    const otherInput = document.createElement('input');
    document.body.appendChild(otherInput);
    otherInput.focus();
    const { selectModel } = setup({ selectedId: 'a' });
    fireKey('ArrowRight', otherInput);
    expect(selectModel).not.toHaveBeenCalled();
  });

  it('arrow navigation is ignored when navigationEnabled is false (e.g. detail view open)', () => {
    const { selectModel } = setup({ selectedId: 'a', navigationEnabled: false });
    fireKey('ArrowRight');
    expect(selectModel).not.toHaveBeenCalled();
  });

  it('Space toggles bulk selection for the currently selected model', () => {
    const { toggleBulkSelect } = setup({ selectedId: 'b' });
    fireKey(' ');
    expect(toggleBulkSelect).toHaveBeenCalledWith('b');
  });

  it('Space does nothing when no model is selected', () => {
    const { toggleBulkSelect } = setup({ selectedId: null });
    fireKey(' ');
    expect(toggleBulkSelect).not.toHaveBeenCalled();
  });

  it('Space is ignored while typing in an input', () => {
    const otherInput = document.createElement('input');
    document.body.appendChild(otherInput);
    otherInput.focus();
    const { toggleBulkSelect } = setup({ selectedId: 'a' });
    fireKey(' ', otherInput);
    expect(toggleBulkSelect).not.toHaveBeenCalled();
  });

  it('Space is ignored when navigationEnabled is false', () => {
    const { toggleBulkSelect } = setup({ selectedId: 'a', navigationEnabled: false });
    fireKey(' ');
    expect(toggleBulkSelect).not.toHaveBeenCalled();
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
  });
});


describe('P16 regressions A–E', () => {
  afterEach(() => { document.body.innerHTML = ''; });
  it('A: Space on the import button preserves native activation', () => {
    const button = document.createElement('button'); document.body.append(button); button.focus();
    const { toggleBulkSelect } = setup({ selectedId: 'a' });
    expect(fireKey(' ', button).defaultPrevented).toBe(false);
    expect(toggleBulkSelect).not.toHaveBeenCalled();
  });
  it('B: an open sort menu blocks catalog arrows', () => {
    const menu = document.createElement('div'); menu.setAttribute('role', 'menu'); document.body.append(menu);
    const { selectModel } = setup({ selectedId: 'a' }); fireKey('ArrowRight');
    expect(selectModel).not.toHaveBeenCalled();
  });
  it('C: Enter opens the selected model', () => {
    const onOpenDetail = vi.fn(); setup({ selectedId: 'a', ...{ onOpenDetail } }); fireKey('Enter');
    expect(onOpenDetail).toHaveBeenCalledWith('a');
  });
  it('D: modified arrows preserve system shortcuts', () => {
    const { selectModel } = setup({ selectedId: 'a' });
    for (const modifier of ['altKey', 'ctrlKey']) window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', [modifier]: true }));
    expect(selectModel).not.toHaveBeenCalled();
  });
  it('E: folder tree arrows do not move the model selection', () => {
    const tree = document.createElement('div'); tree.setAttribute('role', 'treeitem'); tree.tabIndex = 0; document.body.append(tree); tree.focus();
    const { selectModel } = setup({ selectedId: 'a' }); fireKey('ArrowRight', tree);
    expect(selectModel).not.toHaveBeenCalled();
  });
});

describe('P16 shortcuts', () => {
  afterEach(() => { document.body.innerHTML = ''; vi.unstubAllGlobals(); });
  function key(key: string, options: KeyboardEventInit = {}, target: EventTarget = window) {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...options }); target.dispatchEvent(event); return event;
  }
  it.each(['Win32', 'MacIntel'])('uses only the platform modifier for A and F on %s', platform => {
    vi.stubGlobal('navigator', { platform }); const mac = platform === 'MacIntel';
    const selectAllVisible = vi.fn(); setup({ selectAllVisible });
    key('a', mac ? { ctrlKey: true } : { metaKey: true }); key('a', { ctrlKey: true, metaKey: true });
    expect(selectAllVisible).not.toHaveBeenCalled();
    expect(key('a', mac ? { metaKey: true } : { ctrlKey: true }).defaultPrevented).toBe(true);
    expect(selectAllVisible).toHaveBeenCalledOnce();
    const input = document.createElement('input'); input.id = SEARCH_INPUT_ID; input.value = 'query'; const other = document.createElement('input'); document.body.append(input, other); other.focus();
    key('a', mac ? { metaKey: true } : { ctrlKey: true }, other); expect(selectAllVisible).toHaveBeenCalledOnce();
    expect(key('f', mac ? { metaKey: true } : { ctrlKey: true }, other).defaultPrevented).toBe(true);
    expect(input).toHaveFocus(); expect(input.selectionStart).toBe(0); expect(input.selectionEnd).toBe(5);
  });
  it('Home and End select the boundaries and Shift does not change selection', () => {
    const { selectModel } = setup({ selectedId: 'b' }); key('Home'); key('End');
    expect(selectModel.mock.calls).toEqual([['a'], ['c']]); key('Home', { shiftKey: true }); key('ArrowRight', { shiftKey: true }); expect(selectModel).toHaveBeenCalledTimes(2);
  });
  it('ignores repeated deletion and accepts Cmd+Backspace only on macOS', () => {
    vi.stubGlobal('navigator', { platform: 'MacIntel' }); const { openBulkDeleteConfirm } = setup({ hasBulkSelection: true });
    key('Delete', { repeat: true }); key('Backspace', { metaKey: true, repeat: true }); expect(openBulkDeleteConfirm).not.toHaveBeenCalled();
    key('Backspace', { metaKey: true }); expect(openBulkDeleteConfirm).toHaveBeenCalledOnce();
  });
  it('disabling single keys blocks slash, help, Space and deletion but preserves Ctrl+A and Enter', () => {
    vi.stubGlobal('navigator', { platform: 'Linux' }); const onOpenTips = vi.fn(); const selectAllVisible = vi.fn(); const onOpenDetail = vi.fn();
    const input = document.createElement('input'); input.id = SEARCH_INPUT_ID; document.body.append(input);
    const result = setup({ selectedId: 'a', hasBulkSelection: true, singleKeyShortcuts: false, onOpenTips, selectAllVisible, onOpenDetail });
    for (const value of ['/', '?', ' ', 'Delete', 'Backspace']) expect(key(value).defaultPrevented).toBe(false);
    expect(input).not.toHaveFocus(); expect(onOpenTips).not.toHaveBeenCalled(); expect(result.toggleBulkSelect).not.toHaveBeenCalled(); expect(result.openBulkDeleteConfirm).not.toHaveBeenCalled();
    key('a', { ctrlKey: true }); expect(selectAllVisible).toHaveBeenCalledOnce(); key('Enter'); expect(onOpenDetail).toHaveBeenCalledWith('a');
  });
  it('opens help with shifted ? but keeps Enter and select all out of detail views', () => {
    vi.stubGlobal('navigator', { platform: 'Linux' }); const onOpenTips = vi.fn(); const onOpenDetail = vi.fn(); const selectAllVisible = vi.fn();
    setup({ selectedId: 'a', navigationEnabled: false, onOpenTips, onOpenDetail, selectAllVisible });
    key('?', { shiftKey: true }); expect(onOpenTips).toHaveBeenCalledOnce(); key('Enter'); key('a', { ctrlKey: true });
    expect(onOpenDetail).not.toHaveBeenCalled(); expect(selectAllVisible).not.toHaveBeenCalled();
  });
});
