import { useEffect, useId, useRef, useState, type CSSProperties } from 'react';

interface Props {
  value: string;
  onChange: (value: string) => void;
  options: string[];
  placeholder?: string;
  className?: string;
  inputClassName?: string;
  inputStyle?: CSSProperties;
  onSubmit?: (value: string) => void;
  onEscape?: () => void;
  maxSuggestions?: number;
  maxLength?: number;
  'aria-label'?: string;
  listAriaLabel?: string;
  formatOption?: (value: string) => string;
}

// Custom autocomplete instead of <datalist>: its suggestion list is rendered
// by the system and ignores the dark theme.
export function AutocompleteInput({
  value, onChange, options, placeholder, className, inputClassName, inputStyle,
  onSubmit, onEscape, maxLength, maxSuggestions = 8, 'aria-label': ariaLabel, listAriaLabel, formatOption,
}: Props) {
  const initialHighlight = onSubmit ? -1 : 0;
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(initialHighlight);
  const containerRef = useRef<HTMLDivElement>(null);

  const listId = useId();
  const filtered = (
    value.trim() === ''
      ? options
      : options.filter((o) => o.toLowerCase().includes(value.trim().toLowerCase()))
  ).slice(0, maxSuggestions);

  useEffect(() => {
    setHighlighted(initialHighlight);
  }, [value, open, initialHighlight]);

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
    if (onSubmit) onSubmit(option);
    else onChange(option);
    setOpen(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      // With the suggestion list open, Escape should only close the list,
      // not also the surrounding popover/form (e.g. RestockPopover)
      // whose Escape handler would otherwise discard the input.
      const listWasOpen = open && filtered.length > 0;
      setOpen(false);
      if (listWasOpen) {
        e.preventDefault();
        e.stopPropagation();
      } else if (onEscape) {
        e.preventDefault();
        e.stopPropagation();
        onEscape();
      }
      return;
    }
    if (e.key === 'Tab') {
      setOpen(false);
      return;
    }
    if (e.key === 'Enter' && onSubmit) {
      e.preventDefault();
      selectOption(open && highlighted >= 0 && highlighted < filtered.length ? filtered[highlighted] : value);
      return;
    }
    if (!open || filtered.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlighted((h) => Math.min(h + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlighted((h) => h < 0 ? filtered.length - 1 : Math.max(h - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      selectOption(filtered[highlighted]);
    }
  };

  return (
    <div ref={containerRef} className="relative">
      <input
        maxLength={maxLength}
        role={onSubmit ? 'combobox' : undefined}
        aria-label={ariaLabel}
        aria-autocomplete="list"
        aria-expanded={open && filtered.length > 0}
        aria-controls={open && filtered.length > 0 ? listId : undefined}
        aria-activedescendant={open && filtered[highlighted] !== undefined ? `${listId}-${highlighted}` : undefined}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        className={inputClassName ?? `w-full ${className ?? ''}`}
        style={inputStyle}
      />
      {open && filtered.length > 0 && (
        <div id={listId} role="listbox" aria-label={listAriaLabel} className="absolute z-40 top-full left-0 right-0 mt-1 max-h-[220px] overflow-y-auto bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)]">
          {filtered.map((option, i) => (
            <div
              key={option}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={i === highlighted}
              onMouseDown={(e) => {
                e.preventDefault();
                selectOption(option);
              }}
              onMouseEnter={() => setHighlighted(i)}
              className={`px-2.5 py-1.5 text-body cursor-pointer ${
                i === highlighted ? 'bg-[var(--accent)] text-[var(--accent-ink)]' : 'text-[var(--ink)]'
              }`}
            >
              {formatOption ? formatOption(option) : option}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
