import { useCallback, useEffect, useRef, useState } from 'react';

export type SpoolDropTarget =
  | { kind: 'slot'; unitId: string; slotIndex: number }
  | { kind: 'storage' };

interface DragSource {
  spoolId: string;
  /** Kommt die Spule aus einem Fach? Nur dann ist "zurueck ins Lager" ein Ziel. */
  fromSlot: { unitId: string; slotIndex: number } | null;
}

interface Handlers {
  onLoad: (spoolId: string, unitId: string, slotIndex: number) => void;
  onUnload: (spoolId: string) => void;
  /**
   * Passt die gezogene Spule in diese Einheit? Resin-Flaschen nur in eine
   * Harzwanne, Filament nie. Ein unpassendes Fach wird nie zum Ziel.
   */
  canDropOnSlot?: (spoolId: string, unitId: string) => boolean;
}

/** Ab so vielen Pixeln Mausbewegung wird aus einem Klick ein Ziehen. */
const DRAG_THRESHOLD_PX = 4;

/**
 * Zeigerbasiertes Ziehen von Spulen zwischen Lager und Faechern. Kein
 * natives HTML5-Drag-&-Drop: unter Tauri/WebKitGTK faengt `dragDropEnabled`
 * native Drag-Sessions auf Fensterebene ab (gleiches Muster wie
 * `useFolderDragAndDrop`). Erst nach `DRAG_THRESHOLD_PX` Bewegung gilt ein
 * Mausdruck als Ziehen, damit ein einfacher Klick auf ein Fach weiter dessen
 * Menue oeffnet. Escape bricht ab.
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
      // Sonst startet der Mausdruck eine Textauswahl, die beim Ziehen ueber die
      // ganze Seite aufgezogen wird (select-none greift erst nach der Schwelle).
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
    // `fits` liest nur den Ref, braucht also keine eigene Abhaengigkeit.
    [active, source],
  );

  // Spaetes mouseleave darf ein neueres mouseenter nicht ueberschreiben (wie in useFolderDragAndDrop).
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
    /** Gezogene Spule, sobald wirklich gezogen wird (nicht schon beim Mausdruck). */
    draggingSpoolId: active && source ? source.spoolId : null,
    draggingFromSlot: active && source ? source.fromSlot : null,
    pointer,
    target,
    startDrag,
    enterTarget,
    leaveTarget,
  };
}
