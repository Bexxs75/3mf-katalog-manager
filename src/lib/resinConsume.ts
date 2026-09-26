import { parseDecimalInput, roundTenth } from './filamentRestock';

/** Input "Used (ml)": rounded to 0.1 and > 0, otherwise null. */
export function parseConsumeAmount(raw: string): number | null {
  const parsed = parseDecimalInput(raw);
  if (typeof parsed !== 'number') return null;
  const amount = roundTenth(parsed);
  return amount > 0 ? amount : null;
}
