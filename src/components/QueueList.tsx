import { useEffect, useState } from 'react';
import type { FilamentCheck, ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { QueueFilamentSymbol } from './QueueFilamentSymbol';

interface QueueListProps {
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  queueFilament?: Map<string, FilamentCheck> | null;
}

export function QueueList({ queue, onQueueReorder, onQueueRemove, onQueueSelect, queueFilament }: QueueListProps) {
  const t = useT();
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

  // Reordering via mouse events instead of HTML5 DnD: dragDropEnabled (needed
  // for file import) intercepts native drag sessions under WebKitGTK.
  useEffect(() => {
    if (dragIndex === null) return;
    const handleMouseUp = () => {
      const from = dragIndex;
      const to = overIndex;
      setDragIndex(null);
      setOverIndex(null);
      if (from === null) return;
      if (to === null || to === from) {
        onQueueSelect(queue[from].id);
        return;
      }
      const ids = queue.map((m) => m.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      onQueueReorder(ids);
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [dragIndex, overIndex, queue, onQueueReorder, onQueueSelect]);

  return (
    <>
      {queue.length === 0 && (
        <div className="px-1.5 pb-2 font-mono-ui text-[10.5px] text-[var(--ink-3)]">
          {t('queueEmptyState')}
        </div>
      )}
      {queue.map((model, index) => (
        <div
          key={model.id}
          onMouseDown={() => {
            setDragIndex(index);
            setOverIndex(index);
          }}
          onMouseEnter={() => {
            if (dragIndex !== null) setOverIndex(index);
          }}
          className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-grab select-none ${
            dragIndex === index ? 'opacity-50' : ''
          } ${
            dragIndex !== null && overIndex === index && dragIndex !== index
              ? 'bg-[var(--panel-2)]'
              : ''
          } text-[var(--ink-2)] hover:text-[var(--ink)]`}
        >
          <span className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] w-3.5">{index + 1}</span>
          <QueueFilamentSymbol check={queueFilament?.get(model.id)} />
          <span className="flex-1 text-[length:var(--font-size-item)] overflow-hidden text-ellipsis whitespace-nowrap">
            {model.name}
          </span>
          <span
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onQueueRemove(model.id);
            }}
            className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
          >
            ✕
          </span>
        </div>
      ))}
    </>
  );
}
