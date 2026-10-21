import { Icon } from './Icon';
import { useState } from 'react';
import { useT, useLanguage, useFormatCount } from '../i18n/LanguageContext';
import { formatSpoolAmount, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool, SpoolKind } from '../types';
import { STOCK_BADGE_CLASS, STOCK_BAR_CLASS, filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { singleSpoolGroups, type SpoolGroup } from '../lib/filamentGroups';
import { suggestSpool } from '../lib/spoolSuggestion';
import { isFromInteractiveElement, startsOnButton } from '../lib/spoolCardEvents';
import { ResinBottleIcon } from './ResinBottleIcon';
import { FilamentSpoolIcon } from './FilamentSpoolIcon';
import { SpoolColorDot } from './SpoolColorDot';
import { useGroupKindTexts, useGroupPlaceText, useGroupStatusLabel } from './spoolGroupParts';
import type { SpoolListProps } from './FilamentDashboard';

interface Props extends SpoolListProps {
  /** Kind of the list shown; for 'resin' the diameter column is omitted. */
  kind?: SpoolKind;
  /** Grouped view: adds the count column. */
  showCount?: boolean;
}

type SortKey = 'material' | 'manufacturer' | 'color' | 'location' | 'diameterMm' | 'remainingWeightG' | 'price';

type RowActions = Pick<
  SpoolListProps,
  | 'confirmDeleteId' | 'onEdit' | 'onRequestDelete' | 'onCancelDelete' | 'onConfirmDelete' | 'onSpoolMouseDown'
  | 'onRestock' | 'restockOpenId' | 'onConsume' | 'consumeOpenId' | 'highlightIds'
>;

export function FilamentTable({ spools, groups, kind = 'filament', showCount = false, ...actions }: Props) {
  const t = useT();
  const [sortKey, setSortKey] = useState<SortKey>('material');
  const [sortDir, setSortDir] = useState<1 | -1>(1);
  // Groups are sorted by the text of their first spool; amounts are the totals of the group.
  const shown = groups ?? singleSpoolGroups(spools);

  const toggleSort = (key: SortKey) => {
    if (key === sortKey) setSortDir((d) => (d === 1 ? -1 : 1));
    else {
      setSortKey(key);
      setSortDir(1);
    }
  };

  const sorted = [...shown].sort((ga, gb) => {
    const a = ga.first;
    const b = gb.first;
    let av: string | number;
    let bv: string | number;
    switch (sortKey) {
      case 'diameterMm':
        av = a.diameterMm;
        bv = b.diameterMm;
        break;
      case 'remainingWeightG':
        av = ga.remainingG;
        bv = gb.remainingG;
        break;
      case 'price':
        av = a.price ?? -1;
        bv = b.price ?? -1;
        break;
      case 'manufacturer':
        av = a.manufacturer ?? '';
        bv = b.manufacturer ?? '';
        break;
      case 'color':
        av = a.color ?? '';
        bv = b.color ?? '';
        break;
      case 'location':
        av = a.location ?? '';
        bv = b.location ?? '';
        break;
      default:
        av = a.material;
        bv = b.material;
    }
    if (typeof av === 'string') return av.localeCompare(bv as string) * sortDir;
    return (av - (bv as number)) * sortDir;
  });

  const columns: { key: SortKey; label: string }[] = [
    { key: 'material', label: t('filamentMaterialLabel') },
    { key: 'manufacturer', label: t('filamentManufacturerLabel') },
    { key: 'color', label: t('filamentColorLabel') },
    { key: 'location', label: t('filamentLocationLabel') },
    { key: 'diameterMm', label: '⌀' },
    { key: 'remainingWeightG', label: t('filamentColumnStock') },
    { key: 'price', label: t('filamentPriceLabel') },
  ];
  const visibleColumns = kind === 'resin' ? columns.filter((c) => c.key !== 'diameterMm') : columns;

  if (shown.length === 0) {
    return <div className="text-body text-[var(--ink-3)]">{t('filamentNoResults')}</div>;
  }

  return (
    <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] overflow-hidden overflow-x-auto">
      <table className="w-full border-collapse text-small">
        <thead>
          <tr>
            {visibleColumns.map((col) => (
              <th
                key={col.key}
                aria-sort={sortKey === col.key ? (sortDir === 1 ? 'ascending' : 'descending') : undefined}
                onClick={() => toggleSort(col.key)}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-left ui-label text-[var(--ink-3)] px-3 py-2.5 border-b border-[var(--line)] bg-[var(--panel-2)] cursor-pointer whitespace-nowrap select-none"
              >
                {col.label}{' '}
                <span className={sortKey === col.key ? 'text-[var(--accent)] opacity-100' : 'opacity-30'}>
                  <Icon name="chevron" size={14} className={sortKey === col.key && sortDir === -1  ? 'rotate-180' : ''} />
                </span>
              </th>
            ))}
            {showCount && (
              <th className="text-left ui-label text-[var(--ink-3)] px-3 py-2.5 border-b border-[var(--line)] bg-[var(--panel-2)]">
                {t('filamentColumnCount')}
              </th>
            )}
            <th className="text-left ui-label text-[var(--ink-3)] px-3 py-2.5 border-b border-[var(--line)] bg-[var(--panel-2)]">
              {t('filamentColumnStatus')}
            </th>
            <th className="border-b border-[var(--line)] bg-[var(--panel-2)]" />
          </tr>
        </thead>
        <tbody>
          {sorted.map((group) =>
            group.spools.length === 1 ? (
              <SpoolRow key={group.key} spool={group.first} group={groups ? group : undefined} kind={kind} showCount={showCount} {...actions} />
            ) : (
              <GroupRows key={group.key} group={group} kind={kind} {...actions} />
            ),
          )}
        </tbody>
      </table>
    </div>
  );
}

const cell = 'px-3 py-2.5 border-b border-[var(--line)]';
const iconButton = 'w-6 h-6 grid place-items-center rounded-full text-caption text-[var(--ink-3)] cursor-pointer';

function Amount({ remaining, kind, pct, status }: { remaining: number; kind: SpoolKind; pct: number; status: ReturnType<typeof filamentStockStatus> }) {
  const { language } = useLanguage();
  return (
    <>
      <div className="flex justify-between font-medium tabular-nums text-caption text-[var(--ink-3)] mb-1">
        <span>{formatSpoolAmount(remaining, kind, language)}</span>
        <span>{pct}%</span>
      </div>
      <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
        <div className={`h-full rounded-full ${STOCK_BAR_CLASS[status]}`} style={{ width: `${pct}%` }} />
      </div>
    </>
  );
}

function DeleteConfirm({ id, onCancelDelete, onConfirmDelete }: { id: string } & Pick<RowActions, 'onCancelDelete' | 'onConfirmDelete'>) {
  const t = useT();
  return (
    <span className="inline-flex items-center gap-1.5">
      <button
        onClick={onCancelDelete}
        className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption cursor-pointer"
      >
        {t('cancel')}
      </button>
      <button
        onClick={() => onConfirmDelete(id)}
        className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-6 px-1.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption cursor-pointer"
      >
        {t('delete')}
      </button>
    </span>
  );
}

/** One spool as a row; inside an expanded group it is indented and has no restock button. */
function SpoolRow({
  spool, group, kind, showCount, nested = false, ...a
}: { spool: FilamentSpool; group?: SpoolGroup; kind: SpoolKind; showCount: boolean; nested?: boolean } & RowActions) {
  const t = useT();
  const { language } = useLanguage();
  const statusLabel = useGroupStatusLabel();
  const status = filamentStockStatus(spool);
  const pct = filamentStockPercent(spool);
  const loaded = spool.unitId !== null;
  const draggable = !!a.onSpoolMouseDown && !loaded;
  const placeText =
    group && group.place.kind === 'unit'
      ? spool.slotIndex === null ? group.place.label : `${group.place.label} · ${t('filamentGroupSlot').replace('{n}', String(spool.slotIndex + 1))}`
      : spool.location || t('noValue');
  const amountText = formatSpoolAmount(spool.remainingWeightG, spool.kind, language);
  const rowLabel = nested ? `: ${placeText}, ${amountText}` : '';
  return (
    <tr
      data-testid={`spool-row-${spool.id}`}
      onMouseDown={(e) => draggable && !startsOnButton(e) && a.onSpoolMouseDown?.(spool.id, e)}
      onDoubleClick={(e) => {
        if (a.confirmDeleteId !== spool.id && !isFromInteractiveElement(e)) a.onEdit(spool);
      }}
      className={`hover:bg-[var(--panel-2)] ${draggable ? 'cursor-grab' : ''} ${
        a.highlightIds?.has(spool.id) ? 'spool-new-row' : ''
      } ${nested ? 'bg-[var(--panel-2)]/40' : ''}`}
    >
      <td className={`${cell} font-semibold ${nested ? 'pl-9' : ''}`}>
        <div className="flex items-center gap-2">
          {spool.imagePng ? (
            <img draggable={false} src={`data:image/png;base64,${spool.imagePng}`} className="w-6 h-6 rounded object-cover border border-[var(--line)]" />
          ) : spool.kind === 'resin' ? (
            <ResinBottleIcon colorHex={spool.colorHex} size={20} />
          ) : (
            <FilamentSpoolIcon colorHex={spool.colorHex} size={20} />
          )}
          {spool.material}
        </div>
      </td>
      <td className={`${cell} text-[var(--ink-2)]`}>{spool.manufacturer || t('noValue')}</td>
      <td className={`${cell} text-[var(--ink-2)]`}>
        <span className="inline-flex items-center gap-1.5">
          <SpoolColorDot spool={spool} />
          {spool.color || t('noValue')}
        </span>
      </td>
      <td className={cell}>
        <span className="tabular-nums text-caption font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap">
          {placeText}
        </span>
      </td>
      {kind !== 'resin' && (
        <td className={`${cell} font-medium tabular-nums text-[var(--ink-2)]`}>{formatDiameterMm(spool.diameterMm, language)}</td>
      )}
      <td className={`${cell} min-w-[130px]`}>
        <Amount remaining={spool.remainingWeightG} kind={spool.kind} pct={pct} status={status} />
      </td>
      <td className={`${cell} font-medium tabular-nums text-right text-[var(--ink-2)]`}>
        {spool.price !== null ? formatPrice(spool.price, language) : t('noValue')}
      </td>
      {showCount && <td className={`${cell} tabular-nums text-[var(--ink-2)]`}>{nested ? '' : 1}</td>}
      <td className={cell}>
        <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-compact-meta font-semibold uppercase ${STOCK_BADGE_CLASS[status]}`}>
          {statusLabel(status, loaded)}
        </span>
      </td>
      <td className={`${cell} text-right whitespace-nowrap`}>
        {a.confirmDeleteId === spool.id ? (
          <DeleteConfirm id={spool.id} onCancelDelete={a.onCancelDelete} onConfirmDelete={a.onConfirmDelete} />
        ) : (
          <span className="inline-flex items-center gap-1">
            {a.onConsume && spool.kind === 'resin' && (
              <button
                type="button"
                onClick={(e) => a.onConsume?.(spool, e.currentTarget)}
                aria-label={`${t('resinConsumeButton')}${rowLabel}`}
                title={t('resinConsumeButton')}
                aria-haspopup="dialog"
                aria-expanded={a.consumeOpenId === spool.id}
                className={`${iconButton} hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]`}
              >
                <Icon name="minus" size={14} />
              </button>
            )}
            {a.onRestock && !nested && (
              <button
                type="button"
                onClick={(e) => a.onRestock?.(spool, e.currentTarget)}
                aria-label={t('filamentRestockButton')}
                title={t('filamentRestockButton')}
                aria-haspopup="dialog"
                aria-expanded={a.restockOpenId === spool.id}
                className={`${iconButton} hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]`}
              >
                <Icon name="plus" size={14} />
              </button>
            )}
            <button
              onClick={() => a.onEdit(spool)}
              aria-label={`${t('filamentEditAria')}${rowLabel}`}
              className={`${iconButton} hover:bg-[var(--panel)]`}
            >
              <Icon name="edit" size={14} />
            </button>
            <button
              onClick={() => a.onRequestDelete(spool.id)}
              aria-label={`${t('deleteAriaLabel')}${rowLabel}`}
              className={`${iconButton} hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]`}
            >
              <Icon name="close" size={14} />
            </button>
          </span>
        )}
      </td>
    </tr>
  );
}

/** A group as one row with a toggle; its spools follow as rows while expanded. */
function GroupRows({ group, kind, ...a }: { group: SpoolGroup; kind: SpoolKind } & RowActions) {
  const t = useT();
  const { language } = useLanguage();
  const formatCount = useFormatCount();
  const [expanded, setExpanded] = useState(false);
  const { first } = group;
  const loaded = group.place.kind === 'unit';
  const place = useGroupPlaceText(group);
  const statusLabel = useGroupStatusLabel();
  const texts = useGroupKindTexts(first.kind);
  const pct = filamentStockPercent({ originalWeightG: group.originalG, remainingWeightG: group.remainingG });
  const dragSpool = !loaded ? suggestSpool(group.spools) ?? first : null;
  const groupName = [first.material, first.color].filter(Boolean).join(' ');
  return (
    <>
      <tr
        data-testid={`group-row-${first.id}`}
        onMouseDown={(e) => dragSpool && a.onSpoolMouseDown && !startsOnButton(e) && a.onSpoolMouseDown(dragSpool.id, e)}
        className={`hover:bg-[var(--panel-2)] ${dragSpool && a.onSpoolMouseDown ? 'cursor-grab' : ''}`}
      >
        <td className={`${cell} font-semibold`}>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setExpanded((open) => !open)}
              aria-expanded={expanded}
              aria-label={t('filamentGroupToggleAria').replace('{group}', groupName).replace('{action}', expanded ? texts.hide : texts.show)}
              title={expanded ? texts.hide : texts.show}
              className={`${iconButton} hover:bg-[var(--panel)]`}
            >
              <Icon name="chevron" size={14} className={expanded ? '' : '-rotate-90'} />
            </button>
            {first.imagePng ? (
              <img draggable={false} src={`data:image/png;base64,${first.imagePng}`} className="w-6 h-6 rounded object-cover border border-[var(--line)]" />
            ) : first.kind === 'resin' ? (
              <ResinBottleIcon colorHex={first.colorHex} size={20} />
            ) : (
              <FilamentSpoolIcon colorHex={first.colorHex} size={20} />
            )}
            {first.material}
          </div>
        </td>
        <td className={`${cell} text-[var(--ink-2)]`}>{first.manufacturer || t('noValue')}</td>
        <td className={`${cell} text-[var(--ink-2)]`}>
          <span className="inline-flex items-center gap-1.5">
            <SpoolColorDot spool={first} />
            {first.color || t('noValue')}
          </span>
        </td>
        <td className={cell}>
          <span className="tabular-nums text-caption font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap">
            {place.label}
          </span>
          {place.note && <div className="mt-1 text-caption text-[var(--ink-3)]">{place.note}</div>}
        </td>
        {kind !== 'resin' && (
          <td className={`${cell} font-medium tabular-nums text-[var(--ink-2)]`}>{formatDiameterMm(first.diameterMm, language)}</td>
        )}
        <td className={`${cell} min-w-[130px]`}>
          <Amount remaining={group.remainingG} kind={first.kind} pct={pct} status={group.status} />
          {group.emptyCount > 0 && <div className="mt-1 text-caption text-[var(--crit)]">{formatCount(texts.emptyCount, group.emptyCount)}</div>}
        </td>
        <td className={`${cell} text-right text-[var(--ink-3)]`}>{t('noValue')}</td>
        <td className={`${cell} tabular-nums text-[var(--ink-2)]`}>{group.spools.length}</td>
        <td className={cell}>
          <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-compact-meta font-semibold uppercase ${STOCK_BADGE_CLASS[group.status]}`}>
            {statusLabel(group.status, loaded)}
          </span>
        </td>
        <td className={`${cell} text-right whitespace-nowrap`}>
          <span className="inline-flex items-center gap-1">
            {a.onConsume && first.kind === 'resin' && (
              <button
                type="button"
                onClick={(e) => a.onConsume?.(suggestSpool(group.spools) ?? first, e.currentTarget, group.spools)}
                aria-label={`${t('resinConsumeButton')}: ${groupName}`}
                title={t('resinConsumeButton')}
                aria-haspopup="dialog"
                aria-expanded={a.consumeOpenId != null && group.spools.some((s) => s.id === a.consumeOpenId)}
                className={`${iconButton} hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]`}
              >
                <Icon name="minus" size={14} />
              </button>
            )}
            {a.onRestock && (
              <button
                type="button"
                onClick={(e) => a.onRestock?.(first, e.currentTarget)}
                aria-label={`${t('filamentRestockButton')}: ${groupName}`}
                title={t('filamentRestockButton')}
                aria-haspopup="dialog"
                aria-expanded={a.restockOpenId === first.id}
                className={`${iconButton} hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]`}
              >
                <Icon name="plus" size={14} />
              </button>
            )}
          </span>
        </td>
      </tr>
      {expanded &&
        group.spools.map((spool) => (
          <SpoolRow
            key={spool.id}
            spool={spool}
            group={group}
            kind={kind}
            showCount
            nested
            {...a}
          />
        ))}
    </>
  );
}
