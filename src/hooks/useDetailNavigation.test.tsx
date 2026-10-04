import { renderHook, fireEvent } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { detailNeighbor, useDetailNavigation } from './useDetailNavigation';

it('uses the filtered order without wrapping or falling back for a missing model', () => {
  expect(detailNeighbor(['c', 'a', 'b'], 'a', 'previous')).toBe('c');
  expect(detailNeighbor(['c', 'a', 'b'], 'a', 'next')).toBe('b');
  expect(detailNeighbor(['a'], 'a', 'next')).toBeNull();
  expect(detailNeighbor(['a'], 'missing', 'next')).toBeNull();
  expect(detailNeighbor(['a'], 'a', 'previous')).toBeNull();
});
it('respects input, modal, menu, viewer, editing and already handled keys', () => {
  const navigate = vi.fn();
  let editing = false;
  renderHook(() => useDetailNavigation(navigate, () => editing));
  fireEvent.keyDown(window, { key: 'ArrowRight' });
  expect(navigate).toHaveBeenCalledWith('next');
  navigate.mockClear();
  for (const markup of ['<input />', '<textarea></textarea>', '<div contenteditable="true"><span></span></div>', '<div role="dialog" aria-modal="true"></div>', '<div data-navigation-menu></div>', '<div data-detail-viewer tabindex="0"></div>']) {
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
