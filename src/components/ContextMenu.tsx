import { useEffect, useRef, useState } from 'react';

interface Props {
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
}

export function ContextMenu({ x, y, onClose, onOpenInSlicer, onDelete }: Props) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[172px] rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg overflow-hidden"
      style={{ left: x, top: y }}
    >
      {confirmDelete ? (
        <div className="px-3 py-2.5">
          <div className="text-[12px] font-medium text-[var(--ink)] pb-2">Eintrag löschen?</div>
          <div className="flex gap-1.5">
            <button
              onClick={() => setConfirmDelete(false)}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              Abbrechen
            </button>
            <button
              onClick={() => {
                onDelete();
                onClose();
              }}
              className="flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
            >
              Löschen
            </button>
          </div>
        </div>
      ) : (
        <>
          <button
            onClick={() => {
              onOpenInSlicer();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            In Slicer öffnen
          </button>
          <button
            onClick={() => setConfirmDelete(true)}
            className="w-full text-left px-3 py-2 text-[12.5px] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            Löschen
          </button>
        </>
      )}
    </div>
  );
}
