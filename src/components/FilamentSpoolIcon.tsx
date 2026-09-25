import { isValidColorHex } from '../lib/filamentColors';

/** Spulen-Symbol (Vorderansicht) fuer Filament ohne eigenes Bild; Wicklung in der Spulenfarbe. */
export function FilamentSpoolIcon({ colorHex, size = 20 }: { colorHex: string | null; size?: number }) {
  const fill = colorHex && isValidColorHex(colorHex) ? colorHex : 'var(--plate)';
  return (
    <svg data-testid="filament-spool-icon" aria-hidden width={size} height={size} viewBox="0 0 26 26" className="text-[var(--ink-2)] flex-none">
      <circle cx="13" cy="13" r="11.5" fill="currentColor" />
      <circle cx="13" cy="13" r="9" fill={fill} />
      <circle cx="13" cy="13" r="4.2" fill="currentColor" />
      <circle cx="13" cy="13" r="2" fill="var(--panel)" />
    </svg>
  );
}
