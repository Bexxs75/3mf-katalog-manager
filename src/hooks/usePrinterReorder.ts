import { useEffect, useState } from 'react';
import { useDragThreshold } from './useDragThreshold';

/** Mouse dragging plus arrow-key reordering for the printer and unit lists. */
export function usePrinterReorder(ids: string[], save: (ids: string[]) => Promise<unknown>) {
  const [dragged, setDragged] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const drag = useDragThreshold(setDragged);
  const move = (from: string, to: string) => {
    const next = [...ids];
    const a = next.indexOf(from), b = next.indexOf(to);
    if (a < 0 || b < 0 || a === b) return;
    next.splice(a, 1);
    next.splice(b, 0, from);
    void save(next).catch(() => {});
  };
  useEffect(() => {
    if (!dragged) return;
    const up = () => {
      if (over) move(dragged, over);
      setDragged(null);
      setOver(null);
    };
    document.addEventListener('mouseup', up);
    return () => document.removeEventListener('mouseup', up);
  });
  return {
    begin: drag.begin,
    enter: (id: string) => { if (dragged) setOver(id); },
    over,
    key: (event: React.KeyboardEvent, id: string) => {
      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
      event.preventDefault();
      const target = ids[ids.indexOf(id) + (event.key === 'ArrowUp' ? -1 : 1)];
      if (target) move(id, target);
    },
  };
}
