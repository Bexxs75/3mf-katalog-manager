import type { FilamentSpool } from '../types';

export type FilamentStockStatus = 'ok' | 'low' | 'empty';

const LOW_STOCK_RATIO = 0.2;

export const STOCK_BADGE_CLASS: Record<FilamentStockStatus, string> = {
  ok: 'bg-[var(--good-soft)] text-[var(--good)]',
  low: 'bg-[var(--warn-soft)] text-[var(--warn)]',
  empty: 'bg-[var(--crit-soft)] text-[var(--crit)]',
};

export const STOCK_BAR_CLASS: Record<FilamentStockStatus, string> = {
  ok: 'bg-[var(--good)]',
  low: 'bg-[var(--warn)]',
  empty: 'bg-[var(--crit)]',
};

export function filamentStockStatus(spool: Pick<FilamentSpool, 'originalWeightG' | 'remainingWeightG'>): FilamentStockStatus {
  if (spool.remainingWeightG <= 0) return 'empty';
  const ratio = spool.originalWeightG > 0 ? spool.remainingWeightG / spool.originalWeightG : 0;
  if (ratio < LOW_STOCK_RATIO) return 'low';
  return 'ok';
}

export function filamentStockPercent(spool: Pick<FilamentSpool, 'originalWeightG' | 'remainingWeightG'>): number {
  if (spool.originalWeightG <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((spool.remainingWeightG / spool.originalWeightG) * 100)));
}
