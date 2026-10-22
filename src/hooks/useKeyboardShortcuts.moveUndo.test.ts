import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';

function press(key: string, init: KeyboardEventInit = {}, target: EventTarget = window) {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

function setup(overrides: Partial<Parameters<typeof useKeyboardShortcuts>[0]> = {}) {
  const onMoveSelection = vi.fn();
  const onUndo = vi.fn();
  renderHook(() => useKeyboardShortcuts({
    filteredIds: ['a'], selectedId: 'a', selectModel: vi.fn(), hasBulkSelection: false,
    openBulkDeleteConfirm: vi.fn(), navigationEnabled: true, toggleBulkSelect: vi.fn(),
    onMoveSelection, onUndo, ...overrides,
  }));
  return { onMoveSelection, onUndo };
}

function inDom<T extends HTMLElement>(element: T): T {
  document.body.append(element);
  return element;
}

afterEach(() => { document.body.innerHTML = ''; });

describe('M opens the move dialog', () => {
  it('on the page', () => {
    const { onMoveSelection } = setup();
    const event = press('m');
    expect(onMoveSelection).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it.each(['input', 'textarea'])('not while typing in a %s', (tag) => {
    const { onMoveSelection } = setup();
    const field = inDom(document.createElement(tag));
    field.focus();
    press('m', {}, field);
    expect(onMoveSelection).not.toHaveBeenCalled();
  });

  it('not with a modifier', () => {
    const { onMoveSelection } = setup();
    press('m', { ctrlKey: true });
    press('m', { altKey: true });
    expect(onMoveSelection).not.toHaveBeenCalled();
  });

  it('not with single-key shortcuts switched off', () => {
    const { onMoveSelection } = setup({ singleKeyShortcuts: false });
    press('m');
    expect(onMoveSelection).not.toHaveBeenCalled();
  });

  it('not behind a dialog', () => {
    const { onMoveSelection } = setup();
    const dialog = inDom(document.createElement('div'));
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    press('m');
    expect(onMoveSelection).not.toHaveBeenCalled();
  });

  it('not when the catalog is not navigable (detail page)', () => {
    const { onMoveSelection } = setup({ navigationEnabled: false });
    press('m');
    expect(onMoveSelection).not.toHaveBeenCalled();
  });
});

describe('Ctrl+Z undoes', () => {
  it('on the page and after a click on a button, which the other shortcuts ignore', () => {
    const { onUndo } = setup();
    const event = press('z', { ctrlKey: true });
    expect(onUndo).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
    const button = inDom(document.createElement('button'));
    button.focus();
    press('z', { ctrlKey: true }, button);
    expect(onUndo).toHaveBeenCalledTimes(2);
  });

  it.each(['input', 'textarea'])('leaves the undo of a %s to the field', (tag) => {
    const { onUndo } = setup();
    const field = inDom(document.createElement(tag));
    field.focus();
    const event = press('z', { ctrlKey: true }, field);
    expect(onUndo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it('does nothing in a dialog or an open menu, without the modifier or with Shift', () => {
    const { onUndo } = setup();
    press('z');
    press('z', { ctrlKey: true, shiftKey: true });
    expect(onUndo).not.toHaveBeenCalled();
    const menu = inDom(document.createElement('div'));
    menu.setAttribute('data-navigation-menu', '');
    press('z', { ctrlKey: true });
    menu.remove();
    const dialog = inDom(document.createElement('div'));
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    press('z', { ctrlKey: true });
    expect(onUndo).not.toHaveBeenCalled();
  });

  it('is off while no undo handler is given (import running)', () => {
    const { onUndo } = setup({ onUndo: undefined });
    const event = press('z', { ctrlKey: true });
    expect(onUndo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});
