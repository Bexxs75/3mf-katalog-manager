import { isValidColorHex } from '../lib/filamentColors';

/** Bottle icon for resin without its own image. */
export function ResinBottleIcon({ colorHex, size = 26 }: { colorHex: string | null; size?: number }) {
  const fill = colorHex && isValidColorHex(colorHex) ? colorHex : 'var(--plate)';
  return (
    <svg data-testid="resin-bottle-icon" aria-hidden width={size} height={size} viewBox="0 0 26 26" className="text-[var(--ink-2)]">
      <rect x="9" y="1.5" width="8" height="4" rx="1" fill="currentColor" />
      <path d="M7 7h12l1.5 3v13a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 23V10z" fill="currentColor" />
      <rect x="8" y="12" width="10" height="10" rx="1" fill={fill} />
    </svg>
  );
}
