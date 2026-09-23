import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { FILAMENT_PALETTE, isValidColorHex } from '../lib/filamentColors';

interface Props {
  value: string | null;
  onChange: (value: string | null) => void;
}

/**
 * Farbwahl fuer Spulen: Palette gaengiger Filamentfarben plus Hex-Eingabe.
 * Bewusst kein `<input type="color">` - native Formularelemente ignorieren
 * das Dark-Theme dieser App.
 */
export function ColorPicker({ value, onChange }: Props) {
  const t = useT();
  const [draft, setDraft] = useState(value ?? '');

  useEffect(() => {
    setDraft(value ?? '');
  }, [value]);

  const commitDraft = (next: string) => {
    setDraft(next);
    const normalized = next.startsWith('#') ? next : `#${next}`;
    if (next.trim() === '') onChange(null);
    else if (isValidColorHex(normalized)) onChange(normalized.toLowerCase());
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-8 gap-1.5 w-fit" role="radiogroup" aria-label={t('filamentColorValueLabel')}>
        {FILAMENT_PALETTE.map((hex) => (
          <button
            key={hex}
            type="button"
            role="radio"
            aria-checked={value === hex}
            aria-label={hex}
            onClick={() => onChange(hex)}
            className={`w-6 h-6 rounded-[4px] border border-[var(--line-strong)] cursor-pointer ${
              value === hex ? 'outline outline-2 outline-offset-1 outline-[var(--accent)]' : ''
            }`}
            style={{ background: hex }}
          />
        ))}
      </div>
      <div className="flex items-center gap-2">
        <span
          className="w-7 h-7 rounded-[4px] border border-[var(--line-strong)] flex-none"
          style={value ? { background: value } : undefined}
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
