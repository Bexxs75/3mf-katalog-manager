import { describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { DRAG_THRESHOLD_PX, useDragThreshold } from './useDragThreshold';

function move(x: number, y: number) {
  document.dispatchEvent(new MouseEvent('mousemove', { clientX: x, clientY: y }));
}

describe('useDragThreshold', () => {
  it('does not start on a click without movement beyond the threshold', () => {
    const onStart = vi.fn();
    const { result } = renderHook(() => useDragThreshold(onStart));
    act(() => result.current.begin({ clientX: 10, clientY: 10 }, 'a'));
    act(() => move(10 + DRAG_THRESHOLD_PX - 1, 10));
    act(() => document.dispatchEvent(new MouseEvent('mouseup')));
    expect(onStart).not.toHaveBeenCalled();
    expect(result.current.draggingId).toBeNull();
  });

  it('starts exactly once past the threshold and resets on mouse up', () => {
    const onStart = vi.fn();
    const { result } = renderHook(() => useDragThreshold(onStart));
    act(() => result.current.begin({ clientX: 0, clientY: 0 }, 'a'));
    act(() => move(DRAG_THRESHOLD_PX, 0));
    act(() => move(40, 40));
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(onStart).toHaveBeenCalledWith('a');
    expect(result.current.draggingId).toBe('a');
    act(() => document.dispatchEvent(new MouseEvent('mouseup')));
    expect(result.current.draggingId).toBeNull();
  });
});
