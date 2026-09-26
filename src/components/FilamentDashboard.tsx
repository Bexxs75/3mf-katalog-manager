import type { MouseEvent as ReactMouseEvent } from 'react';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { formatSpoolAmount, formatVolumeMl, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { isValidColorHex } from '../lib/filamentColors';
import { isFromInteractiveElement } from '../lib/spoolCardEvents';
import { ResinBottleIcon } from './ResinBottleIcon';

interface Props {
  spools: FilamentSpool[];
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
  /** Opens "− Usage" (resin only). */
  onConsume?: (spool: FilamentSpool, anchor: HTMLElement) => void;
  /** Spool whose usage popover is currently open (aria-expanded). */
  consumeOpenId?: string | null;
  /** Entries just created via restock, briefly highlighted. */
  highlightIds?: ReadonlySet<string>;
}

// No dragging when the mouse down lands on a button of the card/row
// (edit, delete, confirm).
function startsOnButton(event: ReactMouseEvent): boolean {
  return (event.target as HTMLElement).closest('button') !== null;
}

const statusClass: Record<string, string> = {
  ok: 'bg-[var(--good-soft)] text-[var(--good)]',
  low: 'bg-[var(--warn-soft)] text-[var(--warn)]',
  empty: 'bg-[var(--crit-soft)] text-[var(--crit)]',
};
const barClass: Record<string, string> = {
  ok: 'bg-[var(--good)]',
  low: 'bg-[var(--warn)]',
  empty: 'bg-[var(--crit)]',
};

export function FilamentDashboard({
  spools,
  confirmDeleteId,
  onEdit,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
  onSpoolMouseDown,
  onRestock,
  restockOpenId,
  onConsume,
  consumeOpenId,
  highlightIds,
}: Props) {
  const t = useT();
  const { language } = useLanguage();

  if (spools.length === 0) {
    return <div className="text-[13px] text-[var(--ink-3)]">{t('filamentNoResults')}</div>;
  }

  const statusLabel = (st: string) =>
    st === 'empty' ? t('filamentStatusEmpty') : st === 'low' ? t('filamentStatusLow') : t('filamentStatusOk');

  return (
    <div className="grid gap-3" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(230px, 1fr))' }}>
      {spools.map((spool) => {
        const status = filamentStockStatus(spool);
        const pct = filamentStockPercent(spool);
        return (
          <div
            key={spool.id}
            data-testid={`spool-card-${spool.id}`}
            onMouseDown={(e) => !startsOnButton(e) && onSpoolMouseDown?.(spool.id, e)}
            onDoubleClick={(e) => {
              if (confirmDeleteId !== spool.id && !isFromInteractiveElement(e)) onEdit(spool);
            }}
            className={`rounded-[10px] overflow-hidden border border-[var(--line)] bg-[var(--panel)] flex flex-col ${
              onSpoolMouseDown ? 'cursor-grab' : ''
            } ${highlightIds?.has(spool.id) ? 'spool-new' : ''}`}
          >
            <div className="flex items-center gap-2.5 px-3 py-2.5">
              {spool.imagePng ? (
                <img
                  src={`data:image/png;base64,${spool.imagePng}`}
                  className="w-9 h-9 rounded-md object-cover border border-[var(--line)] flex-none"
                />
              ) : spool.kind === 'resin' ? (
                <span className="w-9 h-9 rounded-md flex-none grid place-items-center bg-[var(--plate)] border border-[var(--line)]">
                  <ResinBottleIcon colorHex={spool.colorHex} />
                </span>
              ) : (
                <span
                  className="w-9 h-9 rounded-md flex-none grid place-items-center text-[9px] font-bold text-[var(--ink-3)] bg-[var(--plate)] border border-[var(--line)]"
                >
                  {spool.material.slice(0, 3).toUpperCase()}
                </span>
              )}
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px] font-bold truncate flex items-center gap-1.5">
                  {spool.colorHex && isValidColorHex(spool.colorHex) && (
                    <span
                      className="w-3 h-3 rounded-full border border-[var(--line-strong)] flex-none"
                      style={{ backgroundColor: spool.colorHex }}
                      aria-hidden
                    />
                  )}
                  {spool.material}
                </div>
                <div className="text-[11.5px] text-[var(--ink-3)] truncate">
                  {[spool.manufacturer, spool.color].filter(Boolean).join(' · ') || t('noValue')}
                </div>
              </div>
            </div>

            <div className="px-3 pb-3 flex flex-col gap-2">
              <div className="flex items-center justify-between gap-2">
                <span className="inline-flex items-center gap-1.5 font-mono-ui text-[11px] font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] truncate">
                  📍 {spool.location || t('noValue')}
                </span>
                <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-[10px] font-bold uppercase tracking-wide flex-none ${statusClass[status]}`}>
                  {statusLabel(status)}
                </span>
              </div>

              <div>
                <div className="flex justify-between font-mono-ui text-[11px] text-[var(--ink-2)] mb-1">
                  <span>{formatSpoolAmount(spool.remainingWeightG, spool.kind, language)}</span>
                  <span>{pct}%</span>
                </div>
                <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
                  <div className={`h-full rounded-full ${barClass[status]}`} style={{ width: `${pct}%` }} />
                </div>
              </div>

              <div className="flex items-center justify-between pt-2 border-t border-[var(--line)] text-[11.5px] text-[var(--ink-3)]">
                <span>
                  {spool.kind === 'resin'
                    ? t('resinBottleFooter').replace('{amount}', formatVolumeMl(spool.originalWeightG, language))
                    : formatDiameterMm(spool.diameterMm, language)}
                </span>
                {spool.price !== null && <span className="font-mono-ui text-[var(--ink-2)] font-semibold">{formatPrice(spool.price, language)}</span>}
              </div>

              {confirmDeleteId === spool.id ? (
                <div className="flex items-center gap-1.5">
                  <span className="flex-1 text-[10.5px] text-[var(--ink)]">{t('deleteConfirmQuestion')}</span>
                  <button
                    onClick={onCancelDelete}
                    className="h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[10.5px] cursor-pointer"
                  >
                    {t('cancel')}
                  </button>
                  <button
                    onClick={() => onConfirmDelete(spool.id)}
                    className="h-6 px-1.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[10.5px] cursor-pointer"
                  >
                    {t('delete')}
                  </button>
                </div>
              ) : (
                <div className="flex items-center justify-end gap-1">
                  <span className="mr-auto inline-flex items-center gap-1">
                    {onConsume && spool.kind === 'resin' && (
                      <button
                        type="button"
                        onClick={(e) => onConsume(spool, e.currentTarget)}
                        aria-haspopup="dialog"
                        aria-expanded={consumeOpenId === spool.id}
                        className="h-6 px-2 inline-flex items-center gap-1 rounded-full text-[11px] font-semibold text-[var(--ink-2)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] cursor-pointer"
                      >
                        <span aria-hidden>−</span>
                        {t('resinConsumeButton')}
                      </button>
                    )}
                    {onRestock && (
                      <button
                        type="button"
                        onClick={(e) => onRestock(spool, e.currentTarget)}
                        aria-haspopup="dialog"
                        aria-expanded={restockOpenId === spool.id}
                        className="h-6 px-2 inline-flex items-center gap-1 rounded-full text-[11px] font-semibold text-[var(--ink-2)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] cursor-pointer"
                      >
                        <span aria-hidden>＋</span>
                        {t('filamentRestockButton')}
                      </button>
                    )}
                  </span>
                  <button
                    onClick={() => onEdit(spool)}
                    aria-label={t('filamentEditAria')}
                    className="w-6 h-6 grid place-items-center rounded-full text-[11px] text-[var(--ink-3)] hover:bg-[var(--panel-2)] cursor-pointer"
                  >
                    ✎
                  </button>
                  <button
                    onClick={() => onRequestDelete(spool.id)}
                    aria-label={t('deleteAriaLabel')}
                    className="w-6 h-6 grid place-items-center rounded-full text-[11px] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)] cursor-pointer"
                  >
                    ✕
                  </button>
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
