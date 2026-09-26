import { useCallback, useEffect, useRef, useState } from 'react';

export const DRAG_THRESHOLD_PX = 6;

/**
 * Drag start via mouse events instead of HTML5 DnD (under Tauri/WebKitGTK
 * dragDropEnabled intercepts native drag sessions). `begin(e, id)` goes on
 * mouse down; only after DRAG_THRESHOLD_PX of movement does `onStart(id)` fire
 * (once), so a slightly slipping click never starts a drag. `draggingId` stays
 * set until mouse up, e.g. to dim the dragged item.
 */
export function useDragThreshold(onStart: ((id: string) => void) | undefined) {
  const [candidateId, setCandidateId] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);
  const startPos = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!candidateId) return;
    const handleMouseMove = (e: MouseEvent) => {
      if (armed || !startPos.current) return;
      if (Math.hypot(e.clientX - startPos.current.x, e.clientY - startPos.current.y) >= DRAG_THRESHOLD_PX) {
        setArmed(true);
        onStart?.(candidateId);
      }
    };
    const handleMouseUp = () => {
      setCandidateId(null);
      setArmed(false);
      startPos.current = null;
    };
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [candidateId, armed, onStart]);

  const begin = useCallback((e: { clientX: number; clientY: number }, id: string) => {
    startPos.current = { x: e.clientX, y: e.clientY };
    setCandidateId(id);
  }, []);

  return { begin, draggingId: armed ? candidateId : null };
}
