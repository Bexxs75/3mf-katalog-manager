import { useT } from '../i18n/LanguageContext';
import { isValidColorHex } from '../lib/filamentColors';
import { spoolColorList } from '../lib/filamentGroups';
import type { FilamentSpool } from '../types';

/** `conic-gradient` with equal wedges; one wedge per color, first color at the top. */
export function colorDotBackground(colors: string[]): string {
  if (colors.length === 1) return colors[0];
  const step = 100 / colors.length;
  const stops = colors.map((color, i) => `${color} ${+(i * step).toFixed(2)}% ${+((i + 1) * step).toFixed(2)}%`);
  return `conic-gradient(${stops.join(', ')})`;
}

interface Props {
  spool: Pick<FilamentSpool, 'color' | 'colorHex' | 'colorHexes'>;
  /** Diameter class, e.g. `w-3 h-3`. */
  className?: string;
  /** Shape class; the swatch in the printer menu uses a rounded square. */
  shape?: string;
}

/**
 * Color dot of a spool. Several colors show as one split dot. Alternative text: the color
 * name, or "multicolor"; without a name a single color stays decorative (there is no
 * text to give). Renders nothing when the spool has no usable color value.
 */
export function SpoolColorDot({ spool, className = 'w-3 h-3', shape = 'rounded-full' }: Props) {
  const t = useT();
  const colors = spoolColorList(spool).filter(isValidColorHex);
  if (colors.length === 0) return null;
  const name = spool.color?.trim() || null;
  const multi = colors.length > 1;
  const label = multi ? (name ? `${name}, ${t('filamentColorMulti')}` : t('filamentColorMulti')) : name;
  return (
    <span
      data-testid="spool-color-dot"
      data-multicolor={multi ? 'true' : undefined}
      role={label ? 'img' : undefined}
      aria-label={label ?? undefined}
      aria-hidden={label ? undefined : true}
      className={`${className} ${shape} border border-[var(--line-strong)] flex-none inline-block`}
      style={{ background: colorDotBackground(colors) }}
    />
  );
}
