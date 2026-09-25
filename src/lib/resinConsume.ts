import { parseDecimalInput, roundTenth } from './filamentRestock';

/** Eingabe "Verbraucht (ml)": auf 0,1 gerundet und > 0, sonst null. */
export function parseConsumeAmount(raw: string): number | null {
  const parsed = parseDecimalInput(raw);
  if (typeof parsed !== 'number') return null;
  const amount = roundTenth(parsed);
  return amount > 0 ? amount : null;
}
