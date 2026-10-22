import { useRowSelectionAria } from '../hooks/useRowSelectionAria';
import { displayName } from '../lib/displayName';
import { LicenseBadge } from './LicenseBadge';
import { modelLicenseAriaLabel } from '../lib/licenses';
import { useMemo, type RefObject } from 'react';
import { useModelWindow } from '../hooks/useModelWindow';
import { DragGrip } from './DragGrip';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
import { BulkCheckbox } from './BulkCheckbox';
import { shortTagLabel, tagLabel } from '../lib/autoTags';

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
  // Accessible name of the grid; grouped views pass the folder name.
  label?: string;
}

export function ModelList({ containerRef, windowed = true, models, selectedId, onSelect, onOpenDetail, onContextMenu, readOnly, selectedForBulk, onToggleBulkSelect, onDragFileStart, label }: Props) {
  const { language } = useLanguage();
  const t = useT();

  const ids = useMemo(() => models.map(model => model.id), [models]);
  const window = useModelWindow({ ids, containerRef, gap: 0, fallbackHeight: 43, disabled: !windowed });
  const visible = models.slice(window.startIndex, window.endIndex);
  const fileDrag = useDragThreshold(onDragFileStart);
  const selectionAria = useRowSelectionAria(selectedId, selectedForBulk);

  return (
    // aria-rowcount includes the header row; the window renders only a slice of the data rows,
    // so each row carries its absolute index.
    <div role="grid" data-catalog-grid aria-label={label ?? t('modelListLabel')} aria-multiselectable="true"
      aria-rowcount={models.length + 1} aria-colcount={5} className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
      <div
        role="row" aria-rowindex={1}
        data-model-list-header className="min-w-[680px] grid gap-2.5 items-center px-3 py-2 bg-[var(--panel-2)] border-b border-[var(--line)] ui-label text-[var(--ink-3)]"
        style={{ gridTemplateColumns: '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
      >
        <span role="columnheader"><span className="sr-only">{t('columnSelect')}</span></span>
        <span role="columnheader">{t('columnName')}</span>
        <span role="columnheader">{t('columnTags')}</span>
        <span role="columnheader">{t('columnVolume')}</span>
        <span role="columnheader">{t('columnSize')}</span>
      </div>
      <div ref={window.rootRef} role="rowgroup">
      <div data-window-spacer="top" aria-hidden="true" style={{ height: window.topSpacer }} />
      {visible.map((m, index) => (
        <div
          key={m.id}
          data-model-id={m.id}
          role="row"
          aria-rowindex={window.startIndex + index + 2}
          {...selectionAria(m.id)}
          aria-label={modelLicenseAriaLabel(displayName(m.name), m.license, t)}
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
            <span role="gridcell" />
          ) : (
            <BulkCheckbox
              checked={selectedForBulk.has(m.id)}
              onToggle={() => onToggleBulkSelect(m.id)}
              role="gridcell"
              className="justify-self-center"
            />
          )}
          <span role="gridcell" className="flex items-center gap-1.5 min-w-0">
            {onDragFileStart && !readOnly && <DragGrip />}
            <span className="select-none text-[length:var(--font-size-body)] font-medium truncate">{displayName(m.name)}</span>
            <LicenseBadge license={m.license} />
          </span>
          <span role="gridcell" className="flex min-w-0 gap-1 overflow-hidden">
            {m.tags.map((tag) => (
              <span
                key={tag}
                title={`#${tagLabel(tag, language)}`}
                className="inline-flex shrink-0 font-medium tabular-nums text-[length:var(--font-size-meta)] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
              >
                <span className="whitespace-nowrap">{shortTagLabel(`#${tagLabel(tag, language)}`)}</span>
              </span>
            ))}
          </span>
          <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
            {formatVolumeCm3(m.volumeCm3, language)}
          </span>
          <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
            {formatBytes(m.fileSizeBytes, language)}
          </span>
        </div>
      ))}
      <div data-window-spacer="bottom" aria-hidden="true" style={{ height: window.bottomSpacer }} />
      </div>
    </div>
  );
}
