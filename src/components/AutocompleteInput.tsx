import { useEffect, useRef, useState } from 'react';

interface Props {
  value: string;
  onChange: (value: string) => void;
  options: string[];
  placeholder?: string;
  className?: string;
}

// Eigenes, themekonformes Autocomplete statt <input list="..."> +
// <datalist>: die Vorschlagsliste eines nativen <datalist> wird vom
// Betriebssystem/WebKit gerendert und laesst sich per CSS nicht an das
// dunkle Theme der App anpassen (erscheint immer als weisses System-
// Popup) - gleiches Problem wie bei nativen Checkboxen, die aus demselben
// Grund bereits durch eine eigene Komponente ersetzt wurden.
export function AutocompleteInput({ value, onChange, options, placeholder, className }: Props) {
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  const filtered =
    value.trim() === ''
      ? options
      : options.filter((o) => o.toLowerCase().includes(value.trim().toLowerCase()));

  useEffect(() => {
    setHighlighted(0);
  }, [value, open]);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  const selectOption = (option: string) => {
    onChange(option);
    setOpen(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      setOpen(false);
      return;
    }
    if (!open || filtered.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlighted((h) => Math.min(h + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlighted((h) => Math.max(h - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      selectOption(filtered[highlighted]);
    }
  };

  return (
    <div ref={containerRef} className="relative">
      <input
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        className={`w-full ${className ?? ''}`}
      />
      {open && filtered.length > 0 && (
        <div className="absolute z-40 top-full left-0 right-0 mt-1 max-h-[220px] overflow-y-auto bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)]">
          {filtered.map((option, i) => (
            <div
              key={option}
              onMouseDown={(e) => {
                e.preventDefault();
                selectOption(option);
              }}
              onMouseEnter={() => setHighlighted(i)}
              className={`px-2.5 py-1.5 text-[13px] cursor-pointer ${
                i === highlighted ? 'bg-[var(--accent)] text-[var(--accent-ink)]' : 'text-[var(--ink)]'
              }`}
            >
              {option}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
