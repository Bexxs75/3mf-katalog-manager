import type { FilamentSpool } from '../types';

/** Count 1 to 20 (the backend checks the same, RESTOCK_MAX_COUNT in filament.rs). */
export const RESTOCK_MIN_COUNT = 1;
export const RESTOCK_MAX_COUNT = 20;

export function clampRestockCount(n: number): number {
  if (!Number.isFinite(n)) return RESTOCK_MIN_COUNT;
  return Math.min(RESTOCK_MAX_COUNT, Math.max(RESTOCK_MIN_COUNT, Math.round(n)));
}

/** Round to 0.1 (like `round_tenth` in the backend). */
export function roundTenth(n: number): number {
  return Math.round(n * 10) / 10;
}

export interface RestockDefaults {
  /** Grams (filament) or milliliters (resin). */
  weight: number;
  price: number | null;
  location: string;
}

/** Prefill: original amount, price, location (if the template sits in a printer: its home location). */
export function restockDefaults(spool: FilamentSpool): RestockDefaults {
  const location = spool.location?.trim() ? spool.location : (spool.homeLocation ?? '');
  return { weight: spool.originalWeightG, price: spool.price, location };
}

/** "PETG · Red" for titles and messages. */
export function restockSpoolLabel(spool: Pick<FilamentSpool, 'material' | 'color'>): string {
  return [spool.material, spool.color].filter(Boolean).join(' · ');
}

/** "29,95" / "29.95" / "30" → number; empty → null; anything else → undefined (invalid). */
export function parseDecimalInput(raw: string): number | null | undefined {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  if (!/^\d+([.,]\d+)?$/.test(trimmed)) return undefined;
  return Number(trimmed.replace(',', '.'));
}

export interface RestockInput {
  count: number;
  weight: string;
  price: string;
  location: string;
}

export interface RestockRequest {
  count: number;
  weight: number;
  price: number | null;
  location: string | null;
}

export type RestockValidation = { ok: true; value: RestockRequest } | { ok: false; error: 'weight' | 'price' };

export function validateRestockInput(input: RestockInput): RestockValidation {
  const parsed = parseDecimalInput(input.weight);
  const weight = typeof parsed === 'number' ? roundTenth(parsed) : 0;
  if (weight <= 0) return { ok: false, error: 'weight' };
  const price = parseDecimalInput(input.price);
  if (price === undefined) return { ok: false, error: 'price' };
  const location = input.location.trim();
  return {
    ok: true,
    value: { count: clampRestockCount(input.count), weight, price, location: location === '' ? null : location },
  };
}
