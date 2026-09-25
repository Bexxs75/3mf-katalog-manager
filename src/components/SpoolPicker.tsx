import { useEffect, useId, useRef, useState } from 'react';
import { useLanguage } from '../i18n/LanguageContext';
import { formatStockG } from '../i18n/format';
import type { Language } from '../i18n/types';
import type { FilamentSpool } from '../types';

interface Props {
  spools: FilamentSpool[];
  value: string | null;
  onChange: (spoolId: string) => void;
  label: string;
  placeholder?: string;
}

// Material · Farbe · Hersteller/Lagerort (falls vorhanden) · Restgewicht -
// sonst sind baugleiche Spulen (gleiches Material, gleiche Farbe) in der
// Liste nicht unterscheidbar.
const text = (s: FilamentSpool, language: Language) =>
  [s.material, s.color, s.manufacturer, s.location, formatStockG(s.remainingWeightG, language)].filter(Boolean).join(' · ');

/** Eigene Auswahl mit Farbfeld (native <select>-Popups ignorieren das Theme). */
export function SpoolPicker({ spools, value, onChange, label, placeholder }: Props) {
  const { language } = useLanguage();
  const uid = useId();
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const selected = spools.find((s) => s.id === value) ?? null;
  const optionId = (id: string) => `${uid}-${id}`;

  // Beim Oeffnen (bzw. wenn sich der ausgewaehlte Wert aendert) auf die
  // aktuelle Auswahl springen und die Liste fokussieren.
  useEffect(() => {
    if (open) {
      setActiveId(value ?? spools[0]?.id ?? null);
      listRef.current?.focus();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, value]);

  // Laedt sich die Spulenliste waehrend die Liste offen ist neu (z.B. nach
  // einer Vorschau-Antwort), soll die Tastatur-Position NICHT zurueckspringen
  // - nur wenn das aktive Element tatsaechlich verschwunden ist.
  useEffect(() => {
    if (!open) return;
    setActiveId((prev) => (prev !== null && spools.some((s) => s.id === prev) ? prev : (spools[0]?.id ?? null)));
  }, [spools, open]);

  useEffect(() => {
    if (!open || activeId === null) return;
    document.getElementById(optionId(activeId))?.scrollIntoView?.({ block: 'nearest' });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeId, open]);

  const choose = (id: string) => {
    onChange(id);
    setOpen(false);
  };

  const activeIndex = activeId === null ? -1 : spools.findIndex((s) => s.id === activeId);

  return (
    <div className="relative">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${selected ? text(selected, language) : placeholder ?? ''}`}
        onClick={() => setOpen((o) => !o)}
        className="w-full h-8 px-2 flex items-center gap-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel-2)] text-[12.5px] text-left cursor-pointer"
      >
        {selected ? (
          <>
            <span className="w-3 h-3 rounded-[3px] flex-none border border-white/15" style={{ background: selected.colorHex ?? 'transparent' }} />
            <span className="truncate">{text(selected, language)}</span>
          </>
        ) : (
          <span className="text-[var(--ink-3)] truncate">{placeholder}</span>
        )}
        <span className="ml-auto text-[10px] text-[var(--ink-3)]">▾</span>
      </button>
      {open && (
        <ul
          ref={listRef}
          role="listbox"
          tabIndex={-1}
          aria-label={label}
          aria-activedescendant={activeId !== null ? optionId(activeId) : undefined}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              const next = spools[Math.min(spools.length - 1, activeIndex + 1)];
              if (next) setActiveId(next.id);
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              const prev = spools[Math.max(0, activeIndex - 1)];
              if (prev) setActiveId(prev.id);
            } else if (e.key === 'Enter') {
              e.preventDefault();
              if (activeId !== null) choose(activeId);
            } else if (e.key === 'Escape') {
              e.preventDefault();
              // Nicht bis zum umgebenden Dialog durchreichen - Escape soll
              // hier nur die Liste schliessen, nicht den ganzen Dialog.
              e.stopPropagation();
              setOpen(false);
            }
          }}
          onBlur={() => setOpen(false)}
          className="absolute z-50 mt-1 w-full max-h-56 overflow-y-auto rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] py-1 outline-0"
        >
          {spools.map((s) => (
            <li
              key={s.id}
              id={optionId(s.id)}
              role="option"
              aria-selected={s.id === value}
              onMouseDown={(e) => { e.preventDefault(); choose(s.id); }}
              className={`px-2 py-1.5 flex items-center gap-2 text-[12.5px] cursor-pointer ${s.id === activeId ? 'bg-[var(--panel-2)]' : ''}`}
            >
              <span className="w-3 h-3 rounded-[3px] flex-none border border-white/15" style={{ background: s.colorHex ?? 'transparent' }} />
              <span className="truncate">{text(s, language)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
