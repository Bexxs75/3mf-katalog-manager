import { canEditMetadata } from '../lib/webSession';
import { reportMetadataError } from '../lib/metadataErrors';
import { useEffect, useState } from 'react';
import { useDragThreshold } from './useDragThreshold';

/** Mouse dragging plus arrow-key reordering for the printer and unit lists. */
/** `listed` are the ids the user sees, in order; keys step between those while `ids` stays the complete order that is saved. */
export function usePrinterReorder(ids: string[], save: (ids: string[]) => Promise<unknown>, listed: string[] = ids) {
  const [dragged, setDragged] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const drag = useDragThreshold(setDragged);
  const move = (from: string, to: string) => {
    if (!canEditMetadata()) return;
    const next = [...ids];
    const a = next.indexOf(from), b = next.indexOf(to);
    if (a < 0 || b < 0 || a === b) return;
    next.splice(a, 1);
    next.splice(b, 0, from);
    void save(next).catch(reportMetadataError);
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
    begin: (...args: Parameters<typeof drag.begin>) => { if (canEditMetadata()) drag.begin(...args); },
    enter: (id: string) => { if (dragged) setOver(id); },
    over,
    key: (event: React.KeyboardEvent, id: string) => {
      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
      event.preventDefault();
      const target = listed[listed.indexOf(id) + (event.key === 'ArrowUp' ? -1 : 1)];
      if (target) move(id, target);
    },
  };
}
