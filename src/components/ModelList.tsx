import { useEffect, useRef, useState } from 'react';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
import { BulkCheckbox } from './BulkCheckbox';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  readOnly?: boolean;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  onDragFileStart?: (id: string) => void;
}

export function ModelList({ models, selectedId, onSelect, onOpenDetail, onContextMenu, readOnly, selectedForBulk, onToggleBulkSelect, onDragFileStart }: Props) {
  const { language } = useLanguage();
  const t = useT();

  const DRAG_THRESHOLD_PX = 6;
  const [fileDragCandidateId, setFileDragCandidateId] = useState<string | null>(null);
  const [fileDragArmed, setFileDragArmed] = useState(false);
  const fileDragStartPos = useRef<{ x: number; y: number } | null>(null);

  // Identisches Schwellenwert-Muster wie ModelGrid.tsx's Datei-Drag (siehe
  // dort fuer die ausfuehrliche Begruendung) - hier auf Zeilen statt Karten
  // angewendet, damit auch die Listenansicht Dateien per Maus auf
  // Ordner-Kopfzeilen der gruppierten Ansicht ziehen kann.
  useEffect(() => {
    if (!fileDragCandidateId) return;
    const handleMouseMove = (e: MouseEvent) => {
      if (fileDragArmed || !fileDragStartPos.current) return;
      const dx = e.clientX - fileDragStartPos.current.x;
      const dy = e.clientY - fileDragStartPos.current.y;
      if (Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX) {
        setFileDragArmed(true);
        onDragFileStart?.(fileDragCandidateId);
      }
    };
    const handleMouseUp = () => {
      setFileDragCandidateId(null);
      setFileDragArmed(false);
      fileDragStartPos.current = null;
    };
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [fileDragCandidateId, fileDragArmed, onDragFileStart]);

  return (
    <div className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.1em] uppercase text-[var(--ink-3)]"
        style={{ gridTemplateColumns: '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
      >
        <span />
        <span>{t('columnName')}</span>
        <span>{t('columnTags')}</span>
        <span>{t('columnVolume')}</span>
        <span>{t('columnSize')}</span>
      </div>
      {models.map((m) => (
        <div
          key={m.id}
          onClick={() => onSelect(m.id)}
          onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          onMouseDown={(e) => {
            if (!onDragFileStart) return;
            fileDragStartPos.current = { x: e.clientX, y: e.clientY };
            setFileDragCandidateId(m.id);
          }}
          className={`min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          } ${fileDragArmed && fileDragCandidateId === m.id ? 'opacity-50' : ''}`}
          style={{ gridTemplateColumns: '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
        >
          {readOnly ? (
            <span />
          ) : (
            <BulkCheckbox
              checked={selectedForBulk.has(m.id)}
              onToggle={() => onToggleBulkSelect(m.id)}
              className="justify-self-center"
            />
          )}
          <span className="text-[length:var(--font-size-body)] font-medium overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </span>
          <span className="flex gap-1 overflow-hidden">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-mono-ui text-[length:var(--font-size-meta)] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                #{tag}
              </span>
            ))}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span className="font-mono-ui text-[11.5px] text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
        </div>
      ))}
    </div>
  );
}
