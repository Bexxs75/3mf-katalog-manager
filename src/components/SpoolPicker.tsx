import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
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
// Liste nicht unterscheidbar. Wird als vollstaendiger Accessible Name fuer
// Knopf und Optionen verwendet (unabhaengig von der sichtbaren Aufteilung).
const text = (s: FilamentSpool, language: Language) =>
  [s.material, s.color, s.manufacturer, s.location, formatStockG(s.remainingWeightG, language)].filter(Boolean).join(' · ');

// Sichtbare Kurzbeschriftung im geschlossenen Knopf - Hersteller darf
// abgeschnitten werden, das Restgewicht steht separat und bleibt immer lesbar.
const triggerLabel = (s: FilamentSpool) => [s.material, s.color, s.manufacturer].filter(Boolean).join(' · ');

// Zweite Zeile je Option in der Liste (gedaempft): Restgewicht zuerst, dann
// Hersteller/Lagerort - so bleibt das Gewicht auch bei langen Namen sichtbar.
const optionMeta = (s: FilamentSpool, language: Language) =>
  [formatStockG(s.remainingWeightG, language), s.manufacturer, s.location].filter(Boolean).join(' · ');

const POPUP_MIN_WIDTH = 384; // 24rem
const VIEWPORT_MARGIN = 8;

interface PopupRect {
  top: number;
  left: number;
  width: number;
}

function popupPosition(rect: DOMRect): PopupRect {
  const width = Math.min(Math.max(rect.width, POPUP_MIN_WIDTH), window.innerWidth - VIEWPORT_MARGIN * 2);
  let left = rect.left;
  if (left + width > window.innerWidth - VIEWPORT_MARGIN) {
    // Passt rechts nicht mehr in den Viewport - am rechten Rand des Knopfs
    // ausrichten (und noch innerhalb des Viewports halten).
    left = Math.max(VIEWPORT_MARGIN, rect.right - width);
  }
  return { top: rect.bottom + 4, left, width };
}

/** Eigene Auswahl mit Farbfeld (native <select>-Popups ignorieren das Theme). */
export function SpoolPicker({ spools, value, onChange, label, placeholder }: Props) {
  const { language } = useLanguage();
  const uid = useId();
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [popup, setPopup] = useState<PopupRect | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const selected = spools.find((s) => s.id === value) ?? null;
  const optionId = (id: string) => `${uid}-${id}`;

  // Beim Oeffnen (bzw. wenn sich der ausgewaehlte Wert aendert) auf die
  // aktuelle Auswahl springen, die Liste fokussieren und die Popup-Position
  // relativ zum Knopf berechnen (fixed positioniert, damit der ueberlaufende
  // Dialog-Scrollcontainer sie nicht abschneidet).
  useEffect(() => {
    if (open) {
      setActiveId(value ?? spools[0]?.id ?? null);
      if (buttonRef.current) setPopup(popupPosition(buttonRef.current.getBoundingClientRect()));
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

  // Fixed-positioniertes Popup: bei Scroll/Resize schliessen statt neu zu
  // berechnen (einfacher und ausreichend, da der Knopf dabei ohnehin meist
  // aus dem sichtbaren Bereich wandert).
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    window.addEventListener('scroll', close, true);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('scroll', close, true);
      window.removeEventListener('resize', close);
    };
  }, [open]);

  // Klick ausserhalb von Knopf und Liste schliesst das Popup (zusaetzlich zum
  // onBlur unten, das per Tastatur wegfokussierte Faelle abdeckt).
  useEffect(() => {
    if (!open) return;
    const onDocMouseDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (buttonRef.current?.contains(target)) return;
      if (listRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  const choose = (id: string) => {
    onChange(id);
    setOpen(false);
  };

  const activeIndex = activeId === null ? -1 : spools.findIndex((s) => s.id === activeId);

  return (
    <div className="relative">
      <button
        ref={buttonRef}
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
            <span className="truncate min-w-0 flex-1">{triggerLabel(selected)}</span>
            <span className="flex-none font-mono-ui text-[11px] text-[var(--ink-3)]">{formatStockG(selected.remainingWeightG, language)}</span>
          </>
        ) : (
          <span className="text-[var(--ink-3)] truncate flex-1">{placeholder}</span>
        )}
        <span className="flex-none text-[10px] text-[var(--ink-3)]">▾</span>
      </button>
      {open &&
        popup &&
        createPortal(
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
            style={{ position: 'fixed', top: popup.top, left: popup.left, width: popup.width }}
            className="z-[60] max-h-64 overflow-y-auto rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] py-1 outline-0"
          >
            {spools.map((s) => (
              <li
                key={s.id}
                id={optionId(s.id)}
                role="option"
                aria-selected={s.id === value}
                aria-label={text(s, language)}
                onMouseDown={(e) => { e.preventDefault(); choose(s.id); }}
                className={`px-2 py-1.5 flex items-center gap-2 text-[12.5px] cursor-pointer ${s.id === activeId ? 'bg-[var(--panel-2)]' : ''}`}
              >
                <span className="w-3 h-3 mt-0.5 rounded-[3px] flex-none self-start border border-white/15" style={{ background: s.colorHex ?? 'transparent' }} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{[s.material, s.color].filter(Boolean).join(' · ')}</span>
                  <span className="block truncate text-[11px] text-[var(--ink-3)]">{optionMeta(s, language)}</span>
                </span>
              </li>
            ))}
          </ul>,
          document.body,
        )}
    </div>
  );
}
