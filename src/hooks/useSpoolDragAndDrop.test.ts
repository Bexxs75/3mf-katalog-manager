import { describe, expect, it, vi } from 'vitest';
import { renderHook, act, fireEvent } from '@testing-library/react';
import { useSpoolDragAndDrop } from './useSpoolDragAndDrop';

function setup() {
  const onLoad = vi.fn();
  const onUnload = vi.fn();
  const hook = renderHook(() => useSpoolDragAndDrop({ onLoad, onUnload }));
  return { ...hook, onLoad, onUnload };
}

function dragTo(result: ReturnType<typeof setup>['result'], target: Parameters<ReturnType<typeof useSpoolDragAndDrop>['enterTarget']>[0]) {
  act(() => { fireEvent.mouseMove(document, { clientX: 50, clientY: 50 }); });
  act(() => result.current.enterTarget(target));
  act(() => { fireEvent.mouseUp(document); });
}

describe('useSpoolDragAndDrop', () => {
  it('loads a storage spool into the slot it is dropped on', () => {
    const { result, onLoad } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 0, clientY: 0, button: 0 }));
    dragTo(result, { kind: 'slot', unitId: 'u1', slotIndex: 2 });
    expect(onLoad).toHaveBeenCalledWith('s1', 'u1', 2);
    expect(result.current.draggingSpoolId).toBeNull();
  });

  it('unloads a slot spool dropped on the storage area', () => {
    const { result, onUnload } = setup();
    act(() => result.current.startDrag('s1', { unitId: 'u1', slotIndex: 0 }, { clientX: 0, clientY: 0, button: 0 }));
    dragTo(result, { kind: 'storage' });
    expect(onUnload).toHaveBeenCalledWith('s1');
  });

  it('ignores storage drops of storage spools and drops onto the same slot', () => {
    const { result, onLoad, onUnload } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 0, clientY: 0, button: 0 }));
    dragTo(result, { kind: 'storage' });
    act(() => result.current.startDrag('s2', { unitId: 'u1', slotIndex: 1 }, { clientX: 0, clientY: 0, button: 0 }));
    dragTo(result, { kind: 'slot', unitId: 'u1', slotIndex: 1 });
    expect(onLoad).not.toHaveBeenCalled();
    expect(onUnload).not.toHaveBeenCalled();
  });

  it('treats a press without movement as a click, not a drag', () => {
    const { result, onLoad } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 10, clientY: 10, button: 0 }));
    act(() => { fireEvent.mouseMove(document, { clientX: 12, clientY: 11 }); });
    act(() => result.current.enterTarget({ kind: 'slot', unitId: 'u1', slotIndex: 0 }));
    expect(result.current.draggingSpoolId).toBeNull();
    act(() => { fireEvent.mouseUp(document); });
    expect(onLoad).not.toHaveBeenCalled();
  });

  it('cancels with Escape', () => {
    const { result, onLoad } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 0, clientY: 0, button: 0 }));
    act(() => { fireEvent.mouseMove(document, { clientX: 50, clientY: 50 }); });
    expect(result.current.draggingSpoolId).toBe('s1');
    act(() => result.current.enterTarget({ kind: 'slot', unitId: 'u1', slotIndex: 0 }));
    act(() => { fireEvent.keyDown(document, { key: 'Escape' }); });
    act(() => { fireEvent.mouseUp(document); });
    expect(onLoad).not.toHaveBeenCalled();
    expect(result.current.draggingSpoolId).toBeNull();
  });

  it('a late leave of the previous slot does not clear the new target', () => {
    const { result, onLoad } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 0, clientY: 0, button: 0 }));
    act(() => { fireEvent.mouseMove(document, { clientX: 50, clientY: 50 }); });
    act(() => result.current.enterTarget({ kind: 'slot', unitId: 'u1', slotIndex: 0 }));
    act(() => result.current.enterTarget({ kind: 'slot', unitId: 'u1', slotIndex: 1 }));
    act(() => result.current.leaveTarget({ kind: 'slot', unitId: 'u1', slotIndex: 0 }));
    act(() => { fireEvent.mouseUp(document); });
    expect(onLoad).toHaveBeenCalledWith('s1', 'u1', 1);
  });

  it('ignores the right mouse button', () => {
    const { result } = setup();
    act(() => result.current.startDrag('s1', null, { clientX: 0, clientY: 0, button: 2 }));
    act(() => { fireEvent.mouseMove(document, { clientX: 50, clientY: 50 }); });
    expect(result.current.draggingSpoolId).toBeNull();
  });
});
