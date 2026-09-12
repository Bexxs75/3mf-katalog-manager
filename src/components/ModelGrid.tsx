import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { useUiDensity } from '../hooks/UiDensityContext';
import { formatWeightG } from '../i18n/format';
import { BulkCheckbox } from './BulkCheckbox';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface Props {
  models: ModelFile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  onToggleFavorite: (id: string) => void;
  readOnly?: boolean;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  displayPreference: DisplayPreference;
  reorderable?: boolean;
  onReorder?: (orderedIds: string[]) => void;
}

export function ModelGrid({ models, selectedId, onSelect, onOpenDetail, onContextMenu, onToggleFavorite, readOnly, selectedForBulk, onToggleBulkSelect, displayPreference, reorderable, onReorder }: Props) {
  const t = useT();
  const { density } = useUiDensity();
  const { language } = useLanguage();

  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

  // Reihenfolge-Aenderung per Maus-Events statt nativem HTML5-Drag&Drop -
  // identisches Muster wie die Warteschlange in Sidebar.tsx. Natives
  // draggable/onDragStart/onDragOver/onDrop funktioniert unter Tauri/
  // WebKitGTK nicht zuverlaessig, da dragDropEnabled native Drag-Sessions
  // auf Fensterebene abfaengt.
  useEffect(() => {
    if (!reorderable || dragIndex === null) return;
    const handleMouseUp = () => {
      const from = dragIndex;
      const to = overIndex;
      setDragIndex(null);
      setOverIndex(null);
      if (from === null || to === null || to === from) return;
      const ids = models.map((m) => m.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      onReorder?.(ids);
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [reorderable, dragIndex, overIndex, models, onReorder]);

  function renderCompactCard(m: ModelFile) {
    return (
      <div
        key={m.id}
        onClick={() => onSelect(m.id)}
        onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(m.id);
          onContextMenu(m.id, e.clientX, e.clientY);
        }}
        onMouseDown={() => reorderable && setDragIndex(models.findIndex((x) => x.id === m.id))}
        onMouseEnter={() => reorderable && dragIndex !== null && setOverIndex(models.findIndex((x) => x.id === m.id))}
        className={`rounded-[4px] overflow-hidden border cursor-pointer ${
          m.id === selectedId ? 'border-[var(--accent)]' : 'border-[var(--line)]'
        }`}
      >
        <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          {!readOnly && (
            <BulkCheckbox
              checked={selectedForBulk.has(m.id)}
              onToggle={() => onToggleBulkSelect(m.id)}
              className="absolute top-1.5 right-1.5 z-10"
            />
          )}
          {resolveDisplayImage(m, displayPreference) ? (
            <img
              src={resolveDisplayImage(m, displayPreference) ?? undefined}
              alt=""
              className="absolute inset-0 w-full h-full object-cover"
            />
          ) : (
            <>
              <div
                className="absolute inset-0 opacity-90"
                style={{
                  backgroundImage:
                    'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
                }}
              />
              <div className="absolute inset-0 grid place-items-center">
                <div className="flex flex-col items-center gap-1.5">
                  <div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" />
                  <div className="font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                    {t('previewLabel3d')}
                  </div>
                </div>
              </div>
            </>
          )}
          {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
            <div className="absolute left-[7px] top-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
              {t('newBadge')}
            </div>
          )}
          {m.printStatus === 'printed' && (
            <div className="absolute right-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
              ✓ {t('printedBadge')}
            </div>
          )}
          {!readOnly && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(m.id);
              }}
              aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`absolute left-[7px] bottom-[7px] font-mono-ui text-[9px] px-1 py-0.5 rounded border cursor-pointer ${
                m.favorite
                  ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                  : 'border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]'
              }`}
            >
              {m.favorite ? '♥' : '♡'}
            </button>
          )}
        </div>
        <div className="flex flex-col gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]">
          <div className="text-[12.5px] font-semibold overflow-hidden text-ellipsis whitespace-nowrap">
            {m.name}
          </div>
          <div className="flex flex-wrap gap-1">
            {m.tags.map((tag) => (
              <span
                key={tag}
                className="font-mono-ui text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]"
              >
                #{tag}
              </span>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="grid gap-3.5"
      style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${density === 'comfort' ? 220 : 178}px, 1fr))` }}
    >
      {models.map((m) =>
        density === 'comfort' ? (
          <div
            key={m.id}
            onClick={() => onSelect(m.id)}
            onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              onSelect(m.id);
              onContextMenu(m.id, e.clientX, e.clientY);
            }}
            onMouseDown={() => reorderable && setDragIndex(models.findIndex((x) => x.id === m.id))}
            onMouseEnter={() => reorderable && dragIndex !== null && setOverIndex(models.findIndex((x) => x.id === m.id))}
            className={`rounded-[var(--radius-card)] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 ${
              m.id === selectedId ? 'border-[var(--accent)]' : 'border-transparent'
            }`}
          >
            <div className="h-[5px]" style={{ background: 'linear-gradient(90deg, var(--accent), var(--accent-soft))' }} />
            <div className="relative aspect-square bg-[var(--plate)] overflow-hidden">
              {!readOnly && (
                <BulkCheckbox
                  checked={selectedForBulk.has(m.id)}
                  onToggle={() => onToggleBulkSelect(m.id)}
                  className="absolute top-1.5 right-1.5 z-10"
                />
              )}
              {resolveDisplayImage(m, displayPreference) ? (
                <img src={resolveDisplayImage(m, displayPreference) ?? undefined} alt="" className="absolute inset-0 w-full h-full object-cover" />
              ) : (
                <>
                  <div
                    className="absolute inset-0 opacity-90"
                    style={{
                      backgroundImage:
                        'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 12px)',
                    }}
                  />
                  <div className="absolute inset-0 grid place-items-center">
                    <div className="w-[64px] h-[64px] border border-dashed border-[var(--line-strong)] rotate-45" />
                  </div>
                </>
              )}
              {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
                <div className="absolute left-2.5 top-2.5 font-semibold text-[11px] px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]">
                  {t('newBadge')}
                </div>
              )}
              {m.printStatus === 'printed' && (
                <div className="absolute left-2.5 bottom-2.5 font-semibold text-[11px] px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]">
                  ✓ {t('printedBadge')}
                </div>
              )}
              {!readOnly && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFavorite(m.id);
                  }}
                  aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
                  style={{ width: 'var(--icon-badge-size)', height: 'var(--icon-badge-size)' }}
                  className={`absolute right-2.5 bottom-2.5 rounded-full grid place-items-center bg-[var(--panel)]/90 shadow-[var(--shadow)] cursor-pointer text-[17px] ${
                    m.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'
                  }`}
                >
                  {m.favorite ? '♥' : '♡'}
                </button>
              )}
            </div>
            <div className="flex flex-col gap-2" style={{ padding: 'var(--space-card-pad)' }}>
              <div
                className="font-bold overflow-hidden text-ellipsis whitespace-nowrap"
                style={{ fontSize: 'var(--font-size-title)' }}
              >
                {m.name}
              </div>
              <div className="flex flex-wrap gap-1.5">
                {m.tags.map((tag) => (
                  <span
                    key={tag}
                    className="px-2.5 py-1 rounded-full bg-[var(--panel-2)] text-[var(--ink-2)]"
                    style={{ fontSize: 'var(--font-size-meta)' }}
                  >
                    #{tag}
                  </span>
                ))}
              </div>
              <div
                className="flex items-center gap-3 text-[var(--ink-3)] font-medium"
                style={{ fontSize: 'var(--font-size-meta)' }}
              >
                {m.materials[0] && <span>📦 {m.materials[0].name}</span>}
                {m.estimatedWeightG !== null && <span>⚖ ≈ {formatWeightG(m.estimatedWeightG, language)}</span>}
              </div>
            </div>
          </div>
        ) : (
          renderCompactCard(m)
        ),
      )}
    </div>
  );
}
