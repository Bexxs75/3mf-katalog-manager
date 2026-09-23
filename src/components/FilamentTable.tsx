import type { MouseEvent as ReactMouseEvent } from 'react';
import { useState } from 'react';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { formatWeightG, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { isValidColorHex } from '../lib/filamentColors';

interface Props {
  spools: FilamentSpool[];
  confirmDeleteId: string | null;
  onEdit: (spool: FilamentSpool) => void;
  onRequestDelete: (id: string) => void;
  onCancelDelete: () => void;
  onConfirmDelete: (id: string) => void;
  /** Mausdruck auf einer Spule - startet ggf. das Ziehen in ein Fach. */
  onSpoolMouseDown?: (spoolId: string, event: ReactMouseEvent) => void;
}

// Kein Ziehen, wenn der Mausdruck auf einem Knopf der Karte/Zeile landet
// (Bearbeiten, Loeschen, Bestaetigen).
function startsOnButton(event: ReactMouseEvent): boolean {
  return (event.target as HTMLElement).closest('button') !== null;
}

type SortKey = 'material' | 'manufacturer' | 'color' | 'location' | 'diameterMm' | 'remainingWeightG' | 'price';

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

export function FilamentTable({ spools, confirmDeleteId, onEdit, onRequestDelete, onCancelDelete, onConfirmDelete, onSpoolMouseDown }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [sortKey, setSortKey] = useState<SortKey>('material');
  const [sortDir, setSortDir] = useState<1 | -1>(1);

  const statusLabel = (st: string) =>
    st === 'empty' ? t('filamentStatusEmpty') : st === 'low' ? t('filamentStatusLow') : t('filamentStatusOk');

  const toggleSort = (key: SortKey) => {
    if (key === sortKey) setSortDir((d) => (d === 1 ? -1 : 1));
    else {
      setSortKey(key);
      setSortDir(1);
    }
  };

  const sorted = [...spools].sort((a, b) => {
    let av: string | number;
    let bv: string | number;
    switch (sortKey) {
      case 'diameterMm':
      case 'remainingWeightG':
        av = a[sortKey];
        bv = b[sortKey];
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

  if (spools.length === 0) {
    return <div className="text-[13px] text-[var(--ink-3)]">{t('filamentNoResults')}</div>;
  }

  return (
    <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] overflow-hidden overflow-x-auto">
      <table className="w-full border-collapse text-[12.5px]">
        <thead>
          <tr>
            {columns.map((col) => (
              <th
                key={col.key}
                onClick={() => toggleSort(col.key)}
                className="text-left text-[10.5px] uppercase tracking-wider font-bold text-[var(--ink-3)] px-3 py-2.5 border-b border-[var(--line)] bg-[var(--panel-2)] cursor-pointer whitespace-nowrap select-none"
              >
                {col.label}{' '}
                <span className={sortKey === col.key ? 'text-[var(--accent)] opacity-100' : 'opacity-30'}>
                  {sortKey === col.key && sortDir === -1 ? '▴' : '▾'}
                </span>
              </th>
            ))}
            <th className="text-left text-[10.5px] uppercase tracking-wider font-bold text-[var(--ink-3)] px-3 py-2.5 border-b border-[var(--line)] bg-[var(--panel-2)]">
              {t('filamentColumnStatus')}
            </th>
            <th className="border-b border-[var(--line)] bg-[var(--panel-2)]" />
          </tr>
        </thead>
        <tbody>
          {sorted.map((spool) => {
            const status = filamentStockStatus(spool);
            const pct = filamentStockPercent(spool);
            return (
              <tr
                key={spool.id}
                data-testid={`spool-row-${spool.id}`}
                onMouseDown={(e) => !startsOnButton(e) && onSpoolMouseDown?.(spool.id, e)}
                className={`hover:bg-[var(--panel-2)] ${onSpoolMouseDown ? 'cursor-grab' : ''}`}
              >
                <td className="px-3 py-2.5 border-b border-[var(--line)] font-semibold">
                  <div className="flex items-center gap-2">
                    {spool.imagePng ? (
                      <img src={`data:image/png;base64,${spool.imagePng}`} className="w-6 h-6 rounded object-cover border border-[var(--line)]" />
                    ) : (
                      <span className="w-2.5 h-2.5 rounded-sm bg-[var(--plate)] border border-[var(--line-strong)]" />
                    )}
                    {spool.material}
                  </div>
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] text-[var(--ink-2)]">{spool.manufacturer || t('noValue')}</td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] text-[var(--ink-2)]">
                  <span className="inline-flex items-center gap-1.5">
                    {spool.colorHex && isValidColorHex(spool.colorHex) && (
                      <span
                        className="w-3 h-3 rounded-full border border-[var(--line-strong)] flex-none"
                        style={{ backgroundColor: spool.colorHex }}
                        aria-hidden
                      />
                    )}
                    {spool.color || t('noValue')}
                  </span>
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)]">
                  <span className="font-mono-ui text-[11px] font-semibold px-2 py-1 rounded-md bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)] whitespace-nowrap">
                    {spool.location || t('noValue')}
                  </span>
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] font-mono-ui text-[var(--ink-2)]">{formatDiameterMm(spool.diameterMm, language)}</td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] min-w-[130px]">
                  <div className="flex justify-between font-mono-ui text-[10.5px] text-[var(--ink-3)] mb-1">
                    <span>{formatWeightG(spool.remainingWeightG, language)}</span>
                    <span>{pct}%</span>
                  </div>
                  <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
                    <div className={`h-full rounded-full ${barClass[status]}`} style={{ width: `${pct}%` }} />
                  </div>
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] font-mono-ui text-right text-[var(--ink-2)]">
                  {spool.price !== null ? formatPrice(spool.price, language) : t('noValue')}
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)]">
                  <span className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-[10px] font-bold uppercase tracking-wide ${statusClass[status]}`}>
                    {statusLabel(status)}
                  </span>
                </td>
                <td className="px-3 py-2.5 border-b border-[var(--line)] text-right whitespace-nowrap">
                  {confirmDeleteId === spool.id ? (
                    <span className="inline-flex items-center gap-1.5">
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
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1">
                      <button
                        onClick={() => onEdit(spool)}
                        aria-label={t('filamentEditAria')}
                        className="w-6 h-6 grid place-items-center rounded-full text-[11px] text-[var(--ink-3)] hover:bg-[var(--panel)] cursor-pointer"
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
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
