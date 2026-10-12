import { renderHook, fireEvent } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { detailNeighbor, useDetailNavigation } from './useDetailNavigation';

afterEach(() => { document.body.innerHTML = ''; });

it('uses the filtered order without wrapping or falling back for a missing model', () => {
  expect(detailNeighbor(['c', 'a', 'b'], 'a', 'previous')).toBe('c');
  expect(detailNeighbor(['c', 'a', 'b'], 'a', 'next')).toBe('b');
  expect(detailNeighbor(['a'], 'a', 'next')).toBeNull();
  expect(detailNeighbor(['a'], 'missing', 'next')).toBeNull();
  expect(detailNeighbor(['a'], 'a', 'previous')).toBeNull();
});
it('respects input, modal, menu, editing and already handled keys', () => {
  const navigate = vi.fn();
  let editing = false;
  renderHook(() => useDetailNavigation(navigate, () => editing));
  fireEvent.keyDown(window, { key: 'ArrowRight' });
  expect(navigate).toHaveBeenCalledWith('next');
  navigate.mockClear();
  for (const markup of ['<input />', '<textarea></textarea>', '<div contenteditable="true"><span></span></div>', '<div role="dialog" aria-modal="true"></div>', '<div data-navigation-menu></div>']) {
    document.body.insertAdjacentHTML('beforeend', markup);
    const element = document.body.lastElementChild!;
    fireEvent.keyDown(element.firstElementChild ?? element, { key: 'ArrowRight' });
    expect(navigate).not.toHaveBeenCalled();
    element.remove();
  }
  editing = true;
  fireEvent.keyDown(window, { key: 'ArrowLeft' });
  expect(navigate).not.toHaveBeenCalled();
  editing = false;
  const handled = new KeyboardEvent('keydown', {key: 'ArrowRight', cancelable: true});
  handled.preventDefault();
  window.dispatchEvent(handled);
  expect(navigate).not.toHaveBeenCalled();
  fireEvent.keyDown(window, { key: 'ArrowRight', ctrlKey: true });
  expect(navigate).not.toHaveBeenCalled();
  fireEvent.keyDown(window, { key: 'ArrowLeft' });
  expect(navigate).toHaveBeenCalledWith('previous');
});

it.each([
  '<div data-detail-viewer tabindex="0"></div>',
  '<div data-detail-viewer><canvas tabindex="0"></canvas></div>',
  '<button><span>Upload</span></button>',
  '<div data-detail-viewer><div role="group"><button aria-pressed="true">File colors</button></div></div>',
  '<a href="#"><span>Source</span></a>',
  '<div data-detail-viewer><button data-detail-view-toggle>3D</button></div>',
])('navigates both ways while retaining focus on %s', markup => {
  const navigate = vi.fn();
  renderHook(() => useDetailNavigation(navigate, () => false));
  document.body.insertAdjacentHTML('beforeend', markup);
  const target = document.querySelector<HTMLElement>('button, a, canvas, [tabindex]')!;
  target.focus();
  for (const [key, direction] of [['ArrowLeft', 'previous'], ['ArrowRight', 'next']]) {
    fireEvent.keyDown(target.querySelector('span') ?? target, { key });
    expect(navigate).toHaveBeenLastCalledWith(direction);
    expect(target).toHaveFocus();
  }
  expect(navigate).toHaveBeenCalledTimes(2);
});
it.each([
  '<input />', '<textarea></textarea>', '<select></select>',
  '<div contenteditable="true" tabindex="0"></div>',
  ...['slider', 'combobox', 'listbox', 'menuitem', 'tab', 'radio', 'checkbox', 'switch', 'spinbutton', 'treeitem']
    .map(role => `<button role="${role}"></button>`),
  ...['toolbar', 'tablist', 'menu', 'menubar', 'radiogroup']
    .map(role => `<div role="${role}"><button>Local action</button></div>`),
  '<div data-plate-selector><button>Plate</button></div>',
])('leaves arrows with the focused widget %s', markup => {
  const navigate = vi.fn();
  renderHook(() => useDetailNavigation(navigate, () => false));
  document.body.insertAdjacentHTML('beforeend', `<div data-detail-viewer>${markup}</div>`);
  const target = document.querySelector<HTMLElement>('button, input, textarea, select, [tabindex]')!;
  target.focus();
  for (const key of ['ArrowLeft', 'ArrowRight']) {
    fireEvent.keyDown(target, { key });
    fireEvent.keyDown(window, { key });
  }
  expect(navigate).not.toHaveBeenCalled();
  expect(target).toHaveFocus();
});
it('ignores modified, consumed, composing and non-arrow keys on ordinary buttons', () => {
  const navigate = vi.fn();
  renderHook(() => useDetailNavigation(navigate, () => false));
  const button = document.createElement('button');
  document.body.append(button);
  button.focus();
  for (const modifier of ['altKey', 'ctrlKey', 'metaKey', 'shiftKey']) {
    fireEvent.keyDown(button, { key: 'ArrowRight', [modifier]: true });
  }
  for (const key of ['Enter', ' ', 'Home', 'End', 'ArrowUp', 'ArrowDown']) fireEvent.keyDown(button, { key });
  fireEvent.keyDown(button, { key: 'ArrowRight', isComposing: true });
  const consumed = new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true });
  consumed.preventDefault();
  button.dispatchEvent(consumed);
  expect(navigate).not.toHaveBeenCalled();
});

it.each([
  '<div role="dialog" aria-modal="true"></div>',
  '<div role="dialog"></div>', '<div role="menu"></div>',
  '<div role="listbox"></div>', '<div data-navigation-menu></div>', '<div class="menu"></div>',
])('blocks detail arrows while an overlay is open: %s', overlay => {
  const navigate = vi.fn();
  renderHook(() => useDetailNavigation(navigate, () => false));
  document.body.insertAdjacentHTML('beforeend', `<div data-detail-viewer><button>3D</button></div>${overlay}`);
  const button = document.querySelector('button')!;
  button.focus();
  fireEvent.keyDown(button, { key: 'ArrowLeft' });
  fireEvent.keyDown(button, { key: 'ArrowRight' });
  expect(navigate).not.toHaveBeenCalled();
});
