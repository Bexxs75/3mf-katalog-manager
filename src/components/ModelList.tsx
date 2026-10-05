import { useMemo, type RefObject } from 'react';
import { useModelWindow } from '../hooks/useModelWindow';
import { DragGrip } from './DragGrip';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
import { BulkCheckbox } from './BulkCheckbox';
import { tagLabel } from '../lib/autoTags';

interface Props {
  models: ModelFile[];
  containerRef?: RefObject<HTMLDivElement | null>;
  windowed?: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  readOnly?: boolean;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  onDragFileStart?: (id: string) => void;
}

export function ModelList({ containerRef, windowed = true, models, selectedId, onSelect, onOpenDetail, onContextMenu, readOnly, selectedForBulk, onToggleBulkSelect, onDragFileStart }: Props) {
  const { language } = useLanguage();
  const t = useT();

  const ids = useMemo(() => models.map(model => model.id), [models]);
  const window = useModelWindow({ ids, containerRef, gap: 0, fallbackHeight: 43, disabled: !windowed });
  const visible = models.slice(window.startIndex, window.endIndex);
  const fileDrag = useDragThreshold(onDragFileStart);

  return (
    <div className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        data-model-list-header className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] ui-label text-[var(--ink-3)]"
        style={{ gridTemplateColumns: '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
      >
        <span />
        <span>{t('columnName')}</span>
        <span>{t('columnTags')}</span>
        <span>{t('columnVolume')}</span>
        <span>{t('columnSize')}</span>
      </div>
      <div ref={window.rootRef}>
      <div data-window-spacer="top" aria-hidden="true" style={{ height: window.topSpacer }} />
      {visible.map((m) => (
        <div
          key={m.id}
          data-model-id={m.id}
          tabIndex={-1}
          onClick={() => onSelect(m.id)}
          onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          onMouseDown={(e) => {
            if (!onDragFileStart || readOnly) return;
            fileDrag.begin(e, m.id);
          }}
          className={`group min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          } ${fileDrag.draggingId === m.id ? 'opacity-50' : ''}`}
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
          <span className="flex items-center gap-1.5 min-w-0">
            {onDragFileStart && !readOnly && <DragGrip />}
            <span className="text-[length:var(--font-size-body)] font-medium truncate">{m.name}</span>
          </span>
          <span className="flex gap-1 overflow-hidden">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-medium tabular-nums text-[length:var(--font-size-meta)] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                #{tagLabel(tag, language)}
              </span>
            ))}
          </span>
          <span className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
        </div>
      ))}
      <div data-window-spacer="bottom" aria-hidden="true" style={{ height: window.bottomSpacer }} />
      </div>
    </div>
  );
}
