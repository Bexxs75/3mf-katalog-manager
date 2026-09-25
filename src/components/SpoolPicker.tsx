import { useEffect, useRef, useState } from 'react';
import type { FilamentSpool } from '../types';

interface Props {
  spools: FilamentSpool[];
  value: string | null;
  onChange: (spoolId: string) => void;
  label: string;
  placeholder?: string;
}

const text = (s: FilamentSpool) => [s.material, s.color].filter(Boolean).join(' · ');

/** Eigene Auswahl mit Farbfeld (native <select>-Popups ignorieren das Theme). */
export function SpoolPicker({ spools, value, onChange, label, placeholder }: Props) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const selected = spools.find((s) => s.id === value) ?? null;

  useEffect(() => {
    if (open) {
      setActive(Math.max(0, spools.findIndex((s) => s.id === value)));
      listRef.current?.focus();
    }
  }, [open, spools, value]);

  const choose = (i: number) => {
    const s = spools[i];
    if (s) onChange(s.id);
    setOpen(false);
  };

  return (
    <div className="relative">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${selected ? text(selected) : placeholder ?? ''}`}
        onClick={() => setOpen((o) => !o)}
        className="w-full h-8 px-2 flex items-center gap-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel-2)] text-[12.5px] text-left cursor-pointer"
      >
        {selected ? (
          <>
            <span className="w-3 h-3 rounded-[3px] flex-none border border-white/15" style={{ background: selected.colorHex ?? 'transparent' }} />
            <span className="truncate">{text(selected)}</span>
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
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') { e.preventDefault(); setActive((a) => Math.min(spools.length - 1, a + 1)); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); setActive((a) => Math.max(0, a - 1)); }
            else if (e.key === 'Enter') { e.preventDefault(); choose(active); }
            else if (e.key === 'Escape') { e.preventDefault(); setOpen(false); }
          }}
          onBlur={() => setOpen(false)}
          className="absolute z-50 mt-1 w-full max-h-56 overflow-y-auto rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] py-1 outline-0"
        >
          {spools.map((s, i) => (
            <li
              key={s.id}
              role="option"
              aria-selected={s.id === value}
              onMouseDown={(e) => { e.preventDefault(); choose(i); }}
              className={`px-2 py-1.5 flex items-center gap-2 text-[12.5px] cursor-pointer ${i === active ? 'bg-[var(--panel-2)]' : ''}`}
            >
              <span className="w-3 h-3 rounded-[3px] flex-none border border-white/15" style={{ background: s.colorHex ?? 'transparent' }} />
              <span className="truncate">{text(s)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
