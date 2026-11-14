import type { SpoolKind } from '../types';

/** What the user prints with. The choice only hides parts of the interface; no data is removed. */
export type PrintTechnology = 'filament' | 'resin' | 'both';

export const PRINT_TECHNOLOGIES: readonly PrintTechnology[] = ['filament', 'resin', 'both'];

/** What the backend reports: the choice and whether the question was already answered. */
export interface PrintTechnologyState {
  value: PrintTechnology;
  asked: boolean;
}

/** An unknown or missing value never hides anything. */
export function parsePrintTechnology(value: unknown): PrintTechnology {
  return PRINT_TECHNOLOGIES.includes(value as PrintTechnology) ? (value as PrintTechnology) : 'both';
}

/**
 * Reads the backend answer. Anything that is not a well-formed answer counts as
 * "asked, both", so a damaged reply neither hides parts nor nags with the question.
 */
export function parsePrintTechnologyState(raw: unknown): PrintTechnologyState {
  if (typeof raw !== 'object' || raw === null) return { value: 'both', asked: true };
  const { value, asked } = raw as { value?: unknown; asked?: unknown };
  return { value: parsePrintTechnology(value), asked: asked !== false };
}

/** Whether areas of this kind are shown. */
export function showsKind(technology: PrintTechnology, kind: SpoolKind): boolean {
  return technology === 'both' || technology === kind;
}

/** The only kind the material view may show, or null while both are offered. */
export function lockedKind(technology: PrintTechnology): SpoolKind | null {
  return technology === 'both' ? null : technology;
}
