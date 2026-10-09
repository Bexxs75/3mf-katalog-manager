/** Keep form input decimal-only; Number alone also accepts hexadecimal and exponents. */
export function parseDecimalInput(value: string): number {
  const text = value.trim();
  return /^\d*[.,]?\d+$/.test(text) ? Number(text.replace(',', '.')) : NaN;
}
