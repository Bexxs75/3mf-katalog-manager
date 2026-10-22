import { useCanEditMetadata } from '../lib/webSession';
import { SpoolImage } from './SpoolImage';
import { Icon } from './Icon';
import { useId, useState, type MouseEvent as ReactMouseEvent } from 'react';
import { useT, useLanguage, useFormatCount } from '../i18n/LanguageContext';
import { formatSpoolAmount, formatVolumeMl, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { STOCK_BADGE_CLASS, STOCK_BAR_CLASS, filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { singleSpoolGroups, type SpoolGroup } from '../lib/filamentGroups';
import { suggestSpool } from '../lib/spoolSuggestion';
import { isFromInteractiveElement, startsOnButton } from '../lib/spoolCardEvents';
import { ResinBottleIcon } from './ResinBottleIcon';
import { SpoolColorDot } from './SpoolColorDot';
import { useGroupKindTexts, useGroupPlaceText, useGroupStatusLabel } from './spoolGroupParts';

/** Shared by the dashboard cards and the inventory table. */
export interface SpoolListProps {
  spools: FilamentSpool[];
  /**
   * Groups to show (grouped view). Without it every spool is its own card, in the order of
   * `spools`.
   */
  groups?: SpoolGroup[];
  confirmDeleteId: string | null;
  onEdit: (spool: FilamentSpool) => void;
  onRequestDelete: (id: string) => void;
  onCancelDelete: () => void;
  onConfirmDelete: (id: string) => void;
  /** Mouse down on a spool - may start dragging it into a slot. */
  onSpoolMouseDown?: (spoolId: string, event: ReactMouseEvent) => void;
  /** Opens/closes the restock popover; `anchor` = the clicked button. */
  onRestock?: (spool: FilamentSpool, anchor: HTMLElement) => void;
  /** Spool whose restock popover is currently open (aria-expanded). */
  restockOpenId?: string | null;
  /**
   * Opens "− Usage" (resin only). From a group card `choices` holds the bottles of the group,
   * so the popover can preselect one.
   */
  onConsume?: (spool: FilamentSpool, anchor: HTMLElement, choices?: FilamentSpool[]) => void;
  /** Spool whose usage popover is currently open (aria-expanded). */
  consumeOpenId?: string | null;
  /** Entries just created via restock, briefly highlighted. */
  highlightIds?: ReadonlySet<string>;
}

type CardActions = Pick<
  SpoolListProps,
  | 'confirmDeleteId' | 'onEdit' | 'onRequestDelete' | 'onCancelDelete' | 'onConfirmDelete' | 'onSpoolMouseDown'
  | 'onRestock' | 'restockOpenId' | 'onConsume' | 'consumeOpenId' | 'highlightIds'
>;

export function FilamentDashboard({ spools, groups, ...actions }: SpoolListProps) {
  const t = useT();
  const shown = groups ?? singleSpoolGroups(spools);

  if (shown.length === 0) {
    return <div className="text-body text-[var(--ink-3)]">{t('filamentNoResults')}</div>;
  }

  return (
    <div className="grid gap-3 items-start" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(230px, 1fr))' }}>
      {shown.map((group) =>
        group.spools.length === 1 ? (
          <SpoolCard key={group.key} spool={group.first} group={groups ? group : undefined} {...actions} />
        ) : (
          <GroupCard key={group.key} group={group} {...actions} />
        ),
      )}
    </div>
  );
}

const iconButton = 'w-6 h-6 grid place-items-center rounded-full text-caption text-[var(--ink-3)] cursor-pointer';
const pillButton =
  'h-6 px-2 inline-flex items-center gap-1 rounded-full text-caption font-semibold text-[var(--ink-2)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] cursor-pointer';

function SpoolThumb({ spool }: { spool: FilamentSpool }) {
  return (
    <SpoolImage spool={spool} className="w-9 h-9 rounded-md object-cover border border-[var(--line)] flex-none" fallback={spool.kind === 'resin' ? (
      <span className="w-9 h-9 rounded-md flex-none grid place-items-center bg-[var(--plate)] border border-[var(--line)]">
        <ResinBottleIcon colorHex={spool.colorHex} />
      </span>
    ) : (
      <span
        className="w-9 h-9 rounded-md flex-none grid place-items-center text-compact-label font-bold text-[var(--ink-3)] bg-[var(--plate)] border border-[var(--line)]"
      >
        {spool.material.slice(0, 3).toUpperCase()}
      </span>
    )} />
  );
}

function SpoolTitle({ spool }: { spool: FilamentSpool }) {
  const t = useT();
  return (
    <div className="min-w-0 flex-1">
      <div className="text-body font-bold truncate flex items-center gap-1.5">
        <SpoolColorDot spool={spool} />
        {spool.material}
      </div>
      <div className="text-caption text-[var(--ink-3)] truncate">
        {[spool.manufacturer, spool.color].filter(Boolean).join(' · ') || t('noValue')}
      </div>
    </div>
  );
}

function StockBar({ remaining, kind, pct, status }: { remaining: number; kind: FilamentSpool['kind']; pct: number; status: ReturnType<typeof filamentStockStatus> }) {
  const { language } = useLanguage();
  return (
    <div>
      <div className="flex justify-between font-medium tabular-nums text-caption text-[var(--ink-2)] mb-1">
        <span>{formatSpoolAmount(remaining, kind, language)}</span>
        <span>{pct}%</span>
      </div>
      <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
        <div className={`h-full rounded-full ${STOCK_BAR_CLASS[status]}`} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

function ConfirmDelete({ id, onCancelDelete, onConfirmDelete }: { id: string } & Pick<CardActions, 'onCancelDelete' | 'onConfirmDelete'>) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  return (
    <div className="flex items-center gap-1.5">
      <span className="flex-1 text-caption text-[var(--ink)]">{t(import.meta.env.VITE_TARGET === 'web' ? 'webManagerDeleteSpool' : 'deleteConfirmQuestion')}</span>
      <button
        onClick={onCancelDelete}
        className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption cursor-pointer"
      >
        {t('cancel')}
      </button>
      <button
        disabled={!canEdit} onClick={() => onConfirmDelete(id)}
        className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-6 px-1.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption cursor-pointer"
      >
        {t('delete')}
      </button>
    </div>
  );
}

function SpoolCard({ spool, group, ...a }: { spool: FilamentSpool; group?: SpoolGroup } & CardActions) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const { language } = useLanguage();
  const status = filamentStockStatus(spool);
  const pct = filamentStockPercent(spool);
  const loaded = spool.unitId !== null;
  const statusLabel = useGroupStatusLabel();
  // A loaded spool cannot be dragged from here: it leaves its slot through the printer column.
  const draggable = !!a.onSpoolMouseDown && !loaded;
  return (
    <div
      data-testid={`spool-card-${spool.id}`}
      onMouseDown={(e) => canEdit && draggable && !startsOnButton(e) && a.onSpoolMouseDown?.(spool.id, e)}
      onDoubleClick={(e) => {
        if (canEdit && a.confirmDeleteId !== spool.id && !isFromInteractiveElement(e)) a.onEdit(spool);
      }}
      className={`rounded-[10px] overflow-hidden border border-[var(--line)] bg-[var(--panel)] flex flex-col ${
        draggable ? 'cursor-grab' : ''
      } ${a.highlightIds?.has(spool.id) ? 'spool-new' : ''}`}
    >
      <div className="flex items-center gap-2.5 px-3 py-2.5">
        <SpoolThumb spool={spool} />
        <SpoolTitle spool={spool} />
      </div>

      <div className="px-3 pb-3 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <PlaceChip group={group} location={spool.location} loadedSlot={loaded ? spool.slotIndex : null} />
          <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-compact-meta font-semibold uppercase flex-none ${STOCK_BADGE_CLASS[status]}`}>
            {statusLabel(status, loaded)}
          </span>
        </div>

        <StockBar remaining={spool.remainingWeightG} kind={spool.kind} pct={pct} status={status} />

        <div className="flex items-center justify-between pt-2 border-t border-[var(--line)] text-caption text-[var(--ink-3)]">
          <span>
            {spool.kind === 'resin'
              ? t('resinBottleFooter').replace('{amount}', formatVolumeMl(spool.originalWeightG, language))
              : formatDiameterMm(spool.diameterMm, language)}
          </span>
          {spool.price !== null && <span className="tabular-nums text-[var(--ink-2)] font-semibold">{formatPrice(spool.price, language)}</span>}
        </div>

        {a.confirmDeleteId === spool.id ? (
          <ConfirmDelete id={spool.id} onCancelDelete={a.onCancelDelete} onConfirmDelete={a.onConfirmDelete} />
        ) : (
          <div className="flex items-center justify-end gap-1">
            <span className="mr-auto inline-flex items-center gap-1">
              {import.meta.env.VITE_TARGET !== 'web' && a.onConsume && spool.kind === 'resin' && (
                <button
                  type="button"
                  onClick={(e) => a.onConsume?.(spool, e.currentTarget)}
                  aria-haspopup="dialog"
                  aria-expanded={a.consumeOpenId === spool.id}
                  className={pillButton}
                >
                  <span aria-hidden><Icon name="minus" size={14} /></span>
                  {t('resinConsumeButton')}
                </button>
              )}
              {import.meta.env.VITE_TARGET !== 'web' && a.onRestock && (
                <button
                  type="button"
                  onClick={(e) => a.onRestock?.(spool, e.currentTarget)}
                  aria-haspopup="dialog"
                  aria-expanded={a.restockOpenId === spool.id}
                  className={pillButton}
                >
                  <span aria-hidden><Icon name="plus" size={14} /></span>
                  {t('filamentRestockButton')}
                </button>
              )}
            </span>
            <button
              disabled={!canEdit} onClick={() => a.onEdit(spool)}
              aria-label={t('filamentEditAria')}
              className={`${iconButton} hover:bg-[var(--panel-2)]`}
            >
              <Icon name="edit" size={14} />
            </button>
            <button
              disabled={!canEdit} onClick={() => a.onRequestDelete(spool.id)}
              aria-label={t('deleteAriaLabel')}
              className={`${iconButton} hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]`}
            >
              <Icon name="close" size={14} />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

/** Place chip of a card: shelf location, or "printer · unit" (and slot) for loaded spools. */
function PlaceChip({ group, location, loadedSlot }: { group?: SpoolGroup; location: string | null; loadedSlot: number | null }) {
  const t = useT();
  const fallback = location || t('noValue');
  return (
    <span className="inline-flex items-center gap-1.5 tabular-nums text-caption font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] truncate">
      📍 {group && group.place.kind === 'unit' ? <UnitPlaceText group={group} slot={loadedSlot} /> : fallback}
    </span>
  );
}

function UnitPlaceText({ group, slot }: { group: SpoolGroup; slot: number | null }) {
  const t = useT();
  const { label } = useGroupPlaceText(group);
  return <>{slot === null ? label : `${label} · ${t('filamentGroupSlot').replace('{n}', String(slot + 1))}`}</>;
}

function GroupCard({ group, ...a }: { group: SpoolGroup } & CardActions) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const { language } = useLanguage();
  const formatCount = useFormatCount();
  const panelId = useId();
  const [expanded, setExpanded] = useState(false);
  const { first } = group;
  const loaded = group.place.kind === 'unit';
  const place = useGroupPlaceText(group);
  const statusLabel = useGroupStatusLabel();
  const texts = useGroupKindTexts(first.kind);
  const pct = filamentStockPercent({ originalWeightG: group.originalG, remainingWeightG: group.remainingG });
  // Taking a card by the mouse loads the spool that suits best (smallest usable rest); the
  // rows below let the user pick another one.
  const dragSpool = !loaded ? suggestSpool(group.spools) ?? first : null;
  const groupName = [first.material, first.color].filter(Boolean).join(' ');

  return (
    <div
      data-testid={`group-card-${first.id}`}
      onMouseDown={(e) => canEdit && dragSpool && a.onSpoolMouseDown && !startsOnButton(e) && a.onSpoolMouseDown(dragSpool.id, e)}
      className={`rounded-[10px] overflow-hidden border border-[var(--line)] bg-[var(--panel)] flex flex-col ${
        dragSpool && a.onSpoolMouseDown ? 'cursor-grab' : ''
      } ${group.spools.some((s) => a.highlightIds?.has(s.id)) ? 'spool-new' : ''}`}
    >
      <div className="flex items-center gap-2.5 px-3 py-2.5">
        <SpoolThumb spool={first} />
        <SpoolTitle spool={first} />
        <span className="flex-none text-compact-meta font-semibold px-2 py-1 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]">
          {formatCount(texts.count, group.spools.length)}
        </span>
      </div>

      <div className="px-3 pb-3 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <span className="inline-flex items-center gap-1.5 tabular-nums text-caption font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] truncate">
            📍 {place.label}
          </span>
          <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-compact-meta font-semibold uppercase flex-none ${STOCK_BADGE_CLASS[group.status]}`}>
            {statusLabel(group.status, loaded)}
          </span>
        </div>
        {place.note && <div className="text-caption text-[var(--ink-3)]">{place.note}</div>}

        <div>
          <div className="flex justify-between font-medium tabular-nums text-caption text-[var(--ink-2)] mb-1">
            <span>
              {formatSpoolAmount(group.remainingG, first.kind, language)}{' '}
              <span className="text-[var(--ink-3)]">{t('filamentGroupOfTotal').replace('{total}', formatSpoolAmount(group.originalG, first.kind, language))}</span>
            </span>
            <span>{pct}%</span>
          </div>
          <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
            <div className={`h-full rounded-full ${STOCK_BAR_CLASS[group.status]}`} style={{ width: `${pct}%` }} />
          </div>
          {group.emptyCount > 0 && (
            <div className="mt-1 text-caption text-[var(--crit)]">{formatCount(texts.emptyCount, group.emptyCount)}</div>
          )}
        </div>

        <div className="flex items-center justify-between pt-2 border-t border-[var(--line)] text-caption text-[var(--ink-3)]">
          <span>
            {first.kind === 'resin'
              ? t('resinBottleFooter').replace('{amount}', formatVolumeMl(first.originalWeightG, language))
              : formatDiameterMm(first.diameterMm, language)}
          </span>
        </div>

        <div className="flex items-center justify-between gap-1">
          <button
            type="button"
            onClick={() => setExpanded((open) => !open)}
            aria-expanded={expanded}
            aria-controls={panelId}
            aria-label={t('filamentGroupToggleAria').replace('{group}', groupName).replace('{action}', expanded ? texts.hide : texts.show)}
            className="h-6 px-2 inline-flex items-center gap-1 rounded-full text-caption font-semibold text-[var(--ink-2)] hover:bg-[var(--panel-2)] cursor-pointer"
          >
            {expanded ? texts.hide : texts.show}
            <Icon name="chevron" size={14} className={expanded ? 'rotate-180' : ''} />
          </button>
          <span className="inline-flex items-center gap-1">
            {import.meta.env.VITE_TARGET !== 'web' && a.onConsume && first.kind === 'resin' && (
              <button
                type="button"
                onClick={(e) => a.onConsume?.(suggestSpool(group.spools) ?? first, e.currentTarget, group.spools)}
                aria-haspopup="dialog"
                aria-expanded={a.consumeOpenId !== null && a.consumeOpenId !== undefined && group.spools.some((s) => s.id === a.consumeOpenId)}
                className={pillButton}
              >
                <span aria-hidden><Icon name="minus" size={14} /></span>
                {t('resinConsumeButton')}
              </button>
            )}
            {import.meta.env.VITE_TARGET !== 'web' && a.onRestock && (
              <button
                type="button"
                onClick={(e) => a.onRestock?.(first, e.currentTarget)}
                aria-haspopup="dialog"
                aria-expanded={a.restockOpenId === first.id}
                className={pillButton}
              >
                <span aria-hidden><Icon name="plus" size={14} /></span>
                {t('filamentRestockButton')}
              </button>
            )}
          </span>
        </div>

        {expanded && (
          <ul id={panelId} className="m-0 p-0 list-none flex flex-col gap-2 pt-2 border-t border-[var(--line)]">
            {group.spools.map((spool) => (
              <GroupSpoolRow key={spool.id} spool={spool} loaded={loaded} {...a} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function GroupSpoolRow({ spool, loaded, ...a }: { spool: FilamentSpool; loaded: boolean } & CardActions) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const { language } = useLanguage();
  const status = filamentStockStatus(spool);
  const pct = filamentStockPercent(spool);
  const amount = formatSpoolAmount(spool.remainingWeightG, spool.kind, language);
  const where = loaded && spool.slotIndex !== null ? t('filamentGroupSlot').replace('{n}', String(spool.slotIndex + 1)) : spool.location || t('noValue');
  const draggable = !!a.onSpoolMouseDown && !loaded;
  return (
    <li
      data-testid={`spool-row-${spool.id}`}
      onMouseDown={(e) => {
        e.stopPropagation();
        if (canEdit && draggable && !startsOnButton(e)) a.onSpoolMouseDown?.(spool.id, e);
      }}
      onDoubleClick={(e) => {
        if (canEdit && a.confirmDeleteId !== spool.id && !isFromInteractiveElement(e)) a.onEdit(spool);
      }}
      className={`flex flex-col gap-1 ${draggable ? 'cursor-grab' : ''} ${a.highlightIds?.has(spool.id) ? 'spool-new-row' : ''}`}
    >
      <div className="flex justify-between gap-2 font-medium tabular-nums text-caption text-[var(--ink-2)]">
        <span className="truncate">{where}</span>
        <span>{amount}</span>
      </div>
      <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
        <div className={`h-full rounded-full ${STOCK_BAR_CLASS[status]}`} style={{ width: `${pct}%` }} />
      </div>
      {a.confirmDeleteId === spool.id ? (
        <ConfirmDelete id={spool.id} onCancelDelete={a.onCancelDelete} onConfirmDelete={a.onConfirmDelete} />
      ) : (
        <div className="flex items-center justify-end gap-1">
          {import.meta.env.VITE_TARGET !== 'web' && a.onConsume && spool.kind === 'resin' && (
            <button
              type="button"
              onClick={(e) => a.onConsume?.(spool, e.currentTarget)}
              aria-label={`${t('resinConsumeButton')}: ${where}, ${amount}`}
              aria-haspopup="dialog"
              aria-expanded={a.consumeOpenId === spool.id}
              className={`${iconButton} hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]`}
            >
              <Icon name="minus" size={14} />
            </button>
          )}
          <button
            disabled={!canEdit} onClick={() => a.onEdit(spool)}
            aria-label={`${t('filamentEditAria')}: ${where}, ${amount}`}
            className={`${iconButton} hover:bg-[var(--panel-2)]`}
          >
            <Icon name="edit" size={14} />
          </button>
          <button
            disabled={!canEdit} onClick={() => a.onRequestDelete(spool.id)}
            aria-label={`${t('deleteAriaLabel')}: ${where}, ${amount}`}
            className={`${iconButton} hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]`}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      )}
    </li>
  );
}
