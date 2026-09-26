import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';
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

// Material · color · manufacturer/location (if any) · remaining weight -
// otherwise identical spools (same material, same color) can't be told
// apart in the list. Used as the full accessible name for button and
// options (independent of the visible layout).
const text = (s: FilamentSpool, language: Language) =>
  [s.material, s.color, s.manufacturer, s.location, formatStockG(s.remainingWeightG, language)].filter(Boolean).join(' · ');

// Short visible label in the closed button - the manufacturer may be
// truncated, the remaining weight is separate and always stays readable.
const triggerLabel = (s: FilamentSpool) => [s.material, s.color, s.manufacturer].filter(Boolean).join(' · ');

// Second line per option in the list (muted): remaining weight first, then
// manufacturer/location - so the weight stays visible even with long names.
const optionMeta = (s: FilamentSpool, language: Language) =>
  [formatStockG(s.remainingWeightG, language), s.manufacturer, s.location].filter(Boolean).join(' · ');

const POPUP_MIN_WIDTH = 384; // 24rem

/** Custom picker with color swatch (native <select> popups ignore the theme). */
export function SpoolPicker({ spools: allSpools, value, onChange, label, placeholder }: Props) {
  const { language } = useLanguage();
  // The printer connection only deducts filament; resin bottles are never a
  // candidate, even if a caller passes the full list.
  const spools = useMemo(() => allSpools.filter((s) => s.kind !== 'resin'), [allSpools]);
  const uid = useId();
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  const { popupRef: listRef, style: popup } = useAnchoredPopup<HTMLButtonElement, HTMLUListElement>(buttonRef, open, close, POPUP_MIN_WIDTH);
  const selected = spools.find((s) => s.id === value) ?? null;
  const optionId = (id: string) => `${uid}-${id}`;

  // On open (or when the selected value changes) jump to the current
  // selection and focus the list (position/closing is handled by
  // useAnchoredPopup).
  useEffect(() => {
    if (open) {
      setActiveId(value ?? spools[0]?.id ?? null);
      // `preventScroll` so that focusing itself doesn't scroll the page in
      // real browsers - otherwise our own scroll listener (in useAnchoredPopup)
      // would close the popup as soon as it is open.
      listRef.current?.focus({ preventScroll: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, value]);

  // If the spool list reloads while the list is open (e.g. after a
  // preview answer), the keyboard position must NOT jump back
  // - only if the active element has actually disappeared.
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
                // Don't pass it on to the surrounding dialog - Escape should
                // only close the list here, not the whole dialog.
                e.stopPropagation();
                setOpen(false);
              }
            }}
            onBlur={() => setOpen(false)}
            style={popup}
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
