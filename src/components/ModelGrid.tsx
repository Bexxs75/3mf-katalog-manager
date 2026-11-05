import { useCanEditMetadata } from '../lib/webSession';
import { useRowSelectionAria } from '../hooks/useRowSelectionAria';
import { displayName } from '../lib/displayName';
import { LicenseBadge } from './LicenseBadge';
import { licenseUsage, modelLicenseAriaLabel } from '../lib/licenses';
import { Icon } from './Icon';
import { useModelImages } from '../hooks/useModelImages';
import { useModelWindow } from '../hooks/useModelWindow';
import { DragGrip } from './DragGrip';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import type { ModelFile } from '../types';
import type { BundleCard } from '../lib/bundles';
import { useT, useLanguage, useFormatCount } from '../i18n/LanguageContext';
import { useUiDensity } from '../hooks/UiDensityContext';
import { formatPrintDuration, formatWeightG } from '../i18n/format';
import { fileKindOf, gcodeBadge } from '../lib/fileKind';
import { printFactsOf } from '../lib/printData';
import { tagLabel } from '../lib/autoTags';
import { BulkCheckbox } from './BulkCheckbox';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { DRAG_THRESHOLD_PX, useDragThreshold } from '../hooks/useDragThreshold';
import { useActivePrintFilter } from '../hooks/PrintFilterContext';
import { PrintFactsExtra, printFactsExtraHeight } from './PrintFactsExtra';
import { useBundleCardsSettings, useCardItems } from '../hooks/BundleCardsContext';
import { BundleGridCard, PartOfChip, usePartOfText } from './BundleCardParts';
import { cardNavId, cardSelectionState, cardToggleIds, isCardOpen } from '../lib/bundleCards';

interface Props {
  models: ModelFile[];
  containerRef?: RefObject<HTMLDivElement | null>;
  windowed?: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  /** `bundleId` is set when the menu is for a bundle card (the id is then its title model). */
  onContextMenu: (id: string, x: number, y: number, bundleId?: string) => void;
  onToggleFavorite: (id: string) => void;
  readOnly?: boolean;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  displayPreference: DisplayPreference;
  reorderable?: boolean;
  onReorder?: (orderedIds: string[]) => void;
  onDragFileStart?: (id: string) => void;
  // Accessible name of the grid; grouped views pass the folder name.
  label?: string;
}

function CardTags({ tags, comfort }: { tags: string[]; comfort: boolean }) {
  const { language } = useLanguage();
  const t = useT();
  const formatCount = useFormatCount();
  const rowRef = useRef<HTMLDivElement>(null);
  const probeRef = useRef<HTMLSpanElement>(null);
  const [hiddenCount, setHiddenCount] = useState(0);
  const chipClass = comfort
    ? 'shrink-0 whitespace-nowrap font-medium text-caption px-2.5 py-1 rounded-full bg-[var(--panel-2)] text-[var(--ink-2)]'
    : 'shrink-0 whitespace-nowrap font-medium tabular-nums text-compact-meta px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]';
  useLayoutEffect(() => {
    const row = rowRef.current;
    const probe = probeRef.current;
    if (!row || !probe) return;
    const chips = [...row.querySelectorAll<HTMLElement>('[data-tag-index]')];
    const measure = () => {
      const width = row.clientWidth;
      const end = (index: number) => chips[index].offsetLeft + chips[index].offsetWidth;
      let visible = chips.length;
      if (visible && end(visible - 1) > width) {
        // Reserve the actual counter width, including changes at digit boundaries.
        do {
          visible--;
          probe.textContent = `+${chips.length - visible}`;
        } while (visible > 0 && end(visible - 1) + (comfort ? 6 : 4) + probe.offsetWidth > width);
      }
      setHiddenCount(chips.length - visible);
    };
    measure();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    observer?.observe(row);
    chips.forEach(chip => observer?.observe(chip));
    return () => observer?.disconnect();
  }, [tags, language, comfort]);
  const title = `${formatCount(t('cardMoreTags'), hiddenCount)}: ${tags.map(tag => tagLabel(tag, language)).join(', ')}`;
  return <div ref={rowRef} data-card-tags className={`relative flex flex-nowrap overflow-hidden shrink-0 ${comfort ? 'gap-1.5' : 'gap-1'}`}
    style={{ height: comfort ? 28 : 21, fontSize: comfort ? 'var(--font-size-meta)' : undefined, lineHeight: comfort ? '20px' : '15px' }}>
    {tags.map((tag, index) => <span key={tag} data-tag-index={index} className={chipClass}
      style={{ visibility: index >= tags.length - hiddenCount ? 'hidden' : undefined }} aria-hidden={index >= tags.length - hiddenCount || undefined}>
      #{tagLabel(tag, language)}
    </span>)}
    <span ref={probeRef} data-tag-counter-probe aria-hidden="true" className={`${chipClass} absolute right-0 invisible`}>+{tags.length}</span>
    {hiddenCount > 0 && <span data-more-tags className={`${chipClass} absolute right-0`} title={title} aria-label={title}>+{hiddenCount}</span>}
  </div>;
}

/** Print time of a slicer output, taken from the list's facts; only a time the slicer gave (never a partial sum). */
function gcodePrintSeconds(model: ModelFile): number | null {
  const facts = printFactsOf(model);
  return facts?.printSecondsComplete && facts.printSeconds !== undefined ? facts.printSeconds : null;
}

/** Accessible name of a card: the name and license, plus the file type for slicer output (the badge is only a picture to the eye). */
function cardAriaLabel(model: ModelFile, t: ReturnType<typeof useT>, partOf: string | null): string {
  const label = modelLicenseAriaLabel(displayName(model.name), model.license, t);
  const named = fileKindOf(model) === 'gcode' ? `${label}, ${gcodeBadge(model.path)}` : label;
  return partOf ? `${named}, ${partOf}` : named;
}

/** Type badge on the picture of slicer output; a model needs none. */
function KindBadge({ model, comfort }: { model: ModelFile; comfort: boolean }) {
  if (fileKindOf(model) !== 'gcode') return null;
  return <span data-kind-badge aria-hidden="true"
    className={`absolute z-[1] font-bold tracking-[0.05em] text-caption border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] ${comfort ? 'left-2.5 top-2.5 px-2 py-0.5 rounded-lg' : 'left-[7px] top-[7px] px-1.5 py-px rounded'}`}>
    {gcodeBadge(model.path)}
  </span>;
}

/** Where the "new" badge sits: below the type badge when there is one. */
function newBadgePosition(model: ModelFile, comfort: boolean): string {
  if (fileKindOf(model) === 'gcode') return comfort ? 'left-2.5 top-11' : 'left-[7px] top-[30px]';
  return comfort ? 'left-2.5 top-2.5' : 'left-[7px] top-[7px]';
}

function CardMeta({ model, comfort }: { model: ModelFile; comfort: boolean }) {
  const { language } = useLanguage();
  const t = useT();
  const seconds = fileKindOf(model) === 'gcode' ? gcodePrintSeconds(model) : null;
  return <div className="flex items-center gap-3 text-[var(--ink-3)] font-medium overflow-hidden whitespace-nowrap shrink-0"
    style={{ height: comfort ? 20 : 15, lineHeight: comfort ? '20px' : '15px', fontSize: 'var(--font-size-meta)' }}>
    {seconds !== null && <span data-card-print-time className="shrink-0">{formatPrintDuration(seconds, language, t)}</span>}
    {model.materials[0] && <span className="truncate"><Icon name="box" size={14} /> {model.materials[0].name}</span>}
    {model.estimatedWeightG !== null && <span className="shrink-0"><Icon name="weight" size={14} /> {model.weightSource === 'slicer'
      ? formatWeightG(model.estimatedWeightG, language)
      : `≈ ${formatWeightG(model.estimatedWeightG, language)}`}</span>}
  </div>;
}

export function ModelGrid({ containerRef, windowed = true, models, selectedId, onSelect, onOpenDetail, onContextMenu, onToggleFavorite, readOnly, selectedForBulk, onToggleBulkSelect, displayPreference, reorderable, onReorder, onDragFileStart, label }: Props) {
  const canEdit = useCanEditMetadata();
  const t = useT();
  const { density } = useUiDensity();
  // Only while a print data filter is active; then every card gets the same extra height.
  const printFilter = useActivePrintFilter();
  const extra = (comfort: boolean) => printFilter ? printFactsExtraHeight(comfort) : 0;
  // A bundle shown as one card stands for its title model in the navigation order and the picture list.
  const items = useCardItems(models);
  const trash = useBundleCardsSettings()?.trash ?? false;
  const partOfText = usePartOfText(trash);
  const ids = useMemo(() => items.map(cardNavId), [items]);
  // Tiles keep one fixed width. A stretching row (1fr) hands the leftover width to the tiles,
  // so they grow and shrink every time the detail panel opens or the window changes.
  const fixedTileWidth = density === 'comfort' ? 232 : 190;
  const window = useModelWindow({ ids, containerRef, minWidth: fixedTileWidth, gap: 14,
    fallbackHeight: (density === 'comfort' ? 350 : 265) + extra(density === 'comfort'), disabled: reorderable || !windowed });
  const visible = items.slice(window.startIndex, window.endIndex);
  const images = useModelImages(ids.slice(window.imageStartIndex, window.imageEndIndex));
  const imageFor = (model: ModelFile) => resolveDisplayImage(model, displayPreference, images.get(model.id));

  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const [dragArmed, setDragArmed] = useState(false);
  const dragStartPos = useRef<{ x: number; y: number } | null>(null);

  // Reordering via mouse events instead of HTML5 DnD (see useDragThreshold).
  // Only movement beyond DRAG_THRESHOLD_PX counts as dragging, otherwise a
  // slipped click on the checkbox or star would already reorder.
  useEffect(() => {
    if (!canEdit || !reorderable || dragIndex === null) return;
    const handleMouseMove = (e: MouseEvent) => {
      if (dragArmed || !dragStartPos.current) return;
      const dx = e.clientX - dragStartPos.current.x;
      const dy = e.clientY - dragStartPos.current.y;
      if (Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX) setDragArmed(true);
    };
    const handleMouseUp = () => {
      const from = dragIndex;
      const to = overIndex;
      const armed = dragArmed;
      setDragIndex(null);
      setOverIndex(null);
      setDragArmed(false);
      dragStartPos.current = null;
      if (!armed || from === null || to === null || to === from) return;
      const ids = models.map((m) => m.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      onReorder?.(ids);
    };
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [reorderable, dragIndex, overIndex, dragArmed, models, onReorder]);

  // Cards as drag source for moving into a folder (only without
  // `reorderable`), with the same threshold. The move itself is triggered by the
  // global mouseup handler in App.tsx; here only `onDragFileStart`.
  const fileDrag = useDragThreshold(onDragFileStart);

  const handleCardMouseDown = (e: { clientX: number; clientY: number }, id: string) => {
    if (!canEdit) return;
    if (reorderable) {
      dragStartPos.current = { x: e.clientX, y: e.clientY };
      setDragIndex(models.findIndex((x) => x.id === id));
    } else if (onDragFileStart && !readOnly) {
      fileDrag.begin(e, id);
    }
  };

  // Every card is a grid row with two cells (picture and details). aria-rowindex/aria-rowcount
  // describe the full list, because the window only renders a slice of it.
  const selectionAria = useRowSelectionAria(selectedId, selectedForBulk);
  const rowAria = (m: ModelFile, index: number) => ({
    role: 'row' as const,
    'aria-rowindex': window.startIndex + index + 1,
    ...selectionAria(m.id),
  });

  function renderBundleCard(card: BundleCard, index: number) {
    const comfort = density === 'comfort';
    const state = cardSelectionState(card, selectedForBulk);
    const open = isCardOpen(card, selectedId);
    // Selected means every member is in the batch selection; with no batch selection the open card counts.
    const base = selectionAria(open && selectedId !== null ? selectedId : card.title.id);
    const rowProps = {
      role: 'row' as const,
      'aria-rowindex': window.startIndex + index + 1,
      'aria-selected': selectedForBulk.size > 0 ? state === 'all' : base['aria-selected'],
      'aria-current': open ? ('true' as const) : undefined,
    };
    return (
      <BundleGridCard
        key={card.key}
        card={card}
        comfort={comfort}
        rowProps={rowProps}
        open={open}
        image={imageFor(card.title)}
        checked={state === 'all'}
        readOnly={readOnly}
        metadataHeight={comfort ? 124 + extra(true) : 88 + extra(false)}
        onSelect={() => onSelect(card.title.id)}
        onOpenDetail={() => onOpenDetail(card.title.id)}
        onContextMenu={(x, y) => onContextMenu(card.title.id, x, y, card.bundle.id)}
        onToggleChecked={() => cardToggleIds(card, selectedForBulk).forEach(onToggleBulkSelect)}
      />
    );
  }

  function renderCompactCard(m: ModelFile, index: number, partOf: string | null) {
    return (
      <div
        key={m.id}
        data-model-id={m.id}
        {...rowAria(m, index)}
        aria-label={cardAriaLabel(m, t, partOf)}
        tabIndex={-1}
        onClick={() => onSelect(m.id)}
        onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(m.id);
          onContextMenu(m.id, e.clientX, e.clientY);
        }}
        onMouseDown={(e) => handleCardMouseDown(e, m.id)}
        onMouseEnter={() => reorderable && dragIndex !== null && setOverIndex(models.findIndex((x) => x.id === m.id))}
        className={`hover:border-[var(--accent)] group rounded-[10px] overflow-hidden border cursor-pointer ${
          m.id === selectedId ? 'border-[var(--accent)]' : 'border-[var(--line)]'
        } ${fileDrag.draggingId === m.id ? 'opacity-50' : ''}`}
      >
        <div role="gridcell" className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          {onDragFileStart && !readOnly && canEdit && !reorderable && <DragGrip className="absolute top-1.5 right-9 z-10" />}
          {!readOnly && canEdit && (
            <BulkCheckbox
              checked={selectedForBulk.has(m.id)}
              onToggle={() => onToggleBulkSelect(m.id)}
              className="absolute top-1.5 right-1.5 z-10"
            />
          )}
          {imageFor(m) ? (
            <img draggable={false}
              src={imageFor(m) ?? undefined}
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
                  <div className="text-compact-label ui-label text-[var(--ink-3)]">
                    {t(fileKindOf(m) === 'gcode' ? 'previewLabelNone' : 'previewLabel3d')}
                  </div>
                </div>
              </div>
            </>
          )}
          <KindBadge model={m} comfort={false} />
          {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
            <div className={`absolute ${newBadgePosition(m, false)} font-semibold text-caption px-1 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]`}>
              {t('newBadge')}
            </div>
          )}
          {licenseUsage(m.license) !== 'unknown' && <div className="absolute left-2 right-2 bottom-10 pointer-events-none [&_span]:pointer-events-auto">
            <LicenseBadge license={m.license} />
          </div>}
          {m.printStatus === 'printed' && (
            <div className="absolute right-[7px] bottom-[7px] font-semibold text-caption px-1 py-0.5 rounded border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">
              <Icon name="check" size={14} /> {t('printedBadge')}
            </div>
          )}
          {!readOnly && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(m.id);
              }}
              aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}
              className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] absolute left-[7px] bottom-[7px] font-semibold text-caption px-1 py-0.5 rounded border cursor-pointer ${
                m.favorite
                  ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                  : 'border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]'
              }`}
            >
              <Icon name="heart" size={14} fill={m.favorite  ? 'currentColor' : 'none'} />
            </button>
          )}
        </div>
        <div data-card-metadata role="gridcell" className="flex flex-col gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]" style={{ height: 88 + extra(false) }}>
          <div className="select-none text-small font-semibold overflow-hidden text-ellipsis whitespace-nowrap shrink-0" style={{ height: 19, lineHeight: '19px' }}>
            {displayName(m.name)}
          </div>
          <CardMeta model={m} comfort={false} />
          {partOf ? <PartOfChip text={partOf} comfort={false} /> : <CardTags tags={m.tags} comfort={false} />}
          {printFilter && <PrintFactsExtra model={m} filter={printFilter} comfort={false} />}
        </div>
      </div>
    );
  }

  return (
    <div ref={window.rootRef} role="grid" data-catalog-grid aria-label={label ?? t('modelListLabel')} aria-multiselectable="true"
      aria-rowcount={items.length} aria-colcount={2} style={{ visibility: window.measured ? 'visible' : 'hidden' }}>
      {window.emptyHeight !== null ? <div data-window-spacer="empty" style={{ height: window.emptyHeight }} /> : <>
      <div data-window-spacer="top" aria-hidden="true" style={{ height: window.topSpacer }} />
    <div
      role="presentation"
      className="grid gap-3.5"
      style={window.columns > 2
        ? { gridTemplateColumns: `repeat(${window.columns}, minmax(0, ${fixedTileWidth}px))`, justifyContent: 'start' }
        : { gridTemplateColumns: `repeat(${window.columns}, minmax(0, 1fr))` }}
    >
      {visible.map((item, index) => {
        if (item.type === 'bundle') return renderBundleCard(item, index);
        const m = item.model;
        const partOf = item.partOf ? partOfText(item.partOf.name) : null;
        return density === 'comfort' ? (
          <div
            key={m.id}
            data-model-id={m.id}
            {...rowAria(m, index)}
            aria-label={cardAriaLabel(m, t, partOf)}
            tabIndex={-1}
            onClick={() => onSelect(m.id)}
            onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              onSelect(m.id);
              onContextMenu(m.id, e.clientX, e.clientY);
            }}
            onMouseDown={(e) => handleCardMouseDown(e, m.id)}
            onMouseEnter={() => reorderable && dragIndex !== null && setOverIndex(models.findIndex((x) => x.id === m.id))}
            className={`hover:border-[var(--accent)] group rounded-[10px] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 ${
              m.id === selectedId ? 'border-[var(--accent)]' : 'border-transparent'
            } ${fileDrag.draggingId === m.id ? 'opacity-50' : ''}`}
          >
            <div aria-hidden="true" className="h-[5px]" style={{ background: 'linear-gradient(90deg, var(--accent), var(--accent-soft))' }} />
            <div role="gridcell" className="relative aspect-square bg-[var(--plate)] overflow-hidden">
              {onDragFileStart && !readOnly && canEdit && !reorderable && <DragGrip className="absolute top-1.5 right-9 z-10" />}
              {!readOnly && canEdit && (
                <BulkCheckbox
                  checked={selectedForBulk.has(m.id)}
                  onToggle={() => onToggleBulkSelect(m.id)}
                  className="absolute top-1.5 right-1.5 z-10"
                />
              )}
              {imageFor(m) ? (
                <img draggable={false} src={imageFor(m) ?? undefined} alt="" className="absolute inset-0 w-full h-full object-cover" />
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
              <KindBadge model={m} comfort />
              {Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000 && (
                <div className={`absolute ${newBadgePosition(m, true)} font-semibold text-caption px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]`}>
                  {t('newBadge')}
                </div>
              )}
              {licenseUsage(m.license) !== 'unknown' && <div className="absolute left-2 right-2 bottom-10 pointer-events-none [&_span]:pointer-events-auto">
                <LicenseBadge license={m.license} />
              </div>}
              {m.printStatus === 'printed' && (
                <div className="absolute left-2.5 bottom-2.5 font-semibold text-caption px-2.5 py-1 rounded-lg bg-[var(--panel)] text-[var(--accent)] shadow-[var(--shadow)]">
                  <Icon name="check" size={14} /> {t('printedBadge')}
                </div>
              )}
              {!readOnly && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFavorite(m.id);
                  }}
                  aria-label={m.favorite ? t('favoriteRemove') : t('favoriteAdd')}
                  {...(!canEdit ? { disabled: true, title: t('webReadOnly') } : {})}
                  style={{ width: 'var(--icon-badge-size)', height: 'var(--icon-badge-size)' }}
                  className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] absolute right-2.5 bottom-2.5 rounded-full grid place-items-center bg-[var(--panel)]/90 shadow-[var(--shadow)] cursor-pointer text-title ${
                    m.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'
                  }`}
                >
                  <Icon name="heart" size={14} fill={m.favorite  ? 'currentColor' : 'none'} />
                </button>
              )}
            </div>
            <div data-card-metadata role="gridcell" className="flex flex-col gap-2" style={{ padding: 'var(--space-card-pad)', height: 124 + extra(true) }}>
              <div
                className="select-none font-bold overflow-hidden text-ellipsis whitespace-nowrap shrink-0"
                style={{ fontSize: 'var(--font-size-title)', height: 26, lineHeight: '26px' }}
              >
                {displayName(m.name)}
              </div>
              <CardMeta model={m} comfort />
              {partOf ? <PartOfChip text={partOf} comfort /> : <CardTags tags={m.tags} comfort />}
              {printFilter && <PrintFactsExtra model={m} filter={printFilter} comfort />}
            </div>
          </div>
        ) : (
          renderCompactCard(m, index, partOf)
        );
      })}
    </div>
      <div data-window-spacer="bottom" aria-hidden="true" style={{ height: window.bottomSpacer }} />
      </>}
    </div>
  );
}
