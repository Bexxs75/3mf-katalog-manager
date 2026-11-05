import { useCanEditMetadata } from '../lib/webSession';
import { useRowSelectionAria } from '../hooks/useRowSelectionAria';
import { displayName } from '../lib/displayName';
import { LicenseBadge } from './LicenseBadge';
import { modelLicenseAriaLabel } from '../lib/licenses';
import { Fragment, useMemo, useState, type RefObject } from 'react';
import { useModelWindow } from '../hooks/useModelWindow';
import { DragGrip } from './DragGrip';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { ModelFile } from '../types';
import type { BundleCard, CardMember } from '../lib/bundles';
import { bundleAggregate } from '../lib/bundles';
import { cardNavId, cardSelectionState, cardToggleIds, isCardOpen } from '../lib/bundleCards';
import { useBundleCardsSettings, useCardItems } from '../hooks/BundleCardsContext';
import { useBundleText, usePartOfText } from './BundleCardParts';
import { Icon } from './Icon';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatVolumeCm3 } from '../i18n/format';
import { BulkCheckbox } from './BulkCheckbox';
import { shortTagLabel, tagLabel } from '../lib/autoTags';
import { useActivePrintFilter } from '../hooks/PrintFilterContext';
import { isSlicedOutput, kindBadge } from '../lib/fileKind';
import { missingFilterNames } from './PrintFactsExtra';
import { FILTER_LABEL_KEY } from '../lib/printFilterLabels';

interface Props {
  models: ModelFile[];
  containerRef?: RefObject<HTMLDivElement | null>;
  windowed?: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  /** `bundleId` is set when the menu is for a bundle row (the id is then its title model). */
  onContextMenu: (id: string, x: number, y: number, bundleId?: string) => void;
  readOnly?: boolean;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  onDragFileStart?: (id: string) => void;
  // Accessible name of the grid; grouped views pass the folder name.
  label?: string;
}

export function ModelList({ containerRef, windowed = true, models, selectedId, onSelect, onOpenDetail, onContextMenu, readOnly, selectedForBulk, onToggleBulkSelect, onDragFileStart, label }: Props) {
  const canEdit = useCanEditMetadata();
  const { language } = useLanguage();
  const t = useT();
  // While a print data filter is active, a row says which value of the filter the file lacks.
  const printFilter = useActivePrintFilter();

  // A bundle shown as one card is one row; its members unfold below it. The keyboard order
  // names only the rows, so the arrow keys step from card to card.
  const items = useCardItems(models);
  const trash = useBundleCardsSettings()?.trash ?? false;
  const partOfText = usePartOfText(trash);
  const bundleText = useBundleText();
  const [unfolded, setUnfolded] = useState<ReadonlySet<string>>(() => new Set());
  const toggleUnfolded = (bundleId: string) => setUnfolded(current => {
    const next = new Set(current);
    if (next.has(bundleId)) next.delete(bundleId); else next.add(bundleId);
    return next;
  });
  const ids = useMemo(() => items.map(cardNavId), [items]);
  // Rows of different heights would break the window, so it rests while a bundle is unfolded.
  const unfoldedCards = items.filter((item): item is BundleCard => item.type === 'bundle' && unfolded.has(item.bundle.id));
  const window = useModelWindow({ ids, containerRef, gap: 0, fallbackHeight: 43, disabled: !windowed || unfoldedCards.length > 0 });
  const visible = items.slice(window.startIndex, window.endIndex);
  const memberRowCount = unfoldedCards.reduce((sum, card) => sum + card.visibleMembers.length, 0);
  const fileDrag = useDragThreshold(onDragFileStart);
  const selectionAria = useRowSelectionAria(selectedId, selectedForBulk);

  const rowLabel = (m: ModelFile, partOf: string | null) => {
    const label = modelLicenseAriaLabel(displayName(m.name), m.license, t);
    const named = isSlicedOutput(m) ? `${label}, ${kindBadge(m)}` : label;
    return partOf ? `${named}, ${partOf}` : named;
  };

  function renderSingleRow(m: ModelFile, rowIndex: number, partOf: string | null) {
    return (
        <div
          key={m.id}
          data-model-id={m.id}
          role="row"
          aria-rowindex={rowIndex}
          {...selectionAria(m.id)}
          aria-label={rowLabel(m, partOf)}
          tabIndex={-1}
          onClick={() => onSelect(m.id)}
          onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onSelect(m.id);
            onContextMenu(m.id, e.clientX, e.clientY);
          }}
          onMouseDown={(e) => {
            if (!canEdit || !onDragFileStart || readOnly) return;
            fileDrag.begin(e, m.id);
          }}
          className={`group min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
            m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
          } ${fileDrag.draggingId === m.id ? 'opacity-50' : ''}`}
          style={{ gridTemplateColumns: '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px' }}
        >
          {readOnly || !canEdit ? (
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
            {onDragFileStart && !readOnly && canEdit && <DragGrip />}
            <span className="select-none text-[length:var(--font-size-body)] font-medium truncate">{displayName(m.name)}</span>
            {isSlicedOutput(m) && <span data-kind-badge aria-hidden="true" className="flex-none font-bold tracking-[0.05em] text-caption px-1.5 py-px rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">{kindBadge(m)}</span>}
            <LicenseBadge license={m.license} />
            {partOf && <span data-part-of title={partOf} className="min-w-0 max-w-[40%] truncate px-1.5 rounded-full border border-dashed border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-semibold text-caption">{partOf}</span>}
            {printFilter && (() => {
              const missing = missingFilterNames(m, printFilter, key => t(FILTER_LABEL_KEY[key]) as string);
              if (missing.length === 0) return null;
              const text = t('pfMissingFor').replace('{what}', missing.join(', '));
              return <span data-print-facts-missing title={text} className="min-w-0 max-w-[55%] truncate px-1.5 rounded border border-[var(--warn)] bg-[var(--warn-soft)] text-[var(--warn)] font-semibold text-caption leading-none">{text}</span>;
            })()}
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
    );
  }

  const columns = '24px minmax(150px,2.2fr) minmax(110px,1.6fr) 92px 82px';
  const tagChips = (tags: string[]) => tags.map((tag) => (
    <span
      key={tag}
      title={`#${tagLabel(tag, language)}`}
      className="inline-flex shrink-0 font-medium tabular-nums text-[length:var(--font-size-meta)] px-1.5 py-0.5 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap"
    >
      <span className="whitespace-nowrap">{shortTagLabel(`#${tagLabel(tag, language)}`)}</span>
    </span>
  ));

  function renderBundleRow(card: BundleCard, rowIndex: number) {
    const aggregate = bundleAggregate(card.visibleMembers.map(member => member.model));
    const state = cardSelectionState(card, selectedForBulk);
    const open = isCardOpen(card, selectedId);
    const isUnfolded = unfolded.has(card.bundle.id);
    const name = displayName(card.bundle.name);
    const summary = bundleText.summary(card);
    const base = selectionAria(open && selectedId !== null ? selectedId : card.title.id);
    return (
      <div
        key={card.key}
        data-model-id={card.title.id}
        data-bundle-id={card.bundle.id}
        role="row"
        aria-rowindex={rowIndex}
        aria-selected={selectedForBulk.size > 0 ? state === 'all' : base['aria-selected']}
        aria-current={open ? 'true' : undefined}
        aria-label={bundleText.ariaLabel(card)}
        tabIndex={-1}
        onClick={() => onSelect(card.title.id)}
        onDoubleClick={readOnly ? undefined : () => onOpenDetail(card.title.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(card.title.id);
          onContextMenu(card.title.id, e.clientX, e.clientY, card.bundle.id);
        }}
        className={`group min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer ${
          open ? 'bg-[var(--accent-soft)]' : 'hover:bg-[var(--panel-2)]'
        }`}
        style={{ gridTemplateColumns: columns }}
      >
        {readOnly || !canEdit ? (
          <span role="gridcell" />
        ) : (
          <BulkCheckbox
            checked={state === 'all'}
            onToggle={() => cardToggleIds(card, selectedForBulk).forEach(onToggleBulkSelect)}
            role="gridcell"
            className="justify-self-center"
          />
        )}
        <span role="gridcell" className="flex items-center gap-1.5 min-w-0">
          <button type="button" data-bundle-toggle aria-expanded={isUnfolded}
            aria-label={t(isUnfolded ? 'bundleCollapse' : 'bundleExpand').replace('{name}', name)}
            onClick={(e) => { e.stopPropagation(); toggleUnfolded(card.bundle.id); }}
            onDoubleClick={(e) => e.stopPropagation()}
            className="flex-none w-[22px] h-[22px] grid place-items-center rounded text-[var(--ink-3)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <span className={`transition-transform ${isUnfolded ? '' : '-rotate-90'}`}><Icon name="chevron" size={14} /></span>
          </button>
          <span className="select-none text-[length:var(--font-size-body)] font-medium truncate">{name}</span>
          <span data-bundle-line title={summary}
            className="flex-none inline-flex items-center gap-1 px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-semibold text-caption leading-none py-0.5">
            <Icon name="layers" size={12} />{summary}
          </span>
        </span>
        <span role="gridcell" className="flex min-w-0 gap-1 overflow-hidden">{tagChips(aggregate.tags)}</span>
        <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
          {formatVolumeCm3(aggregate.volumeCm3, language)}
        </span>
        <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
          {formatBytes(aggregate.sizeBytes, language)}
        </span>
      </div>
    );
  }

  /** A member below its unfolded bundle row. Reachable with Tab, because the arrow keys only step between cards. */
  function renderMemberRow(card: BundleCard, member: CardMember, rowIndex: number) {
    const m = member.model;
    const base = selectionAria(m.id);
    const name = displayName(m.name);
    return (
      <div
        key={`${card.key}:${m.id}`}
        data-bundle-member={card.bundle.id}
        data-member-id={m.id}
        role="row"
        aria-rowindex={rowIndex}
        {...base}
        aria-label={t('bundleMemberRow').replace('{name}', member.label ? `${name}, ${member.label}` : name).replace('{bundle}', displayName(card.bundle.name))}
        tabIndex={0}
        onClick={() => onSelect(m.id)}
        onDoubleClick={readOnly ? undefined : () => onOpenDetail(m.id)}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget || (e.key !== ' ' && e.key !== 'Enter')) return;
          e.preventDefault();
          e.stopPropagation();
          onSelect(m.id);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          onSelect(m.id);
          onContextMenu(m.id, e.clientX, e.clientY);
        }}
        className={`group min-w-[680px] grid gap-2.5 items-center px-3 py-2 border-b border-[var(--line)] cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--accent)] ${
          m.id === selectedId ? 'bg-[var(--accent-soft)]' : 'bg-[var(--panel-2)] hover:bg-[var(--plate)]'
        }`}
        style={{ gridTemplateColumns: columns }}
      >
        {readOnly || !canEdit ? (
          <span role="gridcell" />
        ) : (
          <BulkCheckbox checked={selectedForBulk.has(m.id)} onToggle={() => onToggleBulkSelect(m.id)} role="gridcell" className="justify-self-center" />
        )}
        <span role="gridcell" className="flex items-baseline gap-2 min-w-0 pl-[30px]">
          <span className="select-none text-[length:var(--font-size-body)] font-medium truncate">{name}</span>
          {member.label && <span className="flex-none text-caption text-[var(--ink-3)] whitespace-nowrap">{member.label}</span>}
        </span>
        <span role="gridcell" className="flex min-w-0 gap-1 overflow-hidden">{tagChips(m.tags)}</span>
        <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">{formatVolumeCm3(m.volumeCm3, language)}</span>
        <span role="gridcell" className="font-medium tabular-nums text-caption text-[var(--ink-2)]">{formatBytes(m.fileSizeBytes, language)}</span>
      </div>
    );
  }

  // While a bundle is unfolded the window rests, so a row's index only has to count what stands above it.
  let nextRowIndex = window.startIndex + 2;
  const rows = visible.map((item) => {
    if (item.type === 'model') return renderSingleRow(item.model, nextRowIndex++, item.partOf ? partOfText(item.partOf.name) : null);
    const bundleRow = renderBundleRow(item, nextRowIndex++);
    // Always inside the fragment: unfolding must not remount the row, or the button would lose the focus.
    const memberRows = unfolded.has(item.bundle.id) ? item.visibleMembers.map(member => renderMemberRow(item, member, nextRowIndex++)) : [];
    return <Fragment key={item.key}>{bundleRow}{memberRows}</Fragment>;
  });

  return (
    // aria-rowcount includes the header row; the window renders only a slice of the data rows,
    // so each row carries its absolute index.
    <div role="grid" data-catalog-grid aria-label={label ?? t('modelListLabel')} aria-multiselectable="true"
      aria-rowcount={items.length + memberRowCount + 1} aria-colcount={5} className="border border-[var(--line)] rounded overflow-x-auto bg-[var(--panel)]">
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
      {rows}
      <div data-window-spacer="bottom" aria-hidden="true" style={{ height: window.bottomSpacer }} />
      </div>
    </div>
  );
}
