import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { FILAMENT_PALETTE, isValidColorHex } from '../lib/filamentColors';

interface Props {
  value: string | null;
  onChange: (value: string | null) => void;
}

/**
 * Color picker for spools: palette of common filament colors plus hex input.
 * Deliberately no `<input type="color">` - native form controls ignore
 * this app's dark theme.
 *
 * Keyboard: radiogroup with roving tabindex. Only one swatch in the tab order;
 * arrow keys navigate and select.
 */
export function ColorPicker({ value, onChange }: Props) {
  const t = useT();
  const [draft, setDraft] = useState(value ?? '');
  const buttonRefs = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    setDraft(value ?? '');
  }, [value]);

  const commitDraft = (next: string) => {
    setDraft(next);
    const normalized = next.startsWith('#') ? next : `#${next}`;
    if (next.trim() === '') onChange(null);
    else if (isValidColorHex(normalized)) onChange(normalized.toLowerCase());
  };

  const selectedIndex = value ? FILAMENT_PALETTE.indexOf(value) : -1;
  const focusableIndex = selectedIndex >= 0 ? selectedIndex : 0;

  const handleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | null = null;

    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      nextIndex = (index + 1) % FILAMENT_PALETTE.length;
      e.preventDefault();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      nextIndex = (index - 1 + FILAMENT_PALETTE.length) % FILAMENT_PALETTE.length;
      e.preventDefault();
    } else if (e.key === 'Home') {
      nextIndex = 0;
      e.preventDefault();
    } else if (e.key === 'End') {
      nextIndex = FILAMENT_PALETTE.length - 1;
      e.preventDefault();
    }

    if (nextIndex !== null) {
      onChange(FILAMENT_PALETTE[nextIndex]);
      setTimeout(() => buttonRefs.current[nextIndex]?.focus(), 0);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-8 gap-1.5 w-fit" role="radiogroup" aria-label={t('filamentColorValueLabel')}>
        {FILAMENT_PALETTE.map((hex, index) => (
          <button
            key={hex}
            ref={(el) => { buttonRefs.current[index] = el; }}
            type="button"
            role="radio"
            aria-checked={value === hex}
            aria-label={hex}
            tabIndex={index === focusableIndex ? 0 : -1}
            onClick={() => onChange(hex)}
            onKeyDown={(e) => handleKeyDown(e, index)}
            className={`w-6 h-6 rounded-[4px] border border-[var(--line-strong)] cursor-pointer ${
              value === hex ? 'outline outline-2 outline-offset-1 outline-[var(--accent)]' : ''
            }`}
            style={{ backgroundColor: hex }}
          />
        ))}
      </div>
      <div className="flex items-center gap-2">
        <span
          className="w-7 h-7 rounded-[4px] border border-[var(--line-strong)] flex-none"
          style={value && isValidColorHex(value) ? { backgroundColor: value } : undefined}
          aria-hidden
        />
        <input
          value={draft}
          onChange={(e) => commitDraft(e.target.value)}
          placeholder="#1a1a1a"
          maxLength={7}
          aria-label={t('filamentColorValueLabel')}
          className="w-28 h-8 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] font-mono-ui text-[12.5px] outline-0 focus:border-[var(--accent)]"
        />
        {value && (
          <button
            type="button"
            onClick={() => onChange(null)}
            className="text-[11.5px] text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
          >
            {t('filamentColorNone')}
          </button>
        )}
      </div>
    </div>
  );
}
