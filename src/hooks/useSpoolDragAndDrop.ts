import { useCallback, useEffect, useRef, useState } from 'react';

export type SpoolDropTarget =
  | { kind: 'slot'; unitId: string; slotIndex: number }
  | { kind: 'storage' };

interface DragSource {
  spoolId: string;
  /** Does the spool come from a slot? Only then is "back to storage" a target. */
  fromSlot: { unitId: string; slotIndex: number } | null;
}

interface Handlers {
  onLoad: (spoolId: string, unitId: string, slotIndex: number) => void;
  onUnload: (spoolId: string) => void;
  /**
   * Does the dragged spool fit this unit? Resin bottles only into a resin
   * vat, filament never. An unsuitable slot never becomes a target.
   */
  canDropOnSlot?: (spoolId: string, unitId: string) => boolean;
}

/** Mouse movement in pixels after which a click becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

/**
 * Pointer-based dragging of spools between storage and slots. No native
 * HTML5 drag & drop: under Tauri/WebKitGTK `dragDropEnabled` intercepts
 * native drag sessions at window level (same pattern as
 * `useFolderDragAndDrop`). Only after `DRAG_THRESHOLD_PX` of movement does a
 * mouse down count as a drag, so a simple click on a slot still opens its
 * menu. Escape cancels.
 */
export function useSpoolDragAndDrop({ onLoad, onUnload, canDropOnSlot }: Handlers) {
  const [source, setSource] = useState<DragSource | null>(null);
  const [active, setActive] = useState(false);
  const [target, setTarget] = useState<SpoolDropTarget | null>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const origin = useRef<{ x: number; y: number } | null>(null);
  const handlers = useRef({ onLoad, onUnload, canDropOnSlot });
  handlers.current = { onLoad, onUnload, canDropOnSlot };
  const fits = (spoolId: string, unitId: string) => handlers.current.canDropOnSlot?.(spoolId, unitId) ?? true;

  const reset = useCallback(() => {
    setSource(null);
    setActive(false);
    setTarget(null);
    setPointer(null);
    origin.current = null;
  }, []);

  const startDrag = useCallback(
    (
      spoolId: string,
      fromSlot: DragSource['fromSlot'],
      event: { clientX: number; clientY: number; button?: number; preventDefault?: () => void },
    ) => {
      if (event.button !== undefined && event.button !== 0) return;
      // Otherwise the mouse down starts a text selection that spreads across the
      // whole page while dragging (select-none only applies after the threshold).
      event.preventDefault?.();
      origin.current = { x: event.clientX, y: event.clientY };
      setSource({ spoolId, fromSlot });
    },
    [],
  );

  useEffect(() => {
    if (!source) return;
    const handleMove = (e: MouseEvent) => {
      const start = origin.current;
      if (!start) return;
      if (!active && Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_THRESHOLD_PX) return;
      if (!active) window.getSelection()?.removeAllRanges();
      setActive(true);
      setPointer({ x: e.clientX, y: e.clientY });
    };
    const handleUp = () => {
      const current = target;
      const wasActive = active;
      const dragged = source;
      reset();
      if (!wasActive || !current) return;
      if (current.kind === 'slot') {
        const same =
          dragged.fromSlot?.unitId === current.unitId && dragged.fromSlot?.slotIndex === current.slotIndex;
        if (!same && fits(dragged.spoolId, current.unitId)) {
          handlers.current.onLoad(dragged.spoolId, current.unitId, current.slotIndex);
        }
      } else if (dragged.fromSlot) {
        handlers.current.onUnload(dragged.spoolId);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') reset();
    };
    document.addEventListener('mousemove', handleMove);
    document.addEventListener('mouseup', handleUp);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousemove', handleMove);
      document.removeEventListener('mouseup', handleUp);
      document.removeEventListener('keydown', handleKey);
    };
  }, [source, active, target, reset]);

  const enterTarget = useCallback(
    (next: SpoolDropTarget) => {
      if (!active || !source) return;
      if (next.kind === 'slot' && !fits(source.spoolId, next.unitId)) return;
      setTarget(next);
    },
    // `fits` only reads the ref, so it needs no dependency of its own.
    [active, source],
  );

  // A late mouseleave must not overwrite a newer mouseenter (as in useFolderDragAndDrop).
  const leaveTarget = useCallback((left: SpoolDropTarget) => {
    setTarget((current) => {
      if (!current) return null;
      if (current.kind === 'storage' && left.kind === 'storage') return null;
      if (
        current.kind === 'slot' &&
        left.kind === 'slot' &&
        current.unitId === left.unitId &&
        current.slotIndex === left.slotIndex
      ) {
        return null;
      }
      return current;
    });
  }, []);

  return {
    /** Dragged spool once dragging really happens (not already on mouse down). */
    draggingSpoolId: active && source ? source.spoolId : null,
    draggingFromSlot: active && source ? source.fromSlot : null,
    pointer,
    target,
    startDrag,
    enterTarget,
    leaveTarget,
  };
}
